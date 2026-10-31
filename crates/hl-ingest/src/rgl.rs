//! RGL's North American Highlander, read live for the RGL tab's first page
//! (Flashy: "make a very rudimentary rgl page"; PLAN §30). Nothing is
//! stored yet: the seasons, a season's map pool and its divisions' teams
//! come from RGL's public API, `https://api.rgl.gg/v0`, and are kept in
//! memory for half an hour. A team's page (roster, results, its other
//! seasons) and a player's (by SteamID: RGL's name search is broken on its
//! side) are read the same way. Standings, bans and the rest are phase 1.

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

/// A path's answer, from memory while it is fresh; `None` when RGL has no
/// such page.
async fn get_opt(sources: &Sources, path: &str) -> Result<Option<String>> {
    if let Some((at, body)) = HELD.lock().unwrap().get_or_insert_with(HashMap::new).get(path) {
        if at.elapsed() < KEEP {
            return Ok(Some(body.clone()));
        }
    }
    let Some(body) = sources.rgl_get(path).await? else { return Ok(None) };
    HELD.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_string(), (Instant::now(), body.clone()));
    Ok(Some(body))
}

async fn get(sources: &Sources, path: &str) -> Result<String> {
    get_opt(sources, path).await?.with_context(|| format!("RGL has no {path}"))
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

// ---- A team's page ----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RglMember {
    pub name: String,
    pub steam_id: String,
    #[serde(default)]
    pub is_leader: bool,
    pub joined_at: Option<String>,
    pub left_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RglMatchTeam {
    pub team_id: i64,
    pub team_name: String,
    #[serde(default)]
    pub team_tag: Option<String>,
    #[serde(default)]
    pub is_home: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RglMatchMap {
    pub map_name: String,
    #[serde(default)]
    pub home_score: Option<i64>,
    #[serde(default)]
    pub away_score: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RglMatch {
    pub match_id: i64,
    pub match_date: Option<String>,
    /// "Week 1 - Vigil".
    pub match_name: Option<String>,
    #[serde(default)]
    pub is_forfeit: bool,
    pub winner: Option<i64>,
    #[serde(default)]
    pub teams: Vec<RglMatchTeam>,
    #[serde(default)]
    pub maps: Vec<RglMatchMap>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglTeamPage {
    pub id: i64,
    pub name: String,
    pub tag: Option<String>,
    pub season_id: i64,
    pub season_name: Option<String>,
    pub division: Option<String>,
    pub tier: Option<&'static str>,
    pub final_rank: Option<i64>,
    pub status: Option<String>,
    pub players: Vec<RglMember>,
    /// The same team in its other seasons, for `team_seasons`.
    pub linked: Vec<i64>,
    /// Played or scheduled, oldest first.
    pub matches: Vec<RglMatch>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamRaw {
    team_id: i64,
    name: String,
    #[serde(default)]
    tag: Option<String>,
    season_id: i64,
    #[serde(default)]
    division_name: Option<String>,
    #[serde(default)]
    final_rank: Option<i64>,
    #[serde(default)]
    team_status: Option<String>,
    #[serde(default)]
    players: Vec<RglMember>,
    #[serde(default)]
    linked_teams: Vec<i64>,
    #[serde(default)]
    match_ids: Vec<i64>,
}

async fn team_raw(sources: &Sources, id: i64) -> Result<Option<TeamRaw>> {
    let Some(body) = get_opt(sources, &format!("/teams/{id}")).await? else { return Ok(None) };
    Ok(Some(serde_json::from_str(&body).context("reading RGL's team")?))
}

/// A season's name, from NA Highlander's list; `None` for another format's.
async fn season_name(sources: &Sources, id: i64) -> Option<String> {
    seasons(sources).await.ok()?.into_iter().find(|s| s.id == id).map(|s| s.name)
}

/// A team: its roster, its matches one by one (RGL's match list is broken on
/// its side; a season is about ten) and the ids of its other seasons.
pub async fn team(sources: &Sources, id: i64) -> Result<Option<RglTeamPage>> {
    let Some(t) = team_raw(sources, id).await? else { return Ok(None) };
    let mut matches = Vec::new();
    for m in &t.match_ids {
        if let Some(body) = get_opt(sources, &format!("/matches/{m}")).await? {
            matches.push(serde_json::from_str::<RglMatch>(&body).context("reading RGL's match")?);
        }
    }
    matches.sort_by(|a, b| a.match_date.cmp(&b.match_date));
    Ok(Some(RglTeamPage {
        id: t.team_id,
        season_name: season_name(sources, t.season_id).await,
        tier: t.division_name.as_deref().and_then(tier_of),
        name: t.name,
        tag: t.tag,
        season_id: t.season_id,
        division: t.division_name,
        final_rank: t.final_rank,
        status: t.team_status,
        players: t.players,
        linked: t.linked_teams,
        matches,
    }))
}

/// One season of a team, for its "Other seasons".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglTeamSeason {
    pub id: i64,
    pub name: String,
    pub tag: Option<String>,
    pub season_id: i64,
    pub season_name: String,
    pub division: Option<String>,
    pub final_rank: Option<i64>,
}

/// The team's other seasons, newest first: NA Highlander's only.
pub async fn team_seasons(sources: &Sources, ids: Vec<i64>) -> Result<Vec<RglTeamSeason>> {
    let mut out = Vec::new();
    for id in ids.into_iter().take(30) {
        let Some(t) = team_raw(sources, id).await? else { continue };
        let Some(season_name) = season_name(sources, t.season_id).await else { continue };
        out.push(RglTeamSeason { id: t.team_id, name: t.name, tag: t.tag, season_id: t.season_id, season_name, division: t.division_name, final_rank: t.final_rank });
    }
    out.sort_by_key(|t| -t.season_id);
    Ok(out)
}

// ---- A player's page --------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglPlayer {
    pub steam_id: String,
    pub name: String,
    pub avatar: Option<String>,
    pub banned: bool,
    pub probation: bool,
    /// NA Highlander's seasons, newest first.
    pub seasons: Vec<RglPlayerSeason>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglPlayerSeason {
    pub season_id: i64,
    pub season_name: String,
    pub division: Option<String>,
    pub tier: Option<&'static str>,
    pub team_id: i64,
    pub team_name: String,
    pub team_tag: Option<String>,
    pub started_at: Option<String>,
    pub left_at: Option<String>,
    pub wins: i64,
    pub losses: i64,
}

/// The SteamID64 in any form a person pastes, rgl.gg's `?p=` link too.
fn steam64(input: &str) -> Result<u64> {
    let input = input.trim();
    let input = input.rsplit_once("p=").map_or(input, |(_, id)| id.split('&').next().unwrap_or(id));
    Ok(hl_core::steamid::SteamId::parse(input).context("not a SteamID")?.as_u64())
}

/// A player by any SteamID form or profile link; `None` when they have no
/// RGL profile.
pub async fn player(sources: &Sources, input: &str) -> Result<Option<RglPlayer>> {
    let id = steam64(input)?;
    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct Status {
        #[serde(default)]
        is_banned: bool,
        #[serde(default)]
        is_on_probation: bool,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Profile {
        steam_id: String,
        name: String,
        avatar: Option<String>,
        status: Option<Status>,
    }
    #[derive(Deserialize, Default)]
    struct Stats {
        #[serde(default)]
        wins: i64,
        #[serde(default)]
        loses: i64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        format_id: i64,
        region_id: i64,
        season_id: i64,
        season_name: String,
        division_name: Option<String>,
        team_id: i64,
        team_name: String,
        team_tag: Option<String>,
        started_at: Option<String>,
        left_at: Option<String>,
        #[serde(default)]
        stats: Option<Stats>,
    }
    let Some(body) = get_opt(sources, &format!("/profile/{id}")).await? else { return Ok(None) };
    let p: Profile = serde_json::from_str(&body).context("reading RGL's profile")?;
    let rows: Vec<Row> = match get_opt(sources, &format!("/profile/{id}/teams")).await? {
        Some(b) => serde_json::from_str(&b).context("reading a player's RGL teams")?,
        None => Vec::new(),
    };
    let mut seasons: Vec<RglPlayerSeason> = rows
        .into_iter()
        .filter(|r| r.format_id == 2 && r.region_id == NA_HIGHLANDER)
        .map(|r| {
            let s = r.stats.unwrap_or_default();
            RglPlayerSeason {
                tier: r.division_name.as_deref().and_then(tier_of),
                season_id: r.season_id,
                season_name: r.season_name,
                division: r.division_name,
                team_id: r.team_id,
                team_name: r.team_name,
                team_tag: r.team_tag,
                started_at: r.started_at,
                left_at: r.left_at,
                wins: s.wins,
                losses: s.loses,
            }
        })
        .collect();
    seasons.sort_by(|a, b| b.season_id.cmp(&a.season_id).then(b.started_at.cmp(&a.started_at)));
    let status = p.status.unwrap_or_default();
    Ok(Some(RglPlayer { steam_id: p.steam_id, name: p.name, avatar: p.avatar, banned: status.is_banned, probation: status.is_on_probation, seasons }))
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

    #[test]
    fn a_match_reads() {
        let m: RglMatch = serde_json::from_str(r#"{"matchId":34332,"seasonName":"HL Season 24","matchDate":"2026-06-02T01:30:00.000Z","matchName":"Week 1 - Vigil","isForfeit":false,"winner":14908,"teams":[{"teamName":"Run It Back","teamTag":"Rib.","teamId":14956,"isHome":false,"points":"0"},{"teamName":"The Amiable Duds","teamTag":"AD.","teamId":14908,"isHome":true,"points":"9"}],"maps":[{"mapName":"pl_vigil_rc10","homeScore":2,"awayScore":0}]}"#).unwrap();
        assert_eq!((m.winner, m.teams.len(), m.maps[0].home_score), (Some(14908), 2, Some(2)));
    }

    #[test]
    fn steamids_read() {
        assert_eq!(steam64("76561198137137702").unwrap(), 76561198137137702);
        assert_eq!(steam64("https://rgl.gg/Public/PlayerProfile?p=76561198137137702&r=24").unwrap(), 76561198137137702);
        assert!(steam64("voosh").is_err());
    }
}
