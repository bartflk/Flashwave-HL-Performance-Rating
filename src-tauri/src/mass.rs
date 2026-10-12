//! Q69 (Emiel): mass import of a player's demos on one map from demos.tf.
//!
//! `mass_search` lists what demos.tf has (progress on `mass://search`);
//! `mass_start` imports the picked ones in the background (`mass://progress`,
//! then `mass://done` or `mass://error`), holding the sync's turn and the
//! demo download turn so nothing else writes alongside it; `mass_cancel`
//! stops it between demos. The manage window lists runs with
//! `mass_batches`, and `mass_keep` keeps a run's files or deletes them (its
//! routes stay either way).

use crate::error::{CmdError, CmdResult};
use crate::sync_commands::{tf_path, BusyGuard};
use crate::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};

const EV_SEARCH: &str = "mass://search";
const EV_PROGRESS: &str = "mass://progress";
const EV_DONE: &str = "mass://done";
const EV_ERROR: &str = "mass://error";

#[tauri::command]
pub async fn mass_search(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: u32,
    map: String,
    class: Option<String>,
    want: usize,
) -> CmdResult<Vec<hl_ingest::mass::Candidate>> {
    let want = want.clamp(1, 100);
    Ok(hl_ingest::mass::search(&state.db, &state.sources, account_id, &map, class.as_deref(), want, |s| {
        let _ = app.emit(EV_SEARCH, s);
    })
    .await?)
}

/// Start importing; the run's id. Refused while a sync or another import runs.
#[tauri::command]
pub async fn mass_start(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: u32,
    map: String,
    class: Option<String>,
    picks: Vec<hl_ingest::mass::Pick>,
) -> CmdResult<i64> {
    if picks.is_empty() {
        return Err(CmdError::new("bad_input", "Pick at least one demo."));
    }
    let guard = BusyGuard::acquire(&state.busy).ok_or_else(|| CmdError::new("busy", "A sync is running; start the import when it has finished."))?;
    let tf = tf_path(&state).await?;
    let ids: Vec<i64> = picks.iter().map(|p| p.demos_tf_id).collect();
    let batch_id = state.db.create_mass_batch(account_id, &map, class.as_deref(), &ids).await?;
    let db = state.db.clone();
    let sources = state.sources.clone();
    let turn = state.demo_turn.clone();
    let cancel = state.mass_cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let (weights, _) = hl_rating::Weights::load(&state.db_path.with_file_name("weights.toml"));

    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        // One download at a time, app-wide: an STV fetched from a match page
        // waits for this, and this for it.
        let Ok(_permit) = turn.acquire().await else { return };
        let me = db.get_me().await.ok().flatten();
        let emitter = app.clone();
        let result = hl_ingest::mass::run(&db, &sources, &weights, &tf, me, account_id, batch_id, &picks, &cancel, |s| {
            let _ = emitter.emit(EV_PROGRESS, s);
        })
        .await;
        match result {
            Ok(sum) => {
                tracing::info!(batch_id, done = sum.done, failed = sum.failed, skipped = sum.skipped, "mass import finished");
                let _ = app.emit(EV_DONE, sum);
            }
            Err(e) => {
                tracing::warn!(batch_id, error = %format!("{e:#}"), "mass import failed");
                let _ = app.emit(EV_ERROR, serde_json::json!({ "batchId": batch_id, "error": CmdError::from(e) }));
            }
        }
    });
    Ok(batch_id)
}

/// Stop the running import after the demo it is on.
#[tauri::command]
pub async fn mass_cancel(state: State<'_, AppState>) -> CmdResult<()> {
    state.mass_cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn mass_batches(state: State<'_, AppState>) -> CmdResult<Vec<hl_db::MassBatch>> {
    Ok(state.db.mass_batches().await?)
}

/// Keep a run's demo files, or delete them. Deleting keeps everything read
/// from them (routes, timelines) and the demos.tf id to fetch them again.
#[tauri::command]
pub async fn mass_keep(state: State<'_, AppState>, batch_id: i64, keep: bool) -> CmdResult<i64> {
    let mut freed = 0;
    if !keep {
        let batch = state.db.mass_batches().await?.into_iter().find(|b| b.batch_id == batch_id);
        for d in batch.into_iter().flat_map(|b| b.demos) {
            let Some(demo_id) = d.demo_id else { continue };
            let Some((path, already)) = state.db.demo_path(demo_id).await? else { continue };
            if already {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "could not delete a mass import's demo");
                    continue;
                }
            }
            state.db.mark_demo_deleted(demo_id).await?;
            freed += d.size_bytes.unwrap_or(0);
        }
    }
    state.db.set_mass_kept(batch_id, keep).await?;
    Ok(freed)
}

/// Forget a run in the list; its demos and routes are left as they are.
#[tauri::command]
pub async fn mass_forget(state: State<'_, AppState>, batch_id: i64) -> CmdResult<()> {
    Ok(state.db.forget_mass_batch(batch_id).await?)
}
