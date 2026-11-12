//! An RGL match page's statistics, read from the match's logs (Flashy: "keep
//! working and iterating on the rgl page ... i want a lot of the site
//! features [of tf2esports.com] copied": its per-map team comparison, player
//! table, awards and medic matchup).
//!
//! The logs are the ones `rgl::match_more` found and kept. Each log's JSON
//! comes from the app's own database where the log is stored (an owner's
//! match, or a league sample log), else from drops.tf (by Icewind) -- one
//! request, kept on disk for good under `/internal/log-json/{id}` in
//! `rgl_answer`, because a log does not change. It is not put in the league
//! sample's tables: those feed the rating pool and the season tables, and a
//! match someone opened on the RGL tab has no business in either (and
//! writing there would need its ETF2L match). Nothing here asks for a log
//! until the match page is open, and never for a list of matches.
//!
//! The aggregation below is pure (no database, no network) so it can be tested
//! on the logs in the fixtures.

use crate::normalize::normalize;
use crate::rgl::{one_match, rfc3339, roster_at, team_raw};
use crate::sources::Sources;
use anyhow::Result;
use hl_core::matchdata::{NormalizedLog, Team};
use hl_db::Db;
use hl_rating::Weights;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Most logs read for one match: a Highlander match is two or three maps, so
/// more than this is a wrong search, not a bigger match.
const MAX_LOGS: usize = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglMatchStats {
    pub maps: Vec<RglMapStats>,
    /// Logs that could not be read (neither the database, drops.tf nor
    /// logs.tf had them): they are asked for again the next time the page opens.
    pub unread: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglMapStats {
    pub log_id: i64,
    pub map: String,
    pub minutes: f64,
    /// RED's and BLU's totals, in that order.
    pub sides: [SideTotals; 2],
    pub players: Vec<StatPlayer>,
    pub awards: Awards,
    /// Whether the players carry the app's rating (needs the rating's
    /// baselines, which exist after a first full rating pass).
    pub rated: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SideTotals {
    /// "Red" or "Blue".
    pub side: String,
    /// The RGL team that played this colour, where the rosters say.
    pub team_id: Option<i64>,
    /// Rounds won on the map.
    pub score: i64,
    pub kills: i64,
    pub deaths: i64,
    pub damage: i64,
    pub heals: i64,
    pub ubers: i64,
    pub drops: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatPlayer {
    pub steam_id: String,
    pub name: String,
    pub side: String,
    pub team_id: Option<i64>,
    /// Most played.
    pub class: Option<String>,
    pub minutes: f64,
    pub kills: i64,
    pub assists: i64,
    pub deaths: i64,
    pub damage: i64,
    /// Damage and damage taken per minute; none under a minute played.
    pub dpm: Option<f64>,
    pub dtpm: Option<f64>,
    /// DPM minus the class's typical DPM, and what "typical" is:
    /// "archive" (the logs.tf archive's median for the class) or "opponent"
    /// (the other team's player of the class).
    pub dpm_vs: Option<f64>,
    pub dpm_vs_basis: Option<&'static str>,
    pub headshots: i64,
    pub heals: i64,
    pub ubers: i64,
    pub drops: i64,
    /// The app's rating for this game, where it could be rated.
    pub rating: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Awards {
    /// By the app's rating when the game was rated, else by
    /// (kills + half the assists) per death, medics left out: `mvp.basis`.
    pub mvp: Option<Award>,
    /// By heals, an uber counting as much as 500 of them.
    pub medic_mvp: Option<Award>,
    pub top_fragger: Option<Award>,
    pub most_headshots: Option<Award>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Award {
    pub steam_id: String,
    pub name: String,
    pub side: String,
    pub class: Option<String>,
    pub value: f64,
    /// What `value` is: "rating", "kapd", "heals", "kills", "headshots".
    pub basis: &'static str,
    /// Ubers for the medic, deaths for the fragger.
    pub extra: Option<i64>,
}

// ---- Pure aggregation -----------------------------------------------------------

/// The class's typical damage per minute from the logs.tf archive the app
/// ships (the 50th percentile, read between the scale's 40th and 55th): the
/// one cheap class baseline it has, the other being a query over every stored
/// game. `None` when the archive has no such class.
pub fn class_median_dpm(class: &str) -> Option<f64> {
    static ARCHIVE: OnceLock<Option<crate::stat_scale::ArchiveScales>> = OnceLock::new();
    let a = ARCHIVE.get_or_init(crate::stat_scale::archive).as_ref()?;
    median_of(&a.classes.get(class)?.dpm, &a.percentiles)
}

/// The 50th percentile of values given at `pcts`.
fn median_of(cuts: &[f64], pcts: &[f64]) -> Option<f64> {
    if cuts.len() != pcts.len() || cuts.len() < 2 {
        return None;
    }
    let i = pcts.iter().position(|&p| p >= 50.0)?;
    if i == 0 {
        return Some(cuts[0]);
    }
    let t = (50.0 - pcts[i - 1]) / (pcts[i] - pcts[i - 1]);
    Some(cuts[i - 1] + t * (cuts[i] - cuts[i - 1]))
}

fn side_name(t: Team) -> &'static str {
    match t {
        Team::Red => "Red",
        Team::Blue => "Blue",
    }
}

/// One log's statistics. `ratings`: account -> the app's rating in this log.
/// `median`: a class's typical DPM, where there is a baseline for it.
pub fn map_stats(log: &NormalizedLog, ratings: &HashMap<u32, f64>, median: &dyn Fn(&str) -> Option<f64>) -> RglMapStats {
    let mut players: Vec<StatPlayer> = log
        .players
        .iter()
        .map(|p| {
            let secs = p.total_time();
            let minutes = secs as f64 / 60.0;
            let per_min = |n: i64| (secs >= 60).then(|| n as f64 / minutes);
            let class = p.main_class().map(|c| c.as_str().to_string());
            let dpm = per_min(p.stats.dmg);
            let vs = dpm.zip(class.as_deref().and_then(median)).map(|(d, m)| d - m);
            StatPlayer {
                steam_id: p.id.as_u64().to_string(),
                name: p.name.clone().unwrap_or_else(|| p.id.as_u64().to_string()),
                side: side_name(p.team).into(),
                team_id: None,
                class,
                minutes,
                kills: p.stats.kills,
                assists: p.stats.assists,
                deaths: p.stats.deaths,
                damage: p.stats.dmg,
                dpm,
                dtpm: per_min(p.stats.dt),
                dpm_vs: vs,
                dpm_vs_basis: vs.map(|_| "archive"),
                headshots: p.stats.headshots,
                heals: p.stats.heal,
                ubers: p.stats.ubers,
                drops: p.stats.drops,
                rating: ratings.get(&p.id.account_id()).copied(),
            }
        })
        .collect();
    // Without a baseline for a class, the other team's player of it is the
    // yardstick: a Highlander team has one of each.
    let fallbacks: Vec<(usize, f64)> = players
        .iter()
        .enumerate()
        .filter(|(_, p)| p.dpm_vs.is_none() && p.class.is_some())
        .filter_map(|(i, p)| {
            let other = players.iter().find(|o| o.side != p.side && o.class == p.class)?;
            Some((i, p.dpm? - other.dpm?))
        })
        .collect();
    for (i, d) in fallbacks {
        players[i].dpm_vs = Some(d);
        players[i].dpm_vs_basis = Some("opponent");
    }
    players.sort_by(|a, b| {
        class_order(a.class.as_deref()).cmp(&class_order(b.class.as_deref())).then_with(|| a.side.cmp(&b.side)).then_with(|| a.name.cmp(&b.name))
    });

    let totals = |team: Team, score: i64| {
        let mut t = SideTotals { side: side_name(team).into(), score, ..Default::default() };
        for p in log.players.iter().filter(|p| p.team == team) {
            t.kills += p.stats.kills;
            t.deaths += p.stats.deaths;
            t.damage += p.stats.dmg;
            t.heals += p.stats.heal;
            t.ubers += p.stats.ubers;
            t.drops += p.stats.drops;
        }
        t
    };
    let awards = awards(&players);
    RglMapStats {
        log_id: log.log_id,
        map: log.map.clone().unwrap_or_default(),
        minutes: log.duration_s as f64 / 60.0,
        sides: [totals(Team::Red, log.red_score), totals(Team::Blue, log.blue_score)],
        rated: players.iter().any(|p| p.rating.is_some()),
        players,
        awards,
    }
}

/// Highlander lineup order, so the table reads Scout to Spy.
fn class_order(c: Option<&str>) -> usize {
    ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"].iter().position(|x| Some(*x) == c).unwrap_or(9)
}

fn award(p: &StatPlayer, value: f64, basis: &'static str, extra: Option<i64>) -> Award {
    Award { steam_id: p.steam_id.clone(), name: p.name.clone(), side: p.side.clone(), class: p.class.clone(), value, basis, extra }
}

/// The best by `key`; the earlier row keeps a tie.
fn best<'a>(players: impl Iterator<Item = &'a StatPlayer>, key: impl Fn(&StatPlayer) -> f64) -> Option<(&'a StatPlayer, f64)> {
    let mut top: Option<(&StatPlayer, f64)> = None;
    for p in players {
        let k = key(p);
        if top.is_none_or(|(_, b)| k > b) {
            top = Some((p, k));
        }
    }
    top
}

/// The match's awards. Only players who played a minute or more count.
pub fn awards(players: &[StatPlayer]) -> Awards {
    let played = || players.iter().filter(|p| p.minutes >= 1.0);
    let mvp = if players.iter().any(|p| p.rating.is_some()) {
        best(played().filter(|p| p.rating.is_some()), |p| p.rating.unwrap_or(0.0)).map(|(p, v)| award(p, v, "rating", None))
    } else {
        best(played().filter(|p| p.class.as_deref() != Some("medic")), |p| (p.kills as f64 + p.assists as f64 / 2.0) / p.deaths.max(1) as f64)
            .map(|(p, v)| award(p, v, "kapd", None))
    };
    let medic_mvp = best(played().filter(|p| p.class.as_deref() == Some("medic") && p.heals > 0), |p| (p.heals + 500 * p.ubers) as f64)
        .map(|(p, _)| award(p, p.heals as f64, "heals", Some(p.ubers)));
    // Most kills; fewer deaths settle a tie.
    let top_fragger = best(played(), |p| p.kills as f64 - p.deaths as f64 / 1000.0).map(|(p, _)| award(p, p.kills as f64, "kills", Some(p.deaths)));
    let most_headshots = best(played().filter(|p| p.headshots > 0), |p| p.headshots as f64).map(|(p, v)| award(p, v, "headshots", None));
    Awards { mvp, medic_mvp, top_fragger, most_headshots }
}

/// Which of the match's two teams played which colour: the assignment under
/// which more players are on the roster of the team given their colour.
/// `rosters` are the teams' steam ids (64-bit, as strings) in match order;
/// the answer is, per colour (Red, Blue), an index into `rosters`.
pub fn side_teams(players: &[StatPlayer], rosters: &[HashSet<String>; 2]) -> [Option<usize>; 2] {
    let on = |side: &str, team: usize| players.iter().filter(|p| p.side == side && rosters[team].contains(&p.steam_id)).count();
    let straight = on("Red", 0) + on("Blue", 1);
    let crossed = on("Red", 1) + on("Blue", 0);
    match straight.cmp(&crossed) {
        std::cmp::Ordering::Greater => [Some(0), Some(1)],
        std::cmp::Ordering::Less => [Some(1), Some(0)],
        std::cmp::Ordering::Equal => [None, None],
    }
}

/// Names the teams on a map's sides and players: a player takes the team of
/// their colour (a stand-in is on a roster of neither), and without a team
/// for the colour, the team whose roster they are on.
pub fn attach_teams(map: &mut RglMapStats, team_ids: [i64; 2], rosters: &[HashSet<String>; 2]) {
    let sides = side_teams(&map.players, rosters);
    for (s, t) in map.sides.iter_mut().zip(sides) {
        s.team_id = t.map(|i| team_ids[i]);
    }
    let of_side: Vec<Option<i64>> = map.players.iter().map(|p| map.sides.iter().find(|s| s.side == p.side).and_then(|s| s.team_id)).collect();
    for (p, side_team) in map.players.iter_mut().zip(of_side) {
        let own = (0..2).find(|&i| rosters[i].contains(&p.steam_id)).map(|i| team_ids[i]);
        p.team_id = side_team.or(own);
    }
}

// ---- Reading logs ---------------------------------------------------------------

fn log_key(id: i64) -> String {
    format!("/internal/log-json/{id}")
}

/// A log's JSON and its raw server log where the app has one: stored in the
/// database, or kept from an earlier look, or drops.tf's (logs.tf only for a
/// log drops.tf lacks: `Sources::log_json`), and then kept.
async fn log_files(db: &Db, sources: &Sources, id: i64) -> Result<(String, Option<Vec<u8>>)> {
    if let (Some(json), zip) = db.league_log_files(id).await? {
        return Ok((json, zip));
    }
    if let Some(json) = db.raw_log(id).await? {
        return Ok((json, db.rawlog(id).await?));
    }
    let key = log_key(id);
    if let Some((body, _)) = db.rgl_answers(std::slice::from_ref(&key)).await?.remove(&key) {
        return Ok((body, None));
    }
    let (json, _from) = sources.log_json(id).await?;
    db.store_rgl_answer(&key, &json).await?;
    Ok((json, None))
}

/// The app's rating of each player in a log, against the stored baselines;
/// none before a first rating pass has made them. A log with no raw server
/// log is rated without round impact, on that kind of game's own scale.
async fn rate_log(db: &Db, w: &Weights, id: i64, json: &str, zip: Option<Vec<u8>>) -> HashMap<u32, f64> {
    let (Ok(baseline), Ok(Some(scale))) = (crate::rating::load_baseline(db).await, crate::rating::load_scale(db).await) else { return HashMap::new() };
    if baseline.is_empty() {
        return HashMap::new();
    }
    let (w2, json) = (w.clone(), json.to_string());
    let perfs = tokio::task::spawn_blocking(move || crate::league_rating::rate_one(id, &json, zip.as_deref(), &w2)).await;
    let Ok(Ok(perfs)) = perfs else { return HashMap::new() };
    // A player's longest game on a class is the one shown.
    let mut by: HashMap<u32, (f64, f64)> = HashMap::new();
    for p in &perfs {
        let Some(r) = hl_rating::model::rate(p, &baseline, w) else { continue };
        let r = r.scaled_for(&scale);
        if by.get(&p.account_id).is_none_or(|(m, _)| r.minutes > *m) {
            by.insert(p.account_id, (r.minutes, r.score));
        }
    }
    by.into_iter().map(|(a, (_, s))| (a, s)).collect()
}

/// A match's statistics, from the logs `match_more` found for it (read from
/// disk: this does not search logs.tf). `None` when RGL has no such match.
/// Each log not yet kept is one request to drops.tf, once.
pub async fn match_stats(db: &Db, sources: &Sources, w: &Weights, id: i64) -> Result<Option<RglMatchStats>> {
    let Some(m) = one_match(db, sources, id).await? else { return Ok(None) };
    let key = crate::rgl::logs_key(id);
    let logs: Vec<crate::rgl::RglMatchLog> = match db.rgl_answers(std::slice::from_ref(&key)).await?.remove(&key) {
        Some((body, _)) => serde_json::from_str(&body).unwrap_or_default(),
        None => Vec::new(),
    };
    if logs.is_empty() {
        return Ok(Some(RglMatchStats { maps: Vec::new(), unread: Vec::new() }));
    }
    // The two rosters, to say which team played which colour; kept answers
    // (the page just read them for its lineups), and a failure costs the
    // team names only.
    let at = rfc3339(m.match_date.as_deref());
    let mut rosters: [HashSet<String>; 2] = Default::default();
    let mut ids = [0i64; 2];
    let known = m.teams.len() == 2;
    if known {
        for (i, t) in m.teams.iter().enumerate() {
            ids[i] = t.team_id;
            if let Ok(Some(raw)) = team_raw(db, sources, t.team_id).await {
                rosters[i] = roster_at(&raw.players, at).into_iter().map(|p| p.steam_id).collect();
            }
        }
    }
    let mut maps = Vec::new();
    let mut unread = Vec::new();
    for l in logs.iter().take(MAX_LOGS) {
        let (json, zip) = match log_files(db, sources, l.log_id).await {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!(log_id = l.log_id, error = %format!("{e:#}"), "a log for an RGL match's statistics");
                unread.push(l.log_id);
                continue;
            }
        };
        let log = match serde_json::from_str(&json).map_err(anyhow::Error::from).and_then(|v: serde_json::Value| normalize(l.log_id, &v)) {
            Ok(log) => log,
            Err(e) => {
                tracing::warn!(log_id = l.log_id, error = %format!("{e:#}"), "a log for an RGL match's statistics did not read");
                unread.push(l.log_id);
                continue;
            }
        };
        let ratings = rate_log(db, w, l.log_id, &json, zip).await;
        let mut stats = map_stats(&log, &ratings, &class_median_dpm);
        if known {
            attach_teams(&mut stats, ids, &rosters);
        }
        maps.push(stats);
    }
    Ok(Some(RglMatchStats { maps, unread }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> NormalizedLog {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        normalize(1, &v).unwrap()
    }

    fn player(name: &str, side: &str, class: &str, kills: i64, deaths: i64, dpm: f64) -> StatPlayer {
        StatPlayer {
            steam_id: format!("7656{name}"),
            name: name.into(),
            side: side.into(),
            team_id: None,
            class: Some(class.into()),
            minutes: 20.0,
            kills,
            assists: 0,
            deaths,
            damage: (dpm * 20.0) as i64,
            dpm: Some(dpm),
            dtpm: None,
            dpm_vs: None,
            dpm_vs_basis: None,
            headshots: 0,
            heals: 0,
            ubers: 0,
            drops: 0,
            rating: None,
        }
    }

    #[test]
    fn the_median_is_read_between_the_cuts() {
        let pcts = [0.0, 10.0, 25.0, 40.0, 55.0, 70.0];
        let cuts = [0.0, 100.0, 200.0, 300.0, 450.0, 500.0];
        // Ten of the fifteen points from the 40th to the 55th.
        assert!((median_of(&cuts, &pcts).unwrap() - 400.0).abs() < 1e-9);
        assert_eq!(median_of(&cuts[..3], &pcts), None);
    }

    #[test]
    fn the_shipped_archive_has_a_median_for_a_class() {
        let soldier = class_median_dpm("soldier").expect("the archive ships Soldier");
        assert!((100.0..500.0).contains(&soldier), "{soldier}");
        assert_eq!(class_median_dpm("nothing"), None);
    }

    #[test]
    fn a_real_log_adds_up_per_side() {
        let log = fixture("hl_sub_19p_4114301.json");
        let s = map_stats(&log, &HashMap::new(), &class_median_dpm);
        assert_eq!(s.players.len(), 19);
        let red_kills: i64 = log.players.iter().filter(|p| p.team == Team::Red).map(|p| p.stats.kills).sum();
        assert_eq!(s.sides[0].kills, red_kills);
        assert_eq!((s.sides[0].score, s.sides[1].score), (log.red_score, log.blue_score));
        assert_eq!(s.sides[0].damage + s.sides[1].damage, log.players.iter().map(|p| p.stats.dmg).sum::<i64>());
        assert!(!s.rated);
        // Without ratings the MVP is by the plain rule, and is not a medic.
        let mvp = s.awards.mvp.as_ref().unwrap();
        assert_eq!(mvp.basis, "kapd");
        assert_ne!(mvp.class.as_deref(), Some("medic"));
        // Scout to Spy.
        let orders: Vec<usize> = s.players.iter().map(|p| class_order(p.class.as_deref())).collect();
        assert!(orders.windows(2).all(|w| w[0] <= w[1]));
        // The top fragger really has the most kills.
        let top = log.players.iter().map(|p| p.stats.kills).max().unwrap();
        assert_eq!(s.awards.top_fragger.as_ref().unwrap().value as i64, top);
    }

    #[test]
    fn dpm_is_damage_over_the_minutes_played_and_compared_with_the_class() {
        let log = fixture("hl_2023_3445078.json");
        let s = map_stats(&log, &HashMap::new(), &|_| Some(300.0));
        let p = s.players.iter().find(|p| p.dpm.is_some()).unwrap();
        let want = p.damage as f64 / p.minutes;
        assert!((p.dpm.unwrap() - want).abs() < 1e-9);
        assert!((p.dpm_vs.unwrap() - (want - 300.0)).abs() < 1e-9);
        assert_eq!(p.dpm_vs_basis, Some("archive"));
    }

    #[test]
    fn without_a_baseline_the_other_teams_player_of_the_class_is_the_yardstick() {
        let log = fixture("hl_2023_3445078.json");
        let s = map_stats(&log, &HashMap::new(), &|_| None);
        let with: Vec<&StatPlayer> = s.players.iter().filter(|p| p.dpm_vs.is_some()).collect();
        assert!(!with.is_empty());
        assert!(with.iter().all(|p| p.dpm_vs_basis == Some("opponent")));
        // The two Soldiers (or any pair) differ by equal and opposite amounts.
        let a = with[0];
        let b = s.players.iter().find(|o| o.side != a.side && o.class == a.class).unwrap();
        assert!((a.dpm_vs.unwrap() + b.dpm_vs.unwrap()).abs() < 1e-9);
    }

    #[test]
    fn the_rating_picks_the_mvp_and_the_medic_is_judged_on_heals() {
        let mut ps = vec![
            player("a", "Red", "soldier", 20, 10, 300.0),
            player("b", "Blue", "scout", 25, 5, 250.0),
            player("m1", "Red", "medic", 0, 3, 40.0),
            player("m2", "Blue", "medic", 0, 4, 30.0),
        ];
        ps[0].rating = Some(1.4);
        ps[1].rating = Some(1.2);
        ps[2].heals = 9000;
        ps[3].heals = 8000;
        ps[3].ubers = 4; // four ubers outweigh 1000 more heals
        ps[1].headshots = 3;
        let a = awards(&ps);
        assert_eq!(a.mvp.as_ref().map(|x| (x.name.as_str(), x.basis)), Some(("a", "rating")));
        assert_eq!(a.medic_mvp.as_ref().map(|x| x.name.as_str()), Some("m2"));
        assert_eq!(a.medic_mvp.unwrap().extra, Some(4));
        assert_eq!(a.top_fragger.as_ref().map(|x| x.name.as_str()), Some("b"));
        assert_eq!(a.most_headshots.map(|x| x.name), Some("b".into()));
        // Nobody under a minute is an award winner.
        let mut short = vec![player("x", "Red", "scout", 9, 0, 100.0)];
        short[0].minutes = 0.5;
        assert!(awards(&short).top_fragger.is_none());
    }

    #[test]
    fn a_tie_for_most_kills_goes_to_fewer_deaths() {
        let ps = vec![player("a", "Red", "soldier", 20, 10, 1.0), player("b", "Blue", "scout", 20, 5, 1.0)];
        assert_eq!(awards(&ps).top_fragger.unwrap().name, "b");
    }

    #[test]
    fn the_teams_are_told_apart_by_their_rosters() {
        let ps = vec![
            player("a", "Red", "soldier", 1, 1, 1.0),
            player("b", "Red", "scout", 1, 1, 1.0),
            player("c", "Blue", "pyro", 1, 1, 1.0),
            player("stand-in", "Blue", "spy", 1, 1, 1.0),
        ];
        let ids = |names: &[&str]| names.iter().map(|n| format!("7656{n}")).collect::<HashSet<_>>();
        // Red is the match's second team.
        let rosters = [ids(&["c", "x"]), ids(&["a", "b"])];
        assert_eq!(side_teams(&ps, &rosters), [Some(1), Some(0)]);
        let mut log_stats = map_stats(&fixture("hl_sub_19p_4114301.json"), &HashMap::new(), &|_| None);
        log_stats.players = ps;
        attach_teams(&mut log_stats, [10, 20], &rosters);
        assert_eq!((log_stats.sides[0].team_id, log_stats.sides[1].team_id), (Some(20), Some(10)));
        // The stand-in is on neither roster but plays for Blue, so for team 10.
        assert_eq!(log_stats.players[3].team_id, Some(10));
        // Rosters that say nothing say nothing.
        assert_eq!(side_teams(&log_stats.players, &Default::default()), [None, None]);
    }

    /// A log read from disk is not asked for again: no network in this test.
    #[tokio::test]
    async fn a_kept_log_is_read_from_disk() {
        let db = Db::connect_in_memory().await.unwrap();
        db.store_rgl_answer(&log_key(5), "{\"players\":{}}").await.unwrap();
        let sources = Sources::new().unwrap();
        let (json, zip) = log_files(&db, &sources, 5).await.unwrap();
        assert_eq!(json, "{\"players\":{}}");
        assert!(zip.is_none());
        // A stored match comes first.
        db.store_raw_log(6, "{\"players\":{},\"stored\":true}").await.unwrap();
        assert!(log_files(&db, &sources, 6).await.unwrap().0.contains("stored"));
    }
}
