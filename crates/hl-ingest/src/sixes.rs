//! ETF2L 6v6, a first look (prototype; Flashy: "make a prototype for etf2l
//! 6v6"). The app is Highlander's; this reads only the current 6v6 season's
//! tables, so the ETF2L tab can show the other format beside it while the
//! plan for 6v6 everywhere (docs: the 6v6 artifact) is decided.
//!
//! ETF2L runs a 6v6 season as two competitions since Season 53: the
//! Premiership and Divisions 1 to 3 (and Fresh) as round-robin groups of
//! eight, the rest (Division 4, Low, Open) as a Swiss system. Both have the
//! same tables as Highlander's (maps played, won, lost, golden caps, penalty
//! points, score, byes, seeded points), so the tables read the same way.
//!
//! The league's points are taken from ETF2L as they are, never worked out
//! here: the rules page says 3 a map win and 2/1 a golden cap, the live
//! tables count 2 a win and 1 a draw, and ETF2L said it was changing them.
//!
//! Requests: the competition list's first page, then each of the season's
//! competitions and its tables -- about five -- held in memory for half an
//! hour, through the app's throttled ETF2L client.

use crate::sources::Sources;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SixesSeason {
    /// "6v6 Season 53 (Autumn 2026)".
    pub name: String,
    pub competitions: Vec<SixesCompetition>,
    /// The map pool, from the main competition.
    pub pool: Vec<String>,
    /// Best first: the round-robin divisions, then the Swiss ones.
    pub divisions: Vec<SixesDivision>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SixesCompetition {
    pub id: i64,
    pub name: String,
    /// "roundRobin" or "swiss".
    pub system: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SixesDivision {
    pub name: String,
    pub system: &'static str,
    pub competition_id: i64,
    pub teams: Vec<SixesRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SixesRow {
    pub id: i64,
    pub name: String,
    pub country: Option<String>,
    pub maps_played: i64,
    pub maps_won: i64,
    pub maps_lost: i64,
    pub gc_won: i64,
    pub gc_lost: i64,
    pub penalty: i64,
    pub score: i64,
    pub byes: i64,
    pub seeded_points: i64,
    /// Dropped out of the season.
    pub dropped: bool,
}

impl SixesRow {
    /// Maps neither won nor lost: draws (a 5CP map tied at the time limit
    /// in the group stage is a draw, rule 2.2).
    pub fn drawn(&self) -> i64 {
        (self.maps_played - self.maps_won - self.maps_lost).max(0)
    }
}

const KEEP: Duration = Duration::from_secs(30 * 60);
static HELD: Mutex<Option<(Instant, SixesSeason)>> = Mutex::new(None);

/// The current ETF2L 6v6 season's tables, from memory for half an hour.
pub async fn current_season(sources: &Sources) -> Result<Option<SixesSeason>> {
    if let Some((at, s)) = HELD.lock().unwrap().as_ref() {
        if at.elapsed() < KEEP {
            return Ok(Some(s.clone()));
        }
    }
    let Some(body) = sources.etf2l_page("/competition/list", 1).await? else { return Ok(None) };
    let list: Value = serde_json::from_str(&body).context("reading ETF2L's competition list")?;
    let rows = list.pointer("/competitions/data").and_then(Value::as_array).cloned().unwrap_or_default();
    let Some((season, comps)) = season_competitions(&rows) else { return Ok(None) };

    let mut pool = Vec::new();
    let mut divisions = Vec::new();
    for c in &comps {
        if pool.is_empty() {
            if let Ok(Some(body)) = sources.etf2l_get(&format!("/competition/{}", c.id)).await {
                pool = serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|v| v.pointer("/competition/pool").and_then(Value::as_array).cloned())
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|m| m.as_str().map(str::to_string))
                    .collect();
            }
        }
        let Some(body) = sources.etf2l_get(&format!("/competition/{}/tables", c.id)).await? else { continue };
        let v: Value = serde_json::from_str(&body).context("reading ETF2L's 6v6 tables")?;
        divisions.extend(tables_of(&v, c));
    }
    divisions.sort_by_key(|d| (division_order(&d.name), d.name.clone()));
    let out = SixesSeason { name: season, competitions: comps, pool, divisions };
    *HELD.lock().unwrap() = Some((Instant::now(), out.clone()));
    Ok(Some(out))
}

/// The newest "6v6 Season" on the list and its regular-season competitions
/// (not playoffs or third-place matches), newest first as ETF2L lists them.
fn season_competitions(rows: &[Value]) -> Option<(String, Vec<SixesCompetition>)> {
    let sixes = |c: &&Value| c.get("type").and_then(Value::as_str) == Some("6v6") && c.get("category").and_then(Value::as_str) == Some("6v6 Season");
    let newest = rows.iter().find(sixes)?;
    let full = newest.get("name").and_then(Value::as_str)?;
    // "6v6 Season 53 (Autumn 2026): Round Robin" -> "6v6 Season 53 (Autumn 2026)"
    let season = full.split(':').next().unwrap_or(full).trim().to_string();
    let comps = rows
        .iter()
        .filter(sixes)
        .filter_map(|c| {
            let name = c.get("name").and_then(Value::as_str)?;
            let rest = name.strip_prefix(&season)?.trim_start_matches(':').trim();
            // The regular season only: its main competition and its round-robin part.
            let system = match rest {
                "" => "swiss",
                r if r.eq_ignore_ascii_case("Round Robin") => "roundRobin",
                _ => return None,
            };
            Some(SixesCompetition { id: c.get("id")?.as_i64()?, name: name.to_string(), system })
        })
        .collect::<Vec<_>>();
    // A season with one competition only (before Season 53) is all groups.
    let comps = if comps.len() == 1 { comps.into_iter().map(|c| SixesCompetition { system: "roundRobin", ..c }).collect() } else { comps };
    (!comps.is_empty()).then_some((season, comps))
}

fn tables_of(v: &Value, c: &SixesCompetition) -> Vec<SixesDivision> {
    let Some(tables) = v.get("tables").and_then(Value::as_object) else { return Vec::new() };
    let n = |r: &Value, k: &str| r.get(k).and_then(Value::as_i64).unwrap_or(0);
    tables
        .iter()
        .map(|(name, rows)| SixesDivision {
            name: name.clone(),
            system: c.system,
            competition_id: c.id,
            teams: rows
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| SixesRow {
                    id: n(r, "id"),
                    name: r.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    country: r.get("country").and_then(Value::as_str).map(str::to_string),
                    maps_played: n(r, "maps_played"),
                    maps_won: n(r, "maps_won"),
                    maps_lost: n(r, "maps_lost"),
                    gc_won: n(r, "gc_won"),
                    gc_lost: n(r, "gc_lost"),
                    penalty: n(r, "penalty_points"),
                    score: n(r, "score"),
                    byes: n(r, "byes"),
                    seeded_points: n(r, "seeded_points"),
                    dropped: r.get("drop").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect(),
        })
        .collect()
}

/// ETF2L's 6v6 divisions, best first.
pub fn division_order(name: &str) -> u8 {
    match name {
        "Premiership" => 0,
        "Division 1" => 1,
        "Division 2" => 2,
        "Division 3" => 3,
        "Division 4" => 4,
        "Division 5" => 5,
        "Division 6" => 6,
        "Low" => 7,
        "Open" => 8,
        "Fresh" => 9,
        _ => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list() -> Vec<Value> {
        json!([
            {"id": 1062, "type": "6v6", "category": "6v6 Season", "name": "6v6 Season 53 (Autumn 2026): Round Robin"},
            {"id": 1061, "type": "Highlander Fun Team", "category": "Fun Cup", "name": "Crit or Treat!"},
            {"id": 1053, "type": "6v6", "category": "6v6 Season", "name": "6v6 Season 53 (Autumn 2026)"},
            {"id": 1048, "type": "6v6", "category": "Nations' Cup", "name": "6v6 Nations Cup #11: 3rd Place Match"},
            {"id": 1044, "type": "6v6", "category": "6v6 Season", "name": "6v6 Season 52 (Summer 2026): Fresh 3rd Place"},
            {"id": 1040, "type": "6v6", "category": "6v6 Season", "name": "6v6 Season 52 (Summer 2026)"}
        ])
        .as_array()
        .unwrap()
        .clone()
    }

    #[test]
    fn the_newest_season_is_its_round_robin_and_swiss_parts() {
        let (season, comps) = season_competitions(&list()).unwrap();
        assert_eq!(season, "6v6 Season 53 (Autumn 2026)");
        assert_eq!(comps.iter().map(|c| (c.id, c.system)).collect::<Vec<_>>(), vec![(1062, "roundRobin"), (1053, "swiss")]);
    }

    #[test]
    fn a_season_of_one_competition_is_round_robin() {
        let rows: Vec<Value> = list().into_iter().filter(|c| c["id"] == 1040 || c["id"] == 1044).collect();
        let (season, comps) = season_competitions(&rows).unwrap();
        assert_eq!(season, "6v6 Season 52 (Summer 2026)");
        assert_eq!(comps.iter().map(|c| (c.id, c.system)).collect::<Vec<_>>(), vec![(1040, "roundRobin")], "playoffs and 3rd place are left out");
    }

    #[test]
    fn tables_read_etf2ls_fields_and_draws_are_what_is_left() {
        let c = SixesCompetition { id: 1053, name: "x".into(), system: "swiss" };
        let v = json!({"tables": {"Low": [{"id": 1, "name": "bug life", "country": "Finland", "maps_played": 8, "maps_won": 6, "maps_lost": 1,
            "gc_won": 0, "gc_lost": 0, "penalty_points": 0, "score": 13, "byes": 0, "seeded_points": 0, "drop": false}]}});
        let d = tables_of(&v, &c);
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].name.as_str(), d[0].system), ("Low", "swiss"));
        assert_eq!((d[0].teams[0].score, d[0].teams[0].drawn()), (13, 1));
    }

    #[test]
    fn divisions_sort_best_first() {
        let mut v = vec!["Open", "Division 2", "Premiership", "Fresh", "Low", "Division 4"];
        v.sort_by_key(|n| division_order(n));
        assert_eq!(v, ["Premiership", "Division 2", "Division 4", "Low", "Open", "Fresh"]);
    }
}
