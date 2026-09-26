//! Sync and reprocess.
//!
//! ```text
//! sync:      trends.tf index -> logs.tf search -> dedupe -> ETF2L -> fetch queue -> normalize -> rate
//! reprocess: stored index    ->                   dedupe ->               normalize
//! ```
//!
//! Reprocess touches no network. That is the whole point of storing sources
//! verbatim: when parsing or classification rules change, every derived table
//! is rebuilt in seconds.

use crate::classify::classify;
use crate::dedupe::{supersessions, IndexEntry};
use crate::normalize::normalize;
use crate::sources::{LogsTfRow, Sources, TrendsRow};
use anyhow::{Context, Result};
use hl_core::SteamId;
use hl_db::{Db, IndexStats, LogsTfIndexRow, TrendsIndexRow};
use serde::Serialize;

/// A log that has failed this many times is left alone until retried by hand.
const MAX_FETCH_ATTEMPTS: i64 = 3;

/// Consecutive logs we could not connect about before concluding that the
/// server, not the log, is the problem.
const GIVE_UP_AFTER: usize = 3;

#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    /// Ignore the incremental cursor and re-index everything.
    pub full: bool,
    /// Stop after fetching this many logs. For testing and partial syncs.
    pub max_fetch: Option<usize>,
}

/// Progress as the UI and CLI see it.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Progress {
    #[serde(rename_all = "camelCase")]
    Indexing { source: &'static str, rows: usize },
    #[serde(rename_all = "camelCase")]
    Indexed { trends_rows: usize, logstf_rows: usize, superseded: usize },
    #[serde(rename_all = "camelCase")]
    Fetching { done: usize, total: usize, log_id: i64 },
    #[serde(rename_all = "camelCase")]
    FetchFailed { log_id: i64, error: String },
    #[serde(rename_all = "camelCase")]
    Reprocessing { done: usize, total: usize },
    #[serde(rename_all = "camelCase")]
    Rating { done: usize, total: usize },
    /// ETF2L match fetches.
    #[serde(rename_all = "camelCase")]
    Etf2l { done: usize, total: usize },
    /// The per-map logs combined logs were built from.
    #[serde(rename_all = "camelCase")]
    Parts { done: usize, total: usize },
    /// Raw server logs from logs.tf.
    #[serde(rename_all = "camelCase")]
    RawLogs { done: usize, total: usize },
    /// A source could not be reached. The sync carries on without it: every
    /// one of them adds to what is already stored rather than replacing it.
    #[serde(rename_all = "camelCase")]
    SourceFailed { source: &'static str, error: String },
    /// Downloading stopped early because the server stopped answering. What
    /// is left waits for the next sync, unmarked.
    #[serde(rename_all = "camelCase")]
    GaveUp { source: &'static str, done: usize, total: usize },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub fetched: usize,
    pub failed: usize,
    pub stats: IndexStats,
}

pub async fn sync(
    db: &Db,
    sources: &Sources,
    me: SteamId,
    opts: &SyncOptions,
    w: &hl_rating::Weights,
    mut progress: impl FnMut(Progress),
) -> Result<SyncSummary> {
    let steamid64 = me.to_steamid64();

    // 1. trends.tf: the classified index.
    let since = if opts.full { None } else { db.trends_cursor().await? };
    let trends = sources
        .trends_index(&steamid64, since, |rows| {
            progress(Progress::Indexing { source: "trends.tf", rows })
        })
        .await
        .context("indexing trends.tf")?;
    store_trends(db, &trends.iter().map(|(r, raw)| (r.clone(), raw.as_str())).collect::<Vec<_>>())
        .await?;
    if let Some(cursor) = trends.iter().filter_map(|(r, _)| r.updated).max() {
        db.record_sync("trends", Some(&cursor.to_string()), None).await?;
    }

    // 2. logs.tf search: catches what trends.tf never indexed. A supplement,
    //    so a logs.tf that is down costs those older logs, not the sync.
    progress(Progress::Indexing { source: "logs.tf", rows: 0 });
    let logstf = match sources.logstf_search(&steamid64).await {
        Ok(rows) => {
            store_logstf(db, &rows).await?;
            db.record_sync("logstf_search", None, None).await?;
            rows
        }
        Err(e) => {
            let error = format!("{e:#}");
            tracing::warn!(%error, "logs.tf index failed");
            progress(Progress::SourceFailed { source: "logs.tf", error });
            Vec::new()
        }
    };

    // 3. Collapse combined logs and their parts.
    let superseded = recompute_supersessions(db).await?;
    progress(Progress::Indexed {
        trends_rows: trends.len(),
        logstf_rows: logstf.len(),
        superseded,
    });

    // 4. ETF2L, before anything is fetched rather than after.
    //
    // Whether a log is an official decides whether it is worth downloading at
    // all, and the roster match that settles it needs the player list from
    // inside the file. The scheduled times do not: they place a log as an
    // official from the index alone. ETF2L being down is not a failed sync —
    // the marks are simply not refreshed this time.
    match crate::etf2l::fetch(db, sources, me, |done, total| {
        progress(Progress::Etf2l { done, total });
    })
    .await
    {
        Ok(_) => {
            let marked = crate::etf2l::mark_by_time(db).await?;
            tracing::info!(marked, "logs placed inside an official's window");
        }
        Err(e) => {
            let error = format!("{e:#}");
            tracing::warn!(%error, "ETF2L fetch failed");
            progress(Progress::SourceFailed { source: "ETF2L", error });
        }
    }

    // 5. Fetch and normalize, newest first.
    let mut queue = db.fetch_queue(MAX_FETCH_ATTEMPTS).await?;
    if let Some(max) = opts.max_fetch {
        queue.truncate(max);
    }
    let total = queue.len();
    let (mut fetched, mut failed) = (0, 0);

    // A log that logs.tf answered about badly has earned its failed attempt.
    // A log we could not connect about has not, and three attempts is all a
    // log gets before it waits for a retry by hand — so an outage must not
    // spend them. Consecutive unreachable logs end the pass instead.
    let mut unreachable_run = 0usize;

    for (i, log_id) in queue.into_iter().enumerate() {
        progress(Progress::Fetching { done: i, total, log_id });
        let result = async {
            let json = sources.logstf_log(log_id).await?;
            db.store_raw_log(log_id, &json).await?;
            process_raw(db, log_id, &json).await
        }
        .await;

        match result {
            Ok(()) => {
                unreachable_run = 0;
                fetched += 1;
                // Rated now rather than at the end of the sync, so the row
                // that just appeared in the list arrives with its number on
                // it. Against the stored baselines, which a few new games
                // barely move; the full pass at the end refines it.
                if let Err(e) = crate::rating::rate_logs(db, w, &[log_id]).await {
                    tracing::warn!(log_id, error = %format!("{e:#}"), "rating a new log failed");
                }
            }
            Err(e) if crate::http::unreachable(&e) => {
                unreachable_run += 1;
                tracing::warn!(log_id, error = %format!("{e:#}"), "logs.tf unreachable");
                if unreachable_run >= GIVE_UP_AFTER {
                    tracing::warn!(done = i, total, "logs.tf stopped answering; the rest waits for the next sync");
                    progress(Progress::GaveUp { source: "logs.tf", done: i, total });
                    break;
                }
            }
            Err(e) => {
                unreachable_run = 0;
                failed += 1;
                let error = format!("{e:#}");
                tracing::warn!(log_id, %error, "fetch failed");
                db.record_fetch_error(log_id, &error).await?;
                progress(Progress::FetchFailed { log_id, error });
            }
        }
    }
    progress(Progress::Fetching { done: total, total, log_id: 0 });
    // New logs have rounds now: catch any that are parts of another.
    recompute_supersessions(db).await?;

    Ok(SyncSummary { fetched, failed, stats: db.index_stats().await? })
}

/// Rebuild every derived table from stored sources. No network.
pub async fn reprocess(db: &Db, mut progress: impl FnMut(Progress)) -> Result<IndexStats> {
    // Re-derive the index columns from the stored trends.tf rows, so a change
    // to how those rows are interpreted takes effect too.
    let raw_rows = db.trends_raw_rows().await?;
    let parsed: Vec<(TrendsRow, &str)> = raw_rows
        .iter()
        .map(|raw| Ok((serde_json::from_str(raw).context("stored trends.tf row")?, raw.as_str())))
        .collect::<Result<_>>()?;
    store_trends(db, &parsed).await?;
    recompute_supersessions(db).await?;

    let ids = db.raw_log_ids().await?;
    let total = ids.len();
    for (i, log_id) in ids.into_iter().enumerate() {
        progress(Progress::Reprocessing { done: i, total });
        let Some(json) = db.raw_log(log_id).await? else { continue };
        if let Err(e) = process_raw(db, log_id, &json).await {
            // One bad blob must not stop a rebuild of the other thousand.
            tracing::warn!(log_id, error = %format!("{e:#}"), "reprocess failed for log");
        }
    }
    progress(Progress::Reprocessing { done: total, total });
    recompute_supersessions(db).await?;

    db.index_stats().await
}

/// Normalize one stored log, write it, and classify it where trends.tf could not.
async fn process_raw(db: &Db, log_id: i64, json: &str) -> Result<()> {
    let value: serde_json::Value =
        serde_json::from_str(json).with_context(|| format!("log {log_id} is not valid JSON"))?;
    let log = normalize(log_id, &value)?;
    db.write_match(&log).await?;
    // Safe to call unconditionally: the database only applies a heuristic
    // format where trends.tf gave none.
    db.set_heuristic_format(log_id, classify(&log)).await?;
    Ok(())
}

async fn store_trends(db: &Db, rows: &[(TrendsRow, &str)]) -> Result<()> {
    let flat: Vec<TrendsIndexRow> = rows
        .iter()
        .map(|(r, raw)| TrendsIndexRow {
            log_id: r.logid,
            title: non_empty(&r.title),
            map: non_empty(&r.map),
            played_at: r.time,
            duration_s: r.duration,
            format: r.format.as_deref(),
            league: r.league.as_deref(),
            etf2l_match_id: r.matchid,
            demos_tf_id: r.demoid,
            duplicate_of: r.duplicate_of.as_deref(),
            raw_json: raw,
        })
        .collect();
    db.upsert_trends_rows(&flat).await
}

async fn store_logstf(db: &Db, rows: &[(LogsTfRow, String)]) -> Result<()> {
    let flat: Vec<LogsTfIndexRow> = rows
        .iter()
        .map(|(r, raw)| LogsTfIndexRow {
            log_id: r.id,
            title: non_empty(&r.title),
            map: non_empty(&r.map),
            played_at: r.date,
            player_count: r.players,
            raw_json: raw,
        })
        .collect();
    db.upsert_logstf_rows(&flat).await
}

async fn recompute_supersessions(db: &Db) -> Result<usize> {
    let mut entries: Vec<IndexEntry> = db
        .dedupe_inputs()
        .await?
        .into_iter()
        .map(|(log_id, duration_s, duplicate_of)| IndexEntry { log_id, duration_s, duplicate_of })
        .collect();
    // Parts trends.tf never listed, found by their rounds. Only logs already
    // normalized have rounds, so this runs again after new logs are read.
    let rounds: std::collections::HashMap<i64, Vec<i64>> = db
        .all_rounds()
        .await?
        .into_iter()
        .map(|(log, rs)| (log, rs.into_iter().map(|r| r.start).collect()))
        .collect();
    for (whole, part) in crate::dedupe::contained_parts(&rounds) {
        if let Some(e) = entries.iter_mut().find(|e| e.log_id == whole) {
            if !e.duplicate_of.contains(&part) {
                e.duplicate_of.push(part);
            }
        }
    }
    let map = supersessions(&entries);
    db.apply_supersessions(&map).await?;
    Ok(map.len())
}

/// Both sources send `""` for a log with no map (43 on this account). Store
/// that as missing, so it reads the same as any other absent field.
fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.trim().is_empty())
}

