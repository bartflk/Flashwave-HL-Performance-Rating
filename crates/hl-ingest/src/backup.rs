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
//!
//! **Size** (Flashy, October 2026: a tester's folder had reached 14 GB). Five
//! whole copies were fine at 200 MB; the league sample made the database 1.5
//! to 2.3 GB, half of it every official's logs.tf JSON and server log, which
//! the league sample downloads again anyway. So a copy leaves those out
//! ([`hl_db::Db::slim_backup`]), the copies kept are the newest and one a day
//! before it, within [`BUDGET`], and half-written ones left by a sync that was
//! cut off are cleared.

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// How many copies to keep at most: the newest, and one a day before it.
pub const KEEP: usize = 5;

/// What the copies may take together. The newest is kept whatever its size;
/// older ones only while they fit. Five copies without the league's logs are
/// about 3 GB on a large install.
pub const BUDGET: u64 = 3_000_000_000;

/// A copy is not worth taking if the last one is this new.
pub const MIN_GAP_S: u64 = 60 * 60;

/// A file in the backups folder that is not a finished copy -- a `.part` a
/// sync was cut off writing -- is cleared once it is this old.
pub const STRAY_AFTER_S: i64 = 60 * 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub path: String,
    pub bytes: u64,
    /// Unix seconds.
    pub made_at: i64,
}

/// Something beside the copies that takes room: a half-written copy, or a
/// database set aside when it would not open or was replaced by a restore.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stray {
    pub path: String,
    pub bytes: u64,
    pub made_at: i64,
    /// `partial` -- a copy never finished; `unreadable` -- a database that
    /// would not open, moved aside; `replaced` -- the database a restore put
    /// a copy in place of.
    pub kind: &'static str,
}

/// Everything the app keeps on disk beside its database, for the Backups
/// panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub dir: String,
    /// The database itself, its write-ahead log included.
    pub database: u64,
    pub backups: Vec<Backup>,
    pub backups_bytes: u64,
    pub strays: Vec<Stray>,
    pub strays_bytes: u64,
    pub keep: usize,
    pub budget: u64,
}

/// One part of what the database holds.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub key: &'static str,
    pub label: &'static str,
    pub bytes: i64,
    /// Whether a backup holds it: the league's logs are left out.
    pub in_backups: bool,
    /// Whether it downloads again by itself if lost.
    pub comes_back: bool,
    /// Its tables, largest first.
    pub tables: Vec<(String, i64)>,
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
/// Whole, the league's logs included: it is the one copy kept on purpose.
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

/// Take a copy unless a recent one exists, then prune ([`prune`]) and clear
/// what an interrupted copy left behind.
///
/// Returns the copy made, or `None` when the last one was under
/// [`MIN_GAP_S`] old. Errors are the caller's to log: a sync must not fail
/// because a disk is full, but the user should be told.
pub async fn run(db: &hl_db::Db, db_path: &Path, force: bool) -> Result<Option<Backup>> {
    let existing = list(db_path);
    let now = now_s();
    if !force {
        // The newest copy that is not dated in the future: a clock that was
        // set back says nothing about how long ago a copy "from tomorrow"
        // was, and trusting it would take no backup until the clock caught up.
        if let Some(last) = existing.iter().find(|b| b.made_at <= now) {
            if now - last.made_at < MIN_GAP_S as i64 {
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
    hl_db::Db::slim_backup(&temp).await?;
    std::fs::rename(&temp, &path).with_context(|| format!("renaming {}", path.display()))?;

    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let made = Backup { path: path.to_string_lossy().into_owned(), bytes, made_at: now };
    let mut all = vec![made.clone()];
    all.extend(existing.into_iter().filter(|b| b.path != made.path));
    for old in prune(&all) {
        let _ = std::fs::remove_file(&old.path);
    }
    clear_strays(db_path, now);

    tracing::info!(path = %path.display(), bytes, "database backed up");
    Ok(Some(made))
}

/// The copies to delete, of `all` newest first: past the newest, one a day
/// (the newest of its day), at most [`KEEP`], while they fit in [`BUDGET`].
pub fn prune(all: &[Backup]) -> Vec<Backup> {
    let day = |t: i64| t.div_euclid(24 * 3600);
    let mut kept: Vec<&Backup> = Vec::new();
    let mut total = 0u64;
    let mut out = Vec::new();
    for (i, b) in all.iter().enumerate() {
        let new_day = !kept.iter().any(|k| day(k.made_at) == day(b.made_at));
        if i == 0 || (kept.len() < KEEP && new_day && total + b.bytes <= BUDGET) {
            total += b.bytes;
            kept.push(b);
        } else {
            out.push(b.clone());
        }
    }
    out
}

/// Files in the backups folder that are not copies, and databases set aside
/// beside the database.
pub fn strays(db_path: &Path) -> Vec<Stray> {
    let entry = |p: PathBuf, kind: &'static str| {
        let meta = std::fs::metadata(&p).ok()?;
        meta.is_file().then(|| Stray { bytes: meta.len(), made_at: meta.modified().ok().and_then(when).unwrap_or(0), path: p.to_string_lossy().into_owned(), kind })
    };
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir(db_path)).into_iter().flatten().flatten() {
        if !e.path().extension().is_some_and(|x| x == "sqlite3") {
            out.extend(entry(e.path(), "partial"));
        }
    }
    let stem = db_path.file_stem().map_or_else(|| "hl".into(), |s| s.to_string_lossy().into_owned());
    let name = db_path.file_name().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    for e in db_path.parent().and_then(|d| std::fs::read_dir(d).ok()).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with(&format!("{stem}.unreadable")) {
            out.extend(entry(e.path(), "unreadable"));
        } else if n == format!("{name}.replaced") {
            out.extend(entry(e.path(), "replaced"));
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.made_at));
    out
}

