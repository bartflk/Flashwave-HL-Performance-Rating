mod activity_layer;
mod applog;
mod commands;
mod error;
mod mass;
mod sync_commands;
pub mod league;
mod watch;

use hl_db::Db;
use hl_ingest::Sources;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

/// Shared application state.
/// The version as a person should read it, in one place.
///
/// The installers carry a plain `0.4.0` because MSI allows digits only; the
/// window title and Settings both say this instead. It is derived from
/// `CARGO_PKG_VERSION`, which comes from the workspace `Cargo.toml`, so
/// bumping that is enough — the window title used to be a literal in
/// `tauri.conf.json` and shipped 0.4.0 still calling itself 0.3.
pub const DISPLAY_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " beta");

pub struct AppState {
    pub db: Db,
    pub db_path: PathBuf,
    /// One throttled client set for the whole app, so two callers can never
    /// double the request rate against a community API.
    pub sources: Arc<Sources>,
    /// Set while a sync or reprocess is running; a second one is refused.
    pub busy: Arc<AtomicBool>,
    /// Stops the running sync (Flashy): taken and fired by `sync_cancel`.
    pub sync_cancel: Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// Locked by the startup jobs until they finish; a sync waits on it.
    pub startup: Arc<tokio::sync::Mutex<()>>,
    /// What the league sample's background job is doing, for Settings.
    pub league_activity: league::SharedActivity,
    /// Demo downloads waiting their turn, and the one permit they take in
    /// turns to hold. One at a time, in the order they were asked for.
    pub demo_queue: Arc<sync_commands::DemoQueue>,
    pub demo_turn: Arc<tokio::sync::Semaphore>,
    /// Stops a running mass import (Q69) after the demo it is on.
    pub mass_cancel: Arc<AtomicBool>,
    /// Held for the life of the window: the database is this app's alone
    /// while it runs. Never read; dropping it is the point.
    pub _lock: hl_ingest::lock::Lock,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The console, and the log file in the data folder (see `applog`): a
    // release build has no console, so the file is the only place its
    // warnings can be read.
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info,hl_app=debug,hl_db=debug,hl_ingest=debug".into());
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(|| applog::FileWriter))
            .with(activity_layer::ActivityLayer)
            .init();
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Anything that stops the start must be seen: a release build has
            // no console, so a failure here used to be a panic nobody read and
            // a process that simply never showed a window.
            if let Err(e) = start(app) {
                startup_failed(app.handle(), &e);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::get_config,
            commands::update_kind,
            commands::set_steamid,
            commands::inspect_tf_path,
            commands::detect_tf_path,
            commands::set_tf_path,
            commands::reveal_path,
            commands::language_files,
            commands::save_language_file,
            commands::etf2l_names,
            commands::restore_backup,
            commands::decline_restore,
            sync_commands::sync_start,
            sync_commands::reprocess_start,
            sync_commands::rerate_start,
            sync_commands::reread_demos_start,
            sync_commands::get_diagnostics,
            sync_commands::recent_log,
            sync_commands::get_model_shift,
            sync_commands::sync_busy,
            sync_commands::sync_cancel,
            sync_commands::get_demo_folders,
            sync_commands::set_demo_folders,
            sync_commands::newest_log,
            sync_commands::get_rating_guide,
            sync_commands::get_sixes_rating_guide,
            sync_commands::get_league_sample,
            sync_commands::set_league_sample,
            sync_commands::get_league_activity,
            sync_commands::search_catalogue,
            sync_commands::logstf_search,
            sync_commands::logstf_career,
            sync_commands::logstf_lineup,
            sync_commands::get_player_profile,
            sync_commands::get_trends_career,
            sync_commands::get_seasons_overview,
            sync_commands::get_season_banner,
            sync_commands::get_season_podiums,
            sync_commands::get_team_honours,
            sync_commands::get_match_sides,
            sync_commands::get_team_etf2l,
            sync_commands::get_team_transfers,
            sync_commands::get_team_info,
            sync_commands::get_fixtures,
            sync_commands::get_news,
            sync_commands::get_rgl_seasons,
            sync_commands::get_rgl_ranking,
            sync_commands::get_rgl_season,
            sync_commands::get_rgl_season_matches,
            sync_commands::get_rgl_bans,
            sync_commands::export_calendar,
            sync_commands::get_rgl_match,
            sync_commands::get_rgl_match_more,
            sync_commands::get_sixes_season,
            sync_commands::get_sixes_matches,
            sync_commands::get_match_roles,
            sync_commands::get_match_stints,
            sync_commands::get_sixes_match_stats,
            sync_commands::get_sixes_ratings,
            sync_commands::get_sixes_profile,
            sync_commands::get_sixes_sample,
            sync_commands::get_sixes_board,
            sync_commands::get_sixes_honours,
            sync_commands::set_own_role,
            sync_commands::set_sixes_sample,
            sync_commands::get_rgl_match_stats,
            sync_commands::get_rgl_team,
            sync_commands::get_rgl_team_seasons,
            sync_commands::get_rgl_team_history,
            sync_commands::get_rgl_player,
            sync_commands::get_rgl_player_more,
            sync_commands::get_rgl_season_card,
            sync_commands::search_rgl_players,
            sync_commands::search_rgl_teams,
            sync_commands::get_news_post,
            sync_commands::get_season_news,
            sync_commands::get_recent_results,
            sync_commands::get_tournaments,
            sync_commands::get_tournament,
            sync_commands::search_teams,
            sync_commands::get_league_match,
            sync_commands::get_match_page,
            sync_commands::get_recruitment,
            sync_commands::get_recruit_post,
            sync_commands::get_season_results,
            sync_commands::check_new_logs,
            sync_commands::get_player_teams,
            sync_commands::get_player_stats,
            sync_commands::get_rankings,
            sync_commands::get_season_table,
            sync_commands::get_player_deep,
            sync_commands::get_match_impact,
            sync_commands::get_data_audit,
            sync_commands::get_head_to_head,
            sync_commands::get_season_breakdown,
            sync_commands::get_stat_scales,
            sync_commands::get_storage_overview,
            sync_commands::get_storage_breakdown,
            sync_commands::delete_backup,
            sync_commands::old_seasons_status,
            sync_commands::set_logs_dump_index,
            sync_commands::import_old_season,
            sync_commands::get_match_divisions,
            sync_commands::index_stats,
            sync_commands::list_matches,
            sync_commands::get_match,
            sync_commands::get_profile,
            sync_commands::get_profile_sixes,
            sync_commands::get_activity,
            sync_commands::get_etf2l_bans,
            sync_commands::get_rgl_own_links,
            sync_commands::set_activity_verbose,
            sync_commands::list_seasons,
            sync_commands::get_owner,
            sync_commands::get_seasons,
            sync_commands::get_teammates,
            sync_commands::context_counts,
            sync_commands::my_tournaments,
            sync_commands::rawlog_stats,
            sync_commands::get_match_analysis,
            sync_commands::get_aim,
            sync_commands::get_spychecks,
            sync_commands::get_demo_stats,
            sync_commands::get_cart,
            sync_commands::played_filters,
            sync_commands::get_parts,
            sync_commands::fetch_part,
            sync_commands::get_paths,
            sync_commands::get_buildings,
            sync_commands::downloaded_demos,
            sync_commands::auto_delete_demos,
            sync_commands::set_auto_delete_demos,
            sync_commands::formats_played,
            sync_commands::set_formats_played,
            sync_commands::delete_downloaded_demos,
            sync_commands::all_history,
            sync_commands::set_all_history,
            sync_commands::list_backups,
            sync_commands::backup_now,
            sync_commands::save_backup_as,
            sync_commands::search_players,
            sync_commands::get_player,
            sync_commands::get_map_view,
            sync_commands::get_map_overview,
            sync_commands::scan_demos,
            sync_commands::demo_stats,
            sync_commands::fetch_stv,
            sync_commands::cancel_stv,
            sync_commands::failed_logs,
            sync_commands::retry_failed,
            sync_commands::import_log,
            sync_commands::import_demo,
            sync_commands::link_demo,
            sync_commands::link_demostf,
            sync_commands::refresh_demostf,
            sync_commands::find_parts,
            sync_commands::refresh_etf2l,
            sync_commands::etf2l_daily_check,
            sync_commands::update_league,
            sync_commands::league_freshness,
            sync_commands::get_league_sides,
            sync_commands::get_combined_ledger,
            sync_commands::set_match_kind,
            mass::mass_search,
            mass::mass_start,
            mass::mass_cancel,
            mass::mass_batches,
            mass::mass_keep,
            mass::mass_forget,
            sync_commands::lives_maps,
            sync_commands::lives_on_map,
            sync_commands::lives_players,
            sync_commands::lives_opposite,
            sync_commands::match_openings,
            sync_commands::stv_coverage,
            sync_commands::find_stv_parts,
            sync_commands::missing_stv_parts,
            sync_commands::get_bookmarks,
            sync_commands::set_bookmark,
            sync_commands::reread_match,
            sync_commands::get_leagues,
            sync_commands::get_team,
            sync_commands::get_callouts,
            sync_commands::save_callouts,
            sync_commands::reset_callouts,
            sync_commands::known_maps,
            sync_commands::set_round_map,
            sync_commands::maps_overview,
            sync_commands::overview_image,
            sync_commands::import_overview,
            sync_commands::save_overview_placement,
            sync_commands::remove_overview,
            sync_commands::export_callouts,
            sync_commands::inspect_callouts,
            sync_commands::import_callouts,
            sync_commands::undo_callouts,
            sync_commands::get_positions,
            sync_commands::get_tendencies,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| startup_failed_plain(&format!("{e}")));
}

