//! ETF2L's own demos (Q48, Flashy; PLAN §27): the SourceTV demos players
//! upload to a match page on etf2l.org, for the matches demos.tf has none of.
//!
//! A match's page (`/matches/{id}`, already stored for every official in
//! `etf2l_raw`) lists them. Each is a zip of one or more `.dem` files, often
//! one per map, uploaded by hand any time after the match -- so its upload
//! time says nothing about when the match was played. Each demo is instead
//! checked against the match's logs the way a dropped demo is (map, shared
//! players, kills that line up at one shift), which also places it on the
//! log's clock exactly.
//!
//! First-person demos are left alone (one player's view), as are RAR
//! archives, which the app cannot open.

use crate::demos::StvFetched;
use crate::sources::Sources;
use anyhow::{bail, Context, Result};
use hl_db::Db;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The link method stored for a demo found this way.
pub const METHOD: &str = "etf2l";

/// A match page read more than this long ago is read again before giving
/// up on it: demos are uploaded after the match, sometimes days after.
const STALE: i64 = 24 * 3600;

#[derive(Debug, Clone, PartialEq)]
pub struct Etf2lDemo {
    pub url: String,
    pub stv: bool,
    pub pruned: bool,
    pub time: i64,
}

/// The demos an ETF2L match page lists.
pub fn demos_in(match_json: &str) -> Vec<Etf2lDemo> {
    let Ok(v) = serde_json::from_str::<Value>(match_json) else { return Vec::new() };
    let m = v.get("match").unwrap_or(&v);
    m["demos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            Some(Etf2lDemo {
                url: d["download_url"].as_str()?.to_string(),
                stv: d["stv"].as_bool().unwrap_or(false),
                pruned: d["pruned"].as_bool().unwrap_or(false),
                time: d["time"].as_i64().unwrap_or(0),
            })
        })
        .collect()
}

/// The ones worth fetching: SourceTV, still on ETF2L's server.
fn usable(demos: &[Etf2lDemo]) -> Vec<&Etf2lDemo> {
    demos.iter().filter(|d| d.stv && !d.pruned).collect()
}

/// The ETF2L match a log belongs to, from trends.tf's index or the roster
/// match.
pub async fn match_of(db: &Db, log_id: i64) -> Result<Option<i64>> {
    if let Some(id) = db.index_info(log_id).await?.and_then(|i| i.etf2l_match_id) {
        return Ok(Some(id));
    }
    db.context_match_id(log_id).await
}

/// How many SourceTV demos ETF2L lists for a log's match, from what is
/// stored (no request): what the match page offers a download for.
pub async fn listed(db: &Db, log_id: i64) -> Result<usize> {
    let Some(m) = match_of(db, log_id).await? else { return Ok(0) };
    Ok(db.etf2l_raw_one("match", m).await?.map_or(0, |(_, json)| usable(&demos_in(&json)).len()))
}

/// What kind of file a download is, by its first bytes.
#[derive(Debug, PartialEq)]
enum Kind {
    Zip,
    Dem,
    Rar,
    Other,
}

fn kind_of(head: &[u8]) -> Kind {
    if head.starts_with(b"PK\x03\x04") {
        Kind::Zip
    } else if head.starts_with(b"HL2DEMO") {
        Kind::Dem
    } else if head.starts_with(b"Rar!") {
        Kind::Rar
    } else {
        Kind::Other
    }
}

/// The most a demo archive may unpack to, in all, and the most files it may hold.
const UNZIP_MAX: u64 = 1200 << 20;
const UNZIP_ENTRIES_MAX: usize = 100;

/// A file name safe on any disk.
fn safe(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect()
}

