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
    let db_path = state.db_path.clone();
    let weights_path = state.db_path.with_file_name("weights.toml");

    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let emitter = app.clone();
        let opts = SyncOptions { full, max_fetch: None };
        let result = async {
            // A copy first: a sync rewrites derived tables, and the file is
            // the only thing here that cannot be fetched again.
            if let Err(e) = hl_ingest::backup::run(&db, &db_path, false).await {
                tracing::warn!(error = %format!("{e:#}"), "database backup failed");
            }
            let summary = hl_ingest::sync(&db, &sources, me, &opts, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            // Parts of combined logs: best effort, like ETF2L. When logs.tf is
            // not answering the round maps come from the other sources.
            match hl_ingest::maps::fetch_parts(&db, &sources, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await
            {
                Ok(p) if p.gave_up => tracing::warn!(failed = p.failed, "logs.tf not answering; parts wait for the next sync"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "part fetch failed"),
            }
            // Raw logs: every kill with time, classes and positions. Newest
            // first; the first sync fetches the whole history (~15 min).
            let raw = hl_ingest::kills::fetch(&db, &sources, Some(hl_ingest::BULK_PER_SYNC), |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            if raw.failed > 0 {
                tracing::warn!(failed = raw.failed, "some raw logs could not be fetched; next sync retries");
            }
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
            // Your name and picture for the top bar; a failure keeps the old ones.
            if let Err(e) = hl_ingest::owner::refresh(&db, &sources, me).await {
                tracing::warn!(error = %format!("{e:#}"), "owner profile refresh failed");
            }
            // New logs can link to demos already on disk. This also places
            // every log on the real clock, which the round maps use.
            if let Some(tf) = db.get_config().await?.tf_path {
                let s = hl_ingest::index_demos(&db, std::path::Path::new(&tf)).await?;
                let _ = emitter.emit(EV_DEMOS_INDEXED, &s);
            }
            // Every round's map, then the rating: kills are valued on their map.
            hl_ingest::maps::resolve_all(&db).await?;
            hl_ingest::fights::derive_all(&db, false).await?;
            // Aim from any newly linked demo. Seconds a demo, and a demo the
            // parser cannot read must not fail the whole sync.
            match hl_ingest::aim::derive_all(&db, me, false, |_, _| {}).await {
                Ok(s) if s.read > 0 => tracing::info!(read = s.read, kills = s.kills, "aim read from demos"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "reading aim from demos failed"),
            }
            // Every sync ends by re-rating: new matches shift the baselines.
            let (weights, _) = hl_rating::Weights::load(&weights_path);
            hl_ingest::rate_all(&db, Some(me), &weights, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
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
    let db_path = state.db_path.clone();
    let weights_path = state.db_path.with_file_name("weights.toml");

    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let emitter = app.clone();
        let result = async {
            // A rebuild rewrites every derived table; keep a copy of what was.
            if let Err(e) = hl_ingest::backup::run(&db, &db_path, false).await {
                tracing::warn!(error = %format!("{e:#}"), "database backup failed");
            }
            let stats = hl_ingest::reprocess(&db, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            hl_ingest::kills::rederive_all(&db, |p: Progress| {
                let _ = emitter.emit(EV_PROGRESS, p);
            })
            .await?;
            let me = db.get_me().await?;
            if let Some(me) = me {
                hl_ingest::etf2l::derive_context(&db, me).await?;
            }
            hl_ingest::maps::resolve_all(&db).await?;
            hl_ingest::fights::derive_all(&db, true).await?;
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

// A Tauri command takes its arguments flat, one per field the UI sends.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn list_matches(
    state: State<'_, AppState>,
    format: Option<String>,
    kind: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
    limit: i64,
    offset: i64,
    sort: Option<String>,
    ascending: Option<bool>,
) -> CmdResult<MatchPage> {
    let me = state.db.get_me().await?;
    let filter = MatchFilter {
        format,
        kind,
        from,
        to,
        sort,
        ascending: ascending.unwrap_or(false),
        model_version: hl_rating::MODEL_VERSION.to_string(),
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
    /// The maps played, in order, with rounds won on each.
    segments: Vec<hl_db::Segment>,
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
    let segments = state.db.segments(log_id).await?;
    Ok(Some(MatchResponse { detail, context, segments }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResponse {
    /// Classes with rated games, most played first: `(class, games)`.
    pub classes: Vec<(String, i64)>,
    pub profile: Option<hl_rating::Profile>,
    /// Kills in context against the players you face, under the same filters.
    pub fights: Option<hl_ingest::seasons::FightsCard>,
    /// What your demos say about your aim under the same filters (PLAN §14),
    /// and over everything read, to compare against.
    pub aim: Option<hl_db::AimTotals>,
    pub life: Option<hl_db::LifeTotals>,
    pub aim_all: Option<hl_db::AimTotals>,
    pub life_all: Option<hl_db::LifeTotals>,
}

/// The owner's profile on one class, defaulting to their most-rated class.
#[tauri::command]
pub async fn get_profile(
    state: State<'_, AppState>,
    class: Option<String>,
    kind: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
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
    let period = match (from, to) {
        (None, None) => None,
        (f, t) => Some((f.unwrap_or(i64::MIN), t.unwrap_or(i64::MAX))),
    };
    let (profile, fights) = match chosen {
        Some(c) => (
            hl_ingest::load_profile(&state.db, me, c, kind.as_deref(), period).await?,
            hl_ingest::seasons::fights_card(&state.db, me, c, kind.as_deref(), from, to).await?,
        ),
        None => (None, None),
    };
    // The same filters, over what the demos say (PLAN §14).
    let class_name = chosen.map(|c| c.as_str());
    let scope = hl_db::AimFilter {
        me: me.account_id(),
        log_id: None,
        class: class_name,
        kind: kind.as_deref(),
        from,
        to,
    };
    let everything = hl_db::AimFilter { me: me.account_id(), class: class_name, ..Default::default() };
    Ok(ProfileResponse {
        classes,
        profile,
        fights,
        aim: state.db.aim_totals(&scope).await?,
        life: state.db.life_totals(&scope).await?,
        aim_all: state.db.aim_totals(&everything).await?,
        life_all: state.db.life_totals(&everything).await?,
    })
}

/// Copies of the database, newest first, with where they live.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backups {
    pub dir: String,
    pub items: Vec<hl_ingest::backup::Backup>,
}

#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>) -> CmdResult<Backups> {
    Ok(Backups {
        dir: hl_ingest::backup::dir(&state.db_path).to_string_lossy().into_owned(),
        items: hl_ingest::backup::list(&state.db_path),
    })
}

/// Copy the database now, whatever the last copy's age.
#[tauri::command]
pub async fn backup_now(state: State<'_, AppState>) -> CmdResult<Option<hl_ingest::backup::Backup>> {
    Ok(hl_ingest::backup::run(&state.db, &state.db_path, true).await?)
}

/// Your name and profile picture, as stored.
#[tauri::command]
pub async fn get_owner(state: State<'_, AppState>) -> CmdResult<Option<hl_ingest::owner::Owner>> {
    match state.db.get_me().await? {
        Some(me) => Ok(Some(hl_ingest::owner::load(&state.db, me).await?)),
        None => Ok(None),
    }
}

/// Seasons from your officials, newest first.
#[tauri::command]
pub async fn list_seasons(state: State<'_, AppState>) -> CmdResult<Vec<hl_ingest::seasons::Season>> {
    Ok(hl_ingest::seasons::list(&state.db).await?)
}

/// One class, season by season.
#[tauri::command]
pub async fn get_seasons(state: State<'_, AppState>, class: String) -> CmdResult<hl_ingest::seasons::SeasonsView> {
    let me = state
        .db
        .get_me()
        .await?
        .ok_or_else(|| CmdError::new("missing_config", "Set your SteamID first."))?;
    Ok(hl_ingest::seasons::by_season(&state.db, me, hl_core::TfClass::parse(&class)?).await?)
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

/// The analysis views for one match, built from its raw log. `None` without one.
#[tauri::command]
pub async fn get_match_analysis(
    state: State<'_, AppState>,
    log_id: i64,
) -> CmdResult<Option<hl_ingest::analysis::Analysis>> {
    let me = state.db.get_me().await?;
    Ok(hl_ingest::analysis::load(&state.db, log_id, me).await?)
}

/// What the demo says about your aim in one match (PLAN §14): every kill it
/// could answer for, and the averages over them. Empty without a demo.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimResponse {
    pub kills: Vec<hl_db::AimRow>,
    pub deaths: Vec<hl_db::DeathRow>,
    pub totals: Option<hl_db::AimTotals>,
    pub life: Option<hl_db::LifeTotals>,
    /// The same averages over every match with a demo, to compare against.
    pub career: Option<hl_db::AimTotals>,
    pub career_life: Option<hl_db::LifeTotals>,
}

#[tauri::command]
pub async fn get_aim(state: State<'_, AppState>, log_id: i64) -> CmdResult<AimResponse> {
    let me = state.db.get_me().await?.map_or(0, |m| m.account_id());
    let this = hl_db::AimFilter { me, log_id: Some(log_id), ..Default::default() };
    let all = hl_db::AimFilter { me, ..Default::default() };
    Ok(AimResponse {
        kills: state.db.aim_for_log(log_id).await?,
        deaths: state.db.deaths_for_log(log_id).await?,
        totals: state.db.aim_totals(&this).await?,
        life: state.db.life_totals(&this).await?,
        career: state.db.aim_totals(&all).await?,
        career_life: state.db.life_totals(&all).await?,
    })
}

/// Where you walked in one match, one route per life (PLAN §14). Empty
/// without a demo for it.
#[tauri::command]
pub async fn get_paths(state: State<'_, AppState>, log_id: i64) -> CmdResult<Vec<hl_db::PathRow>> {
    Ok(state.db.paths_for_log(log_id).await?)
}

/// A map's outline from every stored kill on it, plus your own kill and death
/// spots across all your matches there. `None` with too few kills.
#[tauri::command]
pub async fn get_map_view(state: State<'_, AppState>, map: String) -> CmdResult<Option<hl_ingest::mapview::MapView>> {
    let me = state.db.get_me().await?;
    Ok(hl_ingest::mapview::load(&state.db, &map, me).await?)
}

/// The overview image for a map, when one is saved in the app's `overviews`
/// folder and its placement is known. `None` otherwise: the kill map then
/// draws the outline from kills.
#[tauri::command]
pub async fn get_map_overview(state: State<'_, AppState>, map: String) -> CmdResult<Option<hl_ingest::overview::Overview>> {
    let dir = state.db_path.with_file_name("overviews");
    Ok(hl_ingest::overview::load(&dir, &map)?)
}

#[tauri::command]
pub async fn rawlog_stats(state: State<'_, AppState>) -> CmdResult<hl_db::RawlogStats> {
    Ok(state.db.rawlog_stats().await?)
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