/// Tell the person why the app cannot start, then stop. A release build has
/// no console, so before this a failed start was a panic nobody saw. The
/// message is also written to `startup-error.txt` in the data folder.
fn startup_failed(app: &tauri::AppHandle, message: &str) -> ! {
    if let Ok(dir) = data_dir(app) {
        let _ = std::fs::create_dir_all(&dir);
        let line = format!("{}  {message}\n\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("startup-error.txt"))
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
    startup_failed_plain(message)
}

/// The native box and the exit, for when there is no data folder to log to.
fn startup_failed_plain(message: &str) -> ! {
    tracing::error!(%message, "could not start");
    eprintln!("Flashwave.tf could not start: {message}");
    show_native_error("Flashwave.tf could not start", message);
    std::process::exit(1)
}

/// A message box that needs nothing from the app: the window may not exist
/// and the dialog plugin needs the event loop, which has not started.
#[cfg(windows)]
fn show_native_error(title: &str, message: &str) {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let wide = |s: &str| std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    // MB_OK | MB_ICONERROR | MB_SETFOREGROUND
    unsafe { MessageBoxW(std::ptr::null_mut(), wide(message).as_ptr(), wide(title).as_ptr(), 0x10 | 0x10000) };
}

#[cfg(not(windows))]
fn show_native_error(_title: &str, _message: &str) {}

/// Everything the window needs before it can open: the lock, a pending
/// restore, the database, and the background jobs. An `Err` is shown to the
/// person by [`startup_failed`].
fn start(app: &mut tauri::App) -> Result<(), String> {
    // The window title carries the version, and a literal in
    // tauri.conf.json is a literal somebody forgets: 0.4.0 shipped
    // with a title saying 0.3. Set it from the one constant instead.
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&format!("Flashwave.tf {DISPLAY_VERSION}"));
    }
    // Errors are stringified rather than passed through as `anyhow`:
    // Tauri's setup wants a `Box<dyn Error>`, and `{:#}` keeps the
    // whole cause chain in the message.
    let db_path = data_dir(app.handle())?.join("hl.sqlite3");
    if let Some(dir) = db_path.parent() {
        applog::open(dir);
    }
    tracing::info!(version = DISPLAY_VERSION, model = hl_rating::MODEL_VERSION, "Flashwave.tf starting");
    tracing::info!(dir = %db_path.parent().map(|p| p.display().to_string()).unwrap_or_default(), "data folder");

    // Hold the database for as long as this window is open, so no
    // other tool can write to it at the same time. Two writers on one
    // SQLite file corrupted this database twice on 25 September 2026.
    //
    // A restart or an update starts this process while the old one is
    // still closing, so the lock is waited for a few seconds. A second
    // window opened by hand waits those seconds too, and then says so.
    let lock = hl_ingest::lock::hold_waiting(&db_path, std::time::Duration::from_secs(8)).map_err(|_| {
        "Flashwave.tf is already running (or another tool has its database open).

Look for its window in the taskbar, or close the other one and start again.".to_string()
    })?;

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
    // September 2026. So a file SQLite calls damaged is moved aside and a
    // fresh one takes its place, which puts the start on the same path as
    // any other empty database: the restore offer, which now says which of
    // the two happened.
    //
    // Only damage earns that. A database from a newer version (a
    // tester going back to an older beta), a locked file and a full disk
    // all fail to open too, and the file is healthy: moving it would hand
    // the person an empty app with their data renamed next door. Those
    // stop the start with a message and leave the file where it is.
    let db = match tauri::async_runtime::block_on(Db::connect(&db_path)) {
        Ok(db) => db,
        Err(first) => {
            tracing::error!(error = %format!("{first:#}"), "database would not open");
            match hl_db::classify_open_error(&first) {
                hl_db::OpenFailure::Newer => {
                    return Err(format!(
                        "This data is from a newer version of Flashwave.tf than this one ({DISPLAY_VERSION}).

                         Install the newer version to keep using it. Nothing has been changed or moved.

{}",
                        db_path.display()
                    ));
                }
                hl_db::OpenFailure::Other => {
                    return Err(format!(
                        "The database could not be opened, and nothing has been moved: {first:#}

{}",
                        db_path.display()
                    ));
                }
                hl_db::OpenFailure::Corrupt => {}
            }
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

    // Held by the startup jobs below until they are done, so a sync or a
    // rebuild asked for in the first minute waits for them instead of
    // running the same passes (rate_all, fights, the demo index) a second
    // time at once. They wait rather than being refused, so the window's
    // first automatic sync still happens.
    let startup = Arc::new(tokio::sync::Mutex::new(()));
    let startup_guard = startup.clone().try_lock_owned().ok();

    // Index demos in the background at startup: the window should not
    // wait on a folder scan, and a missing TF2 folder is not an error.
    {
        let db = db.clone();
        let handle = app.handle().clone();
        let weights_path = db_path.with_file_name("weights.toml");
        let sources = sources.clone();
        tauri::async_runtime::spawn(async move {
            let _startup = startup_guard;
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
            match hl_ingest::fights::derive_all(&db, false, |_, _| {}).await {
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
            // The league snapshot this version ships (Q43): the officials'
            // results, rosters and ratings, so the player lookup works
            // before the league has been downloaded. Once per snapshot.
            match hl_ingest::snapshot::apply_built_in(&db).await {
                Ok(a) if a.rescore => {
                    // Its scale replaced the one measured here: rate
                    // this install's own games on it.
                    let (weights, _) = hl_rating::Weights::load(&weights_path);
                    match hl_ingest::rate_all(&db, cfg.steamid, &weights, startup_progress(&handle)).await {
                        Ok(s) => tracing::info!(rated = s.rated, "rated on the league snapshot's scale"),
                        Err(e) => tracing::warn!(error = %format!("{e:#}"), "rating on the snapshot's scale failed"),
                    }
                    let _ = tauri::Emitter::emit(&handle, "startup://rated", ());
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "league snapshot not imported"),
            }
            // A new rating model has no ratings until something rates:
            // do it now rather than leave the profile empty until a sync.
            if db.rating_count(hl_rating::MODEL_VERSION).await.unwrap_or(1) == 0 {
                let (weights, _) = hl_rating::Weights::load(&weights_path);
                match hl_ingest::rate_all(&db, cfg.steamid, &weights, startup_progress(&handle)).await {
                    Ok(s) => tracing::info!(rated = s.rated, model = hl_rating::MODEL_VERSION, "rated for a new model"),
                    Err(e) => tracing::warn!(error = %format!("{e:#}"), "rating pass failed"),
                }
                let _ = tauri::Emitter::emit(&handle, "startup://rated", ());
            }
        });
    }

    // Watch the demos folder, so a match you have just played shows
    // up on the page while you are alt-tabbing out of the game.
    if let Ok(cfg) = tauri::async_runtime::block_on(db.get_config()) {
        if let Some(tf) = cfg.tf_path {
            watch::spawn(app.handle().clone(), std::path::PathBuf::from(tf));
        }
    }

    // Every player's numbers in every log that has none yet: the
    // season tables, profiles and impact (Q85). A database from
    // before them has thousands of logs to read; a few minutes in
    // the background, a chunk at a time, then nothing until new
    // logs arrive (Flashy: S31 Low's table was all dashes).
    {
        let db = db.clone();
        tauri::async_runtime::spawn(async move {
            // Listed once: a log that cannot be read is passed over,
            // not tried again in a loop.
            let ids = match db.logs_without_stats(i64::MAX).await {
                Ok(ids) => ids,
                Err(e) => return tracing::warn!(error = %format!("{e:#}"), "listing logs without player numbers"),
            };
            let total = ids.len();
            for (i, chunk) in ids.chunks(100).enumerate() {
                match hl_ingest::logstat::fill(&db, chunk).await {
                    Ok(n) => tracing::info!(read = n, done = (i + 1) * 100, total, "player numbers"),
                    Err(e) => return tracing::warn!(error = %format!("{e:#}"), "reading player numbers stopped"),
                }
            }
        });
    }

    // The league sample, when it is switched on (Settings).
    let busy = Arc::new(AtomicBool::new(false));
    let league_activity = league::SharedActivity::default();
    // The mass log downloader is the developer's (Flashy): it runs in
    // a dev build only. A release never starts it, whatever the
    // database's setting says.
    if cfg!(debug_assertions) {
        league::spawn(db.clone(), sources.clone(), busy.clone(), league_activity.clone(), db_path.with_file_name("weights.toml"));
        league::spawn_transfers(db.clone(), sources.clone());
        league::spawn_sixes(db.clone(), sources.clone(), busy.clone());
    }

    app.manage(AppState {
        db,
        db_path,
        _lock: lock,
        sources,
        busy,
        sync_cancel: Arc::default(),
            startup,
        league_activity,
        demo_queue: Arc::new(sync_commands::DemoQueue::default()),
        demo_turn: Arc::new(tokio::sync::Semaphore::new(1)),
        mass_cancel: Arc::default(),
    });
    Ok(())
}

/// The startup rating pass's progress, for the window's card (audit, 12 Nov
/// 2026): it rates every game for a new model, and the profile sat on the old
/// numbers with nothing saying why until it was done. Its own events, not the
/// sync's: no sync is running, and the card must not offer to cancel one.
fn startup_progress(handle: &tauri::AppHandle) -> impl FnMut(hl_ingest::Progress) + '_ {
    move |p| {
        let _ = tauri::Emitter::emit(handle, "startup://progress", p);
    }
}

/// Where this build keeps its database, backups and language files.
///
/// A debug build (`npm run dev`) uses `dev-data/` in the repository, never
/// the installed app's AppData folder. Both lessons are from 27 September
/// 2026:
///
/// - The two builds shared one database. A dev build's newer migrations make
///   the installed app refuse the file as unopenable and move it aside, and
///   the two never saw each other's lock.
/// - Started from a packaged host -- the Claude desktop app is an MSIX
///   package -- Windows redirects every *new* file under AppData into the
///   package's private copy. The database stayed where it was, but its -wal
///   (the latest changes), its lock and the lang folder landed where Explorer
///   and the installed app could not see them.
///
/// `dev-data/` is outside AppData, so nothing redirects it, and it is
/// ignored by git.
fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(PathBuf::from)
            .ok_or("the repository folder could not be worked out")?;
        return Ok(repo.join("dev-data"));
    }
    app.path()
        .app_data_dir()
        .map_err(|e| format!("resolving the application data directory: {e}"))
}
