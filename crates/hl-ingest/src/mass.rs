//! Mass import (Q69, Emiel): one player's demos on one map from demos.tf,
//! downloaded, checked against their matches and read, so Lives on a map
//! has far more than the matches this machine happened to record.
//!
//! Two steps, each its own call: [`search`] lists what demos.tf has (newest
//! first, the class they played where a class was asked for), and [`run`]
//! imports the ones picked. A demo's match is found in the database first
//! (a log played within minutes of the demo's upload, on the same map);
//! failing that, in one logs.tf search for the player, and then imported.
//! Each demo is checked against its log before anything is kept, as a demo
//! dropped on a match page is.

use crate::sources::{DemosTfMeta, Sources};
use anyhow::Result;
use hl_core::maps::map_base;
use hl_core::{SteamId, TfClass};
use hl_db::Db;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// demos.tf pages to walk at most: 50 demos each, so 2,000 demos back.
const MAX_PAGES: usize = 40;
/// Demos to ask the class of at most, when a class was asked for: each is
/// one request at demos.tf's pace of one a second.
const MAX_CLASS_CHECKS: usize = 150;
/// How far apart a demo's upload and its log's may be and be the same match.
/// Both are stamped on upload, within minutes of the game ending.
const NEAR_S: i64 = 20 * 60;

/// One demo demos.tf has for the player on the map.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub demos_tf_id: i64,
    /// Upload time, unix seconds.
    pub time: i64,
    pub duration_s: Option<i64>,
    pub map: String,
    pub name: String,
    /// The class they played (`spy`), when a class was asked for and
    /// demos.tf names it; `None` otherwise.
    pub class: Option<String>,
    /// `Red` or `Blue`, likewise.
    pub team: Option<String>,
    /// Already linked on this machine: nothing to download.
    pub here: bool,
    /// Its match is already in the database: no log to fetch.
    pub local_log: Option<i64>,
}

/// What [`search`] is doing, for the window's progress line.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum SearchStep {
    /// Walking demos.tf's list: `page` pages read, `found` on the map so far.
    Listing { page: usize, found: usize },
    /// Asking demos.tf which class they played: `checked` of at most `of`,
    /// and whether this one was the class asked for.
    Classes { checked: usize, of: usize, hit: bool },
}

fn steamid64(account_id: u32) -> String {
    (u64::from(account_id) + 76_561_197_960_265_728).to_string()
}

/// demos.tf's class names to the app's (`heavyweapons` is `heavy`).
fn class_of(s: &str) -> Option<String> {
    TfClass::parse(s).ok().map(|c| c.as_str().to_string())
}

/// The log in the database a demo belongs to: on the same map, uploaded
/// nearest in time within [`NEAR_S`].
async fn local_log_for(db: &Db, map: &str, time: i64) -> Result<Option<i64>> {
    let want = map_base(map);
    let near = db.logs_near(time, NEAR_S).await?;
    let mut best: Option<(i64, i64)> = None; // (gap, log)
    for (log_id, played_at, log_map) in near {
        let on_map = log_map.as_deref().is_some_and(|m| map_base(m) == want || m.to_ascii_lowercase().contains(&want));
        if !on_map {
            continue;
        }
        let gap = (played_at - time).abs();
        if best.is_none_or(|(g, _)| gap < g) {
            best = Some((gap, log_id));
        }
    }
    Ok(best.map(|(_, id)| id))
}

