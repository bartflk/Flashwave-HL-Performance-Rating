//! RGL's North American Highlander, read live for the RGL tab's first page
//! (Flashy: "make a very rudimentary rgl page"; PLAN §30). Nothing is
//! stored yet: the seasons, a season's map pool and its divisions' teams
//! come from RGL's public API, `https://api.rgl.gg/v0`, and are kept in
//! memory for half an hour. A team's page (roster, results, its other
//! seasons) and a player's (by SteamID: RGL's name search is broken on its
//! side) are read the same way. A season's matches are read one by one
//! (RGL's paged match list ignores its filter and carries no scores), for
//! its standings and results; the league's bans come from its paged list.
//! A decided match does not change, so it is kept on disk (`rgl_match`,
//! Flashy: "store finished seasons on disk"): a finished season opens from
//! there, a live one asks RGL only for the matches still to be played.
//!
//! RGL's API is a free service, so it is asked as little as it can be
//! (Flashy: "make sure to properly use the api to not overburden them"):
//! - at most 4 requests a second, under the 5 its `RateLimit-*` headers
//!   allow, waiting out the window when they say none are left (`http.rs`);
//! - one request at a time for the same answer: two screens wanting a
//!   season wait for the one request, not two;
//! - answers held in memory for half an hour, and on disk for good once
//!   they cannot change: a finished season's page, divisions and teams, a
//!   finished team (`rgl_answer`), a decided match (`rgl_match`);
//! - a match still to be played is kept with when it was read, and asked
//!   for again only when RGL's list of matches changed since (one request
//!   for all of them), or after six hours whatever it says.

use crate::sources::Sources;
use hl_db::Db;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// NA Highlander's region on RGL.
const NA_HIGHLANDER: i64 = 24;
const KEEP: Duration = Duration::from_secs(30 * 60);
/// A match's answer in memory: briefly, as a match still to be played is
/// kept on disk and checked against RGL's changes instead.
const KEEP_MATCH: Duration = Duration::from_secs(60);
/// How many answers memory holds at most.
const HELD_MAX: usize = 3000;

/// Answers by path, with when they go stale.
static HELD: Mutex<Option<HashMap<String, (Instant, String)>>> = Mutex::new(None);
/// One gate per answer being asked for, so the same one is asked once.
static ASKING: Mutex<Option<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = Mutex::new(None);

/// Held in place of a page RGL does not have, so it is not asked again at
/// once (LG-10: a 404 match was asked for on every view).
const MISSING: &str = "\u{0}missing";
/// How long a missing page is remembered.
const KEEP_MISSING: Duration = Duration::from_secs(5 * 60);

fn held(key: &str) -> Option<String> {
    let mut held = HELD.lock().unwrap();
    let map = held.get_or_insert_with(HashMap::new);
    match map.get(key) {
        Some((until, body)) if *until > Instant::now() => Some(body.clone()),
        _ => None,
    }
}

fn hold(key: &str, body: &str, life: Duration) {
    let mut held = HELD.lock().unwrap();
    let map = held.get_or_insert_with(HashMap::new);
    if map.len() >= HELD_MAX {
        let now = Instant::now();
        map.retain(|_, (until, _)| *until > now);
        // Still full: the soonest stale go first.
        while map.len() >= HELD_MAX {
            let Some(first) = map.iter().min_by_key(|(_, (until, _))| *until).map(|(k, _)| k.clone()) else { break };
            map.remove(&first);
        }
    }
    map.insert(key.to_string(), (Instant::now() + life, body.to_string()));
}

/// Drop an answer from memory: RGL changed it, so the copy is stale.
fn forget(key: &str) {
    if let Some(map) = HELD.lock().unwrap().as_mut() {
        map.remove(key);
    }
}

/// An answer from memory while it is fresh, else asked for: once, however
/// many are waiting for it.
async fn once<F, Fut>(key: &str, life: Duration, ask: F) -> Result<Option<String>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Option<String>>>,
{
    if let Some(body) = held(key) {
        return Ok(if body == MISSING { None } else { Some(body) });
    }
    let gate = ASKING.lock().unwrap().get_or_insert_with(HashMap::new).entry(key.to_string()).or_default().clone();
    let turn = gate.lock().await;
    // Whoever held the gate before may have just asked.
    let out = match held(key) {
        Some(body) => Ok(if body == MISSING { None } else { Some(body) }),
        None => {
            let out = ask().await;
            match &out {
                Ok(Some(body)) => hold(key, body, life),
                Ok(None) => hold(key, MISSING, KEEP_MISSING),
                Err(_) => {}
            }
            out
        }
    };
    drop(turn);
    let mut asking = ASKING.lock().unwrap();
    if let Some(map) = asking.as_mut() {
        // Only the map's copy and ours: nobody else is waiting.
        if map.get(key).is_some_and(|g| Arc::strong_count(g) <= 2) {
            map.remove(key);
        }
    }
    out
}

/// A path's answer, from memory while it is fresh; `None` when RGL has no
/// such page.
async fn get_opt(sources: &Sources, path: &str) -> Result<Option<String>> {
    once(path, KEEP, || sources.rgl_get(path)).await
}

async fn get(sources: &Sources, path: &str) -> Result<String> {
    get_opt(sources, path).await?.with_context(|| format!("RGL has no {path}"))
}

/// A POST's answer (RGL's searches and match list), held as a GET is.
async fn post_opt(sources: &Sources, path: &str, body: serde_json::Value, life: Duration) -> Result<Option<String>> {
    let key = format!("POST {path} {body}");
    once(&key, life, || async move { sources.rgl_post(path, &body).await }).await
}

/// A kept answer is asked for again after this long, so a correction RGL
/// makes late still lands (LG-1).
const RECHECK: i64 = 30 * 24 * 3600;
/// A team's page is looked at again sooner: its list of linked seasons grows
/// when the team plays on (LG-2).
const RECHECK_TEAM: i64 = 7 * 24 * 3600;

/// A path's answer from disk when it was kept and is not due for another
/// look, else from RGL; kept from now on when `lasts` says it cannot change
/// any more, forgotten when it no longer holds.
async fn kept_for(db: &Db, sources: &Sources, path: &str, recheck: i64, lasts: impl Fn(&str) -> bool) -> Result<Option<String>> {
    let stored = db.rgl_answers(&[path.to_string()]).await?.remove(path);
    if let Some((body, at)) = &stored {
        if unix_now() - at < recheck {
            return Ok(Some(body.clone()));
        }
    }
    let got = match get_opt(sources, path).await {
        Ok(got) => got,
        // RGL cannot be reached: the old copy still beats nothing.
        Err(_) if stored.is_some() => return Ok(stored.map(|(body, _)| body)),
        Err(e) => return Err(e),
    };
    match &got {
        Some(body) if lasts(body) => db.store_rgl_answer(path, body).await?,
        _ if stored.is_some() => db.drop_rgl_answer(path).await?,
        _ => {}
    }
    Ok(got)
}

async fn kept_opt(db: &Db, sources: &Sources, path: &str, lasts: impl Fn(&str) -> bool) -> Result<Option<String>> {
    kept_for(db, sources, path, RECHECK, lasts).await
}

