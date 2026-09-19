mod commands;
mod error;
mod sync_commands;

use hl_db::Db;
use hl_ingest::Sources;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

/// Shared application state.
pub struct AppState {
    pub db: Db,
    pub db_path: PathBuf,
    /// One throttled client set for the whole app, so two callers can never
    /// double the request rate against a community API.
    pub sources: Arc<Sources>,
    /// Set while a sync or reprocess is running; a second one is refused.
    pub busy: Arc<AtomicBool>,
    /// Set while an STV demo is downloading.
    pub downloading: Arc<AtomicBool>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,hl_app=debug,hl_db=debug,hl_ingest=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Errors are stringified rather than passed through as `anyhow`:
            // Tauri's setup wants a `Box<dyn Error>`, and `{:#}` keeps the
            // whole cause chain in the message.
            let db_path = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("resolving the application data directory: {e}"))?
                .join("hl.sqlite3");

            // Blocking here is deliberate: the window should not appear until
            // migrations have applied, so no command can race an unmigrated db.
            let db = tauri::async_runtime::block_on(Db::connect(&db_path))
                .map_err(|e| format!("{e:#}"))?;
            let sources = Sources::new().map_err(|e| format!("{e:#}"))?;

            // Index demos in the background at startup: the window should not
            // wait on a folder scan, and a missing TF2 folder is not an error.
            {
                let db = db.clone();
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let Ok(cfg) = db.get_config().await else { return };
                    // Classify matches from stored data first: instant, and it
                    // brings a database from before M5 up to date without a sync.
                    if let Some(me) = cfg.steamid {
                        match hl_ingest::etf2l::derive_context(&db, me).await {
                            Ok(s) => tracing::info!(officials = s.officials, scrims = s.scrims, pugs = s.pugs, "matches classified"),
                            Err(e) => tracing::warn!(error = %format!("{e:#}"), "context pass failed"),
                        }
                    }
                    let Some(tf) = cfg.tf_path else { return };
                    match hl_ingest::index_demos(&db, std::path::Path::new(&tf)).await {
                        Ok(s) => {
                            tracing::info!(demos = s.scanned, linked = s.demos_linked, "demos indexed");
                            let _ = tauri::Emitter::emit(&handle, "demos://indexed", &s);
                        }
                        Err(e) => tracing::warn!(error = %format!("{e:#}"), "demo index failed"),
                    }
                });
            }

            app.manage(AppState {
                db,
                db_path,
                sources: Arc::new(sources),
                busy: Arc::new(AtomicBool::new(false)),
                downloading: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::get_config,
            commands::set_steamid,
            commands::inspect_tf_path,
            commands::detect_tf_path,
            commands::set_tf_path,
            sync_commands::sync_start,
            sync_commands::reprocess_start,
            sync_commands::sync_busy,
            sync_commands::index_stats,
            sync_commands::list_matches,
            sync_commands::get_match,
            sync_commands::get_profile,
            sync_commands::get_teammates,
            sync_commands::context_counts,
            sync_commands::rawlog_stats,
            sync_commands::get_match_analysis,
            sync_commands::get_map_view,
            sync_commands::scan_demos,
            sync_commands::demo_stats,
            sync_commands::fetch_stv,
        ])
        .run(tauri::generate_context!())
        .expect("error while running application");
}
