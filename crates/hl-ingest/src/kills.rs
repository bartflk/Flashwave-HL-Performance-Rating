//! Raw logs in, kills out: fetching logs.tf's raw server logs, deriving every
//! kill from them, and valuing those kills for the rating and the match page.
//!
//! ```text
//! fetch:   rawlog_queue -> logs.tf/logs/log_<id>.log.zip -> rawlog (verbatim)
//! derive:  rawlog -> unzip -> parse -> kill_event + chat_event
//! value:   kill_event -> KillCtx -> Impact per player (victim class, map, side)
//! ```

use crate::rawlog::{self, Kill, RawLog};
use crate::sources::Sources;
use crate::Progress;
use anyhow::{Context, Result};
use hl_core::matchdata::{NormalizedLog, Team};
use hl_core::{SteamId, TfClass};
use hl_db::{ChatRow, Db, KillRow, RoundWindow, StoredKill};
use hl_rating::detail::EventRow;
use hl_rating::impact::{impacts, Impact, KillCtx};
use hl_rating::{MatchDetail, Weights};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawlogSummary {
    pub fetched: usize,
    /// logs.tf had no raw file.
    pub missing: usize,
    pub failed: usize,
    pub kills: usize,
    /// logs.tf stopped answering; the rest waits for the next sync.
    pub gave_up: bool,
}

/// Consecutive logs we could not connect about before giving up on the server.
const GIVE_UP_AFTER: usize = 3;

/// Fetch raw logs for kept Highlander logs that lack one, newest first, and
/// derive their kills. `max` caps how many are fetched in one go.
pub async fn fetch(
    db: &Db,
    sources: &Sources,
    max: Option<usize>,
    mut progress: impl FnMut(Progress),
) -> Result<RawlogSummary> {
    let mut queue = db.rawlog_queue().await?;
    if let Some(m) = max {
        queue.truncate(m);
    }
    let total = queue.len();
    let mut s = RawlogSummary::default();
    // Same rule as the log fetch: a server that is not answering is not a
    // reason to spend an hour asking it politely.
    let mut unreachable_run = 0usize;
    for (i, log_id) in queue.into_iter().enumerate() {
        progress(Progress::RawLogs { done: i, total });
        // drops.tf's copy first, logs.tf's zip behind it.
        match sources.rawlog_zip(log_id).await {
            Ok(None) => {
                unreachable_run = 0;
                db.mark_rawlog_missing(log_id, "neither drops.tf nor logs.tf has a raw log").await?;
                s.missing += 1;
            }
            // A zip that does not open is not stored: it would only fail again
            // on every rebuild.
            Ok(Some(zip)) => match rawlog::unzip(&zip) {
                Ok(text) => {
                    unreachable_run = 0;
                    db.store_rawlog(log_id, &zip).await?;
                    s.kills += store(db, log_id, rawlog::parse(&text)).await?;
                    s.fetched += 1;
                }
                Err(e) => {
                    tracing::warn!(log_id, error = %format!("{e:#}"), "raw log unreadable");
                    db.mark_rawlog_missing(log_id, "unreadable zip").await?;
                    s.failed += 1;
                }
            },
            Err(e) if crate::http::unreachable(&e) => {
                unreachable_run += 1;
                tracing::warn!(log_id, error = %format!("{e:#}"), "logs.tf unreachable");
                // A refusal ends the pass at once (see `http::Refused`).
                if unreachable_run >= GIVE_UP_AFTER || crate::http::refused(&e) {
                    s.gave_up = true;
                    progress(Progress::GaveUp { source: "logs.tf", done: i, total, what: "raw server logs" });
                    break;
                }
            }
            Err(e) => {
                unreachable_run = 0;
                tracing::warn!(log_id, error = %format!("{e:#}"), "raw log fetch failed");
                s.failed += 1;
            }
        }
    }
    progress(Progress::RawLogs { done: total, total });
    Ok(s)
}


