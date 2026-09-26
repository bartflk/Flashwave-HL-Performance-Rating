//! Noticing a demo the moment TF2 finishes writing it.
//!
//! The point is the alt-tab: you finish a match, switch out, and the game is
//! already on the page with its rating on it. Until now the app only looked
//! at the demos folder at startup and after a sync you asked for, so a match
//! you had just played was the one thing it could not show you.
//!
//! **Polling, not a filesystem watcher.** A demo is written continuously
//! while the match runs, so a change event fires hundreds of times and means
//! nothing; what matters is the file being *finished*, which no event
//! reports. Polling every few seconds and waiting for the size to stop
//! moving says exactly that, costs a directory listing, and needs no new
//! dependency.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// How often the demo folders are listed.
const POLL: Duration = Duration::from_secs(10);

/// A file whose size has not moved for this long is finished. TF2 flushes as
/// it records, so a demo still being written grows every few seconds.
const SETTLED_FOR: Duration = Duration::from_secs(20);

pub const EV_NEW_DEMO: &str = "demos://new";

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewDemo {
    pub file_name: String,
    /// Bytes, so the card can say how big the recording was.
    pub bytes: u64,
}

/// Where TF2 leaves recordings: the game folder, `demos`, and `demos/stv`.
fn demo_dirs(tf: &Path) -> Vec<PathBuf> {
    [tf.to_path_buf(), tf.join("demos"), tf.join("demos").join("stv")]
        .into_iter()
        .filter(|p| p.is_dir())
        .collect()
}

fn demos_in(dirs: &[PathBuf]) -> HashMap<PathBuf, u64> {
    let mut out = HashMap::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("dem")) {
                if let Ok(meta) = e.metadata() {
                    out.insert(path, meta.len());
                }
            }
        }
    }
    out
}

/// Watch until the app closes.
///
/// Everything present at startup is taken as already known: the app is not
/// interested in the four hundred demos you recorded last season, only in
/// the one that appeared while it was running.
pub fn spawn(app: AppHandle, tf: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let dirs = demo_dirs(&tf);
        if dirs.is_empty() {
            tracing::info!(tf = %tf.display(), "no demo folders to watch");
            return;
        }
        let mut known = demos_in(&dirs);
        // Files seen but not yet finished: path -> (size, polls unchanged).
        let mut settling: HashMap<PathBuf, (u64, u32)> = HashMap::new();
        let needed = (SETTLED_FOR.as_secs() / POLL.as_secs().max(1)).max(1) as u32;

        loop {
            tokio::time::sleep(POLL).await;
            let now = demos_in(&dirs);
            for (path, size) in &now {
                if known.contains_key(path) {
                    continue;
                }
                match settling.get(path).copied() {
                    Some((last, n)) if last == *size => {
                        if n + 1 < needed {
                            settling.insert(path.clone(), (*size, n + 1));
                            continue;
                        }
                        // Finished: announce it once and never again.
                        settling.remove(path);
                        known.insert(path.clone(), *size);
                        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                        tracing::info!(demo = %name, bytes = size, "new demo finished");
                        let _ = app.emit(EV_NEW_DEMO, NewDemo { file_name: name, bytes: *size });
                    }
                    // Still growing, or seen for the first time.
                    _ => {
                        settling.insert(path.clone(), (*size, 0));
                    }
                }
            }
            // A demo deleted while settling should not be waited on forever.
            settling.retain(|p, _| now.contains_key(p));
        }
    });
}
