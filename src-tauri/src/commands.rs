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

/// How this copy is updated: `"self"` when the updater can replace it (the
/// Windows installer, or an AppImage, which Linux runs from one file), and
/// `"manual"` for a Linux package, which belongs to the system's package
/// manager: there the app points at the download instead.
#[tauri::command]
pub fn update_kind() -> &'static str {
    if cfg!(windows) || std::env::var_os("APPIMAGE").is_some() {
        "self"
    } else {
        "manual"
    }
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
/// file be replaced underneath.
///
/// A database with matches in it is replaced too: this used to refuse unless
/// the database was empty, which made every Restore button in Settings >
/// Backups fail for anyone who had a history, and told them to rename files
/// by hand. What was there is not lost: the swap keeps it beside the new one
/// as `hl.replaced-<time>.sqlite3` with its write-ahead log, and the Backups
/// panel lists it.
#[tauri::command]
pub async fn restore_backup(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<()> {
    // A sync writing to the database as it goes down would leave the kept
    // copy half-written.
    let Some(_guard) = crate::sync_commands::BusyGuard::acquire(&state.busy) else {
        return Err(CmdError::new("busy", "A sync is running; restore when it has finished."));
    };
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

/// The folder translators drop `.lang` files into: beside the database, so
/// it survives updates and reinstalls, and Settings can open it in one click.
fn lang_dir(state: &AppState) -> std::path::PathBuf {
    state
        .db_path
        .parent()
        .map(|p| p.join("lang"))
        .unwrap_or_else(|| std::path::PathBuf::from("lang"))
}

/// A file name that is a language id and nothing else: no separators, so a
/// save can never land outside the folder.
fn language_id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 16 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[derive(Debug, Serialize)]
pub struct LanguageFile {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct LanguageFiles {
    pub dir: String,
    pub files: Vec<LanguageFile>,
}

/// The translations built into this version, from `lang/` in the repository.
/// They are also written into the user's lang folder, so a translator finds
/// their language there ready to edit instead of having to export it first.
const SHIPPED_LANGUAGES: &[(&str, &str)] = &[
    ("fr", include_str!("../../lang/fr.lang")),
    ("pt", include_str!("../../lang/pt.lang")),
    ("es", include_str!("../../lang/es.lang")),
    ("ru", include_str!("../../lang/ru.lang")),
    // The English on both sides, for correcting the English itself.
    ("en", include_str!("../../lang/en.lang")),
];

/// Which shipped files this app wrote, by content hash, so it can tell its
/// own untouched copy from one somebody edited.
const SHIPPED_RECORD: &str = ".shipped.json";

/// FNV-1a over the text. Stable across builds and Rust versions, which the
/// standard hasher does not promise, and the record outlives both.
fn content_hash(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Put the shipped language files in the user's folder.
///
/// A file in that folder beats the built-in text line by line, so a stale
/// copy would hide every fix a later version ships. Hence three cases:
/// - missing: written;
/// - exactly as an earlier version of this app wrote it: replaced with this
///   version's, so an update's corrections reach the screen;
/// - anything else: somebody edited it, and it is never touched.
///
/// Best effort throughout: a folder that cannot be written costs only the
/// convenience, and the built-in translations still work.
fn ship_languages(dir: &std::path::Path) {
    let record_path = dir.join(SHIPPED_RECORD);
    let mut record: std::collections::BTreeMap<String, String> = std::fs::read_to_string(&record_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let before = record.clone();
    for (id, text) in SHIPPED_LANGUAGES {
        let path = dir.join(format!("{id}.lang"));
        let ours = content_hash(text);
        let write = match std::fs::read_to_string(&path) {
            Ok(current) if current == *text => {
                record.insert((*id).to_string(), ours.clone());
                false
            }
            Ok(current) => record.get(*id) == Some(&content_hash(&current)),
            Err(_) => !path.exists(),
        };
        if !write {
            continue;
        }
        match std::fs::write(&path, text) {
            Ok(()) => {
                record.insert((*id).to_string(), ours);
                tracing::info!(path = %path.display(), "shipped language file written");
            }
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "writing a shipped language file failed"),
        }
    }
    if record != before {
        if let Ok(json) = serde_json::to_string_pretty(&record) {
            if let Err(e) = std::fs::write(&record_path, json) {
                tracing::warn!(path = %record_path.display(), error = %e, "recording shipped language files failed");
            }
        }
    }
}

/// Every `.lang` file in the user's language folder, read as text.
///
/// The UI lays each over the built-in translation of the same name, so a
/// fixed line wins and a new file (`pl.lang`) adds a language. Parsing stays
/// in the UI, which already has the format, and a file that cannot be read
/// is skipped with a warning rather than failing the rest.
#[tauri::command]
pub async fn language_files(state: State<'_, AppState>) -> CmdResult<LanguageFiles> {
    let dir = lang_dir(&state);
    // Made on first ask, so "Open the language folder" always has one to open.
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(dir = %dir.display(), error = %e, "creating the language folder failed");
    }
    ship_languages(&dir);
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("lang") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|s| language_id_ok(s)) else {
                continue;
            };
            match std::fs::read_to_string(&path) {
                Ok(text) => files.push(LanguageFile { id: id.to_lowercase(), text }),
                Err(e) => tracing::warn!(path = %path.display(), error = %e, "reading a language file failed"),
            }
        }
    }
    files.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(LanguageFiles { dir: dir.display().to_string(), files })
}

