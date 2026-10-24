//! Is a season's data right? (Flashy, October 2026: "I need these bugs
//! ironed out and audited so the MVPs can be calculated correctly.")
//!
//! Every played official of a season is checked against ETF2L's own record
//! of it -- the maps it lists, their scores -- and against the logs held for
//! it: is every map there, is any log on a map ETF2L does not list, do two
//! uploads cover the same minutes (a combined log over its parts, or the
//! same game uploaded twice), are there nine a side, was every log read and
//! rated. What is found is listed, never silently fixed: the season tables
//! already count each official once, by its logs' minutes, so a split map or
//! an owner's combined upload no longer skews them; what is left is data
//! that is missing or doubtful, and that is what this names.

use anyhow::Result;
use hl_db::Db;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Logs closer than this are back to back, not overlapping.
const OVERLAP_S: i64 = 60;
/// A side's players in a Highlander log, with a sub or two.
const PLAYERS: std::ops::RangeInclusive<i64> = 16..=22;
/// A map in this many logs or more is split unusually finely.
const SPLIT_LOGS: usize = 3;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// "error": changes who is counted for what (a map missing, a game
    /// counted twice); "warn": doubtful; "info": worth knowing.
    pub severity: String,
    pub code: String,
    pub match_id: i64,
    pub division: String,
    pub tier: Option<i64>,
    pub round: String,
    pub teams: String,
    pub detail: String,
    pub logs: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub season: i64,
    pub officials: usize,
    pub logs: usize,
    /// Officials with nothing wrong.
    pub clean: usize,
    pub by_code: BTreeMap<String, usize>,
    pub issues: Vec<Issue>,
}

struct Match {
    id: i64,
    stage: String,
    division: String,
    tier: Option<i64>,
    round: String,
    teams: String,
    maps: Vec<(String, Option<(i64, i64)>)>,
}

struct Log {
    id: i64,
    map: String,
    start: i64,
    end: i64,
    players: i64,
    has_json: bool,
    rated: bool,
    score: Option<(i64, i64)>,
}

fn base(map: &str) -> String {
    hl_core::maps::map_base(map)
}

/// What two overlapping logs of one match are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlap {
    /// The same game uploaded twice: the same map, about the same length.
    /// The newer upload is left out.
    Duplicate { drop: i64 },
    /// A combined upload over its part: the longer log names several maps,
    /// or runs half again as long as the part. The combined one is left out.
    Combined { drop: i64 },
    /// Neither: two different games in the same minutes, a clock gone
    /// wrong. Both stay; it is reported.
    Unclear,
}

fn several_maps(map: &str) -> bool {
    map.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|w| w.len() >= 3).count() >= 2
}

/// The rule [`excluded_logs`] and the season check share.
pub fn classify(a: (i64, &str, i64), b: (i64, &str, i64)) -> Overlap {
    let (long, short) = if a.2 >= b.2 { (a, b) } else { (b, a) };
    let same_map = !long.1.is_empty() && base(long.1) == base(short.1);
    if same_map && short.2 * 4 >= long.2 * 3 {
        return Overlap::Duplicate { drop: a.0.max(b.0) };
    }
    if several_maps(long.1) || (long.2 * 2 >= short.2 * 3 && (same_map || long.1.is_empty())) {
        return Overlap::Combined { drop: long.0 };
    }
    Overlap::Unclear
}

/// League logs the season tables leave out: duplicates and combined
/// uploads filed beside the per-map logs they repeat.
pub async fn excluded_logs(db: &Db) -> Result<HashSet<i64>> {
    Ok(db
        .league_overlaps()
        .await?
        .into_iter()
        .filter_map(|(a, am, ad, b, bm, bd)| match classify((a, &am, ad), (b, &bm, bd)) {
            Overlap::Duplicate { drop } | Overlap::Combined { drop } => Some(drop),
            Overlap::Unclear => None,
        })
        .collect())
}