async fn kept(db: &Db, sources: &Sources, path: &str, lasts: impl Fn(&str) -> bool) -> Result<String> {
    kept_opt(db, sources, path, lasts).await?.with_context(|| format!("RGL has no {path}"))
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
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

/// A season's page on RGL: its name and maps, its divisions' group ids
/// with their order (Admin Placement and Dead Teams at 0), and its matches.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeasonHead {
    name: String,
    #[serde(default)]
    maps: Vec<String>,
    #[serde(default)]
    division_sorting: HashMap<String, i64>,
    #[serde(default)]
    matches_played_during_season: Vec<i64>,
}

impl SeasonHead {
    /// The divisions' group ids, best first (ties by id, so it is the same
    /// every time: LG-12).
    fn groups(&self) -> Vec<i64> {
        let mut g: Vec<(i64, i64)> = self.division_sorting.iter().filter(|(_, order)| **order > 0).filter_map(|(id, order)| Some((*order, id.parse().ok()?))).collect();
        g.sort_by_key(|&(order, id)| (-order, id));
        g.into_iter().map(|(_, id)| id).collect()
    }

    /// The best division's group: the highest order.
    fn top_group(&self) -> Option<i64> {
        self.groups().first().copied()
    }
}

/// A season's divisions, as `/seasons/{id}/groups` lists them.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Group {
    group_id: i64,
    group_name: String,
    #[serde(default)]
    sort_order: i64,
}

fn groups_of(body: &str) -> Result<Vec<Group>> {
    serde_json::from_str(body).context("reading RGL's divisions")
}

/// A division's teams, as RGL lists them.
fn teams_of(body: &str) -> Result<Vec<RglTeam>> {
    serde_json::from_str(body).context("reading RGL's teams")
}

/// A division whose winner is decided.
fn decided_division(body: &str) -> bool {
    teams_of(body).is_ok_and(|teams| teams.iter().any(|t| t.final_rank == Some(1)))
}

/// A division that has nothing left to decide: a winner, or no teams.
fn finished_division(body: &str) -> bool {
    teams_of(body).is_ok_and(|teams| teams.is_empty() || teams.iter().any(|t| t.final_rank == Some(1)))
}

/// Whether the season is over: every division is decided, not the best
/// alone (LG-1: the lower divisions' playoffs run after it ends). The best
/// division is read first, and the rest only once it is decided.
async fn season_over(db: &Db, sources: &Sources, head: &SeasonHead) -> Result<bool> {
    let groups = head.groups();
    let Some(top) = groups.first() else { return Ok(false) };
    if !decided_division(&kept(db, sources, &format!("/groups/{top}/teams"), decided_division).await?) {
        return Ok(false);
    }
    for g in &groups[1..] {
        if !finished_division(&kept(db, sources, &format!("/groups/{g}/teams"), decided_division).await?) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A season's page, and whether the season is over. A page kept on disk is
/// over (and looked at again after a month); otherwise it is read, and kept
/// once every division is decided.
async fn season_head(db: &Db, sources: &Sources, id: i64) -> Result<(SeasonHead, bool)> {
    let path = format!("/seasons/{id}");
    if let Some((body, at)) = db.rgl_answers(std::slice::from_ref(&path)).await?.get(&path) {
        if unix_now() - at < RECHECK {
            return Ok((serde_json::from_str(body).context("reading RGL's season")?, true));
        }
    }
    let mut head: SeasonHead = serde_json::from_str(&get(sources, &path).await?).context("reading RGL's season")?;
    if head.division_sorting.is_empty() {
        // A page without its divisions' order: the divisions' own list has it.
        let groups = groups_of(&get(sources, &format!("/seasons/{id}/groups")).await?)?;
        head.division_sorting = groups.iter().map(|g| (g.group_id.to_string(), g.sort_order)).collect();
    }
    let over = season_over(db, sources, &head).await?;
    if over {
        db.store_rgl_answer(&path, &serde_json::to_string(&head)?).await?;
    } else {
        db.drop_rgl_answer(&path).await?;
    }
    Ok((head, over))
}

/// One season: its maps and each division's teams.
pub async fn season(db: &Db, sources: &Sources, id: i64) -> Result<RglSeason> {
    let (head, over) = season_head(db, sources, id).await?;
    // Its page is kept once the season is over; so are its divisions then.
    let mut groups = groups_of(&kept(db, sources, &format!("/seasons/{id}/groups"), |_| over).await?)?;
    // Admin Placement and Dead Teams are not divisions.
    groups.retain(|g| g.sort_order > 0);
    groups.sort_by_key(|g| (-g.sort_order, g.group_id));
    let mut divisions = Vec::new();
    for g in groups {
        let mut teams = teams_of(&kept(db, sources, &format!("/groups/{}/teams", g.group_id), |b| over || decided_division(b)).await?)?;
        // RGL's "Bye Week - Invite" is a placeholder for the odd team out.
        teams.retain(|t| !t.name.starts_with("Bye Week"));
        teams.sort_by_key(|t| (t.final_rank.unwrap_or(99), t.seeding.unwrap_or(999), t.name.to_lowercase()));
        divisions.push(RglDivision { tier: tier_of(&g.group_name), name: g.group_name, teams });
    }
    Ok(RglSeason { id, name: head.name, maps: head.maps, divisions })
}

/// A season's card on the Seasons page: its best division's top three, from
/// that division alone (two requests a season, then none once it is over).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglSeasonCard {
    pub id: i64,
    pub maps: usize,
    /// Teams in its best division only (`division`), not the season's.
    pub teams: usize,
    /// The best division's name ("Invite"), which `teams` counts.
    pub division: Option<String>,
    /// Every division decided.
    pub over: bool,
    /// Matches played or scheduled so far; none means it has not started.
    pub matches: usize,
    /// The best division's top three, best first; empty while it is played.
    pub podium: Vec<RglTeam>,
}

pub async fn season_card(db: &Db, sources: &Sources, id: i64) -> Result<RglSeasonCard> {
    let (head, over) = season_head(db, sources, id).await?;
    let mut card = RglSeasonCard { id, maps: head.maps.len(), teams: 0, division: None, over, matches: head.matches_played_during_season.len(), podium: Vec::new() };
    let Some(top) = head.top_group() else { return Ok(card) };
    card.division = groups_of(&kept(db, sources, &format!("/seasons/{id}/groups"), |_| over).await?).ok().and_then(|g| g.into_iter().find(|g| g.group_id == top)).map(|g| g.group_name);
    let mut teams = teams_of(&kept(db, sources, &format!("/groups/{top}/teams"), decided_division).await?)?;
    teams.retain(|t| !t.name.starts_with("Bye Week"));
    card.teams = teams.len();
    teams.retain(|t| matches!(t.final_rank, Some(1..=3)));
    teams.sort_by_key(|t| t.final_rank);
    card.podium = teams;
    Ok(card)
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
    /// "Advanced", as RGL names the division.
    #[serde(default)]
    pub division_name: Option<String>,
    #[serde(default)]
    pub season_id: Option<i64>,
    /// "HL Season 25".
    #[serde(default)]
    pub season_name: Option<String>,
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

#[derive(Deserialize, Clone)]
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

/// A team, from disk once its season is over. A team that dropped out
/// mid-season has a final rank too, so the rank alone proves nothing
/// (LG-2); and the page lists the team's later seasons, so even a kept one
/// is read again after a week.
async fn team_raw(db: &Db, sources: &Sources, id: i64) -> Result<Option<TeamRaw>> {
    let path = format!("/teams/{id}");
    if let Some((body, at)) = db.rgl_answers(std::slice::from_ref(&path)).await?.get(&path) {
        if unix_now() - at < RECHECK_TEAM {
            return Ok(Some(serde_json::from_str(body).context("reading RGL's team")?));
        }
    }
    let Some(body) = get_opt(sources, &path).await? else { return Ok(None) };
    let t: TeamRaw = serde_json::from_str(&body).context("reading RGL's team")?;
    // Another format's team is never shown, and its season never changes.
    let keep = match season_name(sources, t.season_id).await {
        None => true,
        Some(_) => t.final_rank.is_some() && season_head(db, sources, t.season_id).await.is_ok_and(|(_, over)| over),
    };
    if keep {
        db.store_rgl_answer(&path, &body).await?;
    } else {
        db.drop_rgl_answer(&path).await?;
    }
    Ok(Some(t))
}

/// A season's name, from NA Highlander's list; `None` for another format's.
async fn season_name(sources: &Sources, id: i64) -> Option<String> {
    seasons(sources).await.ok()?.into_iter().find(|s| s.id == id).map(|s| s.name)
}

/// A team: its roster, its matches one by one (RGL's match list is broken on
/// its side; a season is about ten) and the ids of its other seasons.
pub async fn team(db: &Db, sources: &Sources, id: i64) -> Result<Option<RglTeamPage>> {
    let Some(t) = team_raw(db, sources, id).await? else { return Ok(None) };
    let mut matches = read_matches(db, sources, &t.match_ids).await?;
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
pub async fn team_seasons(db: &Db, sources: &Sources, ids: Vec<i64>) -> Result<Vec<RglTeamSeason>> {
    let mut out = Vec::new();
    // The newest teams first, so the cut at 30 drops the oldest seasons; an
    // unreadable season list is an error, not an empty panel.
    let mut ids = ids;
    ids.sort_by_key(|id| std::cmp::Reverse(*id));
    let names: HashMap<i64, String> = seasons(sources).await?.into_iter().map(|s| (s.id, s.name)).collect();
    for id in ids.into_iter().take(30) {
        let Some(t) = team_raw(db, sources, id).await? else { continue };
        let Some(season_name) = names.get(&t.season_id).cloned() else { continue };
        out.push(RglTeamSeason { id: t.team_id, name: t.name, tag: t.tag, season_id: t.season_id, season_name, division: t.division_name, final_rank: t.final_rank });
    }
    out.sort_by_key(|t| -t.season_id);
    Ok(out)
}

// ---- A team across its seasons -------------------------------------------------

/// Wins and losses, forfeits apart: a forfeit says nothing about how a team
/// plays, so it is counted on its own and left out of the win rate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglRecord {
    pub won: i64,
    pub lost: i64,
    pub forfeits_won: i64,
    pub forfeits_lost: i64,
}

/// One season of a team, with its record there.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglHistorySeason {
    pub team_id: i64,
    pub name: String,
    pub tag: Option<String>,
    pub season_id: i64,
    pub season_name: String,
    pub division: Option<String>,
    pub tier: Option<&'static str>,
    pub final_rank: Option<i64>,
    pub record: RglRecord,
}

/// The record against one opponent, in every season.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglOpponent {
    pub name: String,
    pub tag: Option<String>,
    pub won: i64,
    pub lost: i64,
    /// The latest id this opponent played under, to open its page.
    pub team_id: i64,
    /// When they last met (RGL's time).
    pub last: Option<String>,
}

