//! Seasons from before the league sample's window (Flashy, October 2026,
//! for Clark: "are you still working on adding seasons prior to s24?").
//!
//! ETF2L's API has every Highlander season back to Season 1 (January 2012):
//! results, tables, and every match's page with who played. What it lacks is
//! the logs: the app finds a match's logs through trends.tf, whose links
//! start in March 2019. Before that, they are found here from the logs.tf
//! dump's index (`hl logsdump`), which holds every Highlander log since
//! logs.tf began in 2013 with the players in it: a match's logs are the ones
//! on its maps, near its time, with most of both lineups in them. Only the
//! log ids come from the dump; the logs are then downloaded, read and rated
//! by the league sample like any other ([`crate::league_sample::step`]).

use anyhow::Result;
use hl_db::{Db, DumpIndex, DumpLog, LeagueLogRow};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// How long before ETF2L's time a log may start: servers are often up and
/// the first map going a little early.
const BEFORE_S: i64 = 3 * 3600;
/// ...and after: a match played late on its night, or both maps back to back.
const AFTER_S: i64 = 8 * 3600;
/// A week's deadline stands for a match played any day of the week before
/// it, and a match is often a day or two late.
const DEADLINE_WEEK_S: i64 = 8 * 86400;
/// Players of each lineup a log must hold to be the match's: a scrim of one
/// of the teams the same evening has one lineup, not both.
const MIN_EACH_SIDE: usize = 5;
/// Shorter logs are restarts and forfeits.
const MIN_DURATION_S: i64 = 300;
/// trends.tf's links of ETF2L logs to their matches start here (24 March
/// 2019). A season from then on takes its links first, and the dump only for
/// the matches left: S19 to S21's pages give a deadline for a time, players
/// on the wrong team or no players at all.
const TRENDS_FROM: i64 = 1_553_385_600;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub season: i64,
    pub competitions: usize,
    pub matches: usize,
    /// Played, not a default win.
    pub played: usize,
    /// Played with ETF2L's list of who played.
    pub with_lineups: usize,
    /// Played matches some log was found for.
    pub linked: usize,
    pub logs: usize,
    /// Played matches whose page lists nobody (Season 19's), found by the
    /// teams' rosters on the day instead.
    pub by_roster: usize,
    /// Matches linked by trends.tf, and their logs.
    pub from_trends: usize,
    pub trends_logs: usize,
    /// Matches with lineups but no log: no upload, or one too far from the time.
    pub unlinked: Vec<i64>,
}

/// A team's players on a day, from ETF2L's transfers (read once per team):
/// `account -> name` for everyone whose last move before `at` was joining.
async fn roster_on(db: &Db, sources: &crate::sources::Sources, team: i64, at: i64) -> Result<HashMap<i64, String>> {
    if db.transfers_fetched("team", team).await?.is_none() {
        crate::transfers::fetch_team(db, sources, team).await?;
    }
    let mut last: HashMap<i64, (i64, bool, Option<u32>, String)> = HashMap::new();
    for t in db.team_transfers(team).await? {
        if t.time > at {
            continue;
        }
        let e = last.entry(t.player_id).or_insert((i64::MIN, false, None, String::new()));
        if t.time >= e.0 {
            *e = (t.time, t.kind == "joined", t.account_id.or(e.2), t.player_name.clone());
        }
    }
    let mut out = HashMap::new();
    for (player, (_, member, account, name)) in last {
        if !member {
            continue;
        }
        let account = match account {
            Some(a) => Some(a),
            None => crate::leagues::steam_of_player(db, sources, player).await?,
        };
        if let Some(a) = account {
            out.insert(i64::from(a), name);
        }
    }
    Ok(out)
}

/// A player's log and the server's of one map this close in length are the
/// same game uploaded twice. By length, not time: the dump's time
/// is the upload's, and a player uploads all their logs in the same minute.
const SAME_GAME_S: i64 = 15;
/// A log whose length is the sum of two or more of the match's others, to
/// this many seconds, is their combine.
const COMBINE_SLACK_S: i64 = 30;

/// A week's deadline for a time: midnight in Central Europe, 23:00 UTC in
/// winter and 22:00 in summer, on the hour. Nobody plays an official then.
fn is_deadline(time: i64) -> bool {
    time % 3600 == 0 && matches!(time.rem_euclid(86400) / 3600, 22 | 23)
}

