//! RGL's North American Highlander, read live for the RGL tab's first page
//! (Flashy: "make a very rudimentary rgl page"; PLAN §30). Nothing is
//! stored yet: the seasons, a season's map pool and its divisions' teams
//! come from RGL's public API, `https://api.rgl.gg/v0`, and are kept in
//! memory for half an hour. Matches, players and the rest are phase 1.

use crate::sources::Sources;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// NA Highlander's region on RGL.
const NA_HIGHLANDER: i64 = 24;
const KEEP: Duration = Duration::from_secs(30 * 60);

static HELD: Mutex<Option<HashMap<String, (Instant, String)>>> = Mutex::new(None);

/// A path's answer, from memory while it is fresh.
async fn get(sources: &Sources, path: &str) -> Result<String> {
    if let Some((at, body)) = HELD.lock().unwrap().get_or_insert_with(HashMap::new).get(path) {
        if at.elapsed() < KEEP {
            return Ok(body.clone());
        }
    }
    let body = sources.rgl_get(path).await?.with_context(|| format!("RGL has no {path}"))?;
    HELD.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_string(), (Instant::now(), body.clone()));
    Ok(body)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglSeasonItem {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglSeason {
    pub id: i64,
    pub name: String,
    pub maps: Vec<String>,
    /// Best division first.
    pub divisions: Vec<RglDivision>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglDivision {
    pub name: String,
    /// Its place on the app's ETF2L ladder (PLAN §30), for comparing.
    pub tier: Option<&'static str>,
    pub teams: Vec<RglTeam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RglTeam {
    #[serde(rename(deserialize = "teamId"))]
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub tag: Option<String>,
    /// RGL's final placing; 99 for a team that dropped out.
    #[serde(default)]
    pub final_rank: Option<i64>,
    /// Where the team was seeded, in a season still running.
    #[serde(default)]
    pub seeding: Option<i64>,
    #[serde(default)]
    pub team_status: Option<String>,
}

/// RGL's division on the ETF2L ladder, as Flashy chose.
pub fn tier_of(division: &str) -> Option<&'static str> {
    Some(match division {
        "Invite" => "Premiership",
        "Advanced" => "High",
        "Main" => "Mid",
        "Intermediate" | "Amateur" => "Low",
        "Newcomer" => "Open",
        _ => return None,
    })
}

/// NA Highlander's seasons, newest first.
pub async fn seasons(sources: &Sources) -> Result<Vec<RglSeasonItem>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        season_id: i64,
        season_name: String,
    }
    let rows: Vec<Row> = serde_json::from_str(&get(sources, &format!("/seasons?regionId={NA_HIGHLANDER}")).await?).context("reading RGL's seasons")?;
    let mut out: Vec<RglSeasonItem> = rows.into_iter().map(|r| RglSeasonItem { id: r.season_id, name: r.season_name }).collect();
    out.sort_by_key(|s| -s.id);
    Ok(out)
}

/// One season: its maps and each division's teams.
pub async fn season(sources: &Sources, id: i64) -> Result<RglSeason> {
    #[derive(Deserialize)]
    struct Head {
        name: String,
        #[serde(default)]
        maps: Vec<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Group {
        group_id: i64,
        group_name: String,
        #[serde(default)]
        sort_order: i64,
    }
    let head: Head = serde_json::from_str(&get(sources, &format!("/seasons/{id}")).await?).context("reading RGL's season")?;
    let mut groups: Vec<Group> = serde_json::from_str(&get(sources, &format!("/seasons/{id}/groups")).await?).context("reading RGL's divisions")?;
    // Admin Placement and Dead Teams are not divisions.
    groups.retain(|g| g.sort_order > 0);
    groups.sort_by_key(|g| -g.sort_order);
    let mut divisions = Vec::new();
    for g in groups {
        let mut teams: Vec<RglTeam> = serde_json::from_str(&get(sources, &format!("/groups/{}/teams", g.group_id)).await?).context("reading RGL's teams")?;
        // RGL's "Bye Week - Invite" is a placeholder for the odd team out.
        teams.retain(|t| !t.name.starts_with("Bye Week"));
        teams.sort_by_key(|t| (t.final_rank.unwrap_or(99), t.seeding.unwrap_or(999), t.name.to_lowercase()));
        divisions.push(RglDivision { tier: tier_of(&g.group_name), name: g.group_name, teams });
    }
    Ok(RglSeason { id, name: head.name, maps: head.maps, divisions })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ladder() {
        assert_eq!(tier_of("Invite"), Some("Premiership"));
        assert_eq!(tier_of("Amateur"), Some("Low"));
        assert_eq!(tier_of("Dead Teams (DO NOT USE)"), None);
    }

    #[test]
    fn a_team_reads() {
        let t: RglTeam = serde_json::from_str(r#"{"teamId":14908,"name":"The Amiable Duds","tag":"AD.","seasonId":193,"groupId":1181,"teamStatusId":1,"teamStatus":"Ready","finalRank":1,"seeding":null}"#).unwrap();
        assert_eq!((t.id, t.final_rank, t.tag.as_deref()), (14908, Some(1), Some("AD.")));
    }
}
