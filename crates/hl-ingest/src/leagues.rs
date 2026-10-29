//! Teams and seasons (Q29, Flashy): a year of ETF2L Highlander, every team.
//!
//! "Add a teams page per season... look at their win percentages on each map
//! in the map pool. List their best players... profile pages for teams."
//! All of it from ETF2L's own API, which lists each season's divisions,
//! their map pool, results and, per match, who played and each map's score.
//! The player ratings on a team page are the pool's (players who appear in
//! the owner's matches); a whole team's rated history is Q14b.

use crate::sources::Sources;
use anyhow::Result;
use hl_core::maps::map_name;
use hl_core::SteamId;
use hl_db::{CompetitionRow, Db, SeasonMatchRow};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Seasons kept besides the current one: two before it is about a year.
pub const SEASONS_BACK: i64 = 2;
/// Match pages read per sync. ~450 matches a year at ETF2L's 60 a minute
/// would add eight minutes to one sync; spread out, a year fills in over a
/// few, and a live season keeps up easily.
pub const DETAILS_PER_SYNC: i64 = 60;
/// Competition-list pages read at most: a guard, not a limit.
const MAX_LIST_PAGES: i64 = 25;
/// The same guard for an old season's walk, which goes to the list's end
/// (51 pages in October 2026, Season 1 on page 47).
const OLD_LIST_PAGES: i64 = 80;

/// Today's ladder, top first: the tier every division of every era is put
/// on (see [`canonical_tier`]).
pub const TIER_NAMES: [&str; 6] = ["Premiership", "High", "Mid", "Low", "Open", "Fresh Meat"];

/// A division's place on today's ladder, whatever the season called it
/// (Flashy): ETF2L once numbered its divisions, and its own tier numbers
/// do not line up across eras. Division 1 and 2 are High, 3 and 4 Mid, 5
/// Low, 6 Open, 7 Fresh Meat; a group letter ("Division 5A", "Mid B",
/// "Open A") is dropped. `None` for a name that is not a division ("Top
/// Tiers", a cup's label), which falls back to ETF2L's own tier.
pub fn canonical_tier(division: &str) -> Option<i64> {
    let d = division.trim().to_ascii_lowercase();
    // "Division 5A" -> 5; "Div 2" -> 2.
    let number = d
        .strip_prefix("division ")
        .or_else(|| d.strip_prefix("div "))
        .and_then(|n| n.trim().trim_end_matches(|c: char| c.is_ascii_alphabetic()).trim().parse::<i64>().ok());
    if let Some(n) = number {
        return match n {
            1 | 2 => Some(1),
            3 | 4 => Some(2),
            5 => Some(3),
            6 => Some(4),
            7 => Some(5),
            _ => None,
        };
    }
    // "Mid B" -> "mid": the last word, when it is a single letter, is a group.
    let base = match d.rsplit_once(' ') {
        Some((head, last)) if last.len() == 1 && last.chars().all(|c| c.is_ascii_alphabetic()) => head,
        _ => d.as_str(),
    };
    match base {
        "premiership" | "prem" => Some(0),
        "high" => Some(1),
        "mid" => Some(2),
        "low" => Some(3),
        "open" => Some(4),
        "fresh" | "freshest" | "fresh meat" => Some(5),
        _ => None,
    }
}

/// ETF2L's seasons between 27 and 32 carry no number: they are named by the
/// time of year. Their places in the numbering, by date -- Season 27 ended
/// in October 2022 and Season 32 began in July 2024 (Flashy's results page).
const UNNUMBERED: [(&str, i64); 4] = [("Winter 2023", 28), ("Spring 2023", 29), ("Autumn 2023", 30), ("Winter 2024", 31)];

/// Seasons ETF2L ran between two numbered ones, with no number free for
/// them: [`OFF_SEASON`] plus the season they followed. "AFA 2025" came after
/// Season 34 (Summer 2025) and before Season 35 (Spring 2026).
const BETWEEN: [(&str, i64); 1] = [("AFA 2025", OFF_SEASON + 34)];

/// Where the seasons between numbered ones are kept: 134 is the one after
/// Season 34. Never shown as a number; sorted by [`season_order`].
pub const OFF_SEASON: i64 = 100;

/// A season's place in time, for sorting: 134 falls between 34 and 35.
pub fn season_order(season: i64) -> i64 {
    if season >= OFF_SEASON {
        (season - OFF_SEASON) * 2 + 1
    } else {
        season * 2
    }
}

