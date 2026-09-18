//! ETF2L: the owner's officials, their competitions, and who was on each side.
//!
//! ```text
//! fetch:   player (by SteamID) -> results pages -> each Highlander match
//! derive:  stored matches -> etf2l_match + etf2l_roster -> match_context
//! ```
//!
//! `derive_context` touches no network, so it also runs on rebuild and at
//! startup; without any ETF2L data it still separates scrims from pugs and
//! marks the officials trends.tf tagged.

use crate::context::{self, Game, Official};
use crate::sources::Sources;
use anyhow::{Context as _, Result};
use hl_core::SteamId;
use hl_db::{ContextRow, Db, Etf2lMatchRow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

/// Where the owner's ETF2L player id is kept once looked up.
pub const PLAYER_KEY: &str = "etf2l_player_id";

/// A match is refetched while it is younger than this when last fetched:
/// results, scores and default wins settle within days of being played.
const SETTLE_S: i64 = 14 * 24 * 3600;

/// Results pages are 20 matches each; this is a guard, not a limit.
const MAX_RESULT_PAGES: i64 = 100;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Etf2lSummary {
    pub player_id: Option<i64>,
    pub fetched: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSummary {
    pub officials: usize,
    pub roster_officials: usize,
    pub scrims: usize,
    pub pugs: usize,
}

/// Pull the owner's ETF2L history. `progress(done, total)` counts match fetches.
pub async fn fetch(
    db: &Db,
    sources: &Sources,
    me: SteamId,
    mut progress: impl FnMut(usize, usize),
) -> Result<Etf2lSummary> {
    // 1. Who the owner is on ETF2L. Not everyone has an account.
    let player_id = match db.get_setting(PLAYER_KEY).await?.and_then(|s| s.parse::<i64>().ok()) {
        Some(id) => id,
        None => {
            let Some(body) = sources.etf2l_get(&format!("/player/{}", me.to_steamid64())).await? else {
                tracing::info!("no ETF2L account for this SteamID");
                return Ok(Etf2lSummary::default());
            };
            let v: Value = serde_json::from_str(&body).context("parsing ETF2L player")?;
            let id = v
                .pointer("/player/id")
                .and_then(Value::as_i64)
                .context("ETF2L player response has no id")?;
            db.store_etf2l_raw("player", id, &body).await?;
            db.set_setting(PLAYER_KEY, &id.to_string()).await?;
            id
        }
    };

    // 2. Every result the player appears in, newest first. Stored so a rebuild
    //    can rediscover officials without asking again.
    let mut wanted: BTreeSet<i64> = BTreeSet::new();
    let mut page = 1;
    loop {
        let body = sources
            .etf2l_get(&format!("/player/{player_id}/results?page={page}"))
            .await?
            .context("ETF2L results not found")?;
        let v: Value = serde_json::from_str(&body).context("parsing ETF2L results")?;
        db.store_etf2l_raw("results", page, &body).await?;
        for r in v.get("data").and_then(Value::as_array).into_iter().flatten() {
            let hl = r.pointer("/competition/type").and_then(Value::as_str) == Some("Highlander");
            if let (true, Some(id)) = (hl, r.get("result").and_then(Value::as_i64)) {
                wanted.insert(id);
            }
        }
        let last = v.get("last_page").and_then(Value::as_i64).unwrap_or(page);
        if page >= last || page >= MAX_RESULT_PAGES {
            break;
        }
        page += 1;
    }

    // 3. The matches themselves: everything the results list plus everything
    //    trends.tf linked, skipping ones already stored and settled.
    wanted.extend(db.trends_etf2l_ids().await?);
    let stored: HashMap<i64, (i64, Option<i64>)> = db
        .etf2l_raw("match")
        .await?
        .into_iter()
        .map(|(id, fetched_at, json)| {
            let time = serde_json::from_str::<Value>(&json)
                .ok()
                .and_then(|v| v.pointer("/match/time").and_then(Value::as_i64));
            (id, (fetched_at, time))
        })
        .collect();
    let todo: Vec<i64> = wanted
        .into_iter()
        .filter(|id| match stored.get(id) {
            None => true,
            Some((fetched_at, time)) => time.is_none_or(|t| fetched_at - t < SETTLE_S),
        })
        .collect();

    let (total, mut fetched, mut failed) = (todo.len(), 0, 0);
    for (i, id) in todo.into_iter().enumerate() {
        progress(i, total);
        match sources.etf2l_get(&format!("/matches/{id}")).await {
            Ok(Some(body)) if serde_json::from_str::<Value>(&body).is_ok_and(|v| v.get("match").is_some()) => {
                db.store_etf2l_raw("match", id, &body).await?;
                fetched += 1;
            }
            Ok(_) => {
                tracing::warn!(match_id = id, "ETF2L match missing or malformed");
                failed += 1;
            }
            Err(e) => {
                tracing::warn!(match_id = id, error = %format!("{e:#}"), "ETF2L match fetch failed");
                failed += 1;
            }
        }
    }
    progress(total, total);
    Ok(Etf2lSummary { player_id: Some(player_id), fetched, failed })
}

/// Rebuild `etf2l_match`, `etf2l_roster` and `match_context` from stored data.
pub async fn derive_context(db: &Db, me: SteamId) -> Result<ContextSummary> {
    let mut rows = Vec::new();
    for (id, _, json) in db.etf2l_raw("match").await? {
        match parse_match(&json) {
            Ok(m) => rows.push(m),
            Err(e) => tracing::warn!(match_id = id, error = %format!("{e:#}"), "stored ETF2L match unreadable"),
        }
    }
    db.replace_etf2l_matches(&rows).await?;

    let officials: Vec<Official> = db
        .official_rows()
        .await?
        .into_iter()
        .map(|o| Official { match_id: o.match_id, time: o.time, clan1: o.clan1, clan2: o.clan2, roster: o.roster })
        .collect();

    let me_id = me.account_id();
    let games: Vec<Game> = db
        .context_games(me_id)
        .await?
        .into_iter()
        .filter_map(|g| {
            let my_team = g.players.iter().find(|(a, _)| *a == me_id)?.1.clone();
            let (mates, opponents): (Vec<_>, Vec<_>) =
                g.players.iter().filter(|(a, _)| *a != me_id).partition(|(_, t)| *t == my_team);
            Some(Game {
                log_id: g.log_id,
                played_at: g.played_at,
                trends_match: g.trends_match,
                mates: mates.into_iter().map(|(a, _)| *a).collect(),
                opponents: opponents.into_iter().map(|(a, _)| *a).collect(),
            })
        })
        .collect();

    let ctx = context::classify(&games, &officials);
    let mut s = ContextSummary::default();
    for c in &ctx {
        match c.kind {
            context::Kind::Official => {
                s.officials += 1;
                if c.link_method == Some("roster") {
                    s.roster_officials += 1;
                }
            }
            context::Kind::Scrim => s.scrims += 1,
            context::Kind::Pug => s.pugs += 1,
        }
    }
    let rows: Vec<ContextRow> = ctx
        .into_iter()
        .map(|c| ContextRow {
            log_id: c.log_id,
            kind: c.kind.as_str(),
            etf2l_match_id: c.etf2l_match_id,
            link_method: c.link_method,
            team: c.team,
            opponent: c.opponent,
            regulars: c.regulars as i64,
        })
        .collect();
    db.replace_match_context(&rows).await?;
    Ok(s)
}

// ---- parsing -----------------------------------------------------------------

#[derive(Deserialize)]
struct MatchEnvelope {
    #[serde(rename = "match")]
    m: MatchJson,
}

#[derive(Deserialize)]
struct MatchJson {
    id: i64,
    clan1: Option<ClanJson>,
    clan2: Option<ClanJson>,
    competition: Option<CompetitionJson>,
    division: Option<DivisionJson>,
    #[serde(default)]
    defaultwin: bool,
    #[serde(default)]
    maps: Vec<String>,
    r1: Option<i64>,
    r2: Option<i64>,
    round: Option<String>,
    time: Option<i64>,
    week: Option<i64>,
    #[serde(default)]
    players: Vec<PlayerJson>,
}

#[derive(Deserialize)]
struct ClanJson {
    id: i64,
    name: String,
}

#[derive(Deserialize)]
struct CompetitionJson {
    id: Option<i64>,
    name: Option<String>,
    category: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct DivisionJson {
    name: Option<String>,
    tier: Option<i64>,
}

#[derive(Deserialize)]
struct PlayerJson {
    name: Option<String>,
    team_id: Option<i64>,
    steam: Option<SteamJson>,
}

#[derive(Deserialize)]
struct SteamJson {
    id64: Option<String>,
}

fn parse_match(json: &str) -> Result<Etf2lMatchRow> {
    let MatchEnvelope { m } = serde_json::from_str(json).context("ETF2L match shape")?;
    let comp = m.competition;
    let roster = m
        .players
        .into_iter()
        .filter_map(|p| {
            let id = SteamId::parse(p.steam?.id64.as_deref()?).ok()?;
            Some((id.account_id(), p.team_id, p.name))
        })
        .collect();
    Ok(Etf2lMatchRow {
        match_id: m.id,
        competition_id: comp.as_ref().and_then(|c| c.id),
        competition: comp.as_ref().and_then(|c| c.name.clone()),
        comp_type: comp.as_ref().and_then(|c| c.kind.clone()),
        category: comp.as_ref().and_then(|c| c.category.clone()),
        division: m.division.as_ref().and_then(|d| d.name.clone()),
        tier: m.division.as_ref().and_then(|d| d.tier),
        week: m.week,
        round: m.round,
        time: m.time,
        clan1: m.clan1.map(|c| (c.id, c.name)),
        clan2: m.clan2.map(|c| (c.id, c.name)),
        r1: m.r1,
        r2: m.r2,
        default_win: m.defaultwin,
        maps: m.maps,
        roster,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from the real response for the S36 official against TWS.
    const S36: &str = r#"{"match":{"clan1":{"country":"France","drop":false,"id":37805,"name":"DD14"},
        "clan2":{"id":37921,"name":"ЭТО МОЁ БОЛОТО"},
        "competition":{"category":"Highlander Season","id":1050,"name":"Highlander Season 36 (Autumn 2026): High","type":"Highlander"},
        "defaultwin":false,"division":{"id":3085,"name":"High","skill_contrib":24,"tier":1},"id":92883,
        "maps":["pl_upward_f12","pl_upward_f12"],"r1":4,"r2":2,"round":"Round 1","time":1787512500,"week":1,
        "players":[
          {"id":58470,"name":"Kaylus","team_id":37805,"steam":{"id":"STEAM_1:0:45409556","id3":"[U:1:90819112]","id64":"76561198051084840"}},
          {"id":97913,"name":"Flashy","team_id":null,"steam":{"id64":"76561198099396919"}},
          {"id":1,"name":"no steam","team_id":37921,"steam":null}],
        "demos":[],"map_results":[]}}"#;

    #[test]
    fn parses_a_real_match() {
        let m = parse_match(S36).unwrap();
        assert_eq!(m.match_id, 92883);
        assert_eq!(m.comp_type.as_deref(), Some("Highlander"));
        assert_eq!(m.division.as_deref(), Some("High"));
        assert_eq!(m.tier, Some(1));
        assert_eq!(m.clan2.as_ref().unwrap().1, "ЭТО МОЁ БОЛОТО");
        assert_eq!((m.r1, m.r2), (Some(4), Some(2)));
        // The merc keeps no team; the player without a SteamID is dropped.
        assert_eq!(m.roster.len(), 2);
        assert!(m.roster.contains(&(139131191, None, Some("Flashy".into()))));
        assert!(m.roster.contains(&(90819112, Some(37805), Some("Kaylus".into()))));
    }
}
