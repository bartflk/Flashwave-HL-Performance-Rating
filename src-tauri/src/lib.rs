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

            app.manage(AppState {
                db,
                db_path,
                sources: Arc::new(sources),
                busy: Arc::new(AtomicBool::new(false)),
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running application");
}