/// Clear the half-written copies (`.part`) an interrupted sync left: only those, and
/// only once they are [`STRAY_AFTER_S`] old, so a copy being written now is
/// not touched. Databases set aside are left for the person to decide.
fn clear_strays(db_path: &Path, now: i64) {
    for s in strays(db_path).into_iter().filter(|s| s.kind == "partial" && s.path.ends_with(".part") && now - s.made_at >= STRAY_AFTER_S) {
        if std::fs::remove_file(&s.path).is_ok() {
            tracing::info!(path = %s.path, bytes = s.bytes, "half-written backup cleared");
        }
    }
}

/// The Backups panel's picture: the database, its copies and what else lies
/// beside them.
pub fn overview(db_path: &Path) -> Overview {
    let database = ["", "-wal", "-shm"]
        .iter()
        .filter_map(|suffix| std::fs::metadata(format!("{}{suffix}", db_path.display())).ok())
        .map(|m| m.len())
        .sum();
    let backups = list(db_path);
    let strays = strays(db_path);
    Overview {
        dir: dir(db_path).to_string_lossy().into_owned(),
        database,
        backups_bytes: backups.iter().map(|b| b.bytes).sum(),
        strays_bytes: strays.iter().map(|s| s.bytes).sum(),
        backups,
        strays,
        keep: KEEP,
        budget: BUDGET,
    }
}

/// Delete one copy or set-aside file the overview lists; nothing else.
pub fn delete(db_path: &Path, path: &str) -> Result<u64> {
    let o = overview(db_path);
    let listed = o.backups.iter().map(|b| (&b.path, b.bytes)).chain(o.strays.iter().map(|s| (&s.path, s.bytes))).find(|(p, _)| p.as_str() == path);
    let Some((p, bytes)) = listed else { anyhow::bail!("{path} is not one of the app's copies") };
    std::fs::remove_file(p).with_context(|| format!("deleting {p}"))?;
    tracing::info!(path = %p, bytes, "backup deleted");
    Ok(bytes)
}

/// Which part of the app's data a table belongs to.
fn part_of(table: &str) -> &'static str {
    let t = table.to_ascii_lowercase();
    if t == "league_rawlog" || t == "league_log_json" {
        "league_logs"
    } else if ["league_", "etf2l_", "catalogue", "trends", "snapshot"].iter().any(|p| t.starts_with(p)) {
        "league"
    } else if ["demo", "aim", "life", "death", "path", "timeline", "position", "stv"].iter().any(|p| t.starts_with(p)) {
        "demos"
    } else if ["rating", "baseline", "scale", "log_stat", "log_weapon", "weights"].iter().any(|p| t.starts_with(p)) {
        "ratings"
    } else if ["log_", "rawlog", "match", "kill", "fight", "round", "chat", "spy", "reflect", "cart"].iter().any(|p| t.starts_with(p)) {
        "your_logs"
    } else {
        "other"
    }
}