/// A competition's name as `(season, season name, division, stage)`:
/// - `"Highlander Season 36 (Autumn 2026): Open Playoffs"` ->
///   `(36, "Autumn 2026", "Open", "Playoffs")`;
/// - before season 32, no brackets: `"Highlander Season 22: Premiership
///   Qualifiers"` -> season name `"Season 22"`;
/// - between 27 and 32, no number at all: `"Highlander Winter 2024: Low"`,
///   `"Highlander Winter 2023 Premiership"` ([`UNNUMBERED`]);
/// - between two numbered seasons: `"Highlander AFA 2025: Top Tiers"`
///   ([`BETWEEN`]).
///
/// A preseason cup is stage `"Cup"`: its officials are the player's, but it
/// is not the league, so it neither makes a division nor gives a medal.
pub fn parse_name(name: &str) -> Option<(i64, String, String, String)> {
    let rest = name.strip_prefix("Highlander ")?;
    let (season, season_name, rest) = if let Some(rest) = rest.strip_prefix("Season ") {
        let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        let season: i64 = rest[..digits].parse().ok()?;
        let rest = rest[digits..].trim_start();
        match rest.strip_prefix('(') {
            Some(r) => {
                let (n, r) = r.split_once(')')?;
                (season, n.to_string(), r)
            }
            None => (season, format!("Season {season}"), rest),
        }
    } else {
        let (label, season) = UNNUMBERED.iter().chain(BETWEEN.iter()).find(|(label, _)| rest.starts_with(label))?;
        (*season, label.to_string(), &rest[label.len()..])
    };
    let label = rest.trim_start_matches([':', ' ', '-']).trim();
    if label.contains("Preseason Cup") {
        // "Preseason Cup: Low Playoffs" is the Low bracket; a bare cup has none.
        let division = label.split_once(':').map_or("", |(_, d)| d).trim().trim_end_matches("Playoffs").trim().to_string();
        return Some((season, season_name, division, "Cup".to_string()));
    }
    // Seasons 12 and 13 opened with "Preseason playoffs": a qualifier that
    // seeded the Premiership before the season, with a Grand Final of its
    // own that is not the season's (it made a Season MVP of a team that never
    // reached the real one).
    let lower = label.to_ascii_lowercase();
    if lower.starts_with("preseason") && lower.contains("playoff") {
        return Some((season, season_name, label.to_string(), "Qualifiers".to_string()));
    }
    const STAGES: [(&str, &str); 8] = [
        ("Grand Final", "Grand Final"),
        ("3rd Place Match", "3rd Place"),
        ("3rd Place", "3rd Place"),
        ("Playoffs", "Playoffs"),
        ("Final", "Final"),
        ("Relegation", "Relegation"),
        ("Qualifiers", "Qualifiers"),
        ("Signups", "Signups"),
    ];
    let (division, stage) = STAGES
        .iter()
        .find_map(|(suffix, stage)| label.strip_suffix(suffix).map(|d| (d.trim().to_string(), (*stage).to_string())))
        .unwrap_or_else(|| (label.to_string(), "regular".to_string()));
    Some((season, season_name, if division.is_empty() { label.to_string() } else { division }, stage))
}

/// ETF2L's other Highlander tournaments (Flashy: "all highlander
/// tournaments"): the Experimental Cups, one-night cups, the Highlander
/// Open, the Nations' Cup. Not the fun-team brackets. They are named after
/// themselves, not a season.
pub fn is_tournament(category: &str, kind: &str, name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(category, "Fun Cup" | "1 Day Cup" | "The Highlander Open" | "Nations' Cup")
        && n.contains("highlander")
        && !n.contains("fun team")
        && !kind.to_ascii_lowercase().contains("fun team")
}

/// A tournament's name as `(tournament, division)`, stage always `"Cup"`:
/// `"Highlander Experimental Cup #10: Mid 3rd Place"` ->
/// `("Highlander Experimental Cup #10", "Mid")`;
/// `"Highlander Nations Cup #7 - Bronze Match"` -> `(".. #7", "")`.
pub fn parse_tournament(name: &str) -> (String, String) {
    let (cup, label) = name.split_once(':').or_else(|| name.split_once(" - ")).unwrap_or((name, ""));
    let mut division = label.trim().to_string();
    for suffix in ["3rd Place Match", "Third Place Match", "3rd Place", "Bronze Match", "Grand Final", "Playoffs", "Final"] {
        if let Some(d) = division.strip_suffix(suffix) {
            division = d.trim().to_string();
            break;
        }
    }
    (cup.trim().to_string(), division)
}