/// One map's name in two spellings: ETF2L's `koth_coalpant_b7` is
/// `koth_coalplant_b7`. The same base, or one letter apart.
fn same_base(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (long, short) = if a.len() > b.len() { (a, b) } else { (b, a) };
    long.len() == short.len() + 1 && (0..long.len()).any(|i| long.get(..i).zip(long.get(i + 1..)).is_some_and(|(x, y)| format!("{x}{y}") == short))
}

/// Which of the match's maps a log is of. The server names the map
/// (`pl_badwater`); a player who uploads it names it as they like
/// ("Badwater pt1", "Product"), and a raw server log uploaded by hand has no
/// map at all, which is the match's when it played only one.
fn map_of(log_map: &str, maps: &[String]) -> Option<String> {
    let lower = log_map.trim().to_ascii_lowercase();
    if lower.is_empty() {
        let mut distinct: Vec<&String> = maps.iter().collect();
        distinct.dedup_by_key(|m| hl_core::maps::map_base(m));
        return (distinct.len() == 1).then(|| distinct[0].clone());
    }
    let base = if ["koth_", "pl_", "cp_", "ctf_", "plr_"].iter().any(|p| lower.starts_with(p)) {
        hl_core::maps::map_base(&lower)
    } else {
        lower.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("").to_string()
    };
    maps.iter().find(|m| same_base(&hl_core::maps::map_base(m), &base)).cloned()
}

/// One match's logs from what the dump found, each with the match's map it
/// is of: both lineups in it, long enough, not a duplicate; not a combine
/// (named so, or as long as two or more of the others together --
/// "Combined", 4091 s, beside its parts of 1860, 951 and 1280); one upload
/// per game, the server's first; and per map the best overlaps, two (the
/// halves) on a payload or attack/defend map and one on king of the hill and
/// 5CP.
fn pick(
    maps: &[String],
    found: &HashMap<i64, Vec<i64>>,
    logs: &HashMap<i64, DumpLog>,
    side_a: &HashSet<i64>,
    side_b: &HashSet<i64>,
    two_halves: impl Fn(&str) -> bool,
) -> Vec<(i64, String)> {
    // (log, map, both lineups' players in it, the server's own upload)
    let mut cands: Vec<(&DumpLog, String, usize, bool)> = Vec::new();
    for (log_id, accounts) in found {
        let Some(l) = logs.get(log_id) else { continue };
        if l.duplicate || l.duration_s < MIN_DURATION_S {
            continue;
        }
        let Some(map) = map_of(&l.map, maps) else { continue };
        let a = accounts.iter().filter(|x| side_a.contains(x)).count();
        let b = accounts.iter().filter(|x| side_b.contains(x)).count();
        // Sides unknown (`side_b` empty): most of the one lineup instead.
        let enough = if side_b.is_empty() { a >= 2 * MIN_EACH_SIDE } else { a >= MIN_EACH_SIDE && b >= MIN_EACH_SIDE };
        if !enough {
            continue;
        }
        let server = ["koth_", "pl_", "cp_", "ctf_", "plr_"].iter().any(|p| l.map.to_ascii_lowercase().starts_with(p));
        cands.push((l, map, a + b, server));
    }
    let combine = |l: &DumpLog| {
        if l.map.to_ascii_lowercase().contains("combined") || l.title.to_ascii_lowercase().contains("combined") {
            return true;
        }
        // Two or more of the others, shortest first, adding up to its length.
        let mut others: Vec<i64> = cands.iter().filter(|(o, ..)| o.log_id != l.log_id && o.duration_s < l.duration_s).map(|(o, ..)| o.duration_s).collect();
        others.sort_unstable();
        let n = others.len().min(16);
        (1u32..(1 << n)).any(|mask| mask.count_ones() >= 2 && ((0..n).filter(|i| mask & (1 << i) != 0).map(|i| others[i]).sum::<i64>() - l.duration_s).abs() <= COMBINE_SLACK_S)
    };
    let kept: Vec<&(&DumpLog, String, usize, bool)> = cands.iter().filter(|(l, ..)| !combine(l)).collect();
    let mut by_map: BTreeMap<String, Vec<&(&DumpLog, String, usize, bool)>> = BTreeMap::new();
    for c in kept {
        by_map.entry(hl_core::maps::map_name(&c.1)).or_default().push(c);
    }
    let mut out = Vec::new();
    for (map, mut group) in by_map {
        // The server's upload first, then most of the lineups, then the later
        // log (a restart's is the earlier one).
        group.sort_by(|x, y| y.3.cmp(&x.3).then(y.2.cmp(&x.2)).then(y.0.played_at.cmp(&x.0.played_at)));
        let halves = if two_halves(&map) { 2 } else { 1 };
        let mut taken: Vec<&(&DumpLog, String, usize, bool)> = Vec::new();
        for c in group {
            if taken.len() == halves {
                break;
            }
            // A player's upload beside the server's of the same length is the
            // same game; two of the server's are two halves, however long.
            if taken.iter().any(|t| t.3 != c.3 && (t.0.duration_s - c.0.duration_s).abs() <= SAME_GAME_S) {
                continue;
            }
            taken.push(c);
        }
        out.extend(taken.into_iter().map(|c| (c.0.log_id, c.1.clone())));
    }
    out
}