/// Re-derive every stored raw log's kills. No network; part of a rebuild.
pub async fn rederive_all(db: &Db, mut progress: impl FnMut(Progress)) -> Result<usize> {
    let ids = db.rawlog_ids().await?;
    let total = ids.len();
    let mut kills = 0;
    // Parsed and shifted logs waiting to be written, a group at a time.
    let mut group: Vec<(i64, RawLog)> = Vec::with_capacity(REBUILD_GROUP);
    for (i, log_id) in ids.into_iter().enumerate() {
        if i % 25 == 0 {
            progress(Progress::RawLogs { done: i, total });
        }
        let Some(zip) = db.rawlog(log_id).await? else { continue };
        match rawlog::unzip(&zip) {
            Ok(text) => group.push((log_id, shifted(db, log_id, rawlog::parse(&text)).await?)),
            Err(e) => tracing::warn!(log_id, error = %format!("{e:#}"), "stored raw log unreadable"),
        }
        if group.len() == REBUILD_GROUP {
            kills += write_group(db, &group).await?;
            group.clear();
        }
    }
    kills += write_group(db, &group).await?;
    progress(Progress::RawLogs { done: total, total });
    Ok(kills)
}

/// Write a group of logs' kills and chat in one transaction.
async fn write_group(db: &Db, group: &[(i64, RawLog)]) -> Result<usize> {
    if group.is_empty() {
        return Ok(0);
    }
    let batch: Vec<(i64, Vec<KillRow>, Vec<ChatRow>)> = group
        .iter()
        .map(|(log_id, log)| {
            let (k, c) = rows(log);
            (*log_id, k, c)
        })
        .collect();
    db.replace_kill_events_many(&batch).await.context("storing a group of re-derived kills")?;
    Ok(batch.iter().map(|(_, k, _)| k.len()).sum())
}

/// Store a parsed raw log's kills and chat, shifted into logs.tf's round-time
/// frame (see [`rawlog::frame_offset`]).
///
/// Takes the log by value and shifts it in place. It used to borrow it and
/// clone the whole thing -- every kill, chat line and event -- only to shift
/// the copy, 755 times a rebuild, while the caller dropped the original.
async fn store(db: &Db, log_id: i64, log: RawLog) -> Result<usize> {
    let log = shifted(db, log_id, log).await?;
    let (kills, chat) = rows(&log);
    db.replace_kill_events(log_id, &kills, &chat).await.with_context(|| format!("storing kills of {log_id}"))?;
    Ok(kills.len())
}

/// Make sure one log's raw log is stored, fetching it now if not: for a
/// demo being linked to an old match the sync has not reached yet (Q54),
/// which has no kills to line the demo up with until it is. `false` when
/// neither drops.tf nor logs.tf has one.
pub async fn ensure_one(db: &Db, sources: &Sources, log_id: i64) -> Result<bool> {
    if db.rawlog(log_id).await?.is_some() {
        return Ok(true);
    }
    let Some(zip) = sources.rawlog_zip(log_id).await? else {
        return Ok(false);
    };
    let text = rawlog::unzip(&zip)?;
    db.store_rawlog(log_id, &zip).await?;
    store(db, log_id, rawlog::parse(&text)).await?;
    Ok(true)
}

/// Re-derive one stored raw log's kills and chat, for a log that did not
/// come through the fetch (Q18: one built from a demo).
pub async fn derive_log(db: &Db, log_id: i64) -> Result<usize> {
    let zip = db.rawlog(log_id).await?.with_context(|| format!("log {log_id} has no raw log"))?;
    store(db, log_id, rawlog::parse(&rawlog::unzip(&zip)?)).await
}

/// A parsed raw log moved into logs.tf's round-time frame, in place.
async fn shifted(db: &Db, log_id: i64, mut log: RawLog) -> Result<RawLog> {
    let logstf_starts = logstf_round_starts(db, log_id).await?;
    match rawlog::frame_offset(&log.round_starts, &logstf_starts) {
        Some(shift) => log.shift(shift),
        None if !logstf_starts.is_empty() => {
            tracing::warn!(log_id, "raw log rounds do not line up with logs.tf's; times left as the server wrote them")
        }
        None => {}
    }
    Ok(log)
}

/// The rows a log's kills and chat are stored as, borrowing from it.
fn rows(log: &RawLog) -> (Vec<KillRow<'_>>, Vec<ChatRow<'_>>) {
    let kills = log.kills.iter().map(kill_row).collect();
    let chat = log
        .chat
        .iter()
        .map(|c| ChatRow { at_raw: c.at, account: c.account, team_chat: c.team_chat, message: &c.message })
        .collect();
    (kills, chat)
}

