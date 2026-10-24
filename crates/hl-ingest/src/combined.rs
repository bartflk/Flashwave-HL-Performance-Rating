//! A combined log, its parts and their demos, round by round (Q84, Flashy:
//! "when there is a combined, and demos with it, correctly match the maps
//! and the sides").
//!
//! A combined log keeps each team on one colour for the whole match; each
//! part was written in its own colours, and each part's Red is whoever was
//! Red overall in that part -- in the S36 High final, the first Upward
//! log's Red is the team the combined log calls Blue. A part's demo records
//! the colours as played, round by round, which the combined log's
//! `colours_swapped` already says. So the ledger ties each round of the
//! combined log to its part (by round start, to the second), that part's
//! map and half, which way the part's own teams run against the combined
//! log's (by its players), and the part's demos.tf demo; and each map's
//! rounds to ETF2L's result for that map, where the match is known.

use anyhow::Result;
use hl_db::Db;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// One part of a combined log.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerPart {
    pub log_id: i64,
    pub map: Option<String>,
    /// "game 1", "game 2" for a stopwatch map's logs, each one game of
    /// both teams attacking once; "whole map" for a KOTH map's one log,
    /// "part 1", "part 2" for a KOTH map split over several.
    pub label: String,
    /// The combined log's round numbers this part holds.
    pub rounds: Vec<i64>,
    /// This part's own Red is the combined log's Blue: its scoreboard and
    /// its page read the other way round. `None` when too few players are
    /// shared to tell.
    pub teams_flipped: Option<bool>,
    pub demos_tf_id: Option<i64>,
}

/// One map of a combined log.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerMap {
    pub map: String,
    pub first_round: i64,
    pub last_round: i64,
    /// Rounds won, in the combined log's colours.
    pub red_rounds: i64,
    pub blue_rounds: i64,
    /// ETF2L's points for the map, in the same colours, when the official
    /// is known.
    pub etf2l_red: Option<i64>,
    pub etf2l_blue: Option<i64>,
    /// The rounds' winner agrees with ETF2L's; `None` when it cannot be
    /// told (no ETF2L result, or a stopwatch map level on rounds, which
    /// time decides).
    pub agrees: Option<bool>,
    /// The log that is exactly this map: a combine of its games uploaded
    /// beside the parts ("ashville", "Combined Upward"), or its only log.
    /// What the page reads for "the whole map" (Flashy: "a per-map setting
    /// instead of per log").
    pub whole_log: Option<i64>,
    /// Every log of this map, the whole-map combine included, oldest first.
    pub logs: Vec<i64>,
}

/// One round of a combined log.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerRound {
    pub round_num: i64,
    pub map: Option<String>,
    pub part: Option<i64>,
    /// The teams wore each other's colours this round (a stopwatch half).
    pub colours_swapped: bool,
    pub winner: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
    pub log_id: i64,
    pub rounds: Vec<LedgerRound>,
    pub parts: Vec<LedgerPart>,
    pub maps: Vec<LedgerMap>,
    /// What does not line up: a round no part holds, a part whose teams
    /// could not be told, a part with no demo, a map ETF2L scores the
    /// other way.
    pub problems: Vec<String>,
}

/// Shared players needed to tell which way a part's teams run.
const MIN_SHARED: usize = 6;