/// What demos.tf has for `account_id` on `map`, newest first, up to `want`
/// (of `class` when one is given).
pub async fn search(
    db: &Db,
    sources: &Sources,
    account_id: u32,
    map: &str,
    class: Option<&str>,
    want: usize,
    mut progress: impl FnMut(SearchStep),
) -> Result<Vec<Candidate>> {
    let id64 = steamid64(account_id);
    let want_base = map_base(map);
    let want_class = class.and_then(class_of);

    // Page by page, newest first, each page's demos on the map checked as it
    // comes, so a class search stops as soon as it has enough.
    let mut out = Vec::new();
    let mut before = None;
    let mut seen_on_map = 0;
    let mut checked = 0;
    'pages: for page in 1..=MAX_PAGES {
        let rows = sources.demostf_for_player(&id64, before).await?;
        let Some(earliest) = rows.iter().filter_map(|d| d.time).min() else { break };
        before = Some(earliest);
        // Gone from demos.tf (before 14 October 2019): no download, so not offered (Q62).
        let done_listing = earliest < crate::demostf::OLDEST_AVAILABLE;
        let mut on_map: Vec<DemosTfMeta> = rows
            .into_iter()
            .filter(|d| d.available() && d.map.as_deref().is_some_and(|m| map_base(m) == want_base))
            .collect();
        on_map.sort_by_key(|d| std::cmp::Reverse(d.time.unwrap_or(0)));
        seen_on_map += on_map.len();
        progress(SearchStep::Listing { page, found: seen_on_map });
        for d in on_map {
            if out.len() >= want {
                break 'pages;
            }
            if let Some(c) = search_one(db, sources, &id64, d, want_class.as_deref(), &mut checked, &mut progress).await? {
                out.push(c);
            }
            if want_class.is_some() && checked >= MAX_CLASS_CHECKS {
                break 'pages;
            }
        }
        if out.len() >= want || done_listing {
            break;
        }
    }
    Ok(out)
}

/// One listed demo: asked its class when a class was wanted (`None` when it
/// is not that class), then looked up here.
async fn search_one(
    db: &Db,
    sources: &Sources,
    id64: &str,
    d: DemosTfMeta,
    want_class: Option<&str>,
    checked: &mut usize,
    progress: &mut impl FnMut(SearchStep),
) -> Result<Option<Candidate>> {
    {
        let (mut class, mut team) = (None, None);
        if let Some(wanted) = want_class {
            *checked += 1;
            match sources.demostf_players(d.id).await {
                Ok(players) => {
                    if let Some((_, c, t)) = players.iter().find(|(p, ..)| *p == id64) {
                        class = class_of(c);
                        team = match t.to_ascii_lowercase().as_str() {
                            "red" => Some("Red".to_string()),
                            "blue" | "blu" => Some("Blue".to_string()),
                            _ => None,
                        };
                    }
                }
                Err(e) => tracing::warn!(demo = d.id, error = %format!("{e:#}"), "demos.tf player list failed"),
            }
            let hit = class.as_deref() == Some(wanted);
            progress(SearchStep::Classes { checked: *checked, of: MAX_CLASS_CHECKS, hit });
            if !hit {
                return Ok(None);
            }
        }
        let time = d.time.unwrap_or(0);
        let map_name = d.map.clone().unwrap_or_default();
        let here = db.demo_by_demostf_id(d.id).await?.is_some_and(|(_, on_disk)| on_disk);
        let local_log = local_log_for(db, &map_name, time).await?;
        Ok(Some(Candidate { demos_tf_id: d.id, time, duration_s: d.duration, map: map_name, name: d.name, class, team, here, local_log }))
    }
}

/// What [`run`] is doing, for the progress card.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStep {
    pub batch_id: i64,
    /// Demos finished (done, failed or skipped) and in the run.
    pub done: usize,
    pub total: usize,
    pub demos_tf_id: Option<i64>,
    /// `log`, `download`, `read`.
    pub stage: &'static str,
    pub bytes: u64,
    pub of_bytes: Option<u64>,
}

/// One picked demo, as [`run`] needs it.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    pub demos_tf_id: i64,
    pub time: i64,
    pub map: String,
}

/// What a run did.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub batch_id: i64,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    /// Bytes of demo files the run left on disk.
    pub bytes_on_disk: i64,
}