/// Logs a rebuild writes per transaction: see `replace_kill_events_many`.
const REBUILD_GROUP: usize = 100;

/// logs.tf's round start times for a stored log.
async fn logstf_round_starts(db: &Db, log_id: i64) -> Result<Vec<i64>> {
    let Some(json) = db.raw_log(log_id).await? else { return Ok(Vec::new()) };
    let v: serde_json::Value = serde_json::from_str(&json).with_context(|| format!("log {log_id} JSON"))?;
    Ok(v.get("rounds")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| r.get("start_time").and_then(serde_json::Value::as_i64))
        .collect())
}

fn kill_row(k: &Kill) -> KillRow<'_> {
    KillRow {
        at_raw: k.at,
        round_num: k.round as i64,
        live: k.live,
        killer: k.killer.account,
        killer_team: k.killer.team.map(|t| t.as_str()),
        killer_class: k.killer.class.map(|c| c.as_str()),
        victim: k.victim.account,
        victim_team: k.victim.team.map(|t| t.as_str()),
        victim_class: k.victim.class.map(|c| c.as_str()),
        weapon: &k.weapon,
        custom: k.custom.as_deref(),
        assister: k.assister,
        killer_pos: k.killer_pos,
        victim_pos: k.victim_pos,
    }
}

/// A parsed raw log's kills as the kill table would give them back, in
/// order -- each kill's index is its `seq`. For logs whose kills are not
/// stored: the league sample's, valued in memory.
pub fn stored_kills(log: &RawLog) -> Vec<hl_db::StoredKill> {
    log.kills
        .iter()
        .map(|k| hl_db::StoredKill {
            at_raw: k.at,
            live: k.live,
            killer: k.killer.account,
            killer_class: k.killer.class.map(|c| c.as_str().to_string()),
            victim: k.victim.account,
            victim_team: k.victim.team.map(|t| t.as_str().to_string()),
            victim_class: k.victim.class.map(|c| c.as_str().to_string()),
            weapon: k.weapon.clone(),
            custom: k.custom.clone(),
            assister: k.assister,
        })
        .collect()
}

/// A stored kill, reduced to what valuing it needs.
pub fn ctx(k: &StoredKill, situation: Option<(i8, i8)>) -> KillCtx {
    KillCtx {
        situation,
        killer: k.killer,
        assister: k.assister,
        victim_class: k.victim_class.as_deref().and_then(|c| TfClass::parse(c).ok()),
        victim_team: k.victim_team.as_deref().and_then(Team::parse),
        counts: k.live && k.custom.as_deref() != Some("feign_death"),
        assist_counts: k.live && k.assister.is_some(),
    }
}

/// Each player's kills in one log, valued in context. Empty without a raw
/// log, and the rating falls back to `classkills`.
///
/// Each kill is valued on its round's map (`windows`, from the round-map
/// pass); a kill outside every window, or a log not yet resolved, falls back
/// to the log's own map name. `situations` is the fights pass's per-kill
/// state, keyed by the kill's `seq` (its index here: every kill is stored).
pub fn impacts_for(
    kills: &[StoredKill],
    situations: Option<&HashMap<i64, (i8, i8)>>,
    windows: &[RoundWindow],
    log_map: Option<&str>,
    w: &Weights,
) -> HashMap<u32, Impact> {
    if kills.is_empty() {
        return HashMap::new();
    }
    impacts(
        kills.iter().enumerate().map(|(seq, k)| {
            let s = situations.and_then(|m| m.get(&(seq as i64)).copied());
            (ctx(k, s), map_at(windows, k.at_raw).or(log_map))
        }),
        w,
    )
}