/// The `.dem` files in a download, written beside it as
/// `etf2l-{match}-{name}.dem`; the download itself is removed.
fn unpack(download: &Path, dir: &Path, prefix: &str) -> Result<Vec<PathBuf>> {
    let mut head = [0u8; 8];
    {
        use std::io::Read;
        let n = std::fs::File::open(download)?.read(&mut head)?;
        if n < 4 {
            bail!("the download is empty");
        }
    }
    let out = match kind_of(&head) {
        Kind::Dem => {
            let to = dir.join(format!("{prefix}.dem"));
            hl_demos::scan::mark_own_write(&to);
            std::fs::rename(download, &to)?;
            return Ok(vec![to]);
        }
        Kind::Zip => {
            let mut zip = zip::ZipArchive::new(std::fs::File::open(download)?).context("opening the zip")?;
            // A player's upload is not ours to trust: a few entries, and no
            // more bytes out than a demo archive can be, however small the zip.
            if zip.len() > UNZIP_ENTRIES_MAX {
                bail!("the zip has {} files; a demo archive has a few", zip.len());
            }
            let mut out = Vec::new();
            let mut left = UNZIP_MAX;
            for i in 0..zip.len() {
                let entry = zip.by_index(i)?;
                let name = entry.name().rsplit(['/', '\\']).next().unwrap_or("").to_string();
                if !name.to_ascii_lowercase().ends_with(".dem") {
                    continue;
                }
                let to = dir.join(format!("{prefix}-{}", safe(&name)));
                hl_demos::scan::mark_own_write(&to);
                let mut f = std::fs::File::create(&to)?;
                let written = std::io::copy(&mut std::io::Read::take(entry, left + 1), &mut f);
                drop(f);
                match written {
                    Ok(n) if n <= left => {
                        left -= n;
                        out.push(to);
                    }
                    other => {
                        // Over the cap, or broken: none of it is kept.
                        let _ = std::fs::remove_file(&to);
                        for done in &out {
                            let _ = std::fs::remove_file(done);
                        }
                        let _ = std::fs::remove_file(download);
                        other?;
                        bail!("the zip unpacks to more than {} MB", UNZIP_MAX >> 20);
                    }
                }
            }
            out
        }
        Kind::Rar => {
            let _ = std::fs::remove_file(download);
            bail!("the demo is a RAR archive, which the app cannot open")
        }
        Kind::Other => {
            let _ = std::fs::remove_file(download);
            bail!("the download is not a demo")
        }
    };
    let _ = std::fs::remove_file(download);
    Ok(out)
}

