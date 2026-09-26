//! The IPC surface. Thin: parse, delegate, return.
//!
//! Anything with real logic belongs in `hl-core` or `hl-db` so it stays
//! testable without a running window.

use crate::error::{CmdError, CmdResult};
use crate::AppState;
use hl_core::config::{keys, AppConfig};
use hl_core::{tfpath, SteamId, TfPathInfo};
use serde::Serialize;
use tauri::State;

/// First-run status, polled by the UI on boot to decide between the setup
/// screen and the app proper.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub version: &'static str,
    pub db_path: String,
    pub ready: bool,
    pub config: AppConfig,
    /// Set only when the database is empty and a backup beside it is not:
    /// the app says so before it asks for anything else.
    pub restore: Option<hl_ingest::restore::RestoreOffer>,
}

#[tauri::command]
pub async fn app_status(state: State<'_, AppState>) -> CmdResult<AppStatus> {
    let config = state.db.get_config().await?;
    // A wipe takes the config with it, so this is checked before the setup
    // screen goes up — a fresh-looking install is exactly the case where a
    // backup matters most.
    let declined = state.db.get_setting(RESTORE_DECLINED).await?.is_some();
    let restore = if declined {
        None
    } else {
        hl_ingest::restore::offer(&state.db, &state.db_path).await.unwrap_or_else(|e| {
            tracing::warn!(error = %format!("{e:#}"), "looking for a backup to offer failed");
            None
        })
    };
    Ok(AppStatus {
        version: crate::DISPLAY_VERSION,
        db_path: state.db_path.to_string_lossy().into_owned(),
        ready: config.is_ready(),
        config,
        restore,
    })
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> CmdResult<AppConfig> {
    Ok(state.db.get_config().await?)
}

/// Accepts any SteamID form; stores the canonical one and marks that player as
/// the owner of this install.
#[tauri::command]
pub async fn set_steamid(state: State<'_, AppState>, input: String) -> CmdResult<AppConfig> {
    let id = SteamId::parse(&input)?;
    state.db.set_me(id).await?;
    tracing::info!(steamid = %id, "owner set");
    // A new owner is a new name and picture: forget the old ones and look the
    // new ones up in the background.
    for key in ["owner_name", "owner_avatar", "owner_avatar_src"] {
        state.db.clear_setting(key).await?;
    }
    let (db, sources) = (state.db.clone(), state.sources.clone());
    tauri::async_runtime::spawn(async move {
        if let Err(e) = hl_ingest::owner::refresh(&db, &sources, id).await {
            tracing::warn!(error = %format!("{e:#}"), "owner profile refresh failed");
        }
    });
    Ok(state.db.get_config().await?)
}

/// Validate a path without committing to it — drives the live feedback under
/// the folder picker.
#[tauri::command]
pub async fn inspect_tf_path(path: String) -> CmdResult<TfPathInfo> {
    Ok(tfpath::inspect(&path)?)
}

#[tauri::command]
pub async fn detect_tf_path() -> CmdResult<Option<TfPathInfo>> {
    Ok(tfpath::detect())
}

/// Commit a `tf` directory. Refuses a path that does not look like one, so a
/// typo cannot silently leave demo scanning pointed at an empty folder.
#[tauri::command]
pub async fn set_tf_path(state: State<'_, AppState>, path: String) -> CmdResult<TfPathInfo> {
    let info = tfpath::inspect(&path)?;
    if !info.valid {
        return Err(CmdError::new(
            "invalid_tf_path",
            format!("`{}` does not look like a TF2 `tf` directory.", info.path),
        ));
    }
    state.db.set_setting(keys::TF_PATH, &info.path).await?;
    tracing::info!(path = %info.path, demos = info.demo_count, "tf path set");
    Ok(info)
}

/// Show a file or folder in the system file manager.
///
/// The database is the one thing here that cannot be downloaded again, so
/// getting to it — to copy it somewhere safe, or to put one back by hand —
/// should not mean typing out an `AppData` path from a label.
#[tauri::command]
pub async fn reveal_path(app: tauri::AppHandle, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| CmdError::new("reveal_failed", format!("Could not open `{path}`: {e}")))
}

/// Put a backup back in place of the current database, and restart.
///
/// The copy itself happens at the next start, with nothing connected: SQLite
/// holds the file open while the app runs, and Windows will not let an open
/// file be replaced underneath. Refused unless the database really is empty,
/// so this can never be the thing that loses a history.
#[tauri::command]
pub async fn restore_backup(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<()> {
    let indexed = state.db.index_stats().await?.indexed;
    if indexed > 0 {
        return Err(CmdError::new(
            "not_empty",
            format!(
                "This database already holds {indexed} matches. \
                 Close the app and rename the copy over `hl.sqlite3` if you mean to replace it."
            ),
        ));
    }
    hl_ingest::restore::request(&state.db_path, std::path::Path::new(&path))
        .map_err(|e| CmdError::new("restore_failed", format!("{e:#}")))?;
    tracing::info!(%path, "restore requested; restarting");
    // Never returns.
    app.restart()
}

/// Don't offer the restore again on this database.
#[tauri::command]
pub async fn decline_restore(state: State<'_, AppState>) -> CmdResult<()> {
    state.db.set_setting(RESTORE_DECLINED, "1").await?;
    Ok(())
}

/// Set once the offer has been turned down, so a deliberate fresh start is
/// not asked about again on every launch.
pub const RESTORE_DECLINED: &str = "restore_declined";