/// Add each player's fight counts (from `Db::fight_counts`) to their impact,
/// creating an entry for players with no kills.
pub fn attach_fights(impacts: &mut HashMap<u32, Impact>, rows: &[(u32, [u32; 14])]) {
    for &(
        account,
        [opening_kills, opening_deaths, kills, traded_kills, deaths, traded_deaths, flank_deaths, stationary_deaths, fights_present, fights_kast, fights_kast_engaged, caps_contested, caps_mates_dead, caps_spawn_delay],
    ) in rows
    {
        impacts.entry(account).or_default().fights = Some(hl_rating::FightCounts {
            opening_kills,
            opening_deaths,
            kills,
            traded_kills,
            deaths,
            traded_deaths,
            flank_deaths,
            stationary_deaths,
            fights_present,
            fights_kast,
            fights_kast_engaged,
            caps_contested,
            caps_mates_dead,
            caps_spawn_delay,
        });
    }
}

/// Add each player's shared fight swing (Q6b) to their impact.
///
/// Kept beside `impacts_for` rather than inside it, as `attach_fights` is:
/// the credits come from the fights pass and every caller already holds
/// the kills and situations this needs. A kill with no stored credits --
/// a log the pass has not read at version 6 -- credits its killer alone,
/// which is exactly the unshared swing.
pub fn attach_shared_swing(
    impacts: &mut HashMap<u32, Impact>,
    kills: &[StoredKill],
    situations: Option<&HashMap<i64, (i8, i8)>>,
    credits: Option<&HashMap<i64, Vec<(u32, f64)>>>,
    w: &Weights,
) {
    let Some(situations) = situations else { return };
    for (seq, k) in kills.iter().enumerate() {
        if !(k.live && k.custom.as_deref() != Some("feign_death")) {
            continue;
        }
        let Some(&(d, a)) = situations.get(&(seq as i64)) else { continue };
        let Some(s) = w.swing(d, a) else { continue };
        match credits.and_then(|c| c.get(&(seq as i64))) {
            Some(shares) => {
                for &(who, share) in shares {
                    *impacts.entry(who).or_default().swing_shared.get_or_insert(0.0) += s * share;
                }
            }
            None => *impacts.entry(k.killer).or_default().swing_shared.get_or_insert(0.0) += s,
        }
    }
}

/// The map of the round holding `t` (logs.tf's round-time frame).
pub fn map_at(windows: &[RoundWindow], t: i64) -> Option<&str> {
    windows
        .iter()
        .find(|w| t >= w.start && t <= w.start + w.length)
        .and_then(|w| w.map.as_deref())
}

/// The owner's own kills and deaths as timeline markers, placed in their
/// rounds. Added before demo enrichment, which then gives each a jump.
pub fn enrich(log: &NormalizedLog, kills: &[StoredKill], me: Option<SteamId>, detail: &mut MatchDetail) {
    let Some(me) = me.map(|m| m.account_id()) else { return };
    let names: HashMap<u32, String> = detail.players.iter().map(|p| (p.account_id, p.name.clone())).collect();
    let my_team = detail.my_team;

    for k in kills.iter().filter(|k| k.live && (k.killer == me || k.victim == me)) {
        let feign = k.custom.as_deref() == Some("feign_death");
        if feign && k.killer == me {
            // Not a kill: the Spy got up again.
            continue;
        }
        let hit = log.rounds.iter().zip(detail.rounds.iter_mut()).find(|(r, _)| {
            matches!((r.start_time, r.length_s), (Some(s), Some(len)) if k.at_raw >= s && k.at_raw <= s + len)
        });
        let Some((r, round)) = hit else { continue };
        let mine = k.killer == me;
        let class = if mine { k.victim_class.as_deref() } else { k.killer_class.as_deref() };
        let detail_bits: Vec<&str> = [class, k.custom.as_deref().filter(|c| *c != "feign_death"), Some(k.weapon.as_str())]
            .into_iter()
            .flatten()
            .collect();
        round.events.push(EventRow {
            at_s: k.at_raw - r.start_time.unwrap_or(k.at_raw),
            kind: if mine { "my_kill" } else { "my_death" }.to_string(),
            // The team that lost a player: theirs for a kill, ours for a death.
            team: my_team.map(|t| if mine { t.other() } else { t }),
            player: names.get(&k.victim).cloned(),
            killer: names.get(&k.killer).cloned(),
            killer_is_me: mine,
            medigun: None,
            point: None,
            value: Some(detail_bits.join(" · ")),
            jump: None,
        });
    }
    for round in &mut detail.rounds {
        round.events.sort_by_key(|e| e.at_s);
    }
}