/// A competition's division and stage, whichever kind it is.
pub fn division_and_stage(name: &str) -> Option<(String, String)> {
    if let Some((_, _, division, stage)) = parse_name(name) {
        return Some((division, stage));
    }
    let (cup, division) = parse_tournament(name);
    (!cup.is_empty()).then(|| (division, "Cup".to_string()))
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchSummary {
    pub competitions: usize,
    pub results: usize,
    pub details: usize,
    pub failed: usize,
    /// League tables read, for ETF2L's own placings.
    pub tables: usize,
}

fn team_of(v: &Value) -> Option<(i64, String, Option<String>, Option<String>)> {
    Some((
        v.get("id")?.as_i64()?,
        v.get("name")?.as_str()?.to_string(),
        v.get("country").and_then(Value::as_str).map(str::to_string),
        v.get("steam").and_then(|s| s.get("avatar")).and_then(Value::as_str).map(str::to_string),
    ))
}

/// Refresh the last year's Highlander seasons. Best effort: ETF2L being
/// down costs this sync's refresh, nothing else.
pub async fn fetch(db: &Db, sources: &Sources, progress: impl FnMut(usize, usize)) -> Result<FetchSummary> {
    fetch_seasons(db, sources, SEASONS_BACK, DETAILS_PER_SYNC, progress).await
}

/// [`fetch`] over `seasons_back` seasons before the newest, reading up to
/// `details` match pages from the last [`SEASONS_BACK`]. The league sample
/// asks for years of results and reads its own matches' pages.
pub async fn fetch_seasons(
    db: &Db,
    sources: &Sources,
    seasons_back: i64,
    details: i64,
    mut progress: impl FnMut(usize, usize),
) -> Result<FetchSummary> {
    let mut out = FetchSummary::default();

    // 1. Which competitions: newest first, until the seasons run out.
    // (id, name, archived, season for a tournament)
    let mut found: Vec<Found> = Vec::new();
    let mut newest: Option<i64> = None;
    // A tournament goes with the season after it (a preseason cup is for the
    // season it comes before): the last numbered season met, walking back.
    let mut after: Option<i64> = None;
    for page in 1..=MAX_LIST_PAGES {
        let Some(body) = sources.etf2l_page("/competition/list", page).await? else { break };
        // ETF2L answers a busy moment with an HTML page: the walk ends
        // there, and what it found so far is still read.
        let Ok(v) = serde_json::from_str::<Value>(&body) else {
            tracing::warn!(page, "ETF2L competition list page was not JSON; stopping there");
            break;
        };
        let data = v.pointer("/competitions/data").and_then(Value::as_array).cloned().unwrap_or_default();
        if data.is_empty() {
            break;
        }
        // Other formats and cups are mixed in; the walk ends at a page with
        // an older Highlander season on it and none of the wanted ones.
        let (mut wanted, mut older) = (false, false);
        for c in &data {
            let (Some(id), Some(name)) = (c.get("id").and_then(Value::as_i64), c.get("name").and_then(Value::as_str)) else { continue };
            // The league and its preseason cups, and the other Highlander
            // tournaments; not the fun-team brackets.
            let category = c.get("category").and_then(Value::as_str).unwrap_or("");
            let kind = c.get("type").and_then(Value::as_str).unwrap_or("");
            let archived = c.get("archived").and_then(Value::as_bool).unwrap_or(false);
            if is_tournament(category, kind, name) {
                let Some(season) = after.or(newest) else {
                    // Newer than every season: the newest, once it is met.
                    found.push((id, name.to_string(), archived, None));
                    continue;
                };
                if season_order(season) >= season_order(newest.unwrap_or(season) - seasons_back) {
                    found.push((id, name.to_string(), archived, Some(season)));
                }
                continue;
            }
            if !matches!(category, "Highlander Season" | "Highlander Cup") {
                continue;
            }
            let Some((season, ..)) = parse_name(name) else { continue };
            if season < OFF_SEASON {
                after = Some(season);
            }
            // The newest numbered season sets the window; one between two
            // is in it when the season before it is.
            if season < OFF_SEASON {
                newest.get_or_insert(season);
            }
            let top = newest.unwrap_or(season % OFF_SEASON);
            if season_order(season) >= season_order(top - seasons_back) {
                wanted = true;
                found.push((id, name.to_string(), archived, None));
            } else {
                older = true;
            }
        }
        if older && !wanted {
            break;
        }
    }

    fetch_competitions(db, sources, &found, newest, &mut out, &mut progress).await?;

    // 3. Match pages: who played, and each map's score.
    let recent = newest.unwrap_or(0) - SEASONS_BACK;
    for match_id in db.season_matches_without_detail(details, recent).await? {
        if !fetch_match_detail(db, sources, match_id).await? {
            out.failed += 1;
            continue;
        }
        out.details += 1;
    }
    Ok(out)
}

/// One competition as the list walk found it: `(id, name, archived, the
/// season a tournament goes with)`.
type Found = (i64, String, bool, Option<i64>);

/// Each competition's pool, results and (for a league) table, into the
/// database. An archived one already held is settled and not asked for again.
async fn fetch_competitions(db: &Db, sources: &Sources, found: &[Found], newest: Option<i64>, out: &mut FetchSummary, progress: &mut impl FnMut(usize, usize)) -> Result<()> {
    let held: HashMap<i64, bool> = db.competitions().await?.into_iter().map(|c| (c.competition_id, c.archived)).collect();
    let todo: Vec<&(i64, String, bool, Option<i64>)> = found.iter().filter(|(id, _, archived, _)| !(*archived && held.get(id) == Some(&true))).collect();
    for (i, (id, name, archived, cup_season)) in todo.iter().enumerate() {
        progress(i, todo.len());
        let (season, season_name, division, stage) = match parse_name(name) {
            Some(x) => x,
            None => {
                // A tournament: named after itself, kept with its season.
                let Some(season) = cup_season.or(newest) else { continue };
                let (cup, division) = parse_tournament(name);
                (season, cup, division, "Cup".to_string())
            }
        };
        let pool = match sources.etf2l_get(&format!("/competition/{id}")).await {
            Ok(Some(body)) => serde_json::from_str::<Value>(&body).ok().and_then(|v| v.pointer("/competition/pool").map(|p| p.to_string())),
            _ => None,
        };
        db.upsert_competition(&CompetitionRow {
            competition_id: *id,
            season,
            season_name: &season_name,
            division: &division,
            stage: &stage,
            name,
            archived: *archived,
            pool: pool.as_deref(),
        })
        .await?;
        out.competitions += 1;
        for page in 1..=20 {
            let Some(body) = sources.etf2l_page(&format!("/competition/{id}/results"), page).await? else { break };
            let Ok(v) = serde_json::from_str::<Value>(&body) else {
                tracing::warn!(competition = id, page, "ETF2L results page was not JSON; next refresh reads it");
                break;
            };
            let data = v.pointer("/results/data").and_then(Value::as_array).cloned().unwrap_or_default();
            for m in &data {
                let (Some(c1), Some(c2)) = (m.get("clan1").and_then(team_of), m.get("clan2").and_then(team_of)) else { continue };
                let Some(match_id) = m.get("id").and_then(Value::as_i64) else { continue };
                for (tid, tname, country, avatar) in [&c1, &c2] {
                    db.upsert_etf2l_team(*tid, tname, country.as_deref(), avatar.as_deref()).await?;
                }
                let maps = m.get("maps").map(|x| x.to_string()).unwrap_or_else(|| "[]".into());
                db.upsert_season_match(&SeasonMatchRow {
                    match_id,
                    competition_id: *id,
                    division: m.pointer("/division/name").and_then(Value::as_str),
                    tier: m.pointer("/division/tier").and_then(Value::as_i64),
                    week: m.get("week").and_then(Value::as_i64),
                    round: m.get("round").and_then(Value::as_str),
                    time: m.get("time").and_then(Value::as_i64),
                    clan1_id: c1.0,
                    clan2_id: c2.0,
                    r1: m.get("r1").and_then(Value::as_i64),
                    r2: m.get("r2").and_then(Value::as_i64),
                    default_win: m.get("defaultwin").and_then(Value::as_bool).unwrap_or(false),
                    maps: &maps,
                })
                .await?;
                out.results += 1;
            }
            let last = v.pointer("/results/last_page").and_then(Value::as_i64).unwrap_or(1);
            if page >= last {
                break;
            }
        }
    }

    // 2b. The league tables, for ETF2L's own placings: once a season is
    // over, its table marks 1st, 2nd and 3rd (`ach`), which the medals are
    // checked against (Flashy: "use the etf2l api to verify the season
    // winners"). A running season's every sync; a finished one's once.
    let have_tables: std::collections::HashSet<i64> = db.etf2l_raw("tables").await?.into_iter().map(|(id, _, _)| id).collect();
    for (id, name, archived, _) in found {
        if *archived && have_tables.contains(id) {
            continue;
        }
        // Only the league itself has a table worth keeping: a cup's or a
        // playoff bracket's is a list of its rounds.
        if parse_name(name).is_none_or(|(_, _, _, stage)| stage != "regular") {
            continue;
        }
        match sources.etf2l_get(&format!("/competition/{id}/tables")).await {
            Ok(Some(body)) if body.trim_start().starts_with('{') => {
                db.store_etf2l_raw("tables", *id, &body).await?;
                out.tables += 1;
            }
            Ok(_) => {}
            Err(e) => tracing::info!(competition = id, error = %format!("{e:#}"), "ETF2L table not read"),
        }
    }

    Ok(())
}

/// One old season, whole (Flashy, October 2026: Clark asked for the seasons
/// before 24): every competition ETF2L files under it -- the league, its
/// tiers and playoffs, not the cups -- with their results, tables and every
/// played match's page (who played, each map's score). The list is walked to
/// its end, since an old season sits far down it.
pub async fn fetch_season(db: &Db, sources: &Sources, season: i64, mut progress: impl FnMut(&str)) -> Result<FetchSummary> {
    let mut out = FetchSummary::default();
    let mut found: Vec<Found> = Vec::new();
    for page in 1..=OLD_LIST_PAGES {
        progress(&format!("Reading ETF2L's competition list, page {page}"));
        let Some(body) = sources.etf2l_page("/competition/list", page).await? else { break };
        let Ok(v) = serde_json::from_str::<Value>(&body) else {
            anyhow::bail!("ETF2L's competition list page {page} was not JSON; try again later");
        };
        let data = v.pointer("/competitions/data").and_then(Value::as_array).cloned().unwrap_or_default();
        if data.is_empty() {
            break;
        }
        let mut older = false;
        for c in &data {
            let (Some(id), Some(name)) = (c.get("id").and_then(Value::as_i64), c.get("name").and_then(Value::as_str)) else { continue };
            if c.get("category").and_then(Value::as_str) != Some("Highlander Season") {
                continue;
            }
            match parse_name(name) {
                Some((s, ..)) if s == season => found.push((id, name.to_string(), c.get("archived").and_then(Value::as_bool).unwrap_or(false), None)),
                Some((s, ..)) if s < OFF_SEASON && s < season => older = true,
                _ => {}
            }
        }
        // Newest first: a page wholly older than the season ends the walk.
        if older && !found.is_empty() {
            break;
        }
    }
    if found.is_empty() {
        anyhow::bail!("ETF2L lists no competition for Highlander Season {season}");
    }
    let mut each = |done: usize, total: usize| progress(&format!("Reading results, competition {} of {total}", done + 1));
    fetch_competitions(db, sources, &found, Some(season), &mut out, &mut each).await?;
    let todo = db.season_matches_without_detail_in(season).await?;
    for (i, match_id) in todo.iter().enumerate() {
        progress(&format!("Reading who played, match {} of {}", i + 1, todo.len()));
        if fetch_match_detail(db, sources, *match_id).await? {
            out.details += 1;
        } else {
            out.failed += 1;
        }
    }
    Ok(out)
}

/// An ETF2L player's Steam account, from their own page, read once and
/// kept (`etf2l_raw` kind `player_steam`; 0 for a page without one).
pub(crate) async fn steam_of_player(db: &Db, sources: &Sources, etf2l_id: i64) -> Result<Option<u32>> {
    if let Some((_, json)) = db.etf2l_raw_one("player_steam", etf2l_id).await? {
        return Ok(json.trim().parse::<u32>().ok().filter(|a| *a != 0));
    }
    let Ok(Some(body)) = sources.etf2l_get(&format!("/player/{etf2l_id}")).await else { return Ok(None) };
    let account = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| v.pointer("/player/steam/id3").and_then(Value::as_str).and_then(|id3| SteamId::parse(id3).ok()))
        .map(|id| id.account_id());
    db.store_etf2l_raw("player_steam", etf2l_id, &account.unwrap_or(0).to_string()).await?;
    Ok(account)
}