/// Every check, for one season and optionally one division (by tier).
pub async fn audit(db: &Db, season: i64, tier: Option<i64>) -> Result<Report> {
    let teams: HashMap<i64, String> = db.team_names().await?.into_iter().collect();
    let team = |id: i64| teams.get(&id).cloned().unwrap_or_else(|| format!("team {id}"));
    let mut matches: Vec<Match> = db
        .audit_matches(season)
        .await?
        .into_iter()
        // A playoff match has no tier of its own: its division's name says.
        .map(|m| {
            let t = m.tier.or_else(|| crate::leagues::canonical_tier(&m.division));
            (t, m)
        })
        .filter(|(t, _)| tier.is_none() || *t == tier)
        .map(|(t, m)| Match {
            id: m.id,
            stage: m.stage,
            division: m.division,
            tier: t,
            round: m.round.filter(|x| !x.is_empty()).or_else(|| m.week.map(|w| format!("Week {w}"))).unwrap_or_default(),
            teams: format!("{} vs {}", team(m.clan1), team(m.clan2)),
            maps: m.maps.and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok()).unwrap_or_default().into_iter().map(|x| (x, None)).collect(),
        })
        .collect();
    let ids: Vec<i64> = matches.iter().map(|m| m.id).collect();
    // ETF2L's own per-map results, where the match page was read.
    let mut per_map: HashMap<i64, Vec<(String, i64, i64)>> = HashMap::new();
    for (id, map, a, b) in db.audit_maps(&ids).await? {
        per_map.entry(id).or_default().push((map, a, b));
    }
    for m in &mut matches {
        if let Some(maps) = per_map.get(&m.id) {
            m.maps = maps.iter().map(|(map, a, b)| (map.clone(), Some((*a, *b)))).collect();
        }
    }
    let mut logs: HashMap<i64, Vec<Log>> = HashMap::new();
    for l in db.audit_logs(&ids, hl_rating::MODEL_VERSION).await? {
        logs.entry(l.match_id).or_default().push(Log {
            id: l.log_id,
            map: l.map,
            start: l.end - l.duration,
            end: l.end,
            players: l.players,
            has_json: l.has_json,
            rated: l.rated,
            score: l.score,
        });
    }
    // The owner's own logs of each match: often one combined upload.
    let mut own: HashMap<i64, Vec<(i64, String, i64, i64)>> = HashMap::new();
    for (m, id, map, end, dur) in db.audit_own_logs(&ids).await? {
        own.entry(m).or_default().push((id, map, end - dur, end));
    }

    let mut issues = Vec::new();
    let mut n_logs = 0;
    let mut dirty: HashSet<i64> = HashSet::new();
    for m in &matches {
        let mut push = |severity: &str, code: &str, detail: String, logs: Vec<i64>| {
            if severity != "info" {
                dirty.insert(m.id);
            }
            issues.push(Issue {
                severity: severity.into(),
                code: code.into(),
                match_id: m.id,
                division: m.division.clone(),
                tier: m.tier,
                round: if m.round.is_empty() { m.stage.clone() } else { m.round.clone() },
                teams: m.teams.clone(),
                detail,
                logs,
            });
        };
        let mut ls: Vec<&Log> = logs.get(&m.id).map(|v| v.iter().collect()).unwrap_or_default();
        ls.sort_by_key(|l| l.start);
        n_logs += ls.len();
        let mine = own.get(&m.id).cloned().unwrap_or_default();
        if ls.is_empty() {
            if mine.is_empty() {
                push("error", "no_logs", "Played, but no log of it is held: nobody's numbers from it count.".into(), vec![]);
            } else {
                push("warn", "own_only", "Only your own upload is held, not the league's per-map logs.".into(), mine.iter().map(|o| o.0).collect());
            }
            continue;
        }
        // Maps: ETF2L's list against the logs'.
        let etf2l: Vec<String> = m.maps.iter().map(|(x, _)| base(x)).collect();
        let held: HashSet<String> = ls.iter().map(|l| base(&l.map)).collect();
        for (i, (name, _)) in m.maps.iter().enumerate() {
            if !held.contains(&etf2l[i]) && !etf2l[..i].contains(&etf2l[i]) {
                push("error", "missing_map", format!("ETF2L lists {name}, but no log of it is held."), vec![]);
            }
        }
        if !etf2l.is_empty() {
            for l in ls.iter().filter(|l| !etf2l.contains(&base(&l.map))) {
                push("warn", "extra_map", format!("A log on {} is filed under this match, which ETF2L lists as {}.", l.map, etf2l.join(", ")), vec![l.id]);
            }
        }
        // Overlapping uploads: the same minutes in two logs.
        for (i, a) in ls.iter().enumerate() {
            for b in &ls[i + 1..] {
                let overlap = a.end.min(b.end) - a.start.max(b.start);
                if overlap <= OVERLAP_S {
                    continue;
                }
                match classify((a.id, &a.map, a.end - a.start), (b.id, &b.map, b.end - b.start)) {
                    Overlap::Duplicate { drop } => push(
                        "warn",
                        "duplicate",
                        format!("Logs {} and {} are the same game uploaded twice ({}); the tables count it once, leaving out {drop}.", a.id, b.id, a.map),
                        vec![a.id, b.id],
                    ),
                    Overlap::Combined { drop } => push(
                        "warn",
                        "combined",
                        format!("Log {drop} is a combined upload over {} ({} min in common); the tables leave it out and count the parts.", if drop == a.id { b.id } else { a.id }, overlap / 60),
                        vec![a.id, b.id],
                    ),
                    Overlap::Unclear => push(
                        "error",
                        "overlap",
                        format!("Logs {} ({}) and {} ({}) share {} min: two games cannot. A clock is wrong or a log is filed under the wrong match; both still count.", a.id, a.map, b.id, b.map, overlap / 60),
                        vec![a.id, b.id],
                    ),
                }
            }
        }
        // Each log's own health.
        for l in &ls {
            if !l.has_json {
                push("warn", "not_downloaded", format!("Log {} ({}) has not been downloaded: its numbers are missing.", l.id, l.map), vec![l.id]);
            } else if !l.rated && l.end - l.start >= 300 {
                push("warn", "unrated", format!("Log {} ({}) was read but nobody in it is rated.", l.id, l.map), vec![l.id]);
            }
            if l.has_json && l.players > 0 && !PLAYERS.contains(&l.players) {
                push("warn", "players", format!("Log {} ({}) has {} players; a Highlander game has 18.", l.id, l.map, l.players), vec![l.id]);
            }
        }
        // Maps split into many logs: fine for the tables now, worth knowing.
        for name in &held {
            let parts: Vec<i64> = ls.iter().filter(|l| &base(&l.map) == name).map(|l| l.id).collect();
            if parts.len() >= SPLIT_LOGS {
                push("info", "split_map", format!("{name} is in {} logs; it counts once, by minutes.", parts.len()), parts);
            }
        }
        // KOTH scores: rounds in the logs against ETF2L's for the map.
        for (name, score) in &m.maps {
            let Some((a, b)) = score else { continue };
            if !name.starts_with("koth_") {
                continue;
            }
            let of_map: Vec<&&Log> = ls.iter().filter(|l| base(&l.map) == base(name)).collect();
            if of_map.is_empty() || of_map.iter().any(|l| l.score.is_none()) {
                continue;
            }
            let rounds: i64 = of_map.iter().map(|l| l.score.map_or(0, |(r, b)| r + b)).sum();
            if rounds != a + b {
                push(
                    "warn",
                    "score",
                    format!("{name}: ETF2L has {a}-{b} ({} rounds), the logs {} rounds.", a + b, rounds),
                    of_map.iter().map(|l| l.id).collect(),
                );
            }
        }
        // The owner's combined upload, where the per-map logs are held too:
        // the tables use the per-map logs; said so the duplicate is known.
        for (id, map, _, _) in &mine {
            if ls.len() >= 2 && !ls.iter().any(|l| l.id == *id) {
                push("info", "own_combined", format!("Your upload {id} ({map}) stands beside the league's {} logs of this match; the tables count those.", ls.len()), vec![*id]);
            }
        }
    }
    let mut by_code: BTreeMap<String, usize> = BTreeMap::new();
    for i in &issues {
        *by_code.entry(i.code.clone()).or_default() += 1;
    }
    issues.sort_by_key(|i| (match i.severity.as_str() { "error" => 0, "warn" => 1, _ => 2 }, i.tier.unwrap_or(99)));
    Ok(Report { season, officials: matches.len(), logs: n_logs, clean: matches.len() - dirty.len(), by_code, issues })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_logs_are_told_apart() {
        // Three uploads of one 14-minute Steel half: duplicates.
        assert_eq!(classify((2841374, "cp_steel", 861), (2841376, "cp_steel", 867)), Overlap::Duplicate { drop: 2841376 });
        // A combined Proot + Product over its Product part.
        assert_eq!(classify((3711424, "proot + product", 2340), (3711420, "koth_product_final", 975)), Overlap::Combined { drop: 3711424 });
        // A whole Vigil upload over one half of it.
        assert_eq!(classify((3922703, "Vigil", 2100), (3922692, "pl_vigil_rc10", 999)), Overlap::Combined { drop: 3922703 });
        // Warmtic and Coalplant in the same minutes: two games, a clock is wrong.
        assert_eq!(classify((2827527, "koth_warmtic_b6", 840), (2827530, "koth_coalplant_b8", 817)), Overlap::Unclear);
    }
}
