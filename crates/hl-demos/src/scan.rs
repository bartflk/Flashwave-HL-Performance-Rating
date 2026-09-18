//! Finding and reading the demos in a `tf` directory.

use crate::header::{DemoHeader, HEADER_LEN};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

/// One demo file on disk, with everything needed to link and jump into it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoFile {
    pub path: PathBuf,
    /// What to pass to `playdemo`: relative to `tf`, forward slashes, no extension.
    pub playdemo_arg: String,
    pub file_name: String,
    pub size_bytes: u64,
    /// Last-modified time, unix seconds: when recording stopped.
    pub mtime: i64,
    pub header: DemoHeader,
    /// When recording started, unix seconds UTC: mtime minus the demo's own
    /// duration. Needs no timezone: checked against the Demo Support filename
    /// timestamp on 95 real demos, median disagreement 0.9 s.
    pub start_utc: Option<f64>,
    /// The Demo Support filename timestamp, if present, as local wall time.
    pub filename_time: Option<String>,
    /// Tick-stamped markers from the Demo Support `.json` sidecar.
    pub events: Vec<SidecarEvent>,
    /// `pov` for your own recordings, `stv` for SourceTV demos.
    pub kind: &'static str,
}

/// Where STV demos fetched from demos.tf are saved, relative to `tf`.
pub const STV_DIR: &str = "demos/stv";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarEvent {
    pub name: String,
    #[serde(default)]
    pub value: Option<String>,
    pub tick: i64,
}

#[derive(Deserialize)]
struct Sidecar {
    #[serde(default)]
    events: Vec<SidecarEvent>,
}

/// Parse a Demo Support sidecar. Unreadable sidecars yield no events rather
/// than failing the demo: the markers are a bonus, the demo is the point.
pub fn read_sidecar(dem_path: &Path) -> Vec<SidecarEvent> {
    let json = dem_path.with_extension("json");
    let Ok(text) = std::fs::read_to_string(&json) else {
        return Vec::new();
    };
    match serde_json::from_str::<Sidecar>(&text) {
        Ok(s) => s.events,
        Err(e) => {
            tracing::debug!(path = %json.display(), error = %e, "ignoring unreadable sidecar");
            Vec::new()
        }
    }
}

/// Every `.dem` directly in `tf` and in `tf/demos`. Unreadable files are
/// reported and skipped.
pub fn scan(tf: &Path) -> (Vec<DemoFile>, Vec<(PathBuf, String)>) {
    let mut found = Vec::new();
    let mut failed = Vec::new();
    for dir in [tf.to_path_buf(), tf.join("demos"), tf.join(STV_DIR)] {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("dem")) {
                continue;
            }
            match read_demo(tf, &path) {
                Ok(d) => found.push(d),
                Err(e) => failed.push((path, format!("{e:#}"))),
            }
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    (found, failed)
}

pub fn read_demo(tf: &Path, path: &Path) -> Result<DemoFile> {
    let mut buf = [0u8; HEADER_LEN];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut buf))
        .with_context(|| format!("reading header of {}", path.display()))?;
    let header = DemoHeader::parse(&buf).with_context(|| path.display().to_string())?;

    let meta = std::fs::metadata(path)?;
    let mtime = meta
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // SourceTV names itself in the recorder slot; downloads also sit in STV_DIR.
    let in_stv_dir = path.parent().is_some_and(|p| p.ends_with("stv"));
    let kind = if in_stv_dir || header.recorder == "SourceTV Demo" { "stv" } else { "pov" };
    // mtime is when recording stopped only for a demo recorded here. A
    // downloaded STV demo's mtime is the download time, so it gets no start;
    // the download step records one from demos.tf instead.
    let start_utc =
        (kind == "pov" && header.playback_s > 0.0).then_some(mtime as f64 - header.playback_s as f64);

    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(DemoFile {
        playdemo_arg: playdemo_arg(tf, path),
        filename_time: filename_time(&file_name),
        size_bytes: meta.len(),
        mtime,
        start_utc,
        events: read_sidecar(path),
        kind,
        header,
        file_name,
        path: path.to_path_buf(),
    })
}

/// `…/tf/demos/x.dem` -> `demos/x`. TF2 resolves `playdemo` relative to `tf`.
pub fn playdemo_arg(tf: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(tf).unwrap_or(path);
    let s = rel.with_extension("").to_string_lossy().replace('\\', "/");
    s.trim_start_matches('/').to_string()
}

/// Demo Support names files `<prefix>YYYY-MM-DD_HH-MM-SS.dem`.
pub fn filename_time(name: &str) -> Option<String> {
    let b = name.as_bytes();
    (0..b.len().saturating_sub(18)).find_map(|i| {
        let s = name.get(i..i + 19)?;
        let ok = s.char_indices().all(|(j, c)| match j {
            4 | 7 => c == '-',
            10 => c == '_',
            13 | 16 => c == '-',
            _ => c.is_ascii_digit(),
        });
        ok.then(|| {
            format!("{} {}:{}:{}", &s[..10], &s[11..13], &s[14..16], &s[17..19])
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playdemo_arg_is_relative_to_tf_without_extension() {
        let tf = Path::new("D:/tf");
        assert_eq!(playdemo_arg(tf, Path::new("D:/tf/demos/abc.dem")), "demos/abc");
        assert_eq!(playdemo_arg(tf, Path::new("D:/tf/abc.dem")), "abc");
    }

    #[test]
    fn reads_the_demo_support_timestamp() {
        assert_eq!(
            filename_time("flashwav2026-09-17_21-00-24.dem").as_deref(),
            Some("2026-09-17 21:00:24")
        );
        assert_eq!(filename_time("peterreview.dem"), None);
    }
}
