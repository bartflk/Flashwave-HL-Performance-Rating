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
/// Players of each lineup a log must hold to be the match's: a scrim of one
/// of the teams the same evening has one lineup, not both.
const MIN_EACH_SIDE: usize = 5;
/// Shorter logs are restarts and forfeits.
const MIN_DURATION_S: i64 = 300;

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
    /// Matches with lineups but no log: no upload, or one too far from the time.
    pub unlinked: Vec<i64>,
}

/// A player's log and the server's of one map this close in length are the
/// same game uploaded twice. By length, not time: the dump's time
/// is the upload's, and a player uploads all their logs in the same minute.
const SAME_GAME_S: i64 = 15;
/// A log whose length is the sum of two or more of the match's others, to
/// this many seconds, is their combine.
const COMBINE_SLACK_S: i64 = 30;

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
        if a < MIN_EACH_SIDE || b < MIN_EACH_SIDE {
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
    // (log, match, the match's map it is of, the dump's row)
    let mut rows: Vec<(i64, i64, String, DumpLog)> = Vec::new();
    for (i, m) in matches.iter().enumerate() {
        let played = !m.default_win && m.r1.unwrap_or(0) + m.r2.unwrap_or(0) > 0;
        let Some(time) = m.time.filter(|_| played) else { continue };
        out.played += 1;
        let Some(players) = lineups.get(&m.match_id).filter(|p| !p.is_empty()) else { continue };
        out.with_lineups += 1;
        progress(&format!("Finding the logs of match {} of {}", i + 1, matches.len()));
        let side = |team: i64| players.iter().filter(|(_, t)| *t == Some(team)).map(|(a, _)| *a).collect::<HashSet<i64>>();
        let (a, b) = (side(m.clan1_id), side(m.clan2_id));
        let accounts: Vec<i64> = players.iter().map(|(a, _)| *a).collect();
        let found = index.logs_with(&accounts, time - BEFORE_S, time + AFTER_S).await?;
        let candidates: Vec<i64> = found.keys().copied().collect();
        let logs: HashMap<i64, DumpLog> = index.logs(&candidates).await?.into_iter().map(|l| (l.log_id, l)).collect();
        let picked = pick(&m.maps, &found, &logs, &a, &b, |map| w.attack_defend(map));
        if picked.is_empty() {
            out.unlinked.push(m.match_id);
            continue;
        }
        out.linked += 1;
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
    db.clear_unfetched_league_logs(&matches.iter().map(|m| m.match_id).collect::<Vec<_>>()).await?;
    db.upsert_league_logs(&league).await?;
    db.pick_league_logs(&rows.iter().map(|r| r.0).collect::<Vec<_>>()).await?;
    out.logs = rows.len();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(id: i64, map: &str, at: i64) -> DumpLog {
        DumpLog { log_id: id, map: map.into(), played_at: at, duration_s: 1500, title: String::new(), players: 18, duplicate: false }
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
