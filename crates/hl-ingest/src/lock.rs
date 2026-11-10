//! One writer at a time.
//!
//! On 25 September 2026 this database was corrupted twice in an afternoon.
//! Both times the shape was the same: the app had it open, or had just had it
//! open, and a second process — the CLI — opened it as well. SQLite survives
//! that in principle; it did not survive it here, and the second time the
//! file was unreadable with the write-ahead log three hours out of step with
//! the database beside it.
//!
//! The fix is not care. Care is what failed. While the app runs it holds this
//! lock, and every other tool refuses to open the database until it lets go.
//!
//! The lock is a file held open with no sharing, so the operating system
//! releases it when the process ends however it ends — a crash leaves nothing
//! stale to clean up, which is the failure mode a PID file has.

use anyhow::{bail, Context, Result};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// The lock file beside the database.
pub fn path(db_path: &Path) -> PathBuf {
    db_path.with_file_name("app.lock")
}

/// Held for as long as the app runs. Dropping it releases the database.
#[derive(Debug)]
pub struct Lock {
    /// An `Option` so `Drop` can close the handle before it removes the
    /// file: fields drop after the `Drop` body, so a plain `File` was still
    /// open while `remove_file` ran (it fails on Windows, and on Linux it
    /// let a second instance lock a fresh file in that gap).
    file: Option<File>,
    path: PathBuf,
}

impl Drop for Lock {
    fn drop(&mut self) {
        // The handle closes first; the file itself is tidiness, and a failure
        // to remove it means nothing because the handle is what locks.
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(windows)]
fn open_exclusive(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    // share_mode 0: nobody else may open this file at all, and Windows drops
    // the handle when the process does.
    OpenOptions::new().create(true).write(true).truncate(true).share_mode(0).open(path)
}

#[cfg(not(windows))]
fn open_exclusive(path: &Path) -> std::io::Result<File> {
    // Linux: an exclusive lock on the open file (flock), which the kernel
    // drops when the process ends, however it ends. The file's presence
    // means nothing, so a crash cannot leave the app locked out; only a
    // process that is still running holds it.
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => {
            Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "held by another process"))
        }
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Take the lock, or say who has it.
pub fn hold(db_path: &Path) -> Result<Lock> {
    let path = path(db_path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = open_exclusive(&path).with_context(|| {
        format!("{} is already open in another window or tool", db_path.display())
    })?;
    Ok(Lock { file: Some(file), path })
}

/// [`hold`], trying again for a few seconds. A restart or an update starts the
/// new process while the old one is still closing, and the old one's handle
/// is what the new one collides with; without a wait the restore or update
/// "did nothing".
pub fn hold_waiting(db_path: &Path, wait: std::time::Duration) -> Result<Lock> {
    let until = std::time::Instant::now() + wait;
    loop {
        match hold(db_path) {
            Ok(lock) => return Ok(lock),
            Err(e) if std::time::Instant::now() >= until => return Err(e),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(250)),
        }
    }
}

/// Whether something else has the database open right now.
pub fn in_use(db_path: &Path) -> bool {
    let path = path(db_path);
    if !path.exists() {
        return false;
    }
    // If it can be opened exclusively then nobody holds it, and the file is
    // a leftover: say so by removing it.
    match open_exclusive(&path) {
        Ok(f) => {
            drop(f);
            let _ = std::fs::remove_file(&path);
            false
        }
        Err(_) => true,
    }
}

/// Refuse to go on while the app has the database, unless told to anyway.
pub fn require_free(db_path: &Path, force: bool) -> Result<()> {
    if !in_use(db_path) || force {
        return Ok(());
    }
    bail!(
        "the app has this database open ({}).\n\
         Close the app window and run this again.\n\
         Two processes writing one SQLite file is what corrupted this database twice on\n\
         25 September 2026. Pass --force only if you know the app is not running.",
        db_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-lock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("hl.sqlite3")
    }

    #[test]
    fn a_held_database_is_in_use_and_a_released_one_is_not() {
        let db = temp("held");
        assert!(!in_use(&db), "nothing holds it yet");

        let lock = hold(&db).expect("the first caller gets it");
        assert!(in_use(&db), "and everyone else sees that");
        assert!(require_free(&db, false).is_err(), "so they are turned away");
        assert!(require_free(&db, true).is_ok(), "unless they insist");

        drop(lock);
        assert!(!in_use(&db), "letting go frees it");
        assert!(require_free(&db, false).is_ok());
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    /// What a crash leaves: the lock file, with nobody holding it. The next
    /// start must get in, on Linux as on Windows.
    #[test]
    fn a_leftover_lock_file_does_not_lock_anyone_out() {
        let db = temp("leftover");
        std::fs::write(path(&db), "").unwrap();
        assert!(!in_use(&db), "nobody holds it");
        std::fs::write(path(&db), "").unwrap();
        assert!(hold(&db).is_ok(), "and the app starts");
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn two_holders_are_refused() {
        let db = temp("two");
        let first = hold(&db).unwrap();
        assert!(hold(&db).is_err(), "the second is refused rather than allowed alongside");
        drop(first);
        assert!(hold(&db).is_ok(), "and gets it once the first lets go");
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn waiting_for_the_lock_gets_it_once_the_old_holder_exits() {
        let db = temp("waiting");
        let first = hold(&db).unwrap();
        let letting_go = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(600));
            drop(first);
        });
        assert!(hold_waiting(&db, std::time::Duration::from_secs(5)).is_ok());
        letting_go.join().unwrap();
        let db2 = temp("waiting-no");
        let _held = hold(&db2).unwrap();
        assert!(hold_waiting(&db2, std::time::Duration::from_millis(300)).is_err(), "gives up when it stays held");
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
        let _ = std::fs::remove_dir_all(db2.parent().unwrap());
    }

    #[test]
    fn a_lock_file_nobody_holds_is_just_litter() {
        let db = temp("stale");
        std::fs::write(path(&db), b"left behind by a crash").unwrap();
        assert!(!in_use(&db), "a file nobody holds locks nothing");
        assert!(!path(&db).exists(), "and is cleared away");
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }
}