/// Import the picked demos of `account_id`, one at a time. Stops between
/// demos when `cancel` is set; the rest are marked skipped.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    db: &Db,
    sources: &Sources,
    w: &hl_rating::Weights,
    tf: &Path,
    me: Option<SteamId>,
    account_id: u32,
    batch_id: i64,
    picks: &[Pick],
    cancel: &AtomicBool,
    mut progress: impl FnMut(RunStep),
) -> Result<RunSummary> {
    let total = picks.len();
    let mut sum = RunSummary { batch_id, ..Default::default() };
    // The player's logs on logs.tf, asked once, for matches not stored here.
    let mut remote: Option<Vec<(i64, String, String, i64, i64)>> = None;

    for p in picks {
        let finished = sum.done + sum.failed + sum.skipped;
        if cancel.load(Ordering::Relaxed) {
            db.set_mass_demo(batch_id, p.demos_tf_id, "skipped", None, None, Some("cancelled")).await?;
            sum.skipped += 1;
            continue;
        }
        progress(RunStep { batch_id, done: finished, total, demos_tf_id: Some(p.demos_tf_id), stage: "log", bytes: 0, of_bytes: None });

        // Already here and linked: nothing to do.
        if let Some((demo_id, _)) = db.demo_by_demostf_id(p.demos_tf_id).await? {
            if let Some((log_id, _)) = db.links_of_demo(demo_id).await?.into_iter().next() {
                db.set_mass_demo(batch_id, p.demos_tf_id, "done", Some(demo_id), Some(log_id), Some("already on this machine")).await?;
                sum.done += 1;
                continue;
            }
        }

        // Its match: in the database, else on logs.tf.
        let mut log_id = local_log_for(db, &p.map, p.time).await?;
        if log_id.is_none() {
            if remote.is_none() {
                remote = Some(sources.logstf_with_players(&[steamid64(account_id)], 10_000).await.unwrap_or_else(|e| {
                    tracing::warn!(error = %format!("{e:#}"), "logs.tf search for a mass import failed");
                    Vec::new()
                }));
            }
            let want = map_base(&p.map);
            let found = remote
                .iter()
                .flatten()
                .filter(|(_, title, map, date, _)| {
                    (date - p.time).abs() <= NEAR_S && (map_base(map) == want || map.to_ascii_lowercase().contains(&want) || title.to_ascii_lowercase().contains(&want))
                })
                .min_by_key(|(_, _, _, date, _)| (date - p.time).abs())
                .map(|(id, ..)| *id);
            if let Some(id) = found {
                match crate::sync::import_log(db, sources, w, id).await {
                    // A part of a combined log: the combined one holds the match.
                    Ok(imported) => log_id = Some(imported.part_of.unwrap_or(id)),
                    Err(e) => tracing::warn!(log_id = id, error = %format!("{e:#}"), "importing a mass import's log failed"),
                }
            }
        }
        let Some(log_id) = log_id else {
            db.set_mass_demo(batch_id, p.demos_tf_id, "failed", None, None, Some("no log found for this demo")).await?;
            sum.failed += 1;
            continue;
        };

        // Download, check against the log, read.
        let mut last = u64::MAX;
        let result = crate::demo_import::link_demostf(db, sources, tf, log_id, p.demos_tf_id, me, |bytes, of| {
            let mb = bytes / 2_000_000;
            if mb != last {
                last = mb;
                progress(RunStep { batch_id, done: finished, total, demos_tf_id: Some(p.demos_tf_id), stage: "download", bytes, of_bytes: of });
            }
        })
        .await;
        match result {
            Ok(linked) => {
                db.set_mass_demo(batch_id, p.demos_tf_id, "done", Some(linked.demo_id), Some(log_id), None).await?;
                sum.done += 1;
            }
            Err(e) => {
                let why = match e.downcast_ref::<crate::demo_import::WrongDemo>() {
                    Some(w) => w.to_string(),
                    None => format!("{e:#}"),
                };
                db.set_mass_demo(batch_id, p.demos_tf_id, "failed", None, Some(log_id), Some(&why)).await?;
                sum.failed += 1;
            }
        }
    }
    db.finish_mass_batch(batch_id).await?;
    sum.bytes_on_disk = db
        .mass_batches()
        .await?
        .into_iter()
        .find(|b| b.batch_id == batch_id)
        .map(|b| b.demos.iter().filter_map(|d| d.size_bytes).sum())
        .unwrap_or(0);
    progress(RunStep { batch_id, done: total, total, demos_tf_id: None, stage: "read", bytes: 0, of_bytes: None });
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_steamid64_and_demostf_class_names_read_right() {
        assert_eq!(steamid64(103548491), "76561198063814219");
        assert_eq!(class_of("heavyweapons").as_deref(), Some("heavy"));
        assert_eq!(class_of("spy").as_deref(), Some("spy"));
        assert_eq!(class_of("civilian"), None);
    }
}
