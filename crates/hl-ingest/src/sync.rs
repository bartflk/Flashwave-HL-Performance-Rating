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

/// The match a slow step is on, for the sync card to name it (Flashy):
/// "Official vs Kebab · upward · 14 Sept".
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingMatch {
    /// Without its version: `pl_upward`.
    pub map: Option<String>,
    pub played_at: Option<i64>,
    /// "official", "scrim" or "pug".
    pub kind: Option<String>,
    pub opponent: Option<String>,
}

impl ReadingMatch {
    pub async fn of(db: &Db, log_id: i64) -> Option<ReadingMatch> {
        let (map, played_at, kind, opponent) = db.match_label(log_id).await.ok()??;
        Some(ReadingMatch { map: map.filter(|m| !m.is_empty()).map(|m| hl_core::maps::map_name(&m)), played_at, kind, opponent: opponent.filter(|o| !o.is_empty()) })
    }
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
    /// A short stage with no count of its own, named so the card can say
    /// what is happening and the bar can run indeterminate rather than sit
    /// at a number looking stuck (PLAN Q24). Seconds, not minutes -- long
    /// work reports a fraction instead.
    #[serde(rename_all = "camelCase")]
    Stage { what: &'static str },
    /// Fights: who was in each one, and what each kill's state was.
    #[serde(rename_all = "camelCase")]
    Fights { done: usize, total: usize },
    /// Demos being read for aim, routes and deaths (PLAN Q24).
    ///
    /// The slowest thing the app does -- about 16 s a demo, and 396 s for
    /// twenty-five of them here. It counted its own work all along and the
    /// window was handed `|_, _| {}`, so the sync card said "Sync finished"
    /// and then sat silent for minutes. `logId` names the match being read,
    /// because a job this slow should say which one it is on.
    #[serde(rename_all = "camelCase")]
    ReadingDemos { done: usize, total: usize, log_id: Option<i64>, what: Option<ReadingMatch> },
    /// A source could not be reached. The sync carries on without it: every
    /// one of them adds to what is already stored rather than replacing it.
    #[serde(rename_all = "camelCase")]
    SourceFailed { source: &'static str, error: String },
    /// Downloading stopped early because the server stopped answering. What
    /// is left waits for the next sync, unmarked.
    #[serde(rename_all = "camelCase")]
    GaveUp { source: &'static str, done: usize, total: usize, what: &'static str },
    /// logs.tf refused us, so new logs come from more.tf's copy instead
    /// (see `moretf.rs`).
    #[serde(rename_all = "camelCase")]
    StandIns { done: usize, total: usize },
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
    // Where logs.tf stopped giving us logs, if it did.
    let mut stopped_at: Option<usize> = None;

    for (i, log_id) in queue.iter().copied().enumerate() {
        progress(Progress::Fetching { done: i, total, log_id });
        let result = async {
            // drops.tf first, logs.tf for what it does not have yet.
            let (json, _) = sources.log_json(log_id).await?;
            db.store_raw_log(log_id, &json).await?;
            process_raw(db, log_id, &json).await
        }
        .await;

        match result {
            Ok(()) => {
                unreachable_run = 0;
                fetched += 1;
                // A log stored from more.tf has its real JSON now.
                db.clear_stand_in(log_id).await?;
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
                // A refusal ends the pass at once: every further request
                // only keeps logs.tf's ban on this address going.
                if unreachable_run >= GIVE_UP_AFTER || crate::http::refused(&e) {
                    tracing::warn!(done = i, total, "logs.tf stopped answering; the rest waits for the next sync");
                    progress(Progress::GaveUp { source: "logs.tf", done: i, total, what: "logs" });
                    stopped_at = Some(i + 1 - unreachable_run);
                    break;
                }
            }
            Err(e) => {
                unreachable_run = 0;
                failed += 1;
                let error = format!("{e:#}");
                tracing::warn!(log_id, %error, "fetch failed");
                // Deleted on logs.tf: never asked again, and more.tf's copy is
                // tried below (Q77).
                db.record_fetch_error(log_id, &error, crate::http::not_found(&e)).await?;
                progress(Progress::FetchFailed { log_id, error });
            }
        }
    }
    progress(Progress::Fetching { done: total, total, log_id: 0 });

    // What logs.tf would not give us, from more.tf's copy of it.
    if let Some(from) = stopped_at {
        fetched += stand_ins(db, sources, w, &queue[from..], &mut progress).await?;
    }
    // A log deleted from logs.tf may still be on more.tf: asked once, then
    // left given up whatever the answer (Q77). Not while more.tf would be
    // asked for the logs above anyway.
    if stopped_at.is_none() {
        let rescue = db.gone_rescue_todo(STAND_INS_PER_SYNC as i64).await?;
        if !rescue.is_empty() {
            let got = stand_ins(db, sources, w, &rescue, &mut progress).await?;
            db.mark_rescue_tried(&rescue).await?;
            if got > 0 {
                tracing::info!(got, asked = rescue.len(), "logs deleted from logs.tf found on more.tf");
            }
            fetched += got;
        }
    }
    // New logs have rounds now: catch any that are parts of another.
    recompute_supersessions(db).await?;

    Ok(SyncSummary { fetched, failed, stats: db.index_stats().await? })
}

/// How many logs one sync takes from more.tf. Newest first, so the game just
/// played is the first of them; the rest wait for logs.tf or the next sync.
const STAND_INS_PER_SYNC: usize = 30;

/// Store logs from more.tf while logs.tf refuses us: its JSON and a server
/// log written from it, rated like any other, and marked as stand-ins so the
/// sync asks logs.tf for the real ones again. Returns how many were stored.
async fn stand_ins(db: &Db, sources: &Sources, w: &hl_rating::Weights, ids: &[i64], progress: &mut impl FnMut(Progress)) -> Result<usize> {
    let mut todo = Vec::new();
    for &id in ids {
        if todo.len() == STAND_INS_PER_SYNC {
            break;
        }
        // Already a stand-in: more.tf's copy has not changed.
        if db.stand_in(id).await?.is_none() {
            todo.push(id);
        }
    }
    let total = todo.len();
    let (mut stored, mut unreachable_run) = (0, 0);
    for (i, log_id) in todo.into_iter().enumerate() {
        progress(Progress::StandIns { done: i, total });
        let result = async {
            let Some(body) = sources.moretf_log(log_id).await? else { return Ok(false) };
            let v: serde_json::Value = serde_json::from_str(&body)?;
            let s = crate::moretf::stand_in(&v)?;
            let json = serde_json::to_string(&s.json)?;
            db.store_raw_log(log_id, &json).await?;
            db.store_rawlog(log_id, &crate::demo_import::zip_text(&s.text)?).await?;
            db.mark_stand_in(log_id, crate::moretf::SOURCE).await?;
            process_raw(db, log_id, &json).await?;
            crate::kills::derive_log(db, log_id).await?;
            anyhow::Ok(true)
        }
        .await;
        match result {
            Ok(true) => {
                unreachable_run = 0;
                stored += 1;
                tracing::info!(log_id, "stored from more.tf while logs.tf refuses us");
                if let Err(e) = crate::rating::rate_logs(db, w, &[log_id]).await {
                    tracing::warn!(log_id, error = %format!("{e:#}"), "rating a new log failed");
                }
            }
            Ok(false) => {
                unreachable_run = 0;
                tracing::info!(log_id, "more.tf does not have this log either");
            }
            Err(e) if crate::http::unreachable(&e) => {
                unreachable_run += 1;
                tracing::warn!(log_id, error = %format!("{e:#}"), "more.tf unreachable");
                if unreachable_run >= GIVE_UP_AFTER || crate::http::refused(&e) {
                    progress(Progress::GaveUp { source: "more.tf", done: i, total, what: "logs" });
                    break;
                }
            }
            // Nothing is marked: the log stays in the queue for logs.tf.
            Err(e) => tracing::warn!(log_id, error = %format!("{e:#}"), "more.tf stand-in failed"),
        }
    }
    progress(Progress::StandIns { done: total, total });
    Ok(stored)
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
    let mut map = supersessions(&entries);
    // A combined log deleted from logs.tf would hide its parts behind a log
    // that will never arrive: the parts stand on their own instead, and are
    // downloaded like any other log (Q77).
    let gone: std::collections::HashSet<i64> = db.gone_logs().await?.into_iter().collect();
    map.retain(|_, keep| !gone.contains(keep));
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
    /// The combined log this one is a part of (Q71): the list shows that
    /// one instead, so a match is counted once.
    pub part_of: Option<i64>,
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
    let (json, _) = sources
        .log_json(log_id)
        .await
        .with_context(|| format!("neither drops.tf nor logs.tf could give us log {log_id}"))?;
    let mut imported = store_log_json(db, w, log_id, &json).await?;
    // A combined log brings its parts, so the match reads its maps and
    // halves one by one; best effort, the import stands either way.
    if serde_json::from_str::<serde_json::Value>(&json).is_ok_and(|v| looks_combined(&v)) {
        match find_parts(db, sources, w, log_id).await {
            Ok(f) => tracing::info!(log_id, candidates = f.candidates, added = f.added, "combined log: parts looked for"),
            Err(e) => tracing::info!(log_id, error = %format!("{e:#}"), "combined log: parts not looked for"),
        }
        imported.part_of = db.superseded_by(log_id).await?;
    }
    Ok(imported)
}

/// Store a log from its logs.tf JSON -- fetched, or built from a demo (Q18)
/// -- and rate it.
pub async fn store_log_json(db: &Db, w: &hl_rating::Weights, log_id: i64, json: &str) -> Result<Imported> {
    let value: serde_json::Value =
        serde_json::from_str(json).with_context(|| format!("log {log_id} is not valid JSON"))?;
    let log = normalize(log_id, &value)?;

    // Index it from the log itself, so a log no index ever listed still has
    // a row to be listed, filtered and classified by.
    db.upsert_logstf_rows(&[LogsTfIndexRow {
        log_id,
        title: log.title.as_deref(),
        map: log.map.as_deref(),
        played_at: log.played_at,
        player_count: Some(log.players.len() as i64),
        raw_json: json,
    }])
    .await?;
    db.store_raw_log(log_id, json).await?;
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
    // Q71: one part of a combined log is folded into it and leaves the list;
    // the import says which log to look for instead.
    let part_of = db.superseded_by(log_id).await?;
    Ok(Imported {
        log_id,
        title: log.title.clone(),
        map: log.map.clone(),
        played_at: log.played_at,
        players: log.players.len(),
        yours,
        part_of,
    })
}

/// What a search for a combined log's parts found.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundParts {
    /// logs.tf's logs with the same players in the match's window.
    pub candidates: usize,
    /// Of those, the ones whose every round is one of the match's: stored,
    /// and folded into it.
    pub added: usize,
}

/// Round starts of a log, to the second, from its logs.tf JSON.
fn round_starts(v: &serde_json::Value) -> Vec<i64> {
    v["rounds"].as_array().into_iter().flatten().filter_map(|r| r["start_time"].as_i64()).collect()
}

/// A log that is several logs in one: more than one map in its rounds'
/// title or its own, "combined" in its title, or longer than one map runs
/// (Flashy: the S36 High final as one 116-minute upload, its seven parts
/// unknown).
pub fn looks_combined(v: &serde_json::Value) -> bool {
    let title = v["info"]["title"].as_str().unwrap_or("").to_ascii_lowercase();
    let map = v["info"]["map"].as_str().unwrap_or("");
    let long = v["info"]["total_length"].as_i64().unwrap_or(0) >= 45 * 60;
    title.contains("combined") || map.contains('/') || map.contains('+') || long || round_starts(v).len() > 4
}

/// The parts of a combined log, found on logs.tf (Flashy: "i want this
/// system expanded ... so it can detect combined games"). trends.tf lists a
/// combined log's parts only once it has indexed them, and the parts' rounds
/// are only known for logs already downloaded -- so a fresh final, or one
/// opened from ETF2L, showed as one log of its own.
///
/// Asks logs.tf for the logs three of its players share, uploaded between
/// its first round and its own upload, and keeps those whose every round
/// starts at one of its round starts: logs.tf copies rounds verbatim when
/// combining, so that is not chance. Nothing else is stored, so the logs of
/// a restarted map or someone else's game never reach the match list.
pub async fn find_parts(db: &Db, sources: &Sources, w: &hl_rating::Weights, log_id: i64) -> Result<FoundParts> {
    let Some(json) = db.raw_log(log_id).await? else { return Ok(FoundParts::default()) };
    let v: serde_json::Value = serde_json::from_str(&json)?;
    let starts: std::collections::HashSet<i64> = round_starts(&v).into_iter().collect();
    let (Some(&first), Some(uploaded)) = (starts.iter().min(), v["info"]["date"].as_i64()) else {
        return Ok(FoundParts::default());
    };
    let players: Vec<String> = v["players"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.keys())
        .filter_map(|k| SteamId::parse(k).ok())
        .take(3)
        .map(|id| id.to_steamid64())
        .collect();
    if players.len() < 3 {
        return Ok(FoundParts::default());
    }
    let near: Vec<i64> = sources
        .logstf_with_players(&players, 50)
        .await?
        .into_iter()
        .filter(|(id, _, _, date, n)| *id != log_id && (16..=22).contains(n) && *date >= first - 300 && *date <= uploaded + 60)
        .map(|(id, ..)| id)
        .collect();
    let mut found = FoundParts { candidates: near.len(), added: 0 };
    for id in near {
        let body = match db.raw_log(id).await? {
            Some(b) => b,
            None => match sources.log_json(id).await {
                Ok((b, _)) => b,
                Err(e) => {
                    tracing::info!(log_id = id, error = %format!("{e:#}"), "a possible part could not be fetched");
                    continue;
                }
            },
        };
        let Ok(pv) = serde_json::from_str::<serde_json::Value>(&body) else { continue };
        let theirs = round_starts(&pv);
        if theirs.is_empty() || !theirs.iter().all(|s| starts.contains(s)) {
            continue;
        }
        store_log_json(db, w, id, &body).await?;
        found.added += 1;
    }
    // Each part is one recording: their demos, found by the match's own
    // players (best effort; the parts stand either way); and every round's
    // map from its part, so the match reads map by map at once (Q84).
    if found.added > 0 {
        if let Err(e) = crate::maps::resolve_all(db).await {
            tracing::info!(log_id, error = %format!("{e:#}"), "maps not resolved after the parts");
        }
        if let Some(me) = db.get_me().await? {
            if let Err(e) = crate::demostf::index_one(db, sources, me, log_id).await {
                tracing::info!(log_id, error = %format!("{e:#}"), "demos.tf not asked for the parts");
            }
        }
    }
    Ok(found)
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
