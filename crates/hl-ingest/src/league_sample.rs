//! The league sample (Flashy, September 2026): ETF2L Highlander officials
//! from every division, so ratings can be read against the league instead
//! of only the owner's matches, and a player can be told which division the
//! people they play against come from.
//!
//! **Finding them is cheap.** ETF2L's results give every official's division
//! and tier ([`crate::leagues::fetch_seasons`]); trends.tf lists every ETF2L
//! Highlander log with the ETF2L match it belongs to. Joined, every log has
//! a division without asking logs.tf anything.
//!
//! **Choosing them** ([`select`]): per tier, [`MATCHES_PER_TIER`] matches,
//! from the last [`PREFER_YEARS`] years unless that cannot fill it, taken
//! round-robin across maps so every map in the pool is in there.
//!
//! **Downloading them is slow on purpose** ([`step`]): one unit of work a
//! call, and the caller waits between calls. logs.tf's JSON first, then its
//! raw server logs; while logs.tf is refusing us, more.tf's copy stands in
//! for the JSON. ETF2L's match pages (who played) come alongside, from a
//! different host. Everything is state in the database, so it picks up
//! wherever it was after a restart.
//!
//! Nothing here touches the owner's matches: the sample lives in its own
//! tables (migration 0033) until it is decided how it should count.

use crate::sources::Sources;
use anyhow::Result;
use hl_db::{Db, LeagueLogRow};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Matches wanted from each division. It began at 300; Flashy wants every
/// official, and no division has played this many in six years, so it is
/// "all of them" with a guard.
pub const MATCHES_PER_TIER: usize = 5_000;
/// Taken from the last three years first...
pub const PREFER_YEARS: i64 = 3;
/// ...and from as far back as six when that does not fill a division. The
/// player catalogue (every official's roster) covers the same six years.
pub const MAX_YEARS: i64 = 6;
/// ETF2L seasons read for their results: two a year, six years.
pub const SEASONS_BACK: i64 = 12;
/// Shorter logs are restarts and forfeits, not played maps.
const MIN_DURATION_S: i64 = 300;
/// How often the lists are read again for new officials.
const REDISCOVER_S: i64 = 24 * 3600;
/// A log neither source could give after this many tries is left alone.
const MAX_JSON_ATTEMPTS: i64 = 3;

const YEAR_S: i64 = 365 * 24 * 3600;
const KEY_ON: &str = "league_sample_on";
const KEY_DISCOVERED: &str = "league_sample_discovered_at";
/// The window the last discovery used: a wider one lists again from scratch.
const KEY_WINDOW: &str = "league_sample_window";

/// Changes when discovery must run again at once: a wider window, or a
/// name parser that reads competitions it skipped before (2: the unnumbered
/// seasons 28-31 and the preseason cups; 3: AFA 2025, between 34 and 35;
/// 4: the other Highlander tournaments; 5: every Grand Final, finals played
/// as a round of the season and divisions named only in their playoffs).
fn window() -> String {
    format!("{SEASONS_BACK}/{MAX_YEARS}/5")
}

pub async fn enabled(db: &Db) -> Result<bool> {
    Ok(db.get_setting(KEY_ON).await?.as_deref() == Some("1"))
}