/// Import one season: ETF2L's competitions, results and match pages, then
/// the dump's logs for each played match, stored and picked for the league
/// sample to download and rate.
pub async fn import(db: &Db, sources: &crate::sources::Sources, w: &hl_rating::Weights, dump: &Path, season: i64, mut progress: impl FnMut(&str)) -> Result<Imported> {
    let fetched = crate::leagues::fetch_season(db, sources, season, &mut progress).await?;
    let index = DumpIndex::open(dump).await?;
    let matches = db.season_matches(Some(season), None).await?;
    let mut lineups: HashMap<i64, Vec<(i64, Option<i64>)>> = HashMap::new();
    for (m, account, team) in db.season_lineups(season).await? {
        lineups.entry(m).or_default().push((account, team));
    }
    let mut out = Imported { season, competitions: fetched.competitions, matches: matches.len(), ..Default::default() };
    // Links found by an earlier run and not downloaded yet are found again.
    db.clear_unfetched_league_logs(&matches.iter().map(|m| m.match_id).collect::<Vec<_>>()).await?;

    // trends.tf's own links, where it has them: newest first, down to the
    // season's start.
    let ids: HashSet<i64> = matches.iter().map(|m| m.match_id).collect();
    let first = matches.iter().filter_map(|m| m.time).filter(|t| *t > 0).min().unwrap_or(0);
    let mut by_trends: HashSet<i64> = HashSet::new();
    if first >= TRENDS_FROM - 30 * 86_400 {
        let mut path: Option<String> = None;
        for page in 0..400 {
            progress(&format!("Reading trends.tf's links, page {}", page + 1));
            // trends.tf down (all of 29-30 October 2026, and on page 79
            // of S21's): the links it gave so far, and the dump for the rest.
            let (rows, next) = match sources.trends_league_page(path.as_deref()).await {
                Ok(page) => page,
                Err(e) => {
                    tracing::warn!(error = %e, "trends.tf's links stopped; the dump finds the rest");
                    break;
                }
            };
            let keep: Vec<&crate::sources::TrendsRow> = rows
                .iter()
                .filter(|r| r.matchid.is_some_and(|m| ids.contains(&m)) && r.duplicate_of.as_ref().is_none_or(|d| d.is_empty()))
                .collect();
            let league: Vec<LeagueLogRow> = keep
                .iter()
                .filter_map(|r| Some(LeagueLogRow { log_id: r.logid, etf2l_match_id: r.matchid?, map: r.map.as_deref()?, played_at: r.time?, duration_s: r.duration, title: r.title.as_deref() }))
                .collect();
            db.upsert_league_logs(&league).await?;
            db.pick_league_logs(&league.iter().map(|l| l.log_id).collect::<Vec<_>>()).await?;
            out.trends_logs += league.len();
            by_trends.extend(league.iter().map(|l| l.etf2l_match_id));
            let oldest = rows.iter().filter_map(|r| r.time).min().unwrap_or(0);
            match next {
                Some(n) if !rows.is_empty() && oldest >= first - 2 * 86_400 => path = Some(n),
                _ => break,
            }
        }
        out.from_trends = by_trends.len();
    }
    // (log, match, the match's map it is of, the dump's row)
    let mut rows: Vec<(i64, i64, String, DumpLog)> = Vec::new();
    for (i, m) in matches.iter().enumerate() {
        let played = !m.default_win && m.r1.unwrap_or(0) + m.r2.unwrap_or(0) > 0;
        let Some(time) = m.time.filter(|_| played) else { continue };
        out.played += 1;
        if by_trends.contains(&m.match_id) {
            out.linked += 1;
            continue;
        }
        progress(&format!("Finding the logs of match {} of {}", i + 1, matches.len()));
        // Who played, from the page; or, where it lists nobody, each team's
        // roster that day, and those of them the logs then show did play.
        let (a, b, rosters) = match lineups.get(&m.match_id).filter(|p| !p.is_empty()) {
            Some(players) => {
                out.with_lineups += 1;
                let side = |team: i64| players.iter().filter(|(_, t)| *t == Some(team)).map(|(a, _)| *a).collect::<HashSet<i64>>();
                let (a, b) = (side(m.clan1_id), side(m.clan2_id));
                if a.len() >= MIN_EACH_SIDE && b.len() >= MIN_EACH_SIDE {
                    (a, b, None)
                } else {
                    // Seasons 20 and 21's pages give each player some other
                    // team (theirs when the API was asked): split them by
                    // the teams' rosters that day, and store them on the
                    // right side; else one lineup of both.
                    let all: HashSet<i64> = players.iter().map(|(a, _)| *a).collect();
                    let (ra, rb) = (roster_on(db, sources, m.clan1_id, time).await?, roster_on(db, sources, m.clan2_id, time).await?);
                    let fa: HashSet<i64> = all.iter().filter(|x| ra.contains_key(x)).copied().collect();
                    let fb: HashSet<i64> = all.iter().filter(|x| rb.contains_key(x) && !fa.contains(x)).copied().collect();
                    if fa.len() >= MIN_EACH_SIDE && fb.len() >= MIN_EACH_SIDE {
                        let fixed: Vec<(u32, i64, &str)> = fa
                            .iter()
                            .map(|x| (*x as u32, m.clan1_id, ra[x].as_str()))
                            .chain(fb.iter().map(|x| (*x as u32, m.clan2_id, rb[x].as_str())))
                            .collect();
                        db.add_season_players(m.match_id, &fixed).await?;
                        (fa, fb, None)
                    } else {
                        (all, HashSet::new(), None)
                    }
                }
            }
            None => {
                let (ra, rb) = (roster_on(db, sources, m.clan1_id, time).await?, roster_on(db, sources, m.clan2_id, time).await?);
                if ra.len() < MIN_EACH_SIDE || rb.len() < MIN_EACH_SIDE {
                    continue;
                }
                out.by_roster += 1;
                (ra.keys().copied().collect(), rb.keys().copied().collect(), Some((ra, rb)))
            }
        };
        let accounts: Vec<i64> = a.iter().chain(b.iter()).copied().collect();
        let mut found = index.logs_with(&accounts, time - BEFORE_S, time + AFTER_S).await?;
        let mut logs: HashMap<i64, DumpLog> = index.logs(&found.keys().copied().collect::<Vec<_>>()).await?.into_iter().map(|l| (l.log_id, l)).collect();
        let mut picked = pick(&m.maps, &found, &logs, &a, &b, |map| w.attack_defend(map));
        if picked.is_empty() && is_deadline(time) {
            // The week's deadline, not when it was played (S20's Week 4:
            // Saturday 23:00 UTC): the week before it, and of that the
            // evening of the latest log both lineups played on the match's
            // maps, not a scrim of the same two teams days before.
            found = index.logs_with(&accounts, time - DEADLINE_WEEK_S, time + AFTER_S).await?;
            logs = index.logs(&found.keys().copied().collect::<Vec<_>>()).await?.into_iter().map(|l| (l.log_id, l)).collect();
            let wide = pick(&m.maps, &found, &logs, &a, &b, |map| w.attack_defend(map));
            if let Some(last) = wide.iter().filter_map(|(id, _)| logs.get(id)).map(|l| l.played_at).max() {
                found.retain(|id, _| logs.get(id).is_some_and(|l| l.played_at > last - BEFORE_S - AFTER_S && l.played_at <= last));
                picked = pick(&m.maps, &found, &logs, &a, &b, |map| w.attack_defend(map));
            }
        }
        if picked.is_empty() {
            out.unlinked.push(m.match_id);
            continue;
        }
        out.linked += 1;
        if let Some((ra, rb)) = &rosters {
            // The roster players in the match's logs: who played, by team.
            let mut played: Vec<(u32, i64, &str)> = Vec::new();
            let mut seen = HashSet::new();
            for (id, _) in &picked {
                for acc in found.get(id).into_iter().flatten() {
                    if !seen.insert(*acc) {
                        continue;
                    }
                    if let Some(n) = ra.get(acc) {
                        played.push((*acc as u32, m.clan1_id, n.as_str()));
                    } else if let Some(n) = rb.get(acc) {
                        played.push((*acc as u32, m.clan2_id, n.as_str()));
                    }
                }
            }
            db.add_season_players(m.match_id, &played).await?;
        }
        for (id, map) in picked {
            if let Some(l) = logs.get(&id) {
                rows.push((id, m.match_id, map, l.clone()));
            }
        }
    }
    let league: Vec<LeagueLogRow> = rows
        .iter()
        // The match's own map name, not a player's ("Badwater pt1"): the
        // sample picks by real maps.
        .map(|(id, m, map, l)| LeagueLogRow { log_id: *id, etf2l_match_id: *m, map: map.as_str(), played_at: l.played_at, duration_s: Some(l.duration_s), title: Some(l.title.as_str()) })
        .collect();
    db.upsert_league_logs(&league).await?;
    db.pick_league_logs(&rows.iter().map(|r| r.0).collect::<Vec<_>>()).await?;
    out.logs = rows.len() + out.trends_logs;
    Ok(out)
}