/// A team's record on one map, across its seasons.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglMapRecord {
    pub map: String,
    pub won: i64,
    pub lost: i64,
    pub rounds_for: i64,
    pub rounds_against: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglTeamHistory {
    /// Every id the team played under in the seasons read, for matching its
    /// matches against another team's.
    pub team_ids: Vec<i64>,
    /// Newest first.
    pub seasons: Vec<RglHistorySeason>,
    pub total: RglRecord,
    /// Names the team had before, newest first, without the current one.
    pub former_names: Vec<String>,
    /// The oldest season read.
    pub first_season: Option<String>,
    /// More seasons are linked than were read: totals and "first season" cover
    /// the newest ones only.
    pub capped: bool,
    /// The most played first, at most twelve.
    pub opponents: Vec<RglOpponent>,
    pub maps: Vec<RglMapRecord>,
    /// Every decided match of the seasons read, newest first.
    pub matches: Vec<RglMatch>,
}

/// How many seasons a team's history reads at most. Each season is a team
/// page and about a dozen matches, every one a request the first time (kept
/// on disk once decided), so a team that has played thirty seasons is read
/// for its newest twelve rather than made to wait ten minutes.
const HISTORY_SEASONS: usize = 12;
/// Opponents listed.
const OPPONENTS_SHOWN: usize = 12;

/// A season read: the team as it was then, and its decided matches.
struct SeasonRead {
    team_id: i64,
    name: String,
    tag: Option<String>,
    season_id: i64,
    season_name: String,
    division: Option<String>,
    final_rank: Option<i64>,
    matches: Vec<RglMatch>,
}

fn add_result(r: &mut RglRecord, m: &RglMatch, us: i64) {
    let Some(w) = m.winner else { return };
    match (m.is_forfeit, w == us) {
        (false, true) => r.won += 1,
        (false, false) => r.lost += 1,
        (true, true) => r.forfeits_won += 1,
        (true, false) => r.forfeits_lost += 1,
    }
}

/// The matches the page counts: RGL's "Bye Week" is a place holder, not a game.
fn is_game(m: &RglMatch) -> bool {
    !m.teams.iter().any(|x| x.team_name.starts_with("Bye Week"))
}