/// What the database is made of, by part, largest first. Measured from the
/// database itself, so it takes a few seconds on a large one.
pub async fn breakdown(db: &hl_db::Db) -> Result<Vec<Part>> {
    const PARTS: [(&str, &str, bool, bool); 6] = [
        ("league_logs", "League officials' logs: logs.tf JSON and server logs", false, true),
        ("league", "League ratings, seasons, rosters and teams", true, true),
        ("your_logs", "Your logs, server logs and kills", true, true),
        ("ratings", "Ratings, baselines and stats", true, false),
        ("demos", "Demo timelines, aim and positions", true, false),
        ("other", "Settings, bookmarks and the rest", true, false),
    ];
    let mut by: std::collections::HashMap<&str, Vec<(String, i64)>> = std::collections::HashMap::new();
    for (t, b) in db.table_sizes().await? {
        by.entry(part_of(&t)).or_default().push((t, b));
    }
    let mut out: Vec<Part> = PARTS
        .iter()
        .map(|(key, label, in_backups, comes_back)| {
            let tables = by.remove(key).unwrap_or_default();
            Part { key, label, bytes: tables.iter().map(|t| t.1).sum(), in_backups: *in_backups, comes_back: *comes_back, tables }
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.bytes));
    Ok(out)
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

    fn copy(day: i64, hour: i64, gb: f64) -> Backup {
        Backup { path: format!("d{day}h{hour}"), bytes: (gb * 1e9) as u64, made_at: day * 86_400 + hour * 3600 }
    }

    #[test]
    fn the_newest_and_one_a_day_are_kept_within_the_budget() {
        // A tester's folder: a copy before each of four syncs a day, a week.
        let mut all: Vec<Backup> = (0..7).flat_map(|d| (0..4).map(move |h| copy(100 + d, h * 5, 0.6))).collect();
        all.sort_by_key(|b| std::cmp::Reverse(b.made_at));
        let gone: std::collections::HashSet<String> = prune(&all).into_iter().map(|b| b.path).collect();
        let kept: Vec<&Backup> = all.iter().filter(|b| !gone.contains(&b.path)).collect();
        assert_eq!(kept.len(), KEEP, "{kept:?}");
        assert_eq!(kept[0].path, all[0].path, "the newest");
        let days: std::collections::HashSet<i64> = kept.iter().map(|b| b.made_at / 86_400).collect();
        assert_eq!(days.len(), KEEP, "one a day");
        // Whole copies from before, 2.3 GB each: 0.6 + 2.3 fits in 3 GB, a
        // second 2.3 does not.
        let big = vec![copy(110, 0, 0.6), copy(109, 0, 2.3), copy(108, 0, 2.3), copy(107, 0, 2.3)];
        assert_eq!(prune(&big).len(), 2);
        // The newest is kept whatever its size.
        assert!(prune(&[copy(110, 0, 9.0)]).is_empty());
    }

    #[test]
    fn tables_fall_into_their_parts() {
        assert_eq!(part_of("league_rawlog"), "league_logs");
        assert_eq!(part_of("league_log_json"), "league_logs");
        assert_eq!(part_of("league_rating"), "league");
        assert_eq!(part_of("etf2l_season_match"), "league");
        assert_eq!(part_of("demo_timeline"), "demos");
        assert_eq!(part_of("baseline"), "ratings");
        assert_eq!(part_of("rawlog"), "your_logs");
        assert_eq!(part_of("app_config"), "other");
    }

    #[tokio::test]
    async fn a_copy_leaves_the_league_logs_out_and_one_a_day_is_kept() {
        let dir = std::env::temp_dir().join(format!("hl-backup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("hl.sqlite3");
        let db = hl_db::Db::connect(&db_path).await.unwrap();
        // One league log with its JSON.
        db.upsert_league_logs(&[hl_db::LeagueLogRow { log_id: 7, etf2l_match_id: 1, map: "pl_upward", played_at: 1, duration_s: Some(1500), title: None }]).await.unwrap();
        db.put_league_json(7, "{}", "logs.tf").await.unwrap();

        let first = run(&db, &db_path, false).await.unwrap();
        assert!(first.is_some(), "the first run always takes one");
        assert!(run(&db, &db_path, false).await.unwrap().is_none(), "an hour has not passed");

        // Forced copies the same day: one a day is kept, the newest.
        for _ in 0..3 {
            // Stamps are per second, so the names would collide otherwise.
            std::thread::sleep(std::time::Duration::from_millis(1100));
            run(&db, &db_path, true).await.unwrap();
        }
        let kept = list(&db_path);
        assert_eq!(kept.len(), 1, "{kept:?}");
        // The copy opens, without the JSON, and marked so.
        let copy = hl_db::Db::connect(std::path::Path::new(&kept[0].path)).await.unwrap();
        assert!(copy.league_logs_with_json().await.unwrap().is_empty(), "the JSON is left out");
        assert_eq!(copy.get_setting("backup_without_league_logs").await.unwrap().as_deref(), Some("1"));
        // The database itself still has it.
        assert_eq!(db.league_logs_with_json().await.unwrap(), vec![7]);
        drop(copy);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod edge_cases {
    use super::*;

    fn at(secs: i64, bytes: u64) -> Backup {
        Backup { path: format!("t{secs}"), bytes, made_at: secs }
    }

    fn paths(v: Vec<Backup>) -> Vec<String> {
        v.into_iter().map(|b| b.path).collect()
    }

    #[test]
    fn pruning_nothing_or_one_changes_nothing() {
        assert!(prune(&[]).is_empty());
        assert!(prune(&[at(5, 0)]).is_empty());
        // Copies of the very same second: the first listed is the newest.
        assert_eq!(paths(prune(&[at(100, 1), at(100, 1)])), ["t100"]);
    }

    #[test]
    fn a_day_ends_at_midnight_and_before_1970_counts_too() {
        let day = 86_400;
        // 23:59:59 and 00:00:00 are different days, so both are kept.
        assert!(prune(&[at(2 * day, 1), at(2 * day - 1, 1)]).is_empty());
        // The last second of a day and the one before it are one day.
        assert_eq!(paths(prune(&[at(2 * day - 1, 1), at(2 * day - 2, 1)])), ["t172798"]);
        // Dates before 1970 divide the same way: -1 is the day before 0.
        assert!(prune(&[at(0, 1), at(-1, 1)]).is_empty());
        assert_eq!(paths(prune(&[at(-1, 1), at(-86_400, 1)])), ["t-86400"]);
    }

    #[test]
    fn the_budget_is_met_exactly_and_one_byte_over_drops_the_copy() {
        let day = 86_400;
        let half = BUDGET / 2;
        assert!(prune(&[at(10 * day, half), at(9 * day, half)]).is_empty(), "exactly the budget fits");
        assert_eq!(paths(prune(&[at(10 * day, half), at(9 * day, half + 1)])).len(), 1, "a byte over does not");
        // A copy too large for what is left is skipped, and a smaller older one still fits.
        let kept = prune(&[at(10 * day, half), at(9 * day, BUDGET), at(8 * day, 10)]);
        assert_eq!(paths(kept), [format!("t{}", 9 * day)]);
    }

    #[test]
    fn more_than_the_most_are_dropped_oldest_first() {
        let day = 86_400;
        let all: Vec<Backup> = (0..9).map(|d| at((20 - d) * day, 1)).collect();
        let gone = paths(prune(&all));
        assert_eq!(gone.len(), 9 - KEEP);
        assert!(gone.contains(&format!("t{}", 12 * day)), "the oldest is the first to go");
        assert!(!gone.contains(&format!("t{}", 20 * day)), "the newest never");
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-backup-edge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn set_age(path: &Path, secs_ago: i64) {
        let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        let t = if secs_ago >= 0 {
            std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago as u64)
        } else {
            std::time::SystemTime::now() + std::time::Duration::from_secs((-secs_ago) as u64)
        };
        f.set_modified(t).unwrap();
    }

    #[tokio::test]
    async fn a_clock_set_back_does_not_stop_backups_and_a_recent_one_still_does() {
        let dir = temp("clock");
        let db_path = dir.join("hl.sqlite3");
        let db = hl_db::Db::connect(&db_path).await.unwrap();
        std::fs::create_dir_all(super::dir(&db_path)).unwrap();
        let old = super::dir(&db_path).join("hl-20990101-000000.sqlite3");
        std::fs::write(&old, b"x").unwrap();
        // Dated a day ahead: the clock was behind when it was made.
        set_age(&old, -86_400);
        assert!(run(&db, &db_path, false).await.unwrap().is_some(), "not taken for a copy made 'tomorrow'");
        // And now one from a minute ago is recent.
        assert!(run(&db, &db_path, false).await.unwrap().is_none());
        db.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn half_written_copies_are_cleared_only_when_old_and_other_files_are_never_copies() {
        let dir = temp("strays");
        let db_path = dir.join("hl.sqlite3");
        let db = hl_db::Db::connect(&db_path).await.unwrap();
        let folder = super::dir(&db_path);
        std::fs::create_dir_all(&folder).unwrap();
        let old = folder.join("hl-old.part");
        let fresh = folder.join("hl-fresh.part");
        let note = folder.join("notes.txt");
        for f in [&old, &fresh, &note] {
            std::fs::write(f, b"half").unwrap();
        }
        set_age(&old, STRAY_AFTER_S + 60);
        set_age(&note, STRAY_AFTER_S + 60);
        assert!(list(&db_path).is_empty(), "none of these is a copy");
        assert_eq!(strays(&db_path).len(), 3);

        run(&db, &db_path, true).await.unwrap();
        assert!(!old.exists(), "an old half-written copy goes");
        assert!(fresh.exists(), "one that may be being written right now stays");
        // Only `.part` files go: a stray text file is left for the person.
        assert!(note.exists());
        assert_eq!(list(&db_path).len(), 1);
        db.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn save_as_writes_a_whole_copy_never_over_a_file_and_leaves_no_part() {
        let dir = temp("saveas");
        let db_path = dir.join("hl.sqlite3");
        let db = hl_db::Db::connect(&db_path).await.unwrap();
        db.set_setting("name", "Zoë ☃").await.unwrap();
        let to = dir.join("elsewhere ☃").join("my copy.sqlite3");
        let made = save_as(&db, &to).await.unwrap();
        assert!(made.bytes > 0 && to.is_file());
        assert!(!to.with_extension("part").exists());
        let copy = hl_db::Db::connect(&to).await.unwrap();
        assert_eq!(copy.get_setting("name").await.unwrap().as_deref(), Some("Zoë ☃"));
        copy.close().await;

        // A second save to the same name is refused and leaves the first alone.
        let before = std::fs::read(&to).unwrap();
        assert!(save_as(&db, &to).await.is_err());
        assert_eq!(std::fs::read(&to).unwrap(), before);
        // It is not one of the automatic copies, so pruning never sees it.
        assert!(list(&db_path).is_empty());
        db.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn only_the_apps_own_copies_can_be_deleted() {
        let dir = temp("delete");
        let db_path = dir.join("hl.sqlite3");
        let db = hl_db::Db::connect(&db_path).await.unwrap();
        let made = run(&db, &db_path, true).await.unwrap().unwrap();
        let other = dir.join("precious.txt");
        std::fs::write(&other, b"mine").unwrap();
        assert!(delete(&db_path, &other.to_string_lossy()).is_err());
        assert!(delete(&db_path, &db_path.to_string_lossy()).is_err(), "never the database itself");
        assert!(delete(&db_path, &format!("{}/../precious.txt", super::dir(&db_path).display())).is_err());
        assert!(other.exists() && db_path.exists());
        assert!(delete(&db_path, &made.path).unwrap() > 0);
        assert!(list(&db_path).is_empty());
        assert!(delete(&db_path, &made.path).is_err(), "a second delete finds nothing");
        db.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_database_set_aside_is_listed_as_a_stray_and_never_pruned() {
        let dir = temp("aside");
        let db_path = dir.join("hl.sqlite3");
        std::fs::write(dir.join("hl.unreadable-20260101-000000.sqlite3"), b"broken").unwrap();
        std::fs::write(dir.join("hl.sqlite3.replaced"), b"old").unwrap();
        std::fs::write(dir.join("unrelated.sqlite3"), b"not ours").unwrap();
        let kinds: Vec<&str> = strays(&db_path).iter().map(|s| s.kind).collect();
        assert_eq!(kinds.len(), 2, "{kinds:?}");
        assert!(kinds.contains(&"unreadable") && kinds.contains(&"replaced"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
