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
    /// Held for the life of the window: the database is this app's alone
    /// while it runs. Never read; dropping it is the point.
    pub _lock: hl_ingest::lock::Lock,
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

            // Hold the database for as long as this window is open, so no
            // other tool can write to it at the same time. Two writers on one
            // SQLite file corrupted this database twice on 25 September 2026.
            let lock = hl_ingest::lock::hold(&db_path)
                .map_err(|e| format!("{e:#}"))?;

            // A backup asked for last run goes in now, while nothing has the
            // file open. Failing here must not stop the app: the marker is
            // cleared either way, and the old database is still there.
            match hl_ingest::restore::apply_pending(&db_path) {
                Ok(Some(from)) => tracing::info!(from = %from.display(), "database restored"),
                Ok(None) => {}
                Err(e) => tracing::error!(error = %format!("{e:#}"), "restore failed"),
            }

            // Blocking here is deliberate: the window should not appear until
            // migrations have applied, so no command can race an unmigrated db.
            //
            // A database that will not open at all used to end here, with the
            // window showing "Could not start" and no way forward -- while
            // five backups sat in the folder next door. That happened on 25
            // September 2026. So the file is moved aside and a fresh one takes
            // its place, which puts the start on the same path as any other
            // empty database: the restore offer, which now says which of the
            // two happened.
            let db = match tauri::async_runtime::block_on(Db::connect(&db_path)) {
                Ok(db) => db,
                Err(first) => {
                    tracing::error!(error = %format!("{first:#}"), "database would not open");
                    let moved = hl_ingest::restore::set_aside(&db_path)
                        .map_err(|e| format!("the database could not be opened ({first:#}), and moving it aside failed too: {e:#}"))?;
                    let db = tauri::async_runtime::block_on(Db::connect(&db_path))
                        .map_err(|e| format!("{e:#}"))?;
                    let note = format!("{}|{first:#}", moved.display());
                    let _ = tauri::async_runtime::block_on(
                        db.set_setting(hl_ingest::restore::SET_ASIDE_KEY, &note),
                    );
                    db
                }
            };
            let sources = Arc::new(Sources::new().map_err(|e| format!("{e:#}"))?);

            // Index demos in the background at startup: the window should not
            // wait on a folder scan, and a missing TF2 folder is not an error.
            {
                let db = db.clone();
                let handle = app.handle().clone();
                let weights_path = db_path.with_file_name("weights.toml");
                let sources = sources.clone();
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
                    if let Some(tf) = cfg.tf_path {
                        match hl_ingest::index_demos(&db, std::path::Path::new(&tf)).await {
                            Ok(s) => {
                                tracing::info!(demos = s.scanned, linked = s.demos_linked, "demos indexed");
                                let _ = tauri::Emitter::emit(&handle, "demos://indexed", &s);
                            }
                            Err(e) => tracing::warn!(error = %format!("{e:#}"), "demo index failed"),
                        }
                    }
                    match hl_ingest::maps::resolve_all(&db).await {
                        Ok(s) => tracing::info!(multi_map = s.multi_map_logs, unresolved = s.unresolved, "round maps resolved"),
                        Err(e) => tracing::warn!(error = %format!("{e:#}"), "round map pass failed"),
                    }
                    match hl_ingest::fights::derive_all(&db, false).await {
                        Ok(s) => tracing::info!(derived = s.derived, total = s.total, "fights derived"),
                        Err(e) => tracing::warn!(error = %format!("{e:#}"), "fights pass failed"),
                    }
                    // Your name and picture, if they have never been fetched.
                    if let Some(me) = cfg.steamid {
                        if db.get_setting("owner_avatar").await.ok().flatten().is_none() {
                            if let Err(e) = hl_ingest::owner::refresh(&db, &sources, me).await {
                                tracing::warn!(error = %format!("{e:#}"), "owner profile refresh failed");
                            }
                        }
                    }
                    // A new rating model has no ratings until something rates:
                    // do it now rather than leave the profile empty until a sync.
                    if db.rating_count(hl_rating::MODEL_VERSION).await.unwrap_or(1) == 0 {
                        let (weights, _) = hl_rating::Weights::load(&weights_path);
                        match hl_ingest::rate_all(&db, cfg.steamid, &weights, |_| {}).await {
                            Ok(s) => tracing::info!(rated = s.rated, model = hl_rating::MODEL_VERSION, "rated for a new model"),
                            Err(e) => tracing::warn!(error = %format!("{e:#}"), "rating pass failed"),
                        }
                    }
                });
            }

            app.manage(AppState {
                db,
                db_path,
                _lock: lock,
                sources,
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
            commands::reveal_path,
            commands::restore_backup,
            commands::decline_restore,
            sync_commands::sync_start,
            sync_commands::reprocess_start,
            sync_commands::sync_busy,
            sync_commands::index_stats,
            sync_commands::list_matches,
            sync_commands::get_match,
            sync_commands::get_profile,
            sync_commands::list_seasons,
            sync_commands::get_owner,
            sync_commands::get_seasons,
            sync_commands::get_teammates,
            sync_commands::context_counts,
            sync_commands::rawlog_stats,
            sync_commands::get_match_analysis,
            sync_commands::get_aim,
            sync_commands::played_filters,
            sync_commands::get_parts,
            sync_commands::fetch_part,
            sync_commands::get_paths,
            sync_commands::all_history,
            sync_commands::set_all_history,
            sync_commands::list_backups,
            sync_commands::backup_now,
            sync_commands::get_map_view,
            sync_commands::get_map_overview,
            sync_commands::scan_demos,
            sync_commands::demo_stats,
            sync_commands::fetch_stv,
        ])
        .run(tauri::generate_context!())
        .expect("error while running application");
}