/// Read one match's page: each map's score and who played. False when
/// ETF2L would not give it.
pub async fn fetch_match_detail(db: &Db, sources: &Sources, match_id: i64) -> Result<bool> {
    {
        let body = match sources.etf2l_get(&format!("/matches/{match_id}")).await {
            Ok(Some(b)) => b,
            _ => return Ok(false),
        };
        let Ok(v) = serde_json::from_str::<Value>(&body) else {
            return Ok(false);
        };
        let m = v.get("match").unwrap_or(&Value::Null);
        let maps: Vec<(i64, String, i64, i64, bool)> = m
            .get("map_results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some((
                    r.get("match_order")?.as_i64()?,
                    r.get("map")?.as_str()?.to_string(),
                    r.get("clan1")?.as_i64()?,
                    r.get("clan2")?.as_i64()?,
                    r.get("golden_cap").and_then(Value::as_bool).unwrap_or(false),
                ))
            })
            .collect();
        let mut players: Vec<(u32, Option<i64>, String)> = Vec::new();
        for p in m.get("players").and_then(Value::as_array).into_iter().flatten() {
            let team = p.get("team_id").and_then(Value::as_i64);
            let name = p.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let account = match p.pointer("/steam/id3").and_then(Value::as_str).and_then(|id3| SteamId::parse(id3).ok()) {
                Some(id) => Some(id.account_id()),
                // Seasons 19 to 21's pages list their players with an empty
                // `steam` (Flashy's old-season import found none of them in
                // a log): the player's own page has it.
                None => match p.get("id").and_then(Value::as_i64) {
                    Some(etf2l_id) => steam_of_player(db, sources, etf2l_id).await?,
                    None => None,
                },
            };
            if let Some(account) = account {
                players.push((account, team, name));
            }
        }
        let maps_ref: Vec<(i64, &str, i64, i64, bool)> = maps.iter().map(|(o, m, a, b, g)| (*o, m.as_str(), *a, *b, *g)).collect();
        let players_ref: Vec<(u32, Option<i64>, &str)> = players.iter().map(|(a, t, n)| (*a, *t, n.as_str())).collect();
        db.put_season_match_detail(match_id, &maps_ref, &players_ref).await?;
    }
    Ok(true)
}

