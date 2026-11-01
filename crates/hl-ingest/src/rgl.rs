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

/// An answer from memory while it is fresh, else asked for: once, however
/// many are waiting for it.
async fn once<F, Fut>(key: &str, life: Duration, ask: F) -> Result<Option<String>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Option<String>>>,
{
    if let Some(body) = held(key) {
        return Ok(Some(body));
    }
    let gate = ASKING.lock().unwrap().get_or_insert_with(HashMap::new).entry(key.to_string()).or_default().clone();
    let turn = gate.lock().await;
    // Whoever held the gate before may have just asked.
    let out = match held(key) {
        Some(body) => Ok(Some(body)),
        None => {
            let out = ask().await;
            if let Ok(Some(body)) = &out {
                hold(key, body, life);
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

/// A path's answer from disk when it was kept, else from RGL; kept from
/// now on when `lasts` says it cannot change any more.
async fn kept_opt(db: &Db, sources: &Sources, path: &str, lasts: impl Fn(&str) -> bool) -> Result<Option<String>> {
    if let Some((body, _)) = db.rgl_answers(&[path.to_string()]).await?.remove(path) {
        return Ok(Some(body));
    }
    let Some(body) = get_opt(sources, path).await? else { return Ok(None) };
    if lasts(&body) {
        db.store_rgl_answer(path, &body).await?;
    }
    Ok(Some(body))
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
#[derive(Deserialize)]
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
    /// The best division's group: the highest order.
    fn top_group(&self) -> Option<i64> {
        self.division_sorting.iter().filter(|(_, order)| **order > 0).max_by_key(|(_, order)| **order).and_then(|(id, _)| id.parse().ok())
    }
}

/// A division's teams, as RGL lists them.
fn teams_of(body: &str) -> Result<Vec<RglTeam>> {
    serde_json::from_str(body).context("reading RGL's teams")
}

/// A division whose winner is decided: its season is over.
fn decided_division(body: &str) -> bool {
    teams_of(body).is_ok_and(|teams| teams.iter().any(|t| t.final_rank == Some(1)))
}

/// A season's page, from disk once it is over. Whether it is over is told
/// by its best division having a winner, read first.
async fn season_head(db: &Db, sources: &Sources, id: i64) -> Result<SeasonHead> {
    let path = format!("/seasons/{id}");
    if let Some((body, _)) = db.rgl_answers(std::slice::from_ref(&path)).await?.get(&path) {
        return serde_json::from_str(body).context("reading RGL's season");
    }
    let body = get(sources, &path).await?;
    let head: SeasonHead = serde_json::from_str(&body).context("reading RGL's season")?;
    if let Some(top) = head.top_group() {
        let teams = kept(db, sources, &format!("/groups/{top}/teams"), decided_division).await?;
        if decided_division(&teams) {
            db.store_rgl_answer(&path, &body).await?;
        }
    }
    Ok(head)
}

/// One season: its maps and each division's teams.
pub async fn season(db: &Db, sources: &Sources, id: i64) -> Result<RglSeason> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Group {
        group_id: i64,
        group_name: String,
        #[serde(default)]
        sort_order: i64,
    }
    let head = season_head(db, sources, id).await?;
    // Its page is kept once the season is over; so are its divisions then.
    let page = format!("/seasons/{id}");
    let over = db.rgl_answers(std::slice::from_ref(&page)).await?.contains_key(&page);
    let mut groups: Vec<Group> = serde_json::from_str(&kept(db, sources, &format!("/seasons/{id}/groups"), |_| over).await?).context("reading RGL's divisions")?;
    // Admin Placement and Dead Teams are not divisions.
    groups.retain(|g| g.sort_order > 0);
    groups.sort_by_key(|g| -g.sort_order);
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
    /// Teams in its best division.
    pub teams: usize,
    /// The best division's top three, best first; empty while it is played.
    pub podium: Vec<RglTeam>,
}

pub async fn season_card(db: &Db, sources: &Sources, id: i64) -> Result<RglSeasonCard> {
    let head = season_head(db, sources, id).await?;
    let mut card = RglSeasonCard { id, maps: head.maps.len(), teams: 0, podium: Vec::new() };
    let Some(top) = head.top_group() else { return Ok(card) };
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

/// A team, from disk once its season placed it.
async fn team_raw(db: &Db, sources: &Sources, id: i64) -> Result<Option<TeamRaw>> {
    let placed = |body: &str| serde_json::from_str::<TeamRaw>(body).is_ok_and(|t| t.final_rank.is_some());
    let Some(body) = kept_opt(db, sources, &format!("/teams/{id}"), placed).await? else { return Ok(None) };
    Ok(Some(serde_json::from_str(&body).context("reading RGL's team")?))
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
    for id in ids.into_iter().take(30) {
        let Some(t) = team_raw(db, sources, id).await? else { continue };
        let Some(season_name) = season_name(sources, t.season_id).await else { continue };
        out.push(RglTeamSeason { id: t.team_id, name: t.name, tag: t.tag, season_id: t.season_id, season_name, division: t.division_name, final_rank: t.final_rank });
    }
    out.sort_by_key(|t| -t.season_id);
    Ok(out)
}

// ---- A season's matches, and the league's bans -------------------------------

/// Whether a match is over, so its answer will not change: it has a winner,
/// or was forfeited.
fn decided(m: &RglMatch) -> bool {
    m.winner.is_some() || m.is_forfeit
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
    // A margin for the two clocks, and rounded so that pages asked a minute
    // apart are one answer.
    let since = (since - 600).div_euclid(600) * 600;
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
        let Some(body) = once(&at, KEEP_MATCH, || sources.rgl_get(&at)).await? else { continue };
        let m: RglMatch = serde_json::from_str(&body).context("reading RGL's match")?;
        if decided(&m) {
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
    let head = season_head(db, sources, id).await?;
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

/// How many of RGL's hits are shown, and so how many are looked up.
const HITS: usize = 8;

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
    let Some(body) = post_opt(sources, &format!("/search/teams?take={HITS}&skip=0"), serde_json::json!({ "nameContains": q }), KEEP).await? else { return Ok(Vec::new()) };
    let ids: Vec<i64> = serde_json::from_str::<Hits>(&body).context("reading RGL's search")?.results.iter().filter_map(|id| id.parse().ok()).take(HITS).collect();
    let names: HashMap<i64, String> = seasons(sources).await?.into_iter().map(|s| (s.id, s.name)).collect();
    let mut out = Vec::new();
    for id in ids {
        let Some(t) = team_raw(db, sources, id).await? else { continue };
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

    #[test]
    fn steamids_read() {
        assert_eq!(steam64("76561198137137702").unwrap(), 76561198137137702);
        assert_eq!(steam64("https://rgl.gg/Public/PlayerProfile?p=76561198137137702&r=24").unwrap(), 76561198137137702);
        assert!(steam64("voosh").is_err());
    }
}
