//! The app's log file, `logs/flashwave.log` in the data folder.
//!
//! A release build has no console (`windows_subsystem = "windows"`), and
//! every `tracing::warn!` the backend writes went to that console: a tester
//! whose raw logs, demos.tf links or ratings failed had nothing to send, and
//! the Problems page only ever saw what the window itself caught (Flashy:
//! "if there should be ... at least some debug options").
//!
//! So the same lines go to a file as well. Each start moves the last run's
//! file to `.1` (and `.1` to `.2`), so the run that went wrong is still there
//! after a restart, and one run never writes more than [`CAP`]: a loop of
//! warnings must not fill the disk.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Most one run writes. A normal day of syncing is well under a megabyte.
const CAP: u64 = 20 * 1024 * 1024;
/// This run's file and the two before it.
const KEEP: usize = 3;
pub const FILE_NAME: &str = "flashwave.log";

struct Sink {
    file: File,
    written: u64,
    capped: bool,
}

static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();
static PATH: OnceLock<PathBuf> = OnceLock::new();

/// The folder the log files are in.
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

/// Start writing to the file. Lines logged before this (the first few of a
/// start) are on the console only. Failing is not an error worth stopping
/// the app for: the console still has everything.
pub fn open(data_dir: &Path) {
    let dir = dir(data_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(FILE_NAME);
    rotate(&dir);
    let Ok(file) = OpenOptions::new().create(true).write(true).truncate(true).open(&path) else { return };
    let _ = SINK.set(Mutex::new(Sink { file, written: 0, capped: false }));
    let _ = PATH.set(path);
}

/// `flashwave.log` -> `.1.log` -> `.2.log`, the oldest dropped.
fn rotate(dir: &Path) {
    let name = |i: usize| if i == 0 { dir.join(FILE_NAME) } else { dir.join(format!("flashwave.{i}.log")) };
    let _ = std::fs::remove_file(name(KEEP - 1));
    for i in (0..KEEP - 1).rev() {
        let _ = std::fs::rename(name(i), name(i + 1));
    }
}

/// This run's log file, once [`open`] has run.
pub fn path() -> Option<&'static Path> {
    PATH.get().map(PathBuf::as_path)
}

/// The last `lines` lines of this run's file, for the Problems page's report.
/// Reads at most the last 256 KB, so a long run costs nothing to copy from.
pub fn recent(lines: usize) -> String {
    let Some(path) = path() else { return String::new() };
    let Ok(mut f) = File::open(path) else { return String::new() };
    let len = f.metadata().map_or(0, |m| m.len());
    let from = len.saturating_sub(256 * 1024);
    if f.seek(SeekFrom::Start(from)).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    tail(&String::from_utf8_lossy(&buf), lines, from > 0)
}

/// The last `n` lines of `text`; the first is dropped when `text` starts
/// part-way through a line.
fn tail(text: &str, n: usize, cut: bool) -> String {
    let mut all: Vec<&str> = text.lines().collect();
    if cut && !all.is_empty() {
        all.remove(0);
    }
    all[all.len().saturating_sub(n)..].join("\n")
}

/// The writer `tracing_subscriber` is given: a no-op until [`open`] has run.
pub struct FileWriter;

impl Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let Some(sink) = SINK.get() else { return Ok(buf.len()) };
        let mut s = sink.lock().unwrap_or_else(|p| p.into_inner());
        if s.capped {
            return Ok(buf.len());
        }
        if s.written + buf.len() as u64 > CAP {
            s.capped = true;
            let _ = s.file.write_all(b"\n-- the log reached 20 MB; nothing more is written this run --\n");
            return Ok(buf.len());
        }
        s.written += buf.len() as u64;
        s.file.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match SINK.get() {
            Some(sink) => sink.lock().unwrap_or_else(|p| p.into_inner()).file.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_the_last_lines() {
        assert_eq!(tail("a\nb\nc\n", 2, false), "b\nc");
        assert_eq!(tail("a\nb", 5, false), "a\nb");
        assert_eq!(tail("", 5, false), "");
    }

    #[test]
    fn a_cut_first_line_is_dropped() {
        assert_eq!(tail("half line\nwhole\nlast", 10, true), "whole\nlast");
    }

    #[test]
    fn rotation_keeps_the_last_three_runs() {
        let dir = std::env::temp_dir().join(format!("hl-applog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for run in 0..5 {
            rotate(&dir);
            std::fs::write(dir.join(FILE_NAME), format!("run {run}")).unwrap();
        }
        let read = |n: &str| std::fs::read_to_string(dir.join(n)).unwrap();
        assert_eq!(read(FILE_NAME), "run 4");
        assert_eq!(read("flashwave.1.log"), "run 3");
        assert_eq!(read("flashwave.2.log"), "run 2");
        assert!(!dir.join("flashwave.3.log").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