/// The ledger of a combined log; `None` for a log with no parts.
pub async fn ledger(db: &Db, log_id: i64) -> Result<Option<Ledger>> {
    let part_ids: Vec<i64> = db.log_and_parts(log_id).await?.into_iter().filter(|id| *id != log_id).collect();
    if part_ids.is_empty() {
        return Ok(None);
    }
    let rounds = db.all_rounds_of(log_id).await?;
    let colours: HashMap<i64, String> = db.log_time_and_colours(log_id).await?.map(|(_, c)| c.into_iter().collect()).unwrap_or_default();
    let maps: HashMap<i64, String> = db.round_maps_of(log_id).await?.into_iter().collect();

    // Each part: its rounds' starts, its map, its players' colours, its demo.
    struct P {
        id: i64,
        map: Option<String>,
        starts: HashSet<i64>,
        colours: HashMap<i64, String>,
        demo: Option<i64>,
        combined: bool,
    }
    let mut parts: Vec<P> = Vec::new();
    for id in &part_ids {
        let info = db.part_info(*id).await?;
        let mut starts: HashSet<i64> = db.all_rounds_of(*id).await?.into_iter().map(|r| r.1).collect();
        let mut colours: HashMap<i64, String> = db.log_time_and_colours(*id).await?.map(|(_, c)| c.into_iter().collect()).unwrap_or_default();
        // A part kept only as its JSON (the sync's part fetch keeps them so)
        // has no rows in the match tables: its rounds and players from there.
        if starts.is_empty() || colours.is_empty() {
            let json = match db.part_raw(*id).await? {
                Some(j) => Some(j),
                None => db.raw_log(*id).await?,
            };
            if let Some(v) = json.and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok()) {
                let rs: Vec<i64> = v["rounds"].as_array().into_iter().flatten().filter_map(|r| r["start_time"].as_i64()).collect();
                if starts.is_empty() {
                    starts = rs.into_iter().collect();
                }
                if colours.is_empty() {
                    colours = v["players"]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter_map(|(k, p)| Some((i64::from(hl_core::SteamId::parse(k).ok()?.account_id()), p["team"].as_str()?.to_string())))
                        .collect();
                }
            }
        }
        let (map, title, demo) = info.unwrap_or_default();
        // A combine of the map's games: logs.tf names it "Combined ..." or
        // the uploader names it after the map, with no map of its own.
        let combined = title.as_deref().is_some_and(|t| t.to_ascii_lowercase().contains("combined")) || map.is_none();
        parts.push(P { id: *id, map, starts, colours, demo, combined });
    }
    // A combine of other parts ("Combined Upward") adds nothing of its own:
    // a part whose rounds all belong to a smaller part is left out.
    let own: Vec<&P> = parts
        .iter()
        .filter(|p| !p.combined || !parts.iter().any(|q| q.id != p.id && !q.starts.is_empty() && q.starts.iter().all(|s| p.starts.contains(s)) && q.starts.len() < p.starts.len()))
        .collect();

    let mut out = Ledger { log_id, ..Default::default() };
    for (num, start, winner, swapped) in &rounds {
        let part = own.iter().filter(|p| p.starts.contains(start)).min_by_key(|p| p.starts.len()).map(|p| p.id);
        if part.is_none() {
            out.problems.push(format!("round {num}: no part holds it"));
        }
        out.rounds.push(LedgerRound { round_num: *num, map: maps.get(num).cloned(), part, colours_swapped: *swapped, winner: winner.clone() });
    }

    // Parts in play order, labelled per map.
    let mut ordered: Vec<&&P> = own.iter().filter(|p| out.rounds.iter().any(|r| r.part == Some(p.id))).collect();
    ordered.sort_by_key(|p| p.starts.iter().min().copied().unwrap_or(i64::MAX));
    let mut per_map: HashMap<String, usize> = HashMap::new();
    let mut of_map: HashMap<String, usize> = HashMap::new();
    for p in &ordered {
        *of_map.entry(p.map.as_deref().map(hl_core::maps::map_base).unwrap_or_default()).or_default() += 1;
    }
    for p in ordered {
        let map = p.map.clone();
        let base = map.as_deref().map(hl_core::maps::map_base).unwrap_or_default();
        let n = per_map.entry(base.clone()).or_default();
        *n += 1;
        let koth = map.as_deref().is_some_and(|m| m.starts_with("koth_"));
        // A stopwatch log is one game: both teams attack once, and ETF2L
        // scores the map in games won (the S36 High final's Upward: three
        // logs, ETF2L 2-1). A KOTH map is one log.
        // A KOTH map split over several logs (a restart, a change of server)
        // reads as its parts.
        let label = match (koth, of_map.get(&base).copied().unwrap_or(1)) {
            (true, 1) => "whole map".to_string(),
            (true, _) => format!("part {n}"),
            (false, _) => format!("game {n}"),
        };
        // Which way its teams run: the combined log's Red players' colour in it.
        let (mut same, mut flipped) = (0usize, 0usize);
        for (account, c) in &colours {
            if let Some(pc) = p.colours.get(account) {
                if pc == c {
                    same += 1;
                } else {
                    flipped += 1;
                }
            }
        }
        let teams_flipped = (same + flipped >= MIN_SHARED).then_some(flipped > same);
        if teams_flipped.is_none() {
            out.problems.push(format!("log {}: too few players shared to tell its sides", p.id));
        }
        if p.demo.is_none() {
            out.problems.push(format!("log {}: no demos.tf demo linked", p.id));
        }
        let rounds: Vec<i64> = out.rounds.iter().filter(|r| r.part == Some(p.id)).map(|r| r.round_num).collect();
        out.parts.push(LedgerPart { log_id: p.id, map, label, rounds, teams_flipped, demos_tf_id: p.demo });
    }

    // Maps, from the stored segments, beside ETF2L's result for each.
    let etf2l = etf2l_maps(db, log_id).await?;
    for (map, first, last, red, blue) in db.segments_of(log_id).await? {
        let Some(map) = map else { continue };
        let theirs = etf2l.iter().find(|(m, _, _)| hl_core::maps::map_base(m) == hl_core::maps::map_base(&map)).map(|(_, r, b)| (*r, *b));
        let stopwatch = map.starts_with("pl_") || map.starts_with("cp_steel");
        let agrees = theirs.and_then(|(er, eb)| {
            if red == blue && stopwatch {
                return None;
            }
            Some((red > blue) == (er > eb) && (red < blue) == (er < eb))
        });
        if agrees == Some(false) {
            out.problems.push(format!("{map}: the rounds say {red}-{blue}, ETF2L {}-{}", theirs.map_or(0, |t| t.0), theirs.map_or(0, |t| t.1)));
        }
        // The map's rounds, by start time, and the logs that cover them:
        // the one holding exactly them is the whole map.
        let map_starts: HashSet<i64> = rounds.iter().filter(|(n, ..)| *n >= first && *n <= last).map(|(_, s, ..)| *s).collect();
        let mut logs: Vec<&P> = parts.iter().filter(|p| !p.starts.is_empty() && p.starts.iter().all(|s| map_starts.contains(s))).collect();
        logs.sort_by_key(|p| (p.starts.iter().min().copied().unwrap_or(i64::MAX), std::cmp::Reverse(p.starts.len())));
        let whole_log = logs.iter().find(|p| p.starts == map_starts).map(|p| p.id);
        out.maps.push(LedgerMap {
            map,
            first_round: first,
            last_round: last,
            red_rounds: red,
            blue_rounds: blue,
            etf2l_red: theirs.map(|t| t.0),
            etf2l_blue: theirs.map(|t| t.1),
            agrees,
            whole_log,
            logs: logs.iter().map(|p| p.id).collect(),
        });
    }
    Ok(Some(out))
}

/// ETF2L's points for each map of the official this log was, in the log's
/// colours: `(map, red, blue)`. Empty when the official is not known.
async fn etf2l_maps(db: &Db, log_id: i64) -> Result<Vec<(String, i64, i64)>> {
    let Some((match_id, red_team)) = db.league_match_of_log(log_id).await? else { return Ok(Vec::new()) };
    let Some((clan1, _)) = db.season_match_clans(match_id).await? else { return Ok(Vec::new()) };
    Ok(db
        .season_map_points(match_id)
        .await?
        .into_iter()
        .map(|(map, c1, c2)| if red_team == clan1 { (map, c1, c2) } else { (map, c2, c1) })
        .collect())
}
