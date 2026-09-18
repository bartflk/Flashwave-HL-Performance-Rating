//! Sync, reprocess and the match list.
//!
//! Sync and reprocess run for minutes, so the commands return immediately and
//! the work streams its progress to the UI as events:
//!
//! ```text
//! sync://progress   Progress        many times
//! sync://done       SyncSummary     once, on success
//! sync://error      CmdError        once, on failure
//! ```

use crate::error::{CmdError, CmdResult};
use crate::AppState;
use hl_db::{IndexStats, MatchFilter, MatchPage};
use hl_ingest::{Progress, SyncOptions, SyncSummary};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

const EV_PROGRESS: &str = "sync://progress";
const EV_DONE: &str = "sync://done";
const EV_ERROR: &str = "sync://error";

/// Clears the busy flag however the task ends — success, error or panic.
struct BusyGuard(Arc<AtomicBool>);

impl BusyGuard {
    fn acquire(flag: &Arc<AtomicBool>) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| BusyGuard(flag.clone()))
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// What `sync://done` carries. Reprocess reuses it with `fetched = 0`.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Done {
    kind: &'static str,
    fetched: usize,
    failed: usize,
    stats: IndexStats,
}

#[tauri::command]
pub async fn sync_start(app: AppHandle, state: State<'_, AppState>, full: bool) -> CmdResult<()> {
    let guard = BusyGuard::acquire(&state.busy)
        .ok_or_else(|| CmdError::new("busy", "A sync is already running."))?;
    let me = state
        .db
        .get_me()
        .await?
        .ok_or_else(|| CmdError::new("missing_config", "Set your SteamID before syncing."))?;

    let db = state.db.clone();
    let sources = state.sources.clone();
    let weights_path = state.db_path.with_file_name("weights.toml");

    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let emitter = app.clone();
        let opts = SyncOptions { full, max_fetch: None };
        let result = async {
            let summary = hl_ingest::sync(&db, &sources, me, &opts, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            // ETF2L is context, not the core: if it is down, the sync still succeeds.
            let etf2l = hl_ingest::etf2l::fetch(&db, &sources, me, |done, total| {
                let _ = emitter.emit(EV_PROGRESS, Progress::Etf2l { done, total });
            })
            .await;
            if let Err(e) = etf2l {
                let error = format!("{e:#}");
                tracing::warn!(%error, "ETF2L fetch failed");
                let _ = emitter.emit(EV_PROGRESS, Progress::Etf2lFailed { error });
            }
            hl_ingest::etf2l::derive_context(&db, me).await?;
            // Every sync ends by re-rating: new matches shift the baselines.
            let (weights, _) = hl_rating::Weights::load(&weights_path);
            hl_ingest::rate_all(&db, Some(me), &weights, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            // New logs can link to demos already on disk.
            if let Some(tf) = db.get_config().await?.tf_path {
                let s = hl_ingest::index_demos(&db, std::path::Path::new(&tf)).await?;
                let _ = emitter.emit(EV_DEMOS_INDEXED, &s);
            }
            anyhow::Ok(summary)
        }
        .await;

        match result {
            Ok(SyncSummary { fetched, failed, stats }) => {
                let _ = app.emit(EV_DONE, Done { kind: "sync", fetched, failed, stats });
            }
            Err(e) => {
                let _ = app.emit(EV_ERROR, CmdError::from(e));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn reprocess_start(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let guard = BusyGuard::acquire(&state.busy)
        .ok_or_else(|| CmdError::new("busy", "A sync is already running."))?;
    let db = state.db.clone();
    let weights_path = state.db_path.with_file_name("weights.toml");

    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let emitter = app.clone();
        let result = async {
            let stats = hl_ingest::reprocess(&db, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            let me = db.get_me().await?;
            if let Some(me) = me {
                hl_ingest::etf2l::derive_context(&db, me).await?;
            }
            let (weights, _) = hl_rating::Weights::load(&weights_path);
            hl_ingest::rate_all(&db, me, &weights, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            anyhow::Ok(stats)
        }
        .await;

        match result {
            Ok(stats) => {
                let _ = app.emit(EV_DONE, Done { kind: "reprocess", fetched: 0, failed: 0, stats });
            }
            Err(e) => {
                let _ = app.emit(EV_ERROR, CmdError::from(e));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn sync_busy(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.busy.load(Ordering::Acquire))
}

#[tauri::command]
pub async fn index_stats(state: State<'_, AppState>) -> CmdResult<IndexStats> {
    Ok(state.db.index_stats().await?)
}

#[tauri::command]
pub async fn list_matches(
    state: State<'_, AppState>,
    format: Option<String>,
    kind: Option<String>,
    limit: i64,
    offset: i64,
) -> CmdResult<MatchPage> {
    let me = state.db.get_me().await?;
    let filter = MatchFilter {
        format,
        kind,
        // Bound the page size so a bad argument cannot pull the whole table.
        limit: limit.clamp(1, 500),
        offset: offset.max(0),
    };
    Ok(state.db.list_matches(me.map(|m| m.account_id()), &filter).await?)
}

/// Everything the match page shows. `None` when the log is not stored yet.
///
/// Weights are re-read on every call, so edits to `weights.toml` show up the
/// next time a match is opened.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchResponse {
    #[serde(flatten)]
    detail: hl_rating::MatchDetail,
    /// Official, scrim or pug, with the ETF2L side of an official.
    context: Option<hl_db::MatchContext>,
}

#[tauri::command]
pub async fn get_match(state: State<'_, AppState>, log_id: i64) -> CmdResult<Option<MatchResponse>> {
    let me = state.db.get_me().await?;
    let (weights, warning) = hl_rating::Weights::load(&state.db_path.with_file_name("weights.toml"));
    let Some(mut detail) = hl_ingest::match_detail(&state.db, log_id, me, &weights).await? else {
        return Ok(None);
    };
    detail.weights_warning = warning;
    let context = state.db.match_context(log_id).await?;
    Ok(Some(MatchResponse { detail, context }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResponse {
    /// Classes with rated games, most played first: `(class, games)`.
    pub classes: Vec<(String, i64)>,
    pub profile: Option<hl_rating::Profile>,
}

/// The owner's profile on one class, defaulting to their most-rated class.
#[tauri::command]
pub async fn get_profile(
    state: State<'_, AppState>,
    class: Option<String>,
    kind: Option<String>,
) -> CmdResult<ProfileResponse> {
    let me = state
        .db
        .get_me()
        .await?
        .ok_or_else(|| CmdError::new("missing_config", "Set your SteamID first."))?;
    let classes = hl_ingest::rated_classes(&state.db, me).await?;
    let chosen = match class.or_else(|| classes.first().map(|(c, _)| c.clone())) {
        Some(c) => Some(hl_core::TfClass::parse(&c)?),
        None => None,
    };
    let profile = match chosen {
        Some(c) => hl_ingest::load_profile(&state.db, me, c, kind.as_deref()).await?,
        None => None,
    };
    Ok(ProfileResponse { classes, profile })
}

// ---- teammates and context ---------------------------------------------------

/// Your ETF2L teams and regular teammates. `all` includes pugs.
#[tauri::command]
pub async fn get_teammates(state: State<'_, AppState>, all: bool) -> CmdResult<hl_ingest::teammates::Teammates> {
    let me = state
        .db
        .get_me()
        .await?
        .ok_or_else(|| CmdError::new("missing_config", "Set your SteamID first."))?;
    let scope = if all { hl_ingest::teammates::Scope::All } else { hl_ingest::teammates::Scope::Team };
    Ok(hl_ingest::teammates::load(&state.db, me, scope).await?)
}

#[tauri::command]
pub async fn context_counts(state: State<'_, AppState>) -> CmdResult<hl_db::ContextCounts> {
    Ok(state.db.context_counts(hl_ingest::etf2l::PLAYER_KEY).await?)
}

// ---- demos -----------------------------------------------------------------

const EV_DEMOS_INDEXED: &str = "demos://indexed";
const EV_STV_PROGRESS: &str = "stv://progress";
const EV_STV_DONE: &str = "stv://done";
const EV_STV_ERROR: &str = "stv://error";

async fn tf_path(state: &AppState) -> CmdResult<std::path::PathBuf> {
    let tf = state
        .db
        .get_config()
        .await?
        .tf_path
        .ok_or_else(|| CmdError::new("missing_config", "Set your TF2 folder first."))?;
    Ok(std::path::PathBuf::from(tf))
}

/// Rescan the TF2 folder for demos and relink them. Header reads only, so
/// this is quick (~0.6 s for 100 demos) and safe to run on every sync.
#[tauri::command]
pub async fn scan_demos(app: AppHandle, state: State<'_, AppState>) -> CmdResult<hl_ingest::DemoIndexSummary> {
    let tf = tf_path(&state).await?;
    let summary = hl_ingest::index_demos(&state.db, &tf).await?;
    let _ = app.emit(EV_DEMOS_INDEXED, &summary);
    Ok(summary)
}

#[tauri::command]
pub async fn demo_stats(state: State<'_, AppState>) -> CmdResult<hl_db::DemoStats> {
    Ok(state.db.demo_stats().await?)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StvProgress {
    log_id: i64,
    bytes: u64,
    total: Option<u64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StvError {
    log_id: i64,
    #[serde(flatten)]
    error: CmdError,
}

/// Download the demos.tf STV demo for a match in the background, then index
/// and link it. Progress streams on `stv://progress`.
#[tauri::command]
pub async fn fetch_stv(app: AppHandle, state: State<'_, AppState>, log_id: i64) -> CmdResult<()> {
    let guard = BusyGuard::acquire(&state.downloading)
        .ok_or_else(|| CmdError::new("busy", "A demo download is already running."))?;
    let tf = tf_path(&state).await?;
    let db = state.db.clone();
    let sources = state.sources.clone();

    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let emitter = app.clone();
        // Progress events at most every ~1%, not per network chunk.
        let mut last_pct = u64::MAX;
        let result = hl_ingest::fetch_stv(&db, &sources, &tf, log_id, |bytes, total| {
            let pct = total.map(|t| bytes * 100 / t.max(1)).unwrap_or(bytes / 1_000_000);
            if pct != last_pct {
                last_pct = pct;
                let _ = emitter.emit(EV_STV_PROGRESS, StvProgress { log_id, bytes, total });
            }
        })
        .await;
        match result {
            Ok(done) => {
                let _ = app.emit(EV_STV_DONE, done);
            }
            Err(e) => {
                let _ = app.emit(EV_STV_ERROR, StvError { log_id, error: CmdError::from(e) });
            }
        }
    });
    Ok(())
}