/// The history from what was read, newest season first. `current` is the
/// name the page shows. An opponent is the same team in every season by its
/// name, trimmed and in lower case, because RGL gives a team a new id each
/// season.
fn build_history(current: &str, mut reads: Vec<SeasonRead>, capped: bool) -> RglTeamHistory {
    reads.sort_by_key(|r| std::cmp::Reverse(r.season_id));
    let mut total = RglRecord::default();
    let mut seasons = Vec::new();
    let mut opponents: HashMap<String, RglOpponent> = HashMap::new();
    let mut maps: HashMap<String, RglMapRecord> = HashMap::new();
    let mut matches = Vec::new();
    let mut former: Vec<String> = Vec::new();
    for r in &reads {
        let mut rec = RglRecord::default();
        let mut rows: Vec<&RglMatch> = r.matches.iter().filter(|m| decided(m) && is_game(m)).collect();
        rows.sort_by(|a, b| a.match_date.cmp(&b.match_date));
        for m in rows {
            add_result(&mut rec, m, r.team_id);
            matches.push(m.clone());
            if m.is_forfeit || m.winner.is_none() {
                continue;
            }
            let won = m.winner == Some(r.team_id);
            if let Some(o) = m.teams.iter().find(|x| x.team_id != r.team_id) {
                let key = o.team_name.trim().to_lowercase();
                let e = opponents.entry(key).or_insert_with(|| RglOpponent { name: o.team_name.clone(), tag: o.team_tag.clone(), won: 0, lost: 0, team_id: o.team_id, last: None });
                if won {
                    e.won += 1;
                } else {
                    e.lost += 1;
                }
                // The name, tag and id shown are the latest meeting's.
                if m.match_date > e.last {
                    e.last = m.match_date.clone();
                    e.team_id = o.team_id;
                    e.name = o.team_name.clone();
                    e.tag = o.team_tag.clone();
                }
            }
            let home = m.teams.iter().find(|x| x.team_id == r.team_id).is_none_or(|x| x.is_home);
            for x in &m.maps {
                let (Some(h), Some(a)) = (x.home_score, x.away_score) else { continue };
                let (mine, theirs) = if home { (h, a) } else { (a, h) };
                let e = maps.entry(x.map_name.clone()).or_insert_with(|| RglMapRecord { map: x.map_name.clone(), won: 0, lost: 0, rounds_for: 0, rounds_against: 0 });
                e.rounds_for += mine;
                e.rounds_against += theirs;
                // A drawn map is neither a win nor a loss.
                if mine > theirs {
                    e.won += 1;
                } else if mine < theirs {
                    e.lost += 1;
                }
            }
        }
        total.won += rec.won;
        total.lost += rec.lost;
        total.forfeits_won += rec.forfeits_won;
        total.forfeits_lost += rec.forfeits_lost;
        let n = r.name.trim();
        if !n.eq_ignore_ascii_case(current.trim()) && !former.iter().any(|f| f.eq_ignore_ascii_case(n)) {
            former.push(n.to_string());
        }
        seasons.push(RglHistorySeason {
            team_id: r.team_id,
            name: r.name.clone(),
            tag: r.tag.clone(),
            season_id: r.season_id,
            season_name: r.season_name.clone(),
            tier: r.division.as_deref().and_then(tier_of),
            division: r.division.clone(),
            final_rank: r.final_rank,
            record: rec,
        });
    }
    matches.sort_by(|a, b| b.match_date.cmp(&a.match_date).then(b.match_id.cmp(&a.match_id)));
    let mut opponents: Vec<RglOpponent> = opponents.into_values().collect();
    opponents.sort_by(|a, b| (b.won + b.lost).cmp(&(a.won + a.lost)).then(b.last.cmp(&a.last)).then(a.name.cmp(&b.name)));
    opponents.truncate(OPPONENTS_SHOWN);
    let mut maps: Vec<RglMapRecord> = maps.into_values().collect();
    maps.sort_by(|a, b| (b.won + b.lost).cmp(&(a.won + a.lost)).then(a.map.cmp(&b.map)));
    RglTeamHistory {
        team_ids: reads.iter().map(|r| r.team_id).collect(),
        first_season: reads.last().map(|r| r.season_name.clone()),
        seasons,
        total,
        former_names: former,
        capped,
        opponents,
        maps,
        matches,
    }
}

/// A team across its linked seasons: records, titles, former names, most
/// played opponents and map pool. RGL requests, all through the helpers
/// above: one for the season list (shared with the rest of the tab), then
/// per season one `/teams/{id}` and one `/matches/{id}` per match, with the
/// finished-or-not check of the season on top. A finished season is all on
/// disk afterwards, so a team with N seasons (N at most 12) costs roughly 13
/// requests a season the first time, at four a second, and none the second
/// time; the live season's decided matches are on disk as well, only its
/// matches still to be played are asked for again, as on the team page,
/// whose own reads (the same ids) this one shares.
pub async fn team_history(db: &Db, sources: &Sources, id: i64) -> Result<Option<RglTeamHistory>> {
    let Some(now) = team_raw(db, sources, id).await? else { return Ok(None) };
    let mut ids = now.linked_teams.clone();
    ids.push(id);
    ids.sort_by_key(|i| std::cmp::Reverse(*i));
    ids.dedup();
    let capped = ids.len() > HISTORY_SEASONS;
    if capped {
        ids.truncate(HISTORY_SEASONS);
        // The team being looked at is always in its own history.
        if !ids.contains(&id) {
            ids.pop();
            ids.push(id);
        }
    }
    let names: HashMap<i64, String> = seasons(sources).await?.into_iter().map(|s| (s.id, s.name)).collect();
    let mut reads = Vec::new();
    for tid in ids {
        let Some(t) = (if tid == id { Some(now.clone()) } else { team_raw(db, sources, tid).await? }) else { continue };
        // Another format's season is not NA Highlander's.
        let Some(season_name) = names.get(&t.season_id).cloned() else { continue };
        let matches = read_matches(db, sources, &t.match_ids).await?.into_iter().filter(decided).collect();
        reads.push(SeasonRead { team_id: t.team_id, name: t.name, tag: t.tag, season_id: t.season_id, season_name, division: t.division_name, final_rank: t.final_rank, matches });
    }
    Ok(Some(build_history(&now.name, reads, capped)))
}

// ---- A season's matches, and the league's bans -------------------------------

/// Whether a match is over, so its answer will not change: it has a winner,
/// or was forfeited.
fn decided(m: &RglMatch) -> bool {
    m.winner.is_some() || m.is_forfeit
}

/// Whether a decided match is also settled enough to keep for good: RGL
/// forfeited it, or its map scores are in, or it is old. A winner named
/// before the scores are entered is still liable to change (LG-3).
fn final_match(m: &RglMatch) -> bool {
    if !decided(m) {
        return false;
    }
    if m.is_forfeit || m.maps.iter().any(|x| x.home_score.is_some() || x.away_score.is_some()) {
        return true;
    }
    m.match_date.as_deref().and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok()).is_some_and(|d| unix_now() - d.timestamp() > 14 * 24 * 3600)
}

/// A match still to be played is asked for again after this long, whatever
/// RGL's changes say; sooner when they cannot be read.
const PENDING_LIFE: i64 = 6 * 3600;
const PENDING_LIFE_UNCHECKED: i64 = 30 * 60;
/// Rows a page of RGL's match list holds, and how many pages are read: more
/// changes than that and every match still to be played is asked for again.
const CHANGES_PAGE: usize = 1000;
const CHANGES_PAGES: usize = 5;

/// The matches RGL changed since `since` (unix s), every format's: one
/// request a page, held for a minute. `None` when the list cannot be read.
async fn changed_since(sources: &Sources, since: i64) -> Option<HashSet<i64>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        match_id: i64,
    }
    // A margin for the two clocks (an hour: a PC clock further off than ten
    // minutes missed changes, LG-10), and rounded so that pages asked a
    // minute apart are one answer.
    let since = (since - 3600).div_euclid(600) * 600;
    let at = chrono::DateTime::from_timestamp(since, 0)?.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut out = HashSet::new();
    for page in 0..CHANGES_PAGES {
        let path = format!("/matches/paged?take={CHANGES_PAGE}&skip={}", page * CHANGES_PAGE);
        let body = post_opt(sources, &path, serde_json::json!({ "updatedSince": at }), KEEP_MATCH).await.ok()??;
        let rows: Vec<Row> = serde_json::from_str(&body).ok()?;
        let full = rows.len() == CHANGES_PAGE;
        out.extend(rows.into_iter().map(|r| r.match_id));
        if !full {
            return Some(out);
        }
    }
    tracing::info!("RGL changed more matches than its list is read for; asking for each again");
    None
}

/// Where, in `rgl_answer`, the time RGL's changes were last read up to is
/// kept.
const CHANGES_SEEN: &str = "/internal/changes-seen";
/// RGL's changes are read for the decided matches at most this often.
const CHANGES_EVERY: Duration = Duration::from_secs(10 * 60);
static CHANGES_READ: Mutex<Option<Instant>> = Mutex::new(None);