/// Where the dump's index is looked for when none is set: where `hl
/// logsdump` wrote it on Flashy's machine.
pub const DEFAULT_INDEX: &str = "F:/logs.tf database/index-v2.sqlite3";
/// The setting naming the index, once chosen.
pub const INDEX_KEY: &str = "logs_dump_index";

/// The dump's index to use: the one chosen, else the default where it
/// exists. `None` on a machine without the dump.
pub async fn index_path(db: &Db) -> Result<Option<std::path::PathBuf>> {
    let chosen = db.get_setting(INDEX_KEY).await?.map(std::path::PathBuf::from);
    Ok(chosen.or_else(|| Some(std::path::PathBuf::from(DEFAULT_INDEX))).filter(|p| p.is_file()))
}

/// One season as the Older seasons panel shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonCoverage {
    pub season: i64,
    pub played: i64,
    pub linked: i64,
    pub logs: i64,
    pub downloaded: i64,
    pub rated: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The dump's index, where there is one.
    pub index: Option<String>,
    pub seasons: Vec<SeasonCoverage>,
}

pub async fn status(db: &Db) -> Result<Status> {
    let seasons = db
        .season_coverage(hl_rating::MODEL_VERSION)
        .await?
        .into_iter()
        .map(|(season, played, linked, logs, downloaded, rated)| SeasonCoverage { season, played, linked, logs, downloaded, rated })
        .collect();
    Ok(Status { index: index_path(db).await?.map(|p| p.to_string_lossy().into_owned()), seasons })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(id: i64, map: &str, at: i64) -> DumpLog {
        DumpLog { log_id: id, map: map.into(), played_at: at, duration_s: 1500, title: String::new(), players: 18, duplicate: false }
    }

    #[test]
    fn a_deadline_is_midnight_in_central_europe_on_the_hour() {
        assert!(is_deadline(1_575_759_600), "S20 Week 4: 7 December 2019, 23:00 UTC");
        assert!(is_deadline(1_563_660_000), "summer: 22:00 UTC");
        assert!(!is_deadline(1_581_178_243), "S20 Open's quarter-final at 16:10:43");
        assert!(!is_deadline(1_575_748_800), "20:00 UTC, an evening's time");
    }

    #[test]
    fn a_match_is_its_maps_with_both_lineups() {
        // S6 as it was found on 26 October 2026: two cp_steel halves and the
        // pl_badwater games with 18 of 19 players; a scrim of one side the
        // same evening on badwater, and a koth game, with 9. S12's re-uploads
        // named after the map ("badwater") beside the server's; a 5CP map
        // restarted six minutes in.
        let a: HashSet<i64> = (1..=9).collect();
        let b: HashSet<i64> = (11..=19).collect();
        let both: Vec<i64> = (1..=9).chain(11..=19).collect();
        let one: Vec<i64> = (1..=9).collect();
        let found: HashMap<i64, Vec<i64>> =
            [(1, one.clone()), (2, one), (3, both.clone()), (4, both.clone()), (5, both.clone()), (6, both.clone()), (7, both.clone()), (8, both.clone()), (9, both.clone()), (10, both)].into();
        let logs: HashMap<i64, DumpLog> = [
            log(1, "pl_badwater", 0),
            log(2, "koth_lakeside_final", 2_000),
            log(3, "cp_steel_rc1", 4_000),
            log(4, "cp_steel_rc1", 6_000),
            log(5, "pl_badwater", 8_000),
            log(6, "pl_badwater", 10_000),
            log(7, "pl_badwater", 12_000),
            // The same half again, uploaded by a player: the same length.
            DumpLog { duration_s: 1_505, ..log(8, "badwater", 12_030) },
            DumpLog { duration_s: 360, ..log(9, "cp_gullywash_final1", 14_000) },
            log(10, "cp_gullywash_final1", 14_400),
        ]
        .into_iter()
        .map(|l| (l.log_id, l))
        .collect();
        let ad = |m: &str| m.starts_with("pl_") || m.starts_with("cp_steel");
        let mut got: Vec<i64> = pick(&["cp_steel".into(), "pl_badwater".into(), "cp_gullywash_final1".into()], &found, &logs, &a, &b, ad).into_iter().map(|p| p.0).collect();
        got.sort();
        assert_eq!(got, vec![3, 4, 6, 7, 10], "both steel halves, the last two badwater halves, gullywash once; not the scrim or the re-upload");
    }

    #[test]
    fn a_players_upload_is_its_maps_and_a_combine_is_not() {
        // S12's 68499 (kiti vs opeens): only a player's uploads, "Product",
        // "Badwater pt1" and "pt2", and their combine of all three.
        let a: HashSet<i64> = (1..=9).collect();
        let b: HashSet<i64> = (11..=19).collect();
        let both: Vec<i64> = (1..=9).chain(11..=19).collect();
        let at = |id: i64, map: &str, t: i64, len: i64| DumpLog { log_id: id, map: map.into(), played_at: t, duration_s: len, title: String::new(), players: 18, duplicate: false };
        let logs: HashMap<i64, DumpLog> =
            // All four uploaded in the same minute, as the dump has them.
            [at(1, "Product", 7_200, 1860), at(2, "Badwater pt1", 7_560, 951), at(3, "Badwater pt2", 7_560, 1280), at(4, "Combined", 7_920, 4091)].into_iter().map(|l| (l.log_id, l)).collect();
        let found: HashMap<i64, Vec<i64>> = (1..=4).map(|i| (i, both.clone())).collect();
        let mut got = pick(&["koth_product_rc8".into(), "pl_badwater".into()], &found, &logs, &a, &b, |m: &str| m.starts_with("pl_"));
        got.sort();
        assert_eq!(got, vec![(1, "koth_product_rc8".into()), (2, "pl_badwater".into()), (3, "pl_badwater".into())]);
        // A raw server log uploaded by hand has no map: the match's, when it played one.
        assert_eq!(map_of("", &["cp_steel".into(), "cp_steel".into()]).as_deref(), Some("cp_steel"));
        assert_eq!(map_of("", &["cp_steel".into(), "pl_upward".into()]), None);
        // ETF2L's spelling of a map one letter off.
        assert_eq!(map_of("koth_coalplant_b7", &["koth_coalpant_b7".into()]).as_deref(), Some("koth_coalpant_b7"));
        assert!(!same_base("upward", "borneo"));
    }
}