// ---------------------------------------------------------------------------
// Importing one log by hand.
//
// A sync gives up on a log after three attempts and says "2 failed" with no
// way to see which two or do anything about them (KamikaZe, September 2026).
// Sometimes the answer is that logs.tf is missing the log, or has it under an
// id trends.tf never listed, or the log is a pug the index never claimed. All
// of those end the same way: the person knows the log id and wants it in.
// ---------------------------------------------------------------------------

/// A log asked for by hand: the id, and what happened.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub log_id: i64,
    pub title: Option<String>,
    pub map: Option<String>,
    pub played_at: Option<i64>,
    pub players: usize,
    /// Whether the owner is in it. A log they did not play is stored all the
    /// same — it still feeds the pool everyone is rated against — but it will
    /// not show up in their match list, and saying so avoids a bug report.
    pub yours: bool,
}

/// Read a log id out of whatever the person pasted: a number, a logs.tf URL,
/// or one with the `#` anchor logs.tf puts on a player's row.
pub fn parse_log_id(text: &str) -> Option<i64> {
    let t = text.trim().trim_end_matches('/');
    let tail = t.rsplit('/').next().unwrap_or(t);
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|id| *id > 0)
}

/// Fetch one log now, whatever the index thinks of it.
///
/// Unlike the sync this does not consult the fetch queue: a log that has
/// failed three times, or that no index ever listed, is exactly what this is
/// for. A success clears the failure — `store_raw_log` does that in the same
/// transaction — so the list of failures is the list of things still wrong.
pub async fn import_log(
    db: &Db,
    sources: &Sources,
    w: &hl_rating::Weights,
    log_id: i64,
) -> Result<Imported> {
    let json = sources
        .logstf_log(log_id)
        .await
        .with_context(|| format!("logs.tf could not give us log {log_id}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&json).with_context(|| format!("log {log_id} is not valid JSON"))?;
    let log = normalize(log_id, &value)?;

    // Index it from the log itself, so a log no index ever listed still has
    // a row to be listed, filtered and classified by.
    db.upsert_logstf_rows(&[LogsTfIndexRow {
        log_id,
        title: log.title.as_deref(),
        map: log.map.as_deref(),
        played_at: log.played_at,
        player_count: Some(log.players.len() as i64),
        raw_json: &json,
    }])
    .await?;
    db.store_raw_log(log_id, &json).await?;
    db.write_match(&log).await?;
    db.set_heuristic_format(log_id, classify(&log)).await?;
    recompute_supersessions(db).await?;

    let yours = match db.get_me().await? {
        Some(me) => log.players.iter().any(|p| p.id == me),
        None => false,
    };
    if let Err(e) = crate::rating::rate_logs(db, w, &[log_id]).await {
        tracing::warn!(log_id, error = %format!("{e:#}"), "rating an imported log failed");
    }
    Ok(Imported {
        log_id,
        title: log.title.clone(),
        map: log.map.clone(),
        played_at: log.played_at,
        players: log.players.len(),
        yours,
    })
}

#[cfg(test)]
mod import_tests {
    use super::parse_log_id;

    #[test]
    fn a_log_id_is_read_out_of_whatever_was_pasted() {
        assert_eq!(parse_log_id("4042136"), Some(4042136));
        assert_eq!(parse_log_id("  4042136 "), Some(4042136));
        assert_eq!(parse_log_id("https://logs.tf/4042136"), Some(4042136));
        assert_eq!(parse_log_id("https://logs.tf/4042136/"), Some(4042136));
        assert_eq!(parse_log_id("logs.tf/4042136"), Some(4042136));
        // logs.tf puts a player anchor on the link you get from a row.
        assert_eq!(parse_log_id("https://logs.tf/4042136#76561198099396919"), Some(4042136));
    }

    #[test]
    fn anything_else_is_refused_rather_than_guessed_at() {
        assert_eq!(parse_log_id(""), None);
        assert_eq!(parse_log_id("the vigil game"), None);
        assert_eq!(parse_log_id("https://logs.tf/"), None);
        assert_eq!(parse_log_id("0"), None, "there is no log zero");
        assert_eq!(parse_log_id("https://demos.tf/1507898"), Some(1507898), "close enough: it is a number");
    }
}
