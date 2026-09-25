//! Putting a backup back, and noticing when one should be offered.
//!
//! A tester lost their database to an uninstaller in September 2026 and the
//! app said nothing about it: it opened empty, asked for a SteamID as if new,
//! and started downloading twelve years of logs again — with a 200 MB copy
//! sitting in the folder next door.
//!
//! Two halves:
//!
//! * [`offer`] looks for that situation — an empty database with a backup
//!   beside it — so the app can say so before it asks anything else.
//! * [`request`] and [`apply_pending`] do the swap. The swap cannot happen
//!   while the app is running, because SQLite holds the file open and Windows
//!   will not let it be replaced underneath. So a request writes a marker and
//!   restarts, and the copy happens at startup with nothing connected.

use crate::backup;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// The file naming the backup to put back on the next start. It lives beside
/// the database, holds one path, and is deleted as soon as it is acted on.
const MARKER: &str = "pending-restore.txt";

/// Where the setting recording an unreadable database is kept, in the fresh
/// database that replaced it.
pub const SET_ASIDE_KEY: &str = "database_set_aside";

/// A backup worth going back to, and why we are asking.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOffer {
    /// `empty` — the database opened and holds nothing.
    /// `unreadable` — it would not open at all and was moved aside.
    pub reason: &'static str,
    /// Where the unreadable file went, so it is not a mystery and can be sent
    /// to someone who might make sense of it.
    pub set_aside: Option<String>,
    /// The backup on offer. `None` means there is nothing to go back to: the
    /// screen then only explains, which is still better than saying nothing.
    pub backup: Option<BackupOffer>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOffer {
    pub path: String,
    pub bytes: u64,
    /// Unix seconds.
    pub made_at: i64,
    /// Highlander matches inside it, so the offer is about what you get back
    /// rather than about a number of megabytes.
    pub matches: i64,
}

/// Whether this start needs to say something before it asks for anything: an
/// empty database, or one that had to be moved aside to open at all.
///
/// Returns `None` in every ordinary case — a database with matches in it, and
/// a genuinely new install with nothing behind it.
pub async fn offer(db: &hl_db::Db, db_path: &Path) -> Result<Option<RestoreOffer>> {
    if db.index_stats().await?.indexed > 0 {
        return Ok(None);
    }
    let set_aside = db.get_setting(SET_ASIDE_KEY).await?;
    let reason = if set_aside.is_some() { "unreadable" } else { "empty" };

    let mut backup = None;
    for b in backup::list(db_path) {
        let matches = hl_db::Db::peek_matches(Path::new(&b.path)).await;
        if matches > 0 {
            backup = Some(BackupOffer { path: b.path, bytes: b.bytes, made_at: b.made_at, matches });
            break;
        }
    }
    // An empty database with nothing behind it is a new install, and has
    // nothing to be told. An unreadable one is worth explaining either way.
    if backup.is_none() && reason == "empty" {
        return Ok(None);
    }
    Ok(Some(RestoreOffer { reason, set_aside, backup }))
}

/// Move a database that will not open out of the way, so a fresh one can take
/// its place. Returns where it went.
///
/// A file this app cannot read is not a file this app should delete: it is the
/// only copy of whatever was in it, and someone with a hex editor may get more
/// out of it than we can. The journal files go with it, since they belong to
/// it and would otherwise be replayed over the replacement.
pub fn set_aside(db_path: &Path) -> Result<PathBuf> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let to = db_path.with_extension(format!("unreadable-{stamp}.sqlite3"));
    std::fs::rename(db_path, &to)
        .with_context(|| format!("moving {} aside", db_path.display()))?;
    for ext in ["sqlite3-wal", "sqlite3-shm"] {
        let from = db_path.with_extension(ext);
        if from.exists() {
            let _ = std::fs::rename(&from, to.with_extension(format!("{ext}.old")));
        }
    }
    tracing::error!(from = %db_path.display(), to = %to.display(), "database would not open; moved aside");
    Ok(to)
}

/// Note that this backup should be put back on the next start.
///
/// The file is checked here rather than at startup, so a bad path is an error
/// the user sees at the moment they ask, not a silent no-op after a restart.
pub fn request(db_path: &Path, backup_path: &Path) -> Result<()> {
    if !backup_path.is_file() {
        bail!("{} is not a file", backup_path.display());
    }
    let marker = db_path.with_file_name(MARKER);
    std::fs::write(&marker, backup_path.to_string_lossy().as_bytes())
        .with_context(|| format!("writing {}", marker.display()))?;
    Ok(())
}

/// The backup a [`request`] is waiting on, if any.
pub fn pending(db_path: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(db_path.with_file_name(MARKER)).ok()?;
    let path = PathBuf::from(text.trim());
    path.is_file().then_some(path)
}

/// Put a requested backup in place. Call before anything opens the database.
///
/// Returns the path restored, or `None` when nothing was waiting. The marker
/// is cleared either way: a restore that cannot be done must not be retried
/// on every start forever.
pub fn apply_pending(db_path: &Path) -> Result<Option<PathBuf>> {
    let marker = db_path.with_file_name(MARKER);
    let Some(from) = pending(db_path) else {
        let _ = std::fs::remove_file(&marker);
        return Ok(None);
    };

    let result = swap(db_path, &from);
    let _ = std::fs::remove_file(&marker);
    result.with_context(|| format!("restoring {}", from.display()))?;
    Ok(Some(from))
}

