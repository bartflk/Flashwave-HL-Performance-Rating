//! Copies of the database, kept before anything large touches it.
//!
//! The database is the only thing here that cannot be downloaded again in a
//! minute: raw logs, demos indexes, ratings and every derived table sit in one
//! file. A tester lost theirs to an uninstaller in September 2026, so a copy
//! is taken before every sync and rebuild.
//!
//! `VACUUM INTO` is SQLite's own way of doing this: it writes a consistent
//! copy while the app is still using the original, and the result is a plain
//! database file that can be copied back by hand.

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// How many copies to keep. Five is a few days of use, and a full history is
/// about 200 MB, so the folder stays under a gigabyte.
pub const KEEP: usize = 5;

/// A copy is not worth taking if the last one is this new.
pub const MIN_GAP_S: u64 = 60 * 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub path: String,
    pub bytes: u64,
    /// Unix seconds.
    pub made_at: i64,
}

/// Where copies live: a `backups` folder beside the database.
pub fn dir(db_path: &Path) -> PathBuf {
    db_path.with_file_name("backups")
}

/// Every copy, newest first.
pub fn list(db_path: &Path) -> Vec<Backup> {
    let mut out: Vec<Backup> = std::fs::read_dir(dir(db_path))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "sqlite3"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some(Backup {
                path: e.path().to_string_lossy().into_owned(),
                bytes: meta.len(),
                made_at: meta.modified().ok().and_then(when).unwrap_or(0),
            })
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.made_at));
    out
}

/// Write a copy wherever the person asked for it, and do not prune it.
///
/// The Backups panel has always said "keep one elsewhere if it matters to
/// you" — uninstalling offers to delete the app's data and takes the
/// automatic copies with it — while giving no way to do that. This is the
/// way. `VACUUM INTO`, like every other copy this app makes, because the
/// database is three files in WAL mode and copying one of them tears it.
pub async fn save_as(db: &hl_db::Db, to: &Path) -> Result<Backup> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    // Never write over something already there: this is the button people
    // press when they are worried about losing data.
    if to.exists() {
        anyhow::bail!("{} already exists — pick another name.", to.display());
    }
    let temp = to.with_extension("part");
    let _ = std::fs::remove_file(&temp);
    db.vacuum_into(&temp).await.with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, to).with_context(|| format!("renaming {}", to.display()))?;
    let bytes = std::fs::metadata(to).map(|m| m.len()).unwrap_or(0);
    Ok(Backup { path: to.display().to_string(), made_at: now_s(), bytes })
}

/// Take a copy unless a recent one exists, then prune to [`KEEP`].
///
/// Returns the copy made, or `None` when the last one was under
/// [`MIN_GAP_S`] old. Errors are the caller's to log: a sync must not fail
/// because a disk is full, but the user should be told.
pub async fn run(db: &hl_db::Db, db_path: &Path, force: bool) -> Result<Option<Backup>> {
    let existing = list(db_path);
    let now = now_s();
    if !force {
        if let Some(last) = existing.first() {
            if now.saturating_sub(last.made_at) < MIN_GAP_S as i64 {
                return Ok(None);
            }
        }
    }

    let dir = dir(db_path);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let stem = db_path.file_stem().map_or_else(|| "hl".into(), |s| s.to_string_lossy().into_owned());
    let path = dir.join(format!("{stem}-{}.sqlite3", stamp(now)));
    // A half-written copy would look like a good one, so it is written under a
    // temporary name and renamed once SQLite is done with it.
    let temp = path.with_extension("part");
    let _ = std::fs::remove_file(&temp);
    db.vacuum_into(&temp).await.with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, &path).with_context(|| format!("renaming {}", path.display()))?;

    for old in existing.iter().skip(KEEP.saturating_sub(1)) {
        let _ = std::fs::remove_file(&old.path);
    }

    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    tracing::info!(path = %path.display(), bytes, "database backed up");
    Ok(Some(Backup { path: path.to_string_lossy().into_owned(), bytes, made_at: now }))
}

fn now_s() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn when(t: std::time::SystemTime) -> Option<i64> {
    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

/// `20260920-134233`, so the folder sorts by hand as well as by date.
fn stamp(unix: i64) -> String {
    let dt = chrono::DateTime::from_timestamp(unix, 0).unwrap_or_default();
    dt.format("%Y%m%d-%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stamp_reads_as_a_date_and_sorts_that_way() {
        let a = stamp(1_789_675_741);
        let b = stamp(1_789_675_741 + 3600);
        assert_eq!(a.len(), 15, "{a}");
        assert!(a < b, "{a} then {b}");
    }

    #[tokio::test]
    async fn a_copy_is_taken_once_an_hour_and_only_five_are_kept() {
        let dir = std::env::temp_dir().join(format!("hl-backup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("hl.sqlite3");
        let db = hl_db::Db::connect(&db_path).await.unwrap();

        let first = run(&db, &db_path, false).await.unwrap();
        assert!(first.is_some(), "the first run always takes one");
        assert!(run(&db, &db_path, false).await.unwrap().is_none(), "an hour has not passed");

        // Forced copies pile up, and the oldest are dropped.
        for _ in 0..6 {
            // Stamps are per second, so the names would collide otherwise.
            std::thread::sleep(std::time::Duration::from_millis(1100));
            run(&db, &db_path, true).await.unwrap();
        }
        let kept = list(&db_path);
        assert_eq!(kept.len(), KEEP, "{kept:?}");
        // Every kept copy is a database that opens.
        let opened = hl_db::Db::connect(std::path::Path::new(&kept[0].path)).await;
        assert!(opened.is_ok(), "{:?}", opened.err());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