/// Players' ETF2L names by account id, for showing in place of the name in
/// the log. Read from rosters already stored: nothing is fetched.
#[tauri::command]
pub async fn etf2l_names(state: State<'_, AppState>) -> CmdResult<std::collections::HashMap<u32, String>> {
    Ok(state.db.etf2l_names().await?)
}

#[derive(Debug, Serialize)]
pub struct SavedLanguageFile {
    pub path: String,
    /// False when a file of that name was already there. It is left alone:
    /// it may hold someone's unsent fixes, and this must never overwrite them.
    pub created: bool,
}

/// Write a language out to the user's folder for editing.
#[tauri::command]
pub async fn save_language_file(
    state: State<'_, AppState>,
    id: String,
    text: String,
) -> CmdResult<SavedLanguageFile> {
    if !language_id_ok(&id) {
        return Err(CmdError::new("bad_language", format!("`{id}` is not a language file name")));
    }
    let dir = lang_dir(&state);
    std::fs::create_dir_all(&dir)
        .map_err(|e| CmdError::new("lang_folder", format!("Could not create `{}`: {e}", dir.display())))?;
    let path = dir.join(format!("{id}.lang"));
    if path.exists() {
        return Ok(SavedLanguageFile { path: path.display().to_string(), created: false });
    }
    std::fs::write(&path, text)
        .map_err(|e| CmdError::new("lang_write", format!("Could not write `{}`: {e}", path.display())))?;
    tracing::info!(path = %path.display(), "language file saved for editing");
    Ok(SavedLanguageFile { path: path.display().to_string(), created: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-lang-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_is_written() {
        let dir = scratch("missing");
        ship_languages(&dir);
        assert_eq!(std::fs::read_to_string(dir.join("fr.lang")).unwrap(), SHIPPED_LANGUAGES[0].1);
    }

    #[test]
    fn an_untouched_old_copy_is_brought_up_to_date() {
        let dir = scratch("old");
        let old = "language = \"Français\"\n\"Matches\" = \"Matchs (old)\"\n";
        std::fs::write(dir.join("fr.lang"), old).unwrap();
        std::fs::write(dir.join(SHIPPED_RECORD), format!("{{\"fr\": \"{}\"}}", content_hash(old))).unwrap();
        ship_languages(&dir);
        assert_eq!(std::fs::read_to_string(dir.join("fr.lang")).unwrap(), SHIPPED_LANGUAGES[0].1);
    }

    #[test]
    fn an_edited_copy_is_never_touched() {
        let dir = scratch("edited");
        ship_languages(&dir);
        let edited = "\"Matches\" = \"Parties\"\n";
        std::fs::write(dir.join("fr.lang"), edited).unwrap();
        ship_languages(&dir);
        assert_eq!(std::fs::read_to_string(dir.join("fr.lang")).unwrap(), edited);
    }
}