/// Replace the database with `from`, keeping what was there under `.replaced`.
///
/// The old file is kept rather than deleted: this runs on a database believed
/// to be empty, and "believed" is not "known" — a mistake here would be the
/// second data loss in a feature written because of the first.
fn swap(db_path: &Path, from: &Path) -> Result<()> {
    // The write-ahead log and shared-memory file belong to the old database.
    // Left behind, SQLite would replay them over the restored one.
    for ext in ["sqlite3-wal", "sqlite3-shm"] {
        let _ = std::fs::remove_file(db_path.with_extension(ext));
    }
    if db_path.exists() {
        let aside = db_path.with_extension("sqlite3.replaced");
        let _ = std::fs::remove_file(&aside);
        std::fs::rename(db_path, &aside)
            .with_context(|| format!("moving the old database to {}", aside.display()))?;
    }
    std::fs::copy(from, db_path)
        .with_context(|| format!("copying to {}", db_path.display()))?;
    tracing::info!(from = %from.display(), "database restored from a backup");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-restore-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_request_names_the_file_and_a_missing_one_is_refused() {
        let dir = temp("request");
        let db = dir.join("hl.sqlite3");
        let backup = dir.join("copy.sqlite3");
        std::fs::write(&backup, b"not really a database").unwrap();

        assert!(request(&db, &dir.join("gone.sqlite3")).is_err(), "a missing file is an error");
        assert_eq!(pending(&db), None, "and leaves nothing behind");

        request(&db, &backup).unwrap();
        assert_eq!(pending(&db), Some(backup));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn applying_puts_the_copy_in_place_and_keeps_the_old_one() {
        let dir = temp("apply");
        let db = dir.join("hl.sqlite3");
        let backup = dir.join("copy.sqlite3");
        std::fs::write(&db, b"the empty one").unwrap();
        std::fs::write(&backup, b"the good one").unwrap();
        // Stale journal files from the database being replaced.
        std::fs::write(dir.join("hl.sqlite3-wal"), b"stale").unwrap();

        request(&db, &backup).unwrap();
        let done = apply_pending(&db).unwrap();

        assert_eq!(done.as_deref(), Some(backup.as_path()));
        assert_eq!(std::fs::read(&db).unwrap(), b"the good one");
        assert_eq!(std::fs::read(dir.join("hl.sqlite3.replaced")).unwrap(), b"the empty one");
        assert!(!dir.join("hl.sqlite3-wal").exists(), "the old write-ahead log is gone");
        assert_eq!(pending(&db), None, "and the marker is cleared");
        assert!(apply_pending(&db).unwrap().is_none(), "a second start does nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The 25 September 2026 case: a file that will not open at all. It used
    /// to end the start with "Could not start" and no way forward.
    #[tokio::test]
    async fn a_database_that_will_not_open_is_moved_aside_rather_than_fatal() {
        let dir = temp("unreadable");
        let db_path = dir.join("hl.sqlite3");
        // A real one, so the bytes that replace it have somewhere to be.
        let good = hl_db::Db::connect(&db_path).await.unwrap();
        good.close().await;
        // Now break it the way it broke: a header that is not a database.
        std::fs::write(&db_path, b"SQLite format 3  and then nonsense").unwrap();
        std::fs::write(dir.join("hl.sqlite3-wal"), b"a journal for a file that is gone").unwrap();

        assert!(hl_db::Db::connect(&db_path).await.is_err(), "the broken file must not open");

        let moved = set_aside(&db_path).unwrap();
        assert!(moved.exists(), "the old file is kept, not deleted");
        assert!(!db_path.exists(), "and is out of the way");
        assert!(!dir.join("hl.sqlite3-wal").exists(), "its journal went with it");

        // A fresh one takes its place, which is the whole point.
        let fresh = hl_db::Db::connect(&db_path).await.unwrap();
        fresh.set_setting(SET_ASIDE_KEY, &format!("{}|broken", moved.display())).await.unwrap();

        let offer = offer(&fresh, &db_path).await.unwrap().expect("this is worth saying out loud");
        assert_eq!(offer.reason, "unreadable");
        assert!(offer.set_aside.unwrap().starts_with(&moved.display().to_string()));
        assert!(offer.backup.is_none(), "nothing was backed up in this test");

        fresh.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_offer_appears_only_for_an_empty_database_with_something_to_go_back_to() {
        let dir = temp("offer");
        let db_path = dir.join("hl.sqlite3");
        let db = hl_db::Db::connect(&db_path).await.unwrap();

        assert!(offer(&db, &db_path).await.unwrap().is_none(), "nothing to offer on a new install");

        // A copy of the empty database is not worth going back to either.
        backup::run(&db, &db_path, true).await.unwrap();
        assert!(offer(&db, &db_path).await.unwrap().is_none(), "an empty copy is not an offer");

        // One with matches in it is. It is made here and then moved next to a
        // fresh, empty database — which is what a wipe leaves behind.
        db.upsert_logstf_rows(&[hl_db::LogsTfIndexRow {
            log_id: 3_902_155,
            title: Some("a match"),
            map: Some("pl_upward"),
            played_at: Some(1_789_675_741),
            player_count: Some(18),
            raw_json: "{}",
        }])
        .await
        .unwrap();
        let full = backup::run(&db, &db_path, true).await.unwrap().unwrap();
        db.close().await;

        let fresh = temp("offer-fresh");
        let fresh_db = fresh.join("hl.sqlite3");
        std::fs::create_dir_all(backup::dir(&fresh_db)).unwrap();
        std::fs::copy(&full.path, backup::dir(&fresh_db).join("hl-old.sqlite3")).unwrap();
        let empty = hl_db::Db::connect(&fresh_db).await.unwrap();

        let found = offer(&empty, &fresh_db).await.unwrap().expect("the copy is worth offering");
        assert_eq!(found.reason, "empty");
        assert_eq!(found.backup.expect("a backup is on offer").matches, 1, "and says what is in it");
        empty.close().await;

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&fresh);
    }
}