/// Fetch ETF2L's SourceTV demos for a log's match and link each to the log
/// of the match it is from. Returns what was linked to `log_id`.
pub async fn fetch(db: &Db, sources: &Sources, tf: &Path, log_id: i64, mut progress: impl FnMut(u64, Option<u64>)) -> Result<StvFetched> {
    let m = match_of(db, log_id).await?.context("this match is not linked to an ETF2L official")?;
    let stored = db.etf2l_raw_one("match", m).await?;
    let fresh = stored.as_ref().is_some_and(|(at, json)| !usable(&demos_in(json)).is_empty() || now() - at < STALE);
    let json = if fresh {
        stored.map(|(_, j)| j).unwrap_or_default()
    } else {
        // Uploaded since the page was read, perhaps: read it again.
        let body = sources.etf2l_get(&format!("/matches/{m}")).await?.context("ETF2L has no page for this match")?;
        if body.trim_start().starts_with('{') {
            db.store_etf2l_raw("match", m, &body).await?;
        }
        body
    };
    let demos = demos_in(&json);
    let wanted = usable(&demos);
    if wanted.is_empty() {
        bail!("ETF2L has no SourceTV demo for this match");
    }

    // The match's logs, this one first: a demo of another map links to its own.
    let mut logs = vec![log_id];
    for l in db.logs_of_etf2l_match(m).await? {
        if !logs.contains(&l) {
            logs.push(l);
        }
    }
    // Each log's kills are what a demo is lined up against (Q54).
    for &l in &logs {
        if let Err(e) = crate::kills::ensure_one(db, sources, l).await {
            tracing::warn!(log_id = l, error = %format!("{e:#}"), "raw log not fetched before linking a demo");
        }
    }
    let dir = tf.join(hl_demos::scan::STV_DIR);
    std::fs::create_dir_all(&dir)?;
    let mut bytes = 0;
    let mut mine: Option<crate::demo_import::DemoLinked> = None;
    let mut linked: Vec<i64> = Vec::new();
    let mut last_error: Option<String> = None;
    for (i, d) in wanted.iter().enumerate() {
        let download = dir.join(format!("etf2l-{m}-{i}.download"));
        bytes += sources.download_etf2l(&d.url, &download, &mut progress).await?;
        let dems = match unpack(&download, &dir, &format!("etf2l-{m}-{i}")) {
            Ok(d) => d,
            Err(e) => {
                last_error = Some(format!("{e:#}"));
                continue;
            }
        };
        for dem in dems {
            let mut taken = false;
            for &l in logs.iter().filter(|l| !linked.contains(l)) {
                match crate::demo_import::link_to_log_by(db, tf, &dem, l, None, METHOD, |_| {}).await {
                    Ok(done) => {
                        linked.push(l);
                        if l == log_id {
                            mine = Some(done);
                        }
                        taken = true;
                        break;
                    }
                    Err(e) if e.downcast_ref::<crate::demo_import::WrongDemo>().is_some() => continue,
                    Err(e) => {
                        tracing::warn!(log_id = l, demo = %dem.display(), error = %format!("{e:#}"), "an ETF2L demo could not be linked");
                        last_error = Some(format!("{e:#}"));
                    }
                }
            }
            // A demo of no stored log is not kept: it is tens of megabytes.
            if !taken {
                let _ = std::fs::remove_file(&dem);
            }
        }
    }
    let done = mine.with_context(|| match last_error {
        Some(e) => format!("none of ETF2L's demos for this match could be linked to this log: {e}"),
        None => "ETF2L's demos for this match are of its other maps, not this log's".to_string(),
    })?;
    Ok(StvFetched {
        log_id,
        demo_id: done.demo_id,
        file_name: done.file_name,
        bytes,
        log_share: ((done.kills_matched as f64 / done.log_kills.max(1) as f64).min(1.0) * 100.0).round() / 100.0,
    })
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_page_lists_its_demos_and_only_live_stv_ones_are_used() {
        let page = r#"{"match":{"id":87301,"demos":[
            {"time":1697314079,"download_url":"https://etf2l.org/demos/15551","stv":false,"first_person":true,"pruned":false},
            {"time":1697314239,"download_url":"https://etf2l.org/demos/15552","stv":true,"first_person":false,"pruned":false},
            {"time":1697314300,"download_url":"https://etf2l.org/demos/15553","stv":true,"first_person":false,"pruned":true}]}}"#;
        let d = demos_in(page);
        assert_eq!(d.len(), 3);
        let u = usable(&d);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].url, "https://etf2l.org/demos/15552");
        assert!(demos_in(r#"{"match":{"demos":[]}}"#).is_empty());
        assert!(demos_in("not json").is_empty());
    }

    #[test]
    fn a_download_is_told_apart_by_its_first_bytes() {
        assert_eq!(kind_of(b"PK\x03\x04rest"), Kind::Zip);
        assert_eq!(kind_of(b"HL2DEMO\0"), Kind::Dem);
        assert_eq!(kind_of(b"Rar!\x1a\x07"), Kind::Rar);
        assert_eq!(kind_of(b"<html>"), Kind::Other);
    }

    #[test]
    fn a_zip_gives_up_its_demos_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("hl-etf2l-demos-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("x.download");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            z.start_file("folder/match_upward.dem", opts).unwrap();
            std::io::Write::write_all(&mut z, b"HL2DEMO\0demo").unwrap();
            z.start_file("readme.txt", opts).unwrap();
            std::io::Write::write_all(&mut z, b"hi").unwrap();
            z.finish().unwrap();
        }
        let out = unpack(&zip_path, &dir, "etf2l-1-0").unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].file_name().unwrap().to_string_lossy(), "etf2l-1-0-match_upward.dem");
        assert!(!zip_path.exists(), "the download is removed");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
