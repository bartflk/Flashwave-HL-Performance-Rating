//! Sync and reprocess.
//!
//! ```text
//! sync:      trends.tf index -> logs.tf search -> dedupe -> fetch queue -> normalize
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
    /// ETF2L could not be reached; the rest of the sync carries on.
    #[serde(rename_all = "camelCase")]
    Etf2lFailed { error: String },
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

    // 2. logs.tf search: catches what trends.tf never indexed.
    progress(Progress::Indexing { source: "logs.tf", rows: 0 });
    let logstf = sources.logstf_search(&steamid64).await.context("indexing logs.tf")?;
    store_logstf(db, &logstf).await?;
    db.record_sync("logstf_search", None, None).await?;

    // 3. Collapse combined logs and their parts.
    let superseded = recompute_supersessions(db).await?;
    progress(Progress::Indexed {
        trends_rows: trends.len(),
        logstf_rows: logstf.len(),
        superseded,
    });

    // 4. Fetch and normalize, newest first.
    let mut queue = db.fetch_queue(MAX_FETCH_ATTEMPTS).await?;
    if let Some(max) = opts.max_fetch {
        queue.truncate(max);
    }
    let total = queue.len();
    let (mut fetched, mut failed) = (0, 0);

    for (i, log_id) in queue.into_iter().enumerate() {
        progress(Progress::Fetching { done: i, total, log_id });
        let result = async {
            let json = sources.logstf_log(log_id).await?;
            db.store_raw_log(log_id, &json).await?;
            process_raw(db, log_id, &json).await
        }
        .await;

        match result {
            Ok(()) => fetched += 1,
            Err(e) => {
                failed += 1;
                let error = format!("{e:#}");
                tracing::warn!(log_id, %error, "fetch failed");
                db.record_fetch_error(log_id, &error).await?;
                progress(Progress::FetchFailed { log_id, error });
            }
        }
    }
    progress(Progress::Fetching { done: total, total, log_id: 0 });

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
    let entries: Vec<IndexEntry> = db
        .dedupe_inputs()
        .await?
        .into_iter()
        .map(|(log_id, duration_s, duplicate_of)| IndexEntry { log_id, duration_s, duplicate_of })
        .collect();
    let map = supersessions(&entries);
    db.apply_supersessions(&map).await?;
    Ok(map.len())
}

/// Both sources send `""` for a log with no map (43 on this account). Store
/// that as missing, so it reads the same as any other absent field.
fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.trim().is_empty())
}