/// RGL corrects matches it has decided (a forfeit overturned, a score
/// edited), so the decided matches it changed since the last look are
/// forgotten, to be read once more (LG-3). One request, every ten minutes
/// at most, however many matches are read.
async fn forget_changed(db: &Db, sources: &Sources) -> Result<()> {
    {
        let mut last = CHANGES_READ.lock().unwrap();
        if last.is_some_and(|t| t.elapsed() < CHANGES_EVERY) {
            return Ok(());
        }
        *last = Some(Instant::now());
    }
    let now = unix_now();
    let seen = db.rgl_answers(&[CHANGES_SEEN.to_string()]).await?.remove(CHANGES_SEEN).and_then(|(body, _)| body.parse::<i64>().ok());
    // The first look has nothing to compare with: start from now.
    if let Some(seen) = seen {
        let Some(changed) = changed_since(sources, seen).await else { return Ok(()) };
        let ids: Vec<i64> = changed.into_iter().collect();
        db.drop_rgl_matches(&ids).await?;
    }
    db.store_rgl_answer(CHANGES_SEEN, &now.to_string()).await
}

/// Whether a match still to be played, kept `age` seconds ago, is used as
/// kept: RGL did not change it since (`changed`, when that could be read),
/// and it is not older than six hours, or half an hour unchecked.
fn still_good(id: i64, age: i64, changed: Option<&HashSet<i64>>) -> bool {
    match changed {
        Some(c) => !c.contains(&id) && age < PENDING_LIFE,
        None => age < PENDING_LIFE_UNCHECKED,
    }
}

/// These matches: the decided ones from disk, those still to be played from
/// disk unless RGL changed them since, the rest from RGL at its pace. A
/// newly decided one is kept for good, one still to be played until next.
async fn read_matches(db: &Db, sources: &Sources, ids: &[i64]) -> Result<Vec<RglMatch>> {
    forget_changed(db, sources).await?;
    let kept = db.rgl_matches(ids).await?;
    let path = |id: i64| format!("/matches/{id}");
    let rest: Vec<String> = ids.iter().filter(|id| !kept.contains_key(id)).map(|&id| path(id)).collect();
    let pending = db.rgl_answers(&rest).await?;
    let now = unix_now();
    let recent = pending.values().map(|(_, at)| *at).filter(|at| now - at < PENDING_LIFE);
    let changed = match recent.min() {
        Some(oldest) => changed_since(sources, oldest).await,
        None => None,
    };
    let mut out = Vec::with_capacity(ids.len());
    for &id in ids {
        if let Some(m) = kept.get(&id).and_then(|body| serde_json::from_str::<RglMatch>(body).ok()) {
            out.push(m);
            continue;
        }
        if let Some((body, at)) = pending.get(&path(id)) {
            if still_good(id, now - at, changed.as_ref()) {
                if let Ok(m) = serde_json::from_str::<RglMatch>(body) {
                    out.push(m);
                    continue;
                }
            }
        }
        let at = path(id);
        if changed.as_ref().is_some_and(|c| c.contains(&id)) {
            forget(&at);
        }
        let Some(body) = once(&at, KEEP_MATCH, || sources.rgl_get(&at)).await? else { continue };
        let m: RglMatch = serde_json::from_str(&body).context("reading RGL's match")?;
        if final_match(&m) {
            db.store_rgl_match(id, &body).await?;
            db.drop_rgl_answer(&path(id)).await?;
        } else {
            db.store_rgl_answer(&path(id), &body).await?;
        }
        out.push(m);
    }
    Ok(out)
}

/// One match, for its own page.
pub async fn one_match(db: &Db, sources: &Sources, id: i64) -> Result<Option<RglMatch>> {
    Ok(read_matches(db, sources, &[id]).await?.into_iter().next())
}