pub async fn set_enabled(db: &Db, on: bool) -> Result<()> {
    db.set_setting(KEY_ON, if on { "1" } else { "0" }).await
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A real map name, not a combined log's title ("Vigil") or a mod map.
fn played_map(map: &str) -> bool {
    let m = map.to_ascii_lowercase();
    ["koth_", "pl_", "cp_", "ctf_", "plr_"].iter().any(|p| m.starts_with(p))
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovered {
    pub competitions: usize,
    pub results: usize,
    pub logs_listed: usize,
    pub picked_matches: usize,
    pub picked_logs: usize,
}

/// Read ETF2L's results and trends.tf's list of ETF2L logs, then choose.
pub async fn discover(db: &Db, sources: &Sources, mut progress: impl FnMut(&str)) -> Result<Discovered> {
    let mut out = Discovered::default();
    progress("Reading ETF2L's results");
    let etf2l = crate::leagues::fetch_seasons(db, sources, SEASONS_BACK, 0, |_, _| {}).await?;
    out.competitions = etf2l.competitions;
    out.results = etf2l.results;

    // trends.tf, newest first, down to the window's start -- or, once the
    // back catalogue is held that far, to a week before the newest log held.
    let (known, oldest_known, newest_known) = db.league_logs_known().await?;
    let floor = now() - MAX_YEARS * YEAR_S;
    let back_catalogue_held = known > 0 && oldest_known.is_some_and(|o| o <= floor + 60 * 24 * 3600);
    let stop_at = if back_catalogue_held { newest_known.map_or(floor, |n| (n - 7 * 24 * 3600).max(floor)) } else { floor };
    let mut path: Option<String> = None;
    for page in 0..2000 {
        progress(&format!("Listing ETF2L logs on trends.tf ({} so far)", out.logs_listed));
        let (rows, next) = sources.trends_league_page(path.as_deref()).await?;
        if rows.is_empty() {
            break;
        }
        let keep: Vec<LeagueLogRow> = rows
            .iter()
            .filter(|r| r.duplicate_of.as_ref().is_none_or(|d| d.is_empty()))
            .filter_map(|r| {
                Some(LeagueLogRow {
                    log_id: r.logid,
                    etf2l_match_id: r.matchid?,
                    map: r.map.as_deref()?,
                    played_at: r.time?,
                    duration_s: r.duration,
                    title: r.title.as_deref(),
                })
            })
            .collect();
        out.logs_listed += keep.len();
        db.upsert_league_logs(&keep).await?;
        let oldest = rows.iter().filter_map(|r| r.time).min().unwrap_or(0);
        if oldest < stop_at || page == 1999 {
            break;
        }
        match next {
            Some(n) => path = Some(n),
            None => break,
        }
    }

    progress("Choosing the matches");
    let (matches, logs) = select(db).await?;
    out.picked_matches = matches;
    out.picked_logs = logs;
    db.set_setting(KEY_DISCOVERED, &now().to_string()).await?;
    db.set_setting(KEY_WINDOW, &window()).await?;
    Ok(out)
}

/// Choose the sample: per tier, [`MATCHES_PER_TIER`] matches, newest first,
/// the last [`PREFER_YEARS`] years before anything older, round-robin across
/// maps. Returns `(matches, logs)` picked.
pub async fn select(db: &Db) -> Result<(usize, usize)> {
    let t = now();
    let candidates = db.league_candidates().await?;
    // tier -> match -> (played_at, logs, maps)
    let mut by_tier: BTreeMap<i64, HashMap<i64, (i64, Vec<i64>, HashSet<String>)>> = BTreeMap::new();
    // Grand Finals always, whatever the caps below: they are what the gold
    // medals and the MVPs are read from (Flashy: "check all the grand
    // finals and download any that are missing").
    let mut finals: Vec<i64> = Vec::new();
    for c in candidates.iter().filter(|c| played_map(&c.map) && (c.duration_s < 0 || c.duration_s >= MIN_DURATION_S) && c.played_at >= t - MAX_YEARS * YEAR_S) {
        // Today's ladder by the division's name first: "Division 2" is
        // High, "Freshest" Fresh Meat, though no regular division of that
        // season shares the name to borrow a tier from.
        let Some(tier) = crate::leagues::canonical_tier(&c.division).or(c.tier) else { continue };
        if c.is_final {
            finals.push(c.log_id);
        }
        let e = by_tier.entry(tier).or_default().entry(c.match_id).or_insert_with(|| (c.played_at, Vec::new(), HashSet::new()));
        e.0 = e.0.max(c.played_at);
        e.1.push(c.log_id);
        e.2.insert(hl_core::maps::map_base(&c.map));
    }

    let mut picked_logs: Vec<i64> = Vec::new();
    let mut picked_matches = 0;
    for matches in by_tier.values() {
        let recent_from = t - PREFER_YEARS * YEAR_S;
        let mut chosen: HashSet<i64> = HashSet::new();
        // The recent years first; older only if a division cannot fill.
        for window in [recent_from, t - MAX_YEARS * YEAR_S] {
            // map -> its matches in this window, newest first
            let mut per_map: BTreeMap<&str, Vec<(i64, i64)>> = BTreeMap::new();
            for (id, (at, _, maps)) in matches.iter().filter(|(_, (at, ..))| *at >= window) {
                for m in maps {
                    per_map.entry(m.as_str()).or_default().push((*at, *id));
                }
            }
            for list in per_map.values_mut() {
                list.sort_by(|a, b| b.cmp(a));
            }
            let mut cursors: Vec<(&str, usize)> = per_map.keys().map(|m| (*m, 0)).collect();
            // Round-robin: each map in turn gives its newest match not yet
            // taken, so a map played less often is still in the sample.
            while chosen.len() < MATCHES_PER_TIER {
                let mut took = false;
                for (map, at) in cursors.iter_mut() {
                    let list = &per_map[*map];
                    while *at < list.len() && chosen.contains(&list[*at].1) {
                        *at += 1;
                    }
                    if *at < list.len() {
                        chosen.insert(list[*at].1);
                        took = true;
                        if chosen.len() == MATCHES_PER_TIER {
                            break;
                        }
                    }
                }
                if !took {
                    break;
                }
            }
            if chosen.len() >= MATCHES_PER_TIER {
                break;
            }
        }
        picked_matches += chosen.len();
        for id in &chosen {
            picked_logs.extend(&matches[id].1);
        }
    }
    for id in finals {
        if !picked_logs.contains(&id) {
            picked_logs.push(id);
        }
    }
    // Logs from before the window are only here because an old season was
    // imported (old_seasons.rs): picking again must not drop them.
    let floor = t - MAX_YEARS * YEAR_S;
    let mut held: HashSet<i64> = picked_logs.iter().copied().collect();
    for c in candidates.iter().filter(|c| c.played_at < floor && played_map(&c.map)) {
        if held.insert(c.log_id) {
            picked_logs.push(c.log_id);
        }
    }
    db.set_league_picked(&picked_logs).await?;
    Ok((picked_matches, picked_logs.len()))
}

/// What one [`step`] did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Step {
    Discovered(Discovered),
    Json { log_id: i64, source: &'static str },
    Raw { log_id: i64, found: bool, source: &'static str },
    Roster { match_id: i64 },
    /// A played official trends.tf linked no log to, searched for on logs.tf.
    Searched { match_id: i64, found: usize },
    /// logs.tf is refusing us and there is nothing more.tf can do meanwhile.
    Waiting,
    /// Everything picked is here; nothing to do until the next discovery.
    Done,
    /// A request failed in a way worth backing off from.
    Failed { what: String },
}

/// How long after an official its logs are looked for: a day, for uploads.
const SEARCH_AFTER_S: i64 = 24 * 3600;
/// How far from ETF2L's time a log may be and still be the match's.
const SEARCH_WINDOW_S: i64 = 36 * 3600;

/// The logs of an official that trends.tf never linked: logs.tf's logs with
/// a rostered player of each team in them, Highlander-sized, on a real map,
/// within a day and a half of the match. Stored and picked like trends.tf's;
/// the search is remembered either way, so it is made once.
pub async fn search_match_logs(db: &Db, sources: &Sources, match_id: i64, time: i64, a: u32, b: u32) -> Result<usize> {
    let ids = [hl_core::SteamId::from_account_id(a).to_steamid64(), hl_core::SteamId::from_account_id(b).to_steamid64()];
    let logs = sources.logstf_with_players(&ids, 40).await?;
    let found = matching_logs(&logs, time);
    let rows: Vec<LeagueLogRow> = found
        .iter()
        .map(|(id, title, map, date, _)| LeagueLogRow { log_id: *id, etf2l_match_id: match_id, map: map.as_str(), played_at: *date, duration_s: None, title: Some(title.as_str()) })
        .collect();
    db.upsert_league_logs(&rows).await?;
    db.pick_league_logs(&found.iter().map(|l| l.0).collect::<Vec<_>>()).await?;
    let note = serde_json::json!({ "found": found.iter().map(|l| l.0).collect::<Vec<_>>(), "players": ids }).to_string();
    db.store_etf2l_raw("log_search", match_id, &note).await?;
    Ok(found.len())
}

/// Which of logs.tf's results are the match's: Highlander-sized (16 to 22
/// players), on one real map (a combined log is left to its parts), within
/// [`SEARCH_WINDOW_S`] of the match.
///
/// A player's own re-upload of both halves sits beside the server's two
/// ("pollos vs mnht w4" beside serveme's RED vs BLU and BLU vs RED), and
/// would count the map twice. So per map at most one log a half -- two on
/// a stopwatch map, one on king of the hill -- the server's own uploads
/// first.
fn matching_logs(logs: &[(i64, String, String, i64, i64)], time: i64) -> Vec<(i64, String, String, i64, i64)> {
    let near: Vec<&(i64, String, String, i64, i64)> =
        logs.iter().filter(|(_, _, map, date, players)| (16..=22).contains(players) && played_map(map) && (date - time).abs() <= SEARCH_WINDOW_S).collect();
    let auto = |title: &str| {
        let t = title.to_ascii_lowercase();
        ["serveme.tf", "tf2center", "quickserver", "red vs blu", "blu vs red"].iter().any(|x| t.contains(x))
    };
    let mut by_map: BTreeMap<String, Vec<&(i64, String, String, i64, i64)>> = BTreeMap::new();
    for l in near {
        by_map.entry(hl_core::maps::map_name(&l.2)).or_default().push(l);
    }
    let mut out = Vec::new();
    for (map, mut group) in by_map {
        group.sort_by_key(|l| (!auto(&l.1), std::cmp::Reverse(l.3)));
        let halves = if map.starts_with("koth_") { 1 } else { 2 };
        out.extend(group.into_iter().take(halves).cloned());
    }
    out.sort_by_key(|l| std::cmp::Reverse(l.3));
    out
}

/// One log downloaded, its JSON first and then its raw log, from drops.tf
/// and, unless logs.tf is `resting`, from logs.tf for what drops.tf has
/// not got. `None` when there is nothing it can do now.
async fn log_step(db: &Db, sources: &Sources, resting: bool, doing: &mut (impl FnMut(&str) + Send)) -> Result<Option<Step>> {
    // A more.tf stand-in is asked for again: drops.tf or logs.tf has the real one.
    if let Some(log_id) = db.league_json_todo(1, true, MAX_JSON_ATTEMPTS).await?.first().copied() {
        doing(&format!("Downloading log {log_id} from drops.tf"));
        match sources.drops_log(log_id).await {
            Ok(Some(body)) => {
                db.put_league_json(log_id, &body, crate::sources::DROPS).await?;
                return Ok(Some(Step::Json { log_id, source: crate::sources::DROPS }));
            }
            Ok(None) => {}
            Err(e) => tracing::info!(log_id, error = %format!("{e:#}"), "drops.tf would not give the log"),
        }
        if !resting {
            doing(&format!("Downloading log {log_id} from logs.tf"));
            return Ok(Some(match sources.logstf_log(log_id).await {
                Ok(body) => {
                    db.put_league_json(log_id, &body, "logs.tf").await?;
                    Step::Json { log_id, source: "logs.tf" }
                }
                Err(e) if crate::http::unreachable(&e) => Step::Failed { what: format!("logs.tf: {e:#}") },
                Err(e) => {
                    db.league_json_failed(log_id).await?;
                    Step::Failed { what: format!("log {log_id}: {e:#}") }
                }
            }));
        }
    }
    if let Some(log_id) = db.league_raw_todo(1).await?.first().copied() {
        doing(&format!("Downloading the server log of {log_id} from drops.tf"));
        match sources.drops_rawlog(log_id).await {
            Ok(Some(zip)) => {
                db.put_league_raw(log_id, Some(&zip)).await?;
                return Ok(Some(Step::Raw { log_id, found: true, source: crate::sources::DROPS }));
            }
            Ok(None) => {}
            Err(e) => tracing::info!(log_id, error = %format!("{e:#}"), "drops.tf would not give the raw log"),
        }
        if !resting {
            doing(&format!("Downloading the server log of {log_id} from logs.tf"));
            return Ok(Some(match sources.logstf_rawlog(log_id).await {
                Ok(zip) => {
                    let found = zip.is_some();
                    db.put_league_raw(log_id, zip.as_deref()).await?;
                    Step::Raw { log_id, found, source: "logs.tf" }
                }
                Err(e) => Step::Failed { what: format!("logs.tf raw log {log_id}: {e:#}") },
            }));
        }
    }
    Ok(None)
}

/// Do one unit of work. The caller waits between calls: this is the pace.
/// `doing` is told what each request is before it is made, so a window can
/// say what the job is doing rather than only what it has done.
pub async fn step(db: &Db, sources: &Sources, mut doing: impl FnMut(&str) + Send) -> Result<Step> {
    let due = db
        .get_setting(KEY_DISCOVERED)
        .await?
        .and_then(|s| s.parse::<i64>().ok())
        .is_none_or(|at| now() - at > REDISCOVER_S)
        || db.get_setting(KEY_WINDOW).await?.as_deref() != Some(window().as_str());
    if due {
        return Ok(Step::Discovered(discover(db, sources, &mut doing).await?));
    }

    // ETF2L is another host: one match page each step, beside the logs.
    let roster = db.league_matches_without_roster(1, now() - MAX_YEARS * YEAR_S).await?.first().copied();
    if let Some(match_id) = roster {
        // A page ETF2L will not give is marked read by nothing: tried again
        // next discovery. Its failure does not stop the log work.
        doing(&format!("Reading who played ETF2L match {match_id}"));
        let _ = crate::leagues::fetch_match_detail(db, sources, match_id).await;
    }

    if !sources.logstf_resting() {
        // An official trends.tf linked nothing to: look on logs.tf itself
        // (Flashy: S36 Premiership's Snipers went unranked for want of them).
        let since = now() - MAX_YEARS * YEAR_S;
        if let Some(&(match_id, time, a, b)) = db.league_matches_without_logs(since, now() - SEARCH_AFTER_S, 1).await?.first() {
            doing(&format!("Looking on logs.tf for the logs of ETF2L match {match_id}"));
            return Ok(match search_match_logs(db, sources, match_id, time, a as u32, b as u32).await {
                Ok(found) => Step::Searched { match_id, found },
                Err(e) => Step::Failed { what: format!("logs.tf search for match {match_id}: {e:#}") },
            });
        }
        // The log's JSON, then its raw log: drops.tf first for both (its copy
        // of logs.tf's, by Icewind), logs.tf for what drops.tf has not got.
        if let Some(step) = log_step(db, sources, false, &mut doing).await? {
            return Ok(step);
        }
    } else if let Some(step) = log_step(db, sources, true, &mut doing).await? {
        // Resting from logs.tf: drops.tf still answers.
        return Ok(step);
    } else if let Some(log_id) = db.league_json_todo(1, false, MAX_JSON_ATTEMPTS).await?.first().copied() {
        // Resting from logs.tf, and drops.tf has not got it: more.tf's copy meanwhile.
        doing(&format!("logs.tf is resting: downloading log {log_id} from more.tf"));
        return Ok(match sources.moretf_log(log_id).await {
            Ok(Some(body)) => {
                let v: serde_json::Value = serde_json::from_str(&body)?;
                match crate::moretf::stand_in(&v) {
                    Ok(s) => {
                        db.put_league_json(log_id, &serde_json::to_string(&s.json)?, "more.tf").await?;
                        Step::Json { log_id, source: "more.tf" }
                    }
                    Err(e) => {
                        db.league_json_failed(log_id).await?;
                        Step::Failed { what: format!("more.tf log {log_id}: {e:#}") }
                    }
                }
            }
            Ok(None) => {
                db.league_json_failed(log_id).await?;
                Step::Failed { what: format!("more.tf has no log {log_id}") }
            }
            Err(e) => Step::Failed { what: format!("more.tf: {e:#}") },
        });
    } else {
        return Ok(if roster.is_some() { Step::Roster { match_id: roster.unwrap_or(0) } } else { Step::Waiting });
    }
    Ok(match roster {
        Some(match_id) => Step::Roster { match_id },
        None => Step::Done,
    })
}

/// Where the sample stands, for Settings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    /// The player catalogue: officials in the window, rosters read, and
    /// players named in them.
    pub officials: i64,
    pub rosters_read: i64,
    pub players: i64,
    pub discovered_at: Option<i64>,
    pub logs_listed: i64,
    pub tiers: Vec<hl_db::TierProgress>,
    pub bytes: i64,
    pub logstf_resting: bool,
    pub target_per_tier: usize,
}

/// Progress rows are counted by ETF2L's tier and name; this puts them on
/// today's ladder ("Division 5A" with Low). Maps are summed, so a division
/// merged from two eras may count a map twice.
fn progress_by_ladder(rows: Vec<hl_db::TierProgress>) -> Vec<hl_db::TierProgress> {
    let mut out: BTreeMap<i64, hl_db::TierProgress> = BTreeMap::new();
    for r in rows {
        let tier = crate::leagues::canonical_tier(&r.division).unwrap_or(r.tier);
        match out.get_mut(&tier) {
            None => {
                let name = crate::leagues::TIER_NAMES.get(tier as usize).map_or(r.division.clone(), |n| n.to_string());
                out.insert(tier, hl_db::TierProgress { tier, division: name, ..r });
            }
            Some(e) => {
                e.matches += r.matches;
                e.logs += r.logs;
                e.json_logstf += r.json_logstf;
                e.json_dropstf += r.json_dropstf;
                e.json_moretf += r.json_moretf;
                e.raw += r.raw;
                e.raw_missing += r.raw_missing;
                e.maps = e.maps.max(r.maps);
                e.rosters += r.rosters;
                e.oldest = e.oldest.min(r.oldest).or(e.oldest.or(r.oldest));
                e.newest = e.newest.max(r.newest);
            }
        }
    }
    out.into_values().collect()
}

/// The sample's size on disk. Summing thousands of stored logs takes over a
/// second and Settings asks every ten, so the answer is kept five minutes.
async fn bytes_held(db: &Db) -> Result<i64> {
    static HELD: std::sync::Mutex<Option<(std::time::Instant, i64)>> = std::sync::Mutex::new(None);
    if let Some((at, bytes)) = *HELD.lock().unwrap() {
        if at.elapsed() < std::time::Duration::from_secs(300) {
            return Ok(bytes);
        }
    }
    let bytes = db.league_bytes().await?;
    *HELD.lock().unwrap() = Some((std::time::Instant::now(), bytes));
    Ok(bytes)
}

pub async fn status(db: &Db, sources: &Sources) -> Result<Status> {
    let (officials, rosters_read, players) = db.league_catalogue(now() - MAX_YEARS * YEAR_S).await?;
    Ok(Status {
        officials,
        rosters_read,
        players,
        enabled: enabled(db).await?,
        discovered_at: db.get_setting(KEY_DISCOVERED).await?.and_then(|s| s.parse().ok()),
        logs_listed: db.league_logs_known().await?.0,
        tiers: progress_by_ladder(db.league_progress().await?),
        bytes: bytes_held(db).await?,
        logstf_resting: sources.logstf_resting(),
        target_per_tier: MATCHES_PER_TIER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_officials_logs_are_the_per_map_ones_near_it() {
        // ETF2L match 92955, ChudsX6 vs TWICE, 20 Sept 2026: logs.tf's search
        // for a player of each team (the combined log, a 6v6 log, and one a
        // week later are not the match's).
        let t = 1789931700;
        let logs = vec![
            (4123879, "prem w4: chuds vs twice".to_string(), "swiftwater".to_string(), t + 5400, 18),
            (4123875, "serveme.tf #1564157 BLU vs RED".to_string(), "pl_swiftwater_final1".to_string(), t + 5040, 18),
            (4123860, "serveme.tf #1564157 RED vs BLU".to_string(), "pl_swiftwater_final1".to_string(), t + 3600, 18),
            (4123001, "6s".to_string(), "cp_process_final".to_string(), t + 600, 12),
            (4130000, "scrim".to_string(), "pl_upward_f12".to_string(), t + 7 * 86400, 18),
        ];
        let ids: Vec<i64> = matching_logs(&logs, t).iter().map(|l| l.0).collect();
        assert_eq!(ids, vec![4123875, 4123860]);
    }

    #[test]
    fn a_players_own_upload_of_both_halves_is_not_counted_again() {
        // ETF2L match 92918, My New Highlander Team vs pollos: the server's
        // two halves and a player's upload of the whole map; and one koth map
        // uploaded twice.
        let t = 1789672560;
        let logs = vec![
            (4122251, "pollos vs mnht w4".to_string(), "pl_swiftwater_final1".to_string(), t + 3600, 18),
            (4122246, "serveme.tf #1563622 RED vs BLU".to_string(), "pl_swiftwater_final1".to_string(), t + 3590, 18),
            (4122226, "serveme.tf #1563622 BLU vs RED".to_string(), "pl_swiftwater_final1".to_string(), t + 1800, 18),
            (4122204, "serveme.tf #1563679 BLU vs RED".to_string(), "koth_proot_b5b".to_string(), t + 400, 18),
            (4122203, "proot".to_string(), "koth_proot_b5b".to_string(), t + 410, 18),
        ];
        let ids: Vec<i64> = matching_logs(&logs, t).iter().map(|l| l.0).collect();
        assert_eq!(ids, vec![4122246, 4122226, 4122204]);
    }

    #[test]
    fn only_real_maps_count() {
        assert!(played_map("pl_vigil_rc10") && played_map("koth_product_final") && played_map("cp_steel_f12"));
        assert!(!played_map("Vigil"), "a combined log's title is not a map");
    }
}

/// What one catch-up brought in.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatchUp {
    /// Officials' logs downloaded (their JSON) and their server logs.
    pub logs: usize,
    pub raw: usize,
    /// Match pages read for who played, and officials looked for on logs.tf.
    pub rosters: usize,
    pub searched: usize,
    /// Logs rated and added to the season tables.
    pub rated: usize,
    /// Everything picked is here: nothing was left for the next one.
    pub done: bool,
}

/// Bring the league sample up to date with ETF2L's latest officials
/// (Flashy: "update this league sample data every time you sync ... but
/// also someone could manually update the snapshot for themselves"): the
/// same steps as the developer's background download, until it is caught
/// up or `max_steps` / `max_s` run out. On an install that imported the
/// built-in snapshot only what is newer than it is fetched (the snapshot's
/// officials are marked held, and the trends.tf listing stops a week before
/// the newest log held), drops.tf first, so a sync's share is a few hundred
/// quick requests. What arrives is rated at once and joins the tables.
pub async fn catch_up(
    db: &Db,
    sources: &Sources,
    w: &hl_rating::Weights,
    max_steps: usize,
    max_s: u64,
    mut progress: impl FnMut(&str, &CatchUp) + Send,
) -> Result<CatchUp> {
    let started = std::time::Instant::now();
    let mut out = CatchUp::default();
    let mut failing = 0;
    for _ in 0..max_steps {
        if started.elapsed().as_secs() >= max_s {
            break;
        }
        let mut what = String::new();
        let step = step(db, sources, |w: &str| what = w.to_string()).await?;
        progress(&what, &out);
        match step {
            Step::Json { .. } => out.logs += 1,
            Step::Raw { found, .. } => out.raw += usize::from(found),
            Step::Roster { .. } => out.rosters += 1,
            Step::Searched { .. } => out.searched += 1,
            Step::Discovered(_) => {}
            Step::Done => {
                out.done = true;
                break;
            }
            // Nothing can be done until logs.tf's rest ends: the next sync.
            Step::Waiting => break,
            Step::Failed { what } => {
                tracing::info!(what, "league catch-up step failed");
                failing += 1;
                if failing >= 3 {
                    break;
                }
                continue;
            }
        }
        failing = 0;
    }
    // Rated as they arrived, and their players indexed for the profiles.
    let mut tried = std::collections::HashSet::new();
    loop {
        let n = crate::league_rating::rate_new(db, w, &mut tried, 200).await?;
        if n == 0 {
            break;
        }
        out.rated += n;
    }
    crate::catalogue::index_league_players(db, 100_000).await?;
    Ok(out)
}
