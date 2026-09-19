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
}

#[tauri::command]
pub async fn app_status(state: State<'_, AppState>) -> CmdResult<AppStatus> {
    let config = state.db.get_config().await?;
    Ok(AppStatus {
        // The installers carry a plain 0.1.0 (MSI allows digits only); the app says what it is.
        version: concat!(env!("CARGO_PKG_VERSION"), " beta"),
        db_path: state.db_path.to_string_lossy().into_owned(),
        ready: config.is_ready(),
        config,
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