/// Every match of a season, oldest first: from disk once decided, else read
/// one by one at RGL's pace (about 40 s for a whole Highlander season).
pub async fn season_matches(db: &Db, sources: &Sources, id: i64) -> Result<Vec<RglMatch>> {
    let (head, _) = season_head(db, sources, id).await?;
    let mut out = read_matches(db, sources, &head.matches_played_during_season).await?;
    out.sort_by(|a, b| a.match_date.cmp(&b.match_date).then(a.match_id.cmp(&b.match_id)));
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RglBan {
    pub steam_id: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    /// Far in the future (3026) for a permanent ban.
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// How many bans a page of the list holds.
pub const BANS_PAGE: i64 = 50;

/// The league's bans, newest first, a page at a time from `skip`.
pub async fn bans(sources: &Sources, skip: i64) -> Result<Vec<RglBan>> {
    let body = get(sources, &format!("/bans/paged?take={BANS_PAGE}&skip={}", skip.max(0))).await?;
    serde_json::from_str(&body).context("reading RGL's bans")
}

// ---- Search by name ------------------------------------------------------------

/// How many of RGL's hits are shown, and so how many are looked up. A name
/// like "Dead" finds teams of every format and region first, so a team
/// search reads more (`TEAM_HITS`), each kept on disk or in memory.
const HITS: usize = 8;
const TEAM_HITS: usize = 20;

#[derive(Deserialize)]
struct Hits {
    #[serde(default)]
    results: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglPlayerHit {
    pub steam_id: String,
    pub name: String,
    pub avatar: Option<String>,
}

/// Players whose RGL name holds `q`: RGL's search gives SteamIDs, and one
/// bulk request their names.
pub async fn search_players(sources: &Sources, q: &str) -> Result<Vec<RglPlayerHit>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Profile {
        steam_id: String,
        name: String,
        #[serde(default)]
        avatar: Option<String>,
    }
    let q = q.trim();
    if q.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let Some(body) = post_opt(sources, &format!("/search/players?take={HITS}&skip=0"), serde_json::json!({ "nameContains": q }), KEEP).await? else { return Ok(Vec::new()) };
    let ids: Vec<String> = serde_json::from_str::<Hits>(&body).context("reading RGL's search")?.results.into_iter().take(HITS).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let Some(body) = post_opt(sources, "/profile/getmany", serde_json::json!(ids), KEEP).await? else { return Ok(Vec::new()) };
    let profiles: Vec<Profile> = serde_json::from_str(&body).context("reading RGL's profiles")?;
    // In the search's order.
    let mut by_id: HashMap<String, Profile> = profiles.into_iter().map(|p| (p.steam_id.clone(), p)).collect();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).map(|p| RglPlayerHit { steam_id: p.steam_id, name: p.name, avatar: p.avatar }).collect())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglTeamHit {
    pub id: i64,
    pub name: String,
    pub tag: Option<String>,
    pub season_id: i64,
    pub season_name: String,
    pub division: Option<String>,
    pub final_rank: Option<i64>,
}

/// NA Highlander teams whose name or tag holds `q`, newest season first:
/// RGL's search gives team ids of every format, each looked up (from disk
/// once its season is over).
pub async fn search_teams(db: &Db, sources: &Sources, q: &str) -> Result<Vec<RglTeamHit>> {
    let q = q.trim();
    if q.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let Some(body) = post_opt(sources, &format!("/search/teams?take={TEAM_HITS}&skip=0"), serde_json::json!({ "nameContains": q }), KEEP).await? else { return Ok(Vec::new()) };
    let ids: Vec<i64> = serde_json::from_str::<Hits>(&body).context("reading RGL's search")?.results.iter().filter_map(|id| id.parse().ok()).take(TEAM_HITS).collect();
    let names: HashMap<i64, String> = seasons(sources).await?.into_iter().map(|s| (s.id, s.name)).collect();
    let mut out = Vec::new();
    for id in ids {
        // One team that cannot be read does not end the search.
        let Ok(Some(t)) = team_raw(db, sources, id).await else { continue };
        let Some(season_name) = names.get(&t.season_id) else { continue };
        out.push(RglTeamHit { id: t.team_id, name: t.name, tag: t.tag, season_id: t.season_id, season_name: season_name.clone(), division: t.division_name, final_rank: t.final_rank });
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

// ---- A match's lineups, earlier meetings and logs ------------------------------

/// What a match page shows beyond RGL's match: who was on each team then,
/// the two teams' earlier meetings, and the match's logs (Flashy: "up to
/// parity with etf2l"; the ETF2L match page has all three).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglMatchMore {
    /// Each team's roster on the match's date, in `teams` order.
    pub lineups: Vec<RglLineup>,
    /// Earlier matches between the two teams (this season and the seasons
    /// either team is linked to), newest first.
    pub meetings: Vec<RglMatch>,
    pub logs: Vec<RglMatchLog>,
    /// Why there are no logs: "notPlayed", "forfeit", "tooSoon" (an hour
    /// after the match, uploads may still be coming), "resting" (logs.tf is
    /// refusing us for now), "unavailable" (logs.tf did not answer),
    /// "noRoster", or `None` when they were looked for.
    pub logs_why: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglLineup {
    pub team_id: i64,
    /// The team's ids in the seasons read for meetings, its own first: a
    /// team has a new id each season, so this says which side of an earlier
    /// meeting it was.
    pub ids: Vec<i64>,
    pub players: Vec<RglMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RglMatchLog {
    pub log_id: i64,
    pub title: String,
    pub map: String,
    pub played_at: i64,
    /// In the app already, so it opens without a download.
    #[serde(default)]
    pub stored: bool,
}

/// How many of a team's other seasons are read for earlier meetings: each is
/// one request the first time, kept on disk after.
const MEETING_SEASONS: usize = 4;
/// How soon after the match its logs are looked for, and how long an empty
/// search is not kept: logs are uploaded during and after the match.
const LOGS_AFTER_S: i64 = 3600;
const LOGS_SETTLED_S: i64 = 3 * 24 * 3600;

fn rfc3339(s: Option<&str>) -> Option<i64> {
    s.and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok()).map(|d| d.timestamp())
}

/// Who was on the team at `at`: joined by then and not yet left. Without a
/// date, whoever is on it now.
fn roster_at(players: &[RglMember], at: Option<i64>) -> Vec<RglMember> {
    let mut out: Vec<RglMember> = players
        .iter()
        .filter(|p| match at {
            Some(at) => rfc3339(p.joined_at.as_deref()).is_none_or(|j| j <= at) && rfc3339(p.left_at.as_deref()).is_none_or(|l| l > at),
            None => p.left_at.is_none(),
        })
        .cloned()
        .collect();
    // Leaders first, then by name, so the list reads the same every time.
    out.sort_by(|a, b| b.is_leader.cmp(&a.is_leader).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then_with(|| a.steam_id.cmp(&b.steam_id)));
    out
}

/// The matches two teams both played, from their ids alone: no match is read
/// to find them.
fn shared(a: &HashSet<i64>, b: &HashSet<i64>, not: i64) -> Vec<i64> {
    let mut ids: Vec<i64> = a.intersection(b).copied().filter(|&id| id != not).collect();
    ids.sort_unstable();
    ids
}

/// A team's match ids, with those of up to [`MEETING_SEASONS`] of its other
/// seasons (the newest: RGL's ids grow), and the team ids they were read for.
async fn match_ids_with_linked(db: &Db, sources: &Sources, team: &TeamRaw) -> (HashSet<i64>, Vec<i64>) {
    let mut ids: HashSet<i64> = team.match_ids.iter().copied().collect();
    let mut teams = vec![team.team_id];
    let mut linked: Vec<i64> = team.linked_teams.iter().copied().filter(|&l| l != team.team_id).collect();
    linked.sort_unstable_by(|a, b| b.cmp(a));
    for l in linked.into_iter().take(MEETING_SEASONS) {
        // One season that will not read costs that season's meetings only.
        if let Ok(Some(t)) = team_raw(db, sources, l).await {
            ids.extend(t.match_ids);
            teams.push(l);
        }
    }
    (ids, teams)
}

/// Where a match's logs search is kept, in `rgl_answer`.
fn logs_key(id: i64) -> String {
    format!("/internal/match-logs/{id}")
}

/// A match's lineups, earlier meetings and logs. RGL is asked for the two
/// teams (kept on disk once their season is over) and a few of their other
/// seasons; logs.tf once, for a played match, and that answer is kept.
pub async fn match_more(db: &Db, sources: &Sources, id: i64) -> Result<Option<RglMatchMore>> {
    let Some(m) = one_match(db, sources, id).await? else { return Ok(None) };
    let at = rfc3339(m.match_date.as_deref());

    let mut raws = Vec::new();
    for t in &m.teams {
        // Best effort, like each panel it feeds: a team RGL will not give
        // costs its lineup and the meetings, not the page.
        raws.push(team_raw(db, sources, t.team_id).await.unwrap_or_else(|e| {
            tracing::warn!(team = t.team_id, error = %format!("{e:#}"), "RGL team for a match page");
            None
        }));
    }
    let mut lineups: Vec<RglLineup> = m
        .teams
        .iter()
        .zip(&raws)
        .map(|(t, raw)| RglLineup { team_id: t.team_id, ids: vec![t.team_id], players: raw.as_ref().map(|r| roster_at(&r.players, at)).unwrap_or_default() })
        .collect();

    let mut meetings = Vec::new();
    if let [Some(a), Some(b)] = raws.as_slice() {
        let (a_matches, a_ids) = match_ids_with_linked(db, sources, a).await;
        let (b_matches, b_ids) = match_ids_with_linked(db, sources, b).await;
        lineups[0].ids = a_ids;
        lineups[1].ids = b_ids;
        let ids = shared(&a_matches, &b_matches, id);
        meetings = read_matches(db, sources, &ids).await.unwrap_or_else(|e| {
            tracing::warn!(match_id = id, error = %format!("{e:#}"), "RGL meetings for a match page");
            Vec::new()
        });
        meetings.sort_by(|x, y| y.match_date.cmp(&x.match_date).then(y.match_id.cmp(&x.match_id)));
    }

    let (mut logs, logs_why) = match_logs(db, sources, &m, at, &lineups).await?;
    let stored = db.logs_in_matches(&logs.iter().map(|l| l.log_id).collect::<Vec<_>>()).await?;
    for l in &mut logs {
        l.stored = stored.contains(&l.log_id);
    }
    Ok(Some(RglMatchMore { lineups, meetings, logs, logs_why }))
}

/// The match's logs: logs.tf's logs with a player of each team in them,
/// Highlander-sized, within a day and a half of the match, as an ETF2L
/// official's are found (`league_sample::matching_logs`). Searched once and
/// kept; an empty answer only once the match is a few days old, as uploads
/// can come late.
async fn match_logs(db: &Db, sources: &Sources, m: &RglMatch, at: Option<i64>, lineups: &[RglLineup]) -> Result<(Vec<RglMatchLog>, Option<&'static str>)> {
    let key = logs_key(m.match_id);
    if let Some((body, _)) = db.rgl_answers(std::slice::from_ref(&key)).await?.remove(&key) {
        if let Ok(logs) = serde_json::from_str::<Vec<RglMatchLog>>(&body) {
            return Ok((logs, None));
        }
    }
    if m.is_forfeit {
        return Ok((Vec::new(), Some("forfeit")));
    }
    let Some(at) = at.filter(|_| decided(m)) else { return Ok((Vec::new(), Some("notPlayed"))) };
    let now = unix_now();
    if now - at < LOGS_AFTER_S {
        return Ok((Vec::new(), Some("tooSoon")));
    }
    if sources.logstf_resting() {
        return Ok((Vec::new(), Some("resting")));
    }
    // A player from each side: the first listed, a leader where there is one.
    let ids: Vec<String> = lineups.iter().filter_map(|l| l.players.first().map(|p| p.steam_id.clone())).collect();
    if ids.len() < 2 {
        return Ok((Vec::new(), Some("noRoster")));
    }
    let listed = match sources.logstf_with_players(&ids, 40).await {
        Ok(listed) => listed,
        Err(e) => {
            tracing::warn!(match_id = m.match_id, error = %format!("{e:#}"), "logs.tf search for an RGL match");
            return Ok((Vec::new(), Some("unavailable")));
        }
    };
    let found = crate::league_sample::matching_logs(&listed, at);
    let logs: Vec<RglMatchLog> =
        found.into_iter().map(|(log_id, title, map, played_at, _)| RglMatchLog { log_id, title, map, played_at, stored: false }).collect();
    if !logs.is_empty() || now - at > LOGS_SETTLED_S {
        db.store_rgl_answer(&key, &serde_json::to_string(&logs)?).await?;
    }
    Ok((logs, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str, leader: bool, joined: Option<&str>, left: Option<&str>) -> RglMember {
        RglMember { name: name.into(), steam_id: format!("7656{name}"), is_leader: leader, joined_at: joined.map(Into::into), left_at: left.map(Into::into) }
    }

    #[test]
    fn a_lineup_is_who_was_on_the_team_that_day() {
        let at = rfc3339(Some("2026-10-01T02:00:00Z"));
        let players = [
            member("zed", false, Some("2026-09-01T00:00:00Z"), None),
            member("amy", false, Some("2026-09-01T00:00:00Z"), Some("2026-10-15T00:00:00Z")),
            member("lead", true, Some("2026-08-01T00:00:00.000Z"), None),
            member("late", false, Some("2026-10-05T00:00:00Z"), None),
            member("gone", false, Some("2026-08-01T00:00:00Z"), Some("2026-09-20T00:00:00Z")),
            member("nodate", false, None, None),
        ];
        let names: Vec<String> = roster_at(&players, at).into_iter().map(|p| p.name).collect();
        // The leader first, then by name; joined later and left earlier are out.
        assert_eq!(names, ["lead", "amy", "nodate", "zed"]);
        // No match date: whoever has not left.
        let now: Vec<String> = roster_at(&players, None).into_iter().map(|p| p.name).collect();
        assert_eq!(now, ["lead", "late", "nodate", "zed"]);
    }

    #[test]
    fn meetings_are_the_matches_both_teams_played_but_this_one() {
        let a: HashSet<i64> = [1, 2, 3, 9].into();
        let b: HashSet<i64> = [9, 3, 7].into();
        assert_eq!(shared(&a, &b, 9), vec![3]);
        assert!(shared(&a, &HashSet::new(), 9).is_empty());
    }

    #[tokio::test]
    async fn a_kept_logs_search_is_read_back_without_asking() {
        let db = Db::connect_in_memory().await.unwrap();
        let logs = vec![RglMatchLog { log_id: 42, title: "RED vs BLU".into(), map: "pl_upward".into(), played_at: 1, stored: false }];
        db.store_rgl_answer(&logs_key(7), &serde_json::to_string(&logs).unwrap()).await.unwrap();
        let m = RglMatch {
            match_id: 7,
            match_date: None,
            match_name: None,
            division_name: None,
            season_id: None,
            season_name: None,
            is_forfeit: false,
            winner: None,
            teams: vec![],
            maps: vec![],
        };
        // Sources is never touched: a kept answer comes first.
        let sources = Sources::new().unwrap();
        let (got, why) = match_logs(&db, &sources, &m, None, &[]).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].log_id, 42);
        assert_eq!(why, None);
        // Without a kept answer, a match not yet played is not searched.
        let m2 = RglMatch { match_id: 8, ..m };
        assert_eq!(match_logs(&db, &sources, &m2, None, &[]).await.unwrap().1, Some("notPlayed"));
        let forfeit = RglMatch { match_id: 9, is_forfeit: true, ..m2 };
        assert_eq!(match_logs(&db, &sources, &forfeit, None, &[]).await.unwrap().1, Some("forfeit"));
    }

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
    fn only_a_decided_match_is_kept() {
        let live: RglMatch = serde_json::from_str(r#"{"matchId":35600,"divisionName":"Newcomer","matchDate":"2026-11-03T01:30:00.000Z","matchName":"Week 6 - Proot","isForfeit":false,"teams":[],"maps":[]}"#).unwrap();
        assert_eq!(live.division_name.as_deref(), Some("Newcomer"));
        assert!(!decided(&live));
        assert!(decided(&RglMatch { winner: Some(15255), ..live.clone() }));
        assert!(decided(&RglMatch { is_forfeit: true, ..live }));
    }

    /// Kept matches come from disk: no request to RGL (this test has no network).
    #[tokio::test]
    async fn a_kept_season_reads_from_disk() {
        let db = Db::connect_in_memory().await.unwrap();
        db.store_rgl_match(2, r#"{"matchId":2,"matchDate":"2026-06-09T01:30:00.000Z","winner":7,"teams":[],"maps":[]}"#).await.unwrap();
        db.store_rgl_match(1, r#"{"matchId":1,"matchDate":"2026-06-02T01:30:00.000Z","winner":8,"teams":[],"maps":[]}"#).await.unwrap();
        let got = read_matches(&db, &Sources::new().unwrap(), &[1, 2]).await.unwrap();
        assert_eq!(got.iter().map(|m| (m.match_id, m.winner)).collect::<Vec<_>>(), [(1, Some(8)), (2, Some(7))]);
    }

    #[test]
    fn a_seasons_best_division_is_its_highest_order() {
        let head: SeasonHead = serde_json::from_str(r#"{"name":"HL Season 24","divisionSorting":{"1181":10,"1182":9,"1186":0},"maps":[],"matchesPlayedDuringSeason":[34331]}"#).unwrap();
        assert_eq!(head.top_group(), Some(1181));
        assert_eq!(head.matches_played_during_season, [34331]);
    }

    #[test]
    fn divisions_tie_by_id_and_an_old_winnerless_match_is_final() {
        let head: SeasonHead = serde_json::from_str(r#"{"name":"S","divisionSorting":{"1182":9,"1181":9,"1186":0,"1190":10}}"#).unwrap();
        assert_eq!(head.groups(), [1190, 1181, 1182]);
        let m = |date: &str, winner: Option<i64>, scores: bool| RglMatch {
            match_id: 1,
            match_date: Some(date.into()),
            match_name: None,
            division_name: None,
            season_id: None,
            season_name: None,
            is_forfeit: false,
            winner,
            teams: vec![],
            maps: vec![RglMatchMap { map_name: "x".into(), home_score: scores.then_some(2), away_score: None }],
        };
        assert!(!final_match(&m("2999-01-01T00:00:00Z", Some(5), false)), "a winner without scores, today: not yet");
        assert!(final_match(&m("2999-01-01T00:00:00Z", Some(5), true)));
        assert!(final_match(&m("2020-01-01T00:00:00Z", Some(5), false)), "old enough");
        assert!(!final_match(&m("2020-01-01T00:00:00Z", None, true)));
    }

    #[test]
    fn a_division_with_no_teams_has_nothing_to_decide() {
        assert!(finished_division("[]"));
        assert!(!finished_division(r#"[{"teamId":1,"name":"A","finalRank":null}]"#));
    }

    #[test]
    fn a_division_with_a_winner_is_over() {
        assert!(decided_division(r#"[{"teamId":1,"name":"A","finalRank":2},{"teamId":2,"name":"B","finalRank":1}]"#));
        assert!(!decided_division(r#"[{"teamId":1,"name":"A","finalRank":null,"seeding":1}]"#));
    }

    #[test]
    fn a_match_still_to_be_played_is_asked_for_again_only_once_changed() {
        let changed: HashSet<i64> = [35561].into();
        assert!(still_good(35600, 3600, Some(&changed)), "unchanged and an hour old: kept");
        assert!(!still_good(35561, 60, Some(&changed)), "changed: asked again");
        assert!(!still_good(35600, 7 * 3600, Some(&changed)), "six hours at most");
        assert!(still_good(35600, 600, None) && !still_good(35600, 3600, None), "unchecked: half an hour");
    }

    #[tokio::test]
    async fn the_same_answer_is_asked_for_once() {
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let ask = || async {
            asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(Some("body".to_string()))
        };
        let (a, b) = tokio::join!(once("test:once", KEEP, ask), once("test:once", KEEP, ask));
        assert_eq!((a.unwrap(), b.unwrap()), (Some("body".into()), Some("body".into())));
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

fn played(id: i64, date: &str, us: (i64, &str), them: (i64, &str), winner: Option<i64>, forfeit: bool, maps: &[(&str, i64, i64)]) -> RglMatch {
        RglMatch {
            match_id: id,
            match_date: Some(date.into()),
            match_name: None,
            division_name: None,
            season_id: None,
            season_name: None,
            is_forfeit: forfeit,
            winner,
            teams: vec![
                RglMatchTeam { team_id: us.0, team_name: us.1.into(), team_tag: None, is_home: true },
                RglMatchTeam { team_id: them.0, team_name: them.1.into(), team_tag: None, is_home: false },
            ],
            maps: maps.iter().map(|(m, h, a)| RglMatchMap { map_name: (*m).into(), home_score: Some(*h), away_score: Some(*a) }).collect(),
        }
    }

    fn read(team: i64, name: &str, season: i64, rank: Option<i64>, matches: Vec<RglMatch>) -> SeasonRead {
        SeasonRead { team_id: team, name: name.into(), tag: None, season_id: season, season_name: format!("HL Season {season}"), division: Some("Advanced".into()), final_rank: rank, matches }
    }

    #[test]
    fn a_history_counts_each_season_and_keeps_forfeits_apart() {
        let h = build_history(
            "Duds",
            vec![
                read(2, "Duds", 24, Some(1), vec![played(1, "2026-06-01", (2, "Duds"), (9, "Rib"), Some(2), false, &[("pl_vigil", 2, 0)]), played(2, "2026-06-08", (2, "Duds"), (8, "Cats"), Some(8), true, &[])]),
                read(1, "Old Duds", 23, Some(2), vec![played(3, "2026-01-01", (1, "Old Duds"), (7, "RIB "), Some(7), false, &[("pl_vigil", 1, 2)])]),
            ],
            false,
        );
        assert_eq!(h.seasons.iter().map(|s| (s.season_id, s.record.won, s.record.lost, s.record.forfeits_lost)).collect::<Vec<_>>(), [(24, 1, 0, 1), (23, 0, 1, 0)]);
        assert_eq!((h.total.won, h.total.lost, h.total.forfeits_lost), (1, 1, 1));
        assert_eq!(h.former_names, ["Old Duds"]);
        assert_eq!(h.first_season.as_deref(), Some("HL Season 23"));
        assert_eq!(h.team_ids, [2, 1]);
        // Newest first.
        assert_eq!(h.matches.iter().map(|m| m.match_id).collect::<Vec<_>>(), [2, 1, 3]);
    }

    #[test]
    fn an_opponent_is_the_same_team_across_seasons_by_name_and_a_forfeit_is_no_meeting() {
        let h = build_history(
            "Duds",
            vec![
                read(2, "Duds", 24, None, vec![played(1, "2026-06-01", (2, "Duds"), (9, "Run It Back"), Some(2), false, &[]), played(2, "2026-06-08", (2, "Duds"), (8, "Cats"), Some(8), true, &[])]),
                read(1, "Duds", 23, None, vec![played(3, "2026-01-01", (1, "Duds"), (7, " run it back"), Some(7), false, &[])]),
            ],
            false,
        );
        assert_eq!(h.opponents.len(), 1);
        let o = &h.opponents[0];
        assert_eq!((o.won, o.lost, o.team_id, o.name.as_str()), (1, 1, 9, "Run It Back"));
        assert!(h.former_names.is_empty());
    }

    #[test]
    fn the_map_pool_counts_maps_and_rounds_from_the_teams_side() {
        // The team is away in the second match: its rounds are the away score.
        let mut away = played(2, "2026-06-08", (9, "Rib"), (2, "Duds"), Some(2), false, &[("pl_vigil", 0, 2), ("cp_gullywash", 1, 1)]);
        away.teams[0].is_home = true;
        let h = build_history("Duds", vec![read(2, "Duds", 24, None, vec![played(1, "2026-06-01", (2, "Duds"), (9, "Rib"), Some(9), false, &[("pl_vigil", 1, 2)]), away])], false);
        let vigil = h.maps.iter().find(|m| m.map == "pl_vigil").unwrap();
        assert_eq!((vigil.won, vigil.lost, vigil.rounds_for, vigil.rounds_against), (1, 1, 3, 2));
        let gully = h.maps.iter().find(|m| m.map == "cp_gullywash").unwrap();
        assert_eq!((gully.won, gully.lost), (0, 0));
    }

    #[test]
    fn a_bye_week_and_an_undecided_match_are_no_games() {
        let bye = played(1, "2026-06-01", (2, "Duds"), (0, "Bye Week"), Some(2), false, &[]);
        let open = played(2, "2026-06-08", (2, "Duds"), (9, "Rib"), None, false, &[]);
        let h = build_history("Duds", vec![read(2, "Duds", 24, None, vec![bye, open])], true);
        assert_eq!((h.total.won, h.total.lost, h.matches.len(), h.capped), (0, 0, 0, true));
    }

    /// Kept matches and a kept finished team come from disk: no request (this test has no network).
    #[tokio::test]
    async fn a_kept_history_reads_from_disk() {
        let db = Db::connect_in_memory().await.unwrap();
        db.store_rgl_match(5, r#"{"matchId":5,"matchDate":"2026-06-02T01:30:00.000Z","winner":7,"teams":[{"teamId":7,"teamName":"A"},{"teamId":8,"teamName":"B"}],"maps":[]}"#).await.unwrap();
        let got = read_matches(&db, &Sources::new().unwrap(), &[5]).await.unwrap();
        let h = build_history("A", vec![read(7, "A", 24, Some(1), got)], false);
        assert_eq!((h.total.won, h.seasons[0].final_rank), (1, Some(1)));
    }

    #[test]
    fn steamids_read() {
        assert_eq!(steam64("76561198137137702").unwrap(), 76561198137137702);
        assert_eq!(steam64("https://rgl.gg/Public/PlayerProfile?p=76561198137137702&r=24").unwrap(), 76561198137137702);
        assert!(steam64("voosh").is_err());
    }
}