// ---- views ------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonInfo {
    pub season: i64,
    pub name: String,
    pub divisions: Vec<String>,
    pub pool: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub played: u32,
    pub won: u32,
    pub lost: u32,
    pub drawn: u32,
}

impl Record {
    fn add(&mut self, us: i64, them: i64) {
        self.played += 1;
        match us.cmp(&them) {
            std::cmp::Ordering::Greater => self.won += 1,
            std::cmp::Ordering::Less => self.lost += 1,
            std::cmp::Ordering::Equal => self.drawn += 1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRow {
    pub team_id: i64,
    pub name: String,
    pub avatar: Option<String>,
    pub record: Record,
    /// ETF2L's own score, summed: what the league table ranks on.
    pub score_for: i64,
    pub score_against: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DivisionTable {
    pub division: String,
    pub teams: Vec<TableRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonView {
    pub seasons: Vec<SeasonInfo>,
    pub season: Option<SeasonInfo>,
    pub divisions: Vec<DivisionTable>,
    /// Matches still to be read in detail, for saying the maps are partial.
    pub pending_details: usize,
}

fn seasons_of(comps: &[hl_db::Competition]) -> Vec<SeasonInfo> {
    let mut by: BTreeMap<i64, SeasonInfo> = BTreeMap::new();
    // A tournament is kept with a season but is not one (its season name
    // is its own: "Highlander Experimental Cup #10").
    for c in comps.iter().filter(|c| !(c.stage == "Cup" && c.season_name.contains("Highlander"))) {
        let s = by.entry(season_order(c.season)).or_insert_with(|| SeasonInfo { season: c.season, name: c.season_name.clone(), divisions: Vec::new(), pool: Vec::new() });
        if s.pool.is_empty() {
            s.pool = c.pool.clone();
        }
    }
    by.into_values().rev().collect()
}

/// One season's division tables, regular season only. `None` picks the newest.
pub async fn season(db: &Db, season: Option<i64>) -> Result<SeasonView> {
    let comps = db.competitions().await?;
    let seasons = seasons_of(&comps);
    let Some(chosen) = season.and_then(|s| seasons.iter().find(|x| x.season == s)).or(seasons.first()).cloned() else {
        return Ok(SeasonView { seasons, season: None, divisions: Vec::new(), pending_details: 0 });
    };
    let matches = db.season_matches(Some(chosen.season), None).await?;
    // Divisions from the matches, top tier first.
    let mut order: Vec<(i64, String)> = Vec::new();
    for m in matches.iter().filter(|m| m.stage == "regular" && !m.division.is_empty()) {
        let tier = m.tier.unwrap_or(99);
        match order.iter_mut().find(|(_, d)| *d == m.division) {
            Some(o) => o.0 = o.0.min(tier),
            None => order.push((tier, m.division.clone())),
        }
    }
    order.sort();
    let mut chosen = chosen;
    chosen.divisions = order.into_iter().map(|(_, d)| d).collect();
    let mut tables: BTreeMap<String, HashMap<i64, TableRow>> = BTreeMap::new();
    for m in matches.iter().filter(|m| m.stage == "regular") {
        let (Some(r1), Some(r2)) = (m.r1, m.r2) else { continue };
        let t = tables.entry(m.division.clone()).or_default();
        for (id, name, us, them) in [(m.clan1_id, &m.clan1_name, r1, r2), (m.clan2_id, &m.clan2_name, r2, r1)] {
            let row = t.entry(id).or_insert_with(|| TableRow { team_id: id, name: name.clone(), avatar: None, record: Record::default(), score_for: 0, score_against: 0 });
            row.record.add(us, them);
            row.score_for += us;
            row.score_against += them;
        }
    }
    let mut divisions: Vec<DivisionTable> = Vec::new();
    for div in &chosen.divisions {
        let Some(t) = tables.remove(div) else { continue };
        let mut teams: Vec<TableRow> = t.into_values().collect();
        for row in &mut teams {
            row.avatar = db.etf2l_team(row.team_id).await?.and_then(|(_, _, a)| a);
        }
        teams.sort_by_key(|t| std::cmp::Reverse((t.record.won, t.score_for - t.score_against)));
        divisions.push(DivisionTable { division: div.clone(), teams });
    }
    let recent = db.newest_etf2l_season().await?.unwrap_or(0) - SEASONS_BACK;
    let pending_details = db.season_matches_without_detail(10_000, recent).await?.len();
    Ok(SeasonView { seasons, season: Some(chosen), divisions, pending_details })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapRecord {
    pub map: String,
    pub record: Record,
    pub rounds_for: i64,
    pub rounds_against: i64,
    /// Whether it is in the newest season's pool.
    pub in_pool: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRow {
    pub match_id: i64,
    pub season: i64,
    pub division: String,
    pub stage: String,
    pub round: Option<String>,
    pub time: Option<i64>,
    pub opponent_id: i64,
    pub opponent: String,
    pub score_for: Option<i64>,
    pub score_against: Option<i64>,
    pub default_win: bool,
    pub maps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterRow {
    pub account_id: u32,
    pub name: String,
    pub matches: i64,
    pub last_played: i64,
    /// The class they played most for this team in its officials, the
    /// officials rated on it, and their average rating there.
    pub class: Option<String>,
    pub games: i64,
    pub rating: Option<f64>,
    /// The same, season by season, newest first: the lineup per season.
    pub seasons: Vec<RosterSeason>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterSeason {
    pub season: i64,
    pub officials: i64,
    pub class: Option<String>,
    pub rating: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamView {
    pub team_id: i64,
    pub name: String,
    pub country: Option<String>,
    pub avatar: Option<String>,
    /// `(season, division)` played in, newest first.
    pub seasons: Vec<(i64, String)>,
    pub record: Record,
    pub maps: Vec<MapRecord>,
    pub results: Vec<ResultRow>,
    pub roster: Vec<RosterRow>,
}

pub async fn team(db: &Db, team_id: i64) -> Result<Option<TeamView>> {
    let Some((name, country, avatar)) = db.etf2l_team(team_id).await? else { return Ok(None) };
    let comps = db.competitions().await?;
    let pool: Vec<String> = seasons_of(&comps).first().map(|s| s.pool.clone()).unwrap_or_default();
    let matches = db.season_matches(None, Some(team_id)).await?;
    let map_rows = db.season_maps(Some(team_id)).await?;
    let mut by_match: HashMap<i64, Vec<&hl_db::SeasonMap>> = HashMap::new();
    for r in &map_rows {
        by_match.entry(r.match_id).or_default().push(r);
    }

    let mut record = Record::default();
    let mut maps: BTreeMap<String, MapRecord> = BTreeMap::new();
    let mut seasons: Vec<(i64, String)> = Vec::new();
    let mut results = Vec::new();
    for m in &matches {
        let first = m.clan1_id == team_id;
        let (us, them) = if first { (m.r1, m.r2) } else { (m.r2, m.r1) };
        let (opp_id, opp) = if first { (m.clan2_id, m.clan2_name.clone()) } else { (m.clan1_id, m.clan1_name.clone()) };
        if !seasons.iter().any(|(s, d)| *s == m.season && *d == m.division) {
            seasons.push((m.season, m.division.clone()));
        }
        if let (Some(a), Some(b)) = (us, them) {
            record.add(a, b);
            // Per map from the match page where read; otherwise the match's
            // map, which in Highlander is one map played twice.
            match by_match.get(&m.match_id) {
                Some(rows) if !m.default_win => {
                    // A stopwatch map is played as two halves, each its own
                    // row: one map is their sum, won or lost once.
                    // Versions of one map are one map: pl_upward_f10 and _f12
                    // are counted together as pl_upward.
                    let mut per_map: BTreeMap<String, (i64, i64)> = BTreeMap::new();
                    for r in rows {
                        let (f, g) = if first { (r.clan1, r.clan2) } else { (r.clan2, r.clan1) };
                        let e = per_map.entry(map_name(&r.map)).or_default();
                        e.0 += f;
                        e.1 += g;
                    }
                    for (map, (f, g)) in per_map {
                        let e = maps.entry(map.clone()).or_insert_with(|| MapRecord { map: map.clone(), record: Record::default(), rounds_for: 0, rounds_against: 0, in_pool: false });
                        e.record.add(f, g);
                        e.rounds_for += f;
                        e.rounds_against += g;
                    }
                }
                _ if !m.default_win => {
                    if let Some(map) = m.maps.first().map(|m| map_name(m)) {
                        let e = maps.entry(map.clone()).or_insert_with(|| MapRecord { map: map.clone(), record: Record::default(), rounds_for: 0, rounds_against: 0, in_pool: false });
                        e.record.add(a, b);
                    }
                }
                _ => {}
            }
        }
        results.push(ResultRow {
            match_id: m.match_id,
            season: m.season,
            division: m.division.clone(),
            stage: m.stage.clone(),
            round: m.round.clone(),
            time: m.time,
            opponent_id: opp_id,
            opponent: opp,
            score_for: us,
            score_against: them,
            default_win: m.default_win,
            maps: m.maps.iter().map(|m| map_name(m)).collect(),
        });
    }
    let mut maps: Vec<MapRecord> = maps.into_values().collect();
    for m in &mut maps {
        m.in_pool = pool.iter().any(|p| map_name(p) == m.map);
    }
    maps.sort_by(|a, b| b.in_pool.cmp(&a.in_pool).then(b.record.played.cmp(&a.record.played)));

    // Each player's class and rating for this team: what they played in its
    // officials, not their most played class anywhere -- a player's pugs
    // and their other teams say nothing about their role here.
    let players = db.season_players(team_id).await?;
    let classes = db.team_classes(team_id).await?;
    let by_season = db.season_players_by_season(team_id).await?;
    let classes_by_season = db.team_classes_by_season(team_id).await?;
    let season_of: HashMap<i64, i64> = matches.iter().map(|m| (m.match_id, m.season)).collect();
    let mut roster = Vec::new();
    for (account_id, name, matches, last_played) in players {
        // Most time first per account.
        let class = classes.iter().find(|c| c.0 == account_id).map(|c| c.1.clone());
        // Their rated games in this team's officials, any class.
        let rated: Vec<hl_db::RatedGame> = db
            .rated_games(hl_rating::MODEL_VERSION, Some(account_id))
            .await?
            .into_iter()
            .filter(|g| g.etf2l_match_id.is_some_and(|id| season_of.contains_key(&id)))
            .collect();
        let avg = |v: Vec<f64>| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
        let on_class: Vec<&hl_db::RatedGame> = rated.iter().filter(|g| class.as_deref() == Some(g.class.as_str())).collect();
        let games = on_class.iter().filter_map(|g| g.etf2l_match_id).collect::<HashSet<i64>>().len() as i64;
        let rating = avg(on_class.iter().map(|g| g.score).collect());
        let mut seasons: Vec<RosterSeason> = by_season
            .iter()
            .filter(|(a, _, _)| *a == account_id)
            .map(|(_, season, n)| {
                let class = classes_by_season.iter().find(|c| c.0 == account_id && c.1 == *season).map(|c| c.2.clone());
                let rating = avg(
                    rated
                        .iter()
                        .filter(|g| class.as_deref() == Some(g.class.as_str()) && g.etf2l_match_id.and_then(|id| season_of.get(&id)) == Some(season))
                        .map(|g| g.score)
                        .collect(),
                );
                RosterSeason { season: *season, officials: *n, class, rating }
            })
            .collect();
        seasons.sort_by_key(|s| std::cmp::Reverse(season_order(s.season)));
        roster.push(RosterRow { account_id, name, matches, last_played, class, games, rating, seasons });
    }

    Ok(Some(TeamView { team_id, name, country, avatar, seasons, record, maps, results, roster }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_competition_name_is_its_season_division_and_stage() {
        assert_eq!(parse_name("Highlander Season 36 (Autumn 2026): High"), Some((36, "Autumn 2026".into(), "High".into(), "regular".into())));
        assert_eq!(parse_name("Highlander Season 36 (Autumn 2026): Open Playoffs"), Some((36, "Autumn 2026".into(), "Open".into(), "Playoffs".into())));
        assert_eq!(parse_name("Highlander Season 36 (Autumn 2026): Low 3rd Place"), Some((36, "Autumn 2026".into(), "Low".into(), "3rd Place".into())));
        assert_eq!(parse_name("6v6 Season 50: Premiership"), None);
        // Before season 32: no season name in brackets.
        assert_eq!(parse_name("Highlander Season 22"), Some((22, "Season 22".into(), "".into(), "regular".into())));
        assert_eq!(parse_name("Highlander Season 22: Premiership Qualifiers"), Some((22, "Season 22".into(), "Premiership".into(), "Qualifiers".into())));
        // S12 and S13's qualifiers before the season, in both spellings.
        assert_eq!(parse_name("Highlander Season 12: Preseason playoffs").map(|p| p.3), Some("Qualifiers".into()));
        assert_eq!(parse_name("Highlander Season 13: Preseason Playoffs").map(|p| p.3), Some("Qualifiers".into()));
        assert_eq!(parse_name("Highlander Season 34: High 3rd Place Match"), Some((34, "Season 34".into(), "High".into(), "3rd Place".into())));
        // Between 27 and 32: named by the time of year.
        assert_eq!(parse_name("Highlander Winter 2024: Low Playoffs"), Some((31, "Winter 2024".into(), "Low".into(), "Playoffs".into())));
        assert_eq!(parse_name("Highlander Winter 2024"), Some((31, "Winter 2024".into(), "".into(), "regular".into())));
        // Between Season 34 and 35.
        assert_eq!(parse_name("Highlander AFA 2025: Top Tiers"), Some((134, "AFA 2025".into(), "Top Tiers".into(), "regular".into())));
        assert_eq!(parse_name("Highlander AFA 2025: Low 3rd Place"), Some((134, "AFA 2025".into(), "Low".into(), "3rd Place".into())));
        assert_eq!(parse_name("Highlander AFA 2025"), Some((134, "AFA 2025".into(), "".into(), "regular".into())));
        // Tournaments: named after themselves, a cup stage.
        assert_eq!(parse_name("Highlander Experimental Cup #10: Mid 3rd Place"), None);
        assert_eq!(parse_tournament("Highlander Experimental Cup #10: Mid 3rd Place"), ("Highlander Experimental Cup #10".into(), "Mid".into()));
        assert_eq!(parse_tournament("Highlander Nations Cup #7 - Bronze Match"), ("Highlander Nations Cup #7".into(), "".into()));
        assert_eq!(parse_tournament("High-Tide Highlander One-Night-Cup"), ("High-Tide Highlander One-Night-Cup".into(), "".into()));
        assert_eq!(division_and_stage("The Highlander Open #4: Playoffs"), Some(("".into(), "Cup".into())));
        assert!(is_tournament("Fun Cup", "Highlander", "Highlander Experimental Cup #10: Mid Grand Final"));
        assert!(!is_tournament("Fun Cup", "Highlander Fun Team", "Highlander Experimental Cup #10 (Fun Teams)"));
        assert!(is_tournament("Nations' Cup", "National Highlander Team", "Highlander Nations Cup #9: 3rd Place Match"));
        assert!(!is_tournament("Fun Cup", "6v6", "6v6 Fun Cup"));
        assert!(season_order(34) < season_order(134) && season_order(134) < season_order(35));
        assert_eq!(parse_name("Highlander Winter 2023 Premiership"), Some((28, "Winter 2023".into(), "Premiership".into(), "regular".into())));
        assert_eq!(parse_name("Highlander Spring 2023 Top Tiers"), Some((29, "Spring 2023".into(), "Top Tiers".into(), "regular".into())));
        assert_eq!(parse_name("Highlander Autumn 2023: Premiership Qualifiers"), Some((30, "Autumn 2023".into(), "Premiership".into(), "Qualifiers".into())));
        // Preseason cups are officials, not the league.
        assert_eq!(parse_name("Highlander Autumn 2023 Preseason Cup"), Some((30, "Autumn 2023".into(), "".into(), "Cup".into())));
        assert_eq!(parse_name("Highlander Spring 2023 Preseason Cup: Low Playoffs"), Some((29, "Spring 2023".into(), "Low".into(), "Cup".into())));
        assert_eq!(parse_name("Highlander Experimental Cup #10"), None);
    }

    #[test]
    fn every_era_s_divisions_land_on_today_s_ladder() {
        for (name, tier) in [
            ("Premiership", 0),
            ("High", 1),
            ("Division 1", 1),
            ("Division 2", 1),
            ("Division 3", 2),
            ("Division 4", 2),
            ("Mid B", 2),
            ("Division 5A", 3),
            ("Low A", 3),
            ("Division 6", 4),
            ("Open A", 4),
            ("Division 7", 5),
            ("Fresh", 5),
            ("Freshest", 5),
        ] {
            assert_eq!(canonical_tier(name), Some(tier), "{name}");
        }
        assert_eq!(canonical_tier("Top Tiers"), None);
    }
}

/// An ETF2L official a log was played as, for a log the owner was not in
/// (Flashy: the S36 High Grand Final, one 116-minute upload, read as RED
/// against BLU): each colour's team, and ETF2L's result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeagueSides {
    pub match_id: i64,
    pub red: hl_db::SideTeam,
    pub blue: hl_db::SideTeam,
    /// ETF2L's result, RED's side first.
    pub red_score: Option<i64>,
    pub blue_score: Option<i64>,
    pub division: Option<String>,
    pub round: Option<String>,
    pub season_name: Option<String>,
}

/// How long after an official's scheduled time a log of it may be stamped:
/// a three-map final uploaded as one log is stamped at its end.
const LEAGUE_LOG_WINDOW_S: i64 = 6 * 3600;

/// Which season official a log was, by its time and its players: the
/// officials ETF2L scheduled in the hours before it, each one's lineup
/// (read from ETF2L once, if not yet), and the one whose two teams' players
/// were on the log's two colours. `None` when none fits: a pug, or a match
/// whose lineup ETF2L does not list.
pub async fn league_sides(db: &Db, sources: &Sources, log_id: i64) -> Result<Option<LeagueSides>> {
    let Some((at, colours)) = db.log_time_and_colours(log_id).await? else { return Ok(None) };
    let mut best: Option<(usize, i64, i64, i64)> = None; // (players placed, match, red team, blue team)
    for (match_id, c1, c2, fetched) in db.season_matches_before(at, LEAGUE_LOG_WINDOW_S).await? {
        if !fetched {
            let _ = fetch_match_detail(db, sources, match_id).await;
        }
        let roster = db.season_match_roster(match_id).await?;
        // RED's players on each team, BLU's on each.
        let (mut r1, mut r2, mut b1, mut b2) = (0usize, 0usize, 0usize, 0usize);
        for (account, colour) in &colours {
            match (roster.get(account), colour.as_str()) {
                (Some(t), "Red") if *t == c1 => r1 += 1,
                (Some(t), "Red") if *t == c2 => r2 += 1,
                (Some(t), "Blue") if *t == c1 => b1 += 1,
                (Some(t), "Blue") if *t == c2 => b2 += 1,
                _ => {}
            }
        }
        let (placed, red, blue) = if r1 + b2 >= r2 + b1 { (r1 + b2, c1, c2) } else { (r2 + b1, c2, c1) };
        // Most of both lineups, each on its own colour.
        if placed >= 8 && best.is_none_or(|b| placed > b.0) {
            best = Some((placed, match_id, red, blue));
        }
    }
    let Some((_, match_id, red, blue)) = best else { return Ok(None) };
    let Some((r1, r2, clan1, division, round, season_name)) = db.season_match_summary(match_id).await? else { return Ok(None) };
    let side = |id: i64, t: Option<(String, Option<String>, Option<String>)>| {
        let (name, country, avatar) = t.unwrap_or_else(|| (format!("team {id}"), None, None));
        hl_db::SideTeam { id: Some(id), name, country, avatar }
    };
    let (red_score, blue_score) = if red == clan1 { (r1, r2) } else { (r2, r1) };
    let sides = LeagueSides {
        match_id,
        red: side(red, db.team_brief(red).await?),
        blue: side(blue, db.team_brief(blue).await?),
        red_score,
        blue_score,
        division,
        round,
        season_name,
    };
    // Kept for the combined-log ledger, which reads ETF2L's map results
    // without asking anyone (Q84).
    db.store_etf2l_raw("league_sides", log_id, &serde_json::to_string(&sides)?).await?;
    Ok(Some(sides))
}

/// A medal the daily check found new or changed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MedalChange {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub place: u8,
    pub team_id: i64,
    pub team: String,
    /// How it was decided: "Grand Final 6-3", ", confirmed by ETF2L".
    pub how: String,
}

/// What the once-a-day ETF2L check did.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCheck {
    /// False when it had already run in the last day (or a sync is
    /// reading ETF2L), and nothing was asked.
    pub ran: bool,
    pub results: usize,
    /// Divisions whose medals ETF2L's own final standings confirm.
    pub confirmed: usize,
    /// Medals that are new, or went to another team, since before the check.
    pub changed: Vec<MedalChange>,
}

const DAILY_KEY: &str = "etf2l_daily_check";
const DAY_S: i64 = 24 * 3600;

/// Once a day (Flashy: "a check 1 time per day when you start up or
/// whenever this info is checked, and if its correct in the app"): read
/// ETF2L's seasons, results and tables again, and say which medals that
/// changed. `force` runs it whatever the last time was.
pub async fn daily_check(db: &Db, sources: &Sources, now: i64, force: bool) -> Result<DailyCheck> {
    let last: i64 = db.get_setting(DAILY_KEY).await?.and_then(|s| s.parse().ok()).unwrap_or(0);
    if !force && now - last < DAY_S {
        return Ok(DailyCheck::default());
    }
    let before = crate::catalogue::Catalogue::load(db).await?.medals();
    let read = fetch(db, sources, |_, _| {}).await?;
    db.set_setting(DAILY_KEY, &now.to_string()).await?;
    let cat = crate::catalogue::Catalogue::load(db).await?;
    let after = cat.medals();
    let mut changed = Vec::new();
    for ((season, division), medals) in &after {
        let was = before.get(&(*season, division.clone()));
        for (place, team, how) in medals {
            if was.is_some_and(|w| w.iter().any(|(p, t, _)| p == place && t == team)) {
                continue;
            }
            changed.push(MedalChange {
                season: *season,
                season_name: cat.season_title(*season),
                division: division.clone(),
                place: *place,
                team_id: *team,
                team: cat.team_name(*team),
                how: how.clone(),
            });
        }
    }
    changed.sort_by_key(|c| (std::cmp::Reverse(c.season), c.division.clone(), c.place));
    Ok(DailyCheck { ran: true, results: read.results, confirmed: cat.official_divisions(), changed })
}
