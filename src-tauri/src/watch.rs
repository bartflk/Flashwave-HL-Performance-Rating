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

/// A file whose size has not moved for this long is finished -- once it has
/// something in it (see [`MIN_FINISHED_BYTES`]).
const SETTLED_FOR: Duration = Duration::from_secs(20);

/// A demo smaller than this is still being recorded, however long it has
/// sat still. TF2 can hold a recording at 0 bytes for the whole match and
/// write it at the end (Flashy, 30 Sept 2026: two demos "finished" 25 s
/// after they were created, at 0 bytes, so the after-game check ran at the
/// start of the match and gave up long before its log existed). A played
/// match's demo is megabytes; a header alone is about a kilobyte.
const MIN_FINISHED_BYTES: u64 = 256 * 1024;

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
                // A demo the app wrote (downloaded, unpacked, imported) is not
                // a match just played: known, and not announced, or each one of
                // a mass import starts two minutes of logs.tf lookups.
                if hl_ingest::app_wrote_demo(path) {
                    known.insert(path.clone(), *size);
                    continue;
                }
                match settle(settling.get(path).copied(), *size, needed) {
                    Some(state) => {
                        settling.insert(path.clone(), state);
                    }
                    // Finished: announce it once and never again.
                    None => {
                        settling.remove(path);
                        known.insert(path.clone(), *size);
                        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                        tracing::info!(demo = %name, bytes = size, "new demo finished");
                        let _ = app.emit(EV_NEW_DEMO, NewDemo { file_name: name, bytes: *size });
                    }
                }
            }
            // A demo deleted while settling should not be waited on forever.
            settling.retain(|p, _| now.contains_key(p));
        }
    });
}

/// One poll of one demo: its new `(size, polls unchanged)`, or `None` once
/// it is finished -- big enough to be a recording, and unchanged for
/// `needed` polls in a row.
fn settle(prev: Option<(u64, u32)>, size: u64, needed: u32) -> Option<(u64, u32)> {
    match prev {
        // Empty or header-only: still recording, however long it sits still.
        _ if size < MIN_FINISHED_BYTES => Some((size, 0)),
        Some((last, n)) if last == size => (n + 1 < needed).then_some((size, n + 1)),
        // Still growing, or seen for the first time.
        _ => Some((size, 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Polls a file of these sizes until it is announced; the poll it was.
    fn finished_at(sizes: &[u64]) -> Option<usize> {
        let mut state = None;
        for (i, &size) in sizes.iter().enumerate() {
            match settle(state, size, 2) {
                Some(s) => state = Some(s),
                None => return Some(i),
            }
        }
        None
    }

    #[test]
    fn an_empty_recording_is_not_finished_however_long_it_sits() {
        assert_eq!(finished_at(&[0; 50]), None, "TF2 can hold a demo at 0 bytes for the whole match");
        assert_eq!(finished_at(&[1_072; 50]), None, "a header alone is not a match");
    }

    #[test]
    fn a_demo_is_finished_once_written_and_still() {
        let mb = 1_000_000;
        // Empty through the match, written at the end, then still.
        assert_eq!(finished_at(&[0, 0, 0, 30 * mb, 30 * mb, 30 * mb]), Some(5));
        // Growing as it records, then still.
        assert_eq!(finished_at(&[mb, 2 * mb, 3 * mb, 3 * mb, 3 * mb]), Some(4));
    }
}
