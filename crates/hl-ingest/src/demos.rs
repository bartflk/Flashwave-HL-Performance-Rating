//! Indexing demos, linking them to logs, and placing jumps on the match page.
//!
//! ```text
//! scan tf/, tf/demos, tf/demos/stv  ->  upsert demo rows, prune vanished files
//! every kept log                    ->  log_clock: its rounds on the real clock
//! demos x logs                      ->  demo_link
//! ```
//!
//! No network, no packet parsing: a full rescan of ~100 demos is a
//! header read per file.

use anyhow::Result;
use hl_core::matchdata::NormalizedLog;
use hl_db::{ClockRow, Db, DemoRow, LinkedDemo};
use hl_demos::{link, tick_for, DemoSpan, LogClock};
use hl_rating::{DemoView, EventRow, Jump, MatchDetail};
use serde::Serialize;
use std::path::Path;

/// Seconds of lead-up before an event when jumping to it.
pub const JUMP_LEAD_S: f64 = 5.0;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoIndexSummary {
    pub scanned: usize,
    pub unreadable: usize,
    pub removed: u64,
    pub logs_placed: usize,
    pub links: usize,
    pub demos_linked: i64,
    pub matches_with_demo: i64,
    pub markers: i64,
}

/// Where the other demo folders are kept: a JSON list of paths.
const EXTRA_DIRS: &str = "extra_demo_dirs";

/// Your other demo folders, as chosen in Settings.
pub async fn extra_demo_dirs(db: &Db) -> Result<Vec<String>> {
    Ok(db.get_setting(EXTRA_DIRS).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

pub async fn set_extra_demo_dirs(db: &Db, dirs: &[String]) -> Result<()> {
    db.set_setting(EXTRA_DIRS, &serde_json::to_string(dirs)?).await
}

pub async fn index_demos(db: &Db, tf: &Path) -> Result<DemoIndexSummary> {
    let (mut files, mut failed) = hl_demos::scan(tf);
    // And your other folders (Flashy): an archive, demoreviews. A file seen
    // twice, a folder inside tf among them, is one demo.
    // A folder that is not there now -- a drive unplugged -- keeps what was
    // found in it last time, rather than having it all pruned.
    let mut offline: Vec<String> = Vec::new();
    for dir in extra_demo_dirs(db).await? {
        if !Path::new(&dir).is_dir() {
            tracing::info!(%dir, "demo folder not reachable; keeping what was found in it");
            offline.push(dir);
            continue;
        }
        let (more, bad) = hl_demos::scan::scan_folder(tf, Path::new(&dir));
        for f in more {
            if !files.iter().any(|x| x.path == f.path) {
                files.push(f);
            }
        }
        failed.extend(bad);
    }
    for (path, error) in &failed {
        tracing::warn!(path = %path.display(), %error, "skipping unreadable demo");
    }

    let mut keep = Vec::with_capacity(files.len());
    for f in &files {
        let path = f.path.to_string_lossy().into_owned();
        db.upsert_demo(&DemoRow {
            path: &path,
            file_name: &f.file_name,
            playdemo_arg: &f.playdemo_arg,
            kind: f.kind,
            size_bytes: f.size_bytes as i64,
            mtime: f.mtime,
            map: &f.header.map,
            server: &f.header.server,
            recorder: &f.header.recorder,
            playback_s: f.header.playback_s as f64,
            ticks: f.header.ticks as i64,
            tick_rate: f.header.tick_rate(),
            start_utc: f.start_utc,
            filename_time: f.filename_time.as_deref(),
            demos_tf_id: None,
            events: f.events.iter().map(|e| (e.tick, e.name.as_str(), e.value.as_deref())).collect(),
        })
        .await?;
        keep.push(path);
    }
    let removed = db.prune_demos_except(&keep, &offline).await?;

    let clocks = place_logs(db).await?;
    let spans: Vec<DemoSpan> = db
        .demo_spans()
        .await?
        .into_iter()
        .map(|(demo_id, map, start_utc, duration_s)| DemoSpan { demo_id, map, start_utc, duration_s })
        .collect();
    let links = link(&spans, &clocks);
    let rows: Vec<(i64, i64, &str, f64)> =
        links.iter().map(|l| (l.demo_id, l.log_id, l.method, l.log_share)).collect();
    db.replace_demo_links(&rows).await?;

    let stats = db.demo_stats().await?;
    Ok(DemoIndexSummary {
        scanned: files.len(),
        unreadable: failed.len(),
        removed,
        logs_placed: clocks.len(),
        links: links.len(),
        demos_linked: stats.linked,
        matches_with_demo: stats.matches_with_demo,
        markers: stats.markers,
    })
}

/// Put every kept log's rounds on the real clock and store the result.
///
/// The anchor for "when did this match really end, in UTC" is the log's own
/// upload — or, for a combined log uploaded long after the fact, the upload
/// of its last per-round part.
async fn place_logs(db: &Db) -> Result<Vec<LogClock>> {
    let mut clocks = Vec::new();
    let mut rows = Vec::new();
    for input in db.clock_inputs().await? {
        let part_times = db.part_upload_times(&input.parts).await?;
        let (anchor, kind) = match part_times.iter().max() {
            Some(&t) => (t, "parts"),
            None => match input.played_at {
                Some(t) => (t, "upload"),
                None => continue,
            },
        };
        let clock = LogClock {
            log_id: input.log_id,
            map: input.map.map(|m| m.to_ascii_lowercase()),
            raw_start: input.raw_start,
            raw_end: input.raw_end,
            anchor_utc: anchor,
        };
        rows.push(ClockRow {
            log_id: clock.log_id,
            raw_start: clock.raw_start,
            raw_end: clock.raw_end,
            anchor_utc: clock.anchor_utc,
            anchor_kind: kind,
            offset_s: clock.offset_s(),
            upload_delay_s: clock.upload_delay_s(),
        });
        clocks.push(clock);
    }
    db.replace_log_clocks(&rows).await?;
    Ok(clocks)
}

/// A log's clock offset: its server log's, else its upload against the end
/// of its last round, to the whole hour (the rule `LogClock` uses).
async fn clock_of(db: &Db, log_id: i64) -> Result<Option<i64>> {
    if let Some(o) = db.log_clock_offset(log_id).await? {
        return Ok(Some(o));
    }
    Ok(db.log_upload_and_end(log_id).await?.map(|(uploaded, end)| (uploaded - end).div_euclid(3600) * 3600))
}

/// Add linked demos, jump ticks and sidecar markers to a built match page.
pub async fn enrich(db: &Db, log: &NormalizedLog, detail: &mut MatchDetail) -> Result<()> {
    let demos = db.demos_for_log(log.log_id).await?;
    if demos.is_empty() {
        return Ok(());
    }
    detail.demos = demos.iter().map(view).collect();

    // Each round's clock offset. A log with a server log has one; a combined
    // log has none (no server log of its own), but its rounds are its parts'
    // rounds to the second, and each part's clock is its upload against its
    // last round's end (Q84: the S36 High final, six parts, six demos).
    let own = db.log_clock_offset(log.log_id).await?;
    let mut offsets: Vec<Option<i64>> = vec![own; log.rounds.len()];
    if own.is_none() {
        let parts: Vec<i64> = db.log_and_parts(log.log_id).await?.into_iter().filter(|id| *id != log.log_id).collect();
        let mut by_start: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        for p in parts {
            let Some(offset) = clock_of(db, p).await? else { continue };
            for (_, start, _, _) in db.all_rounds_of(p).await? {
                by_start.insert(start, offset);
            }
        }
        for (o, r) in offsets.iter_mut().zip(&log.rounds) {
            *o = r.start_time.and_then(|s| by_start.get(&s).copied());
        }
    }
    let offset = offsets.iter().flatten().next().copied();
    let Some(offset) = offset else {
        return Ok(());
    };

    // The first linked demo that contains a moment gets the jump.
    let jump_at = |raw_time: i64, offset: i64, lead_s: f64| -> Option<Jump> {
        demos.iter().find_map(|d| {
            let tick = tick_for(raw_time, offset, d.start_utc?, d.tick_rate?, d.ticks, lead_s)?;
            Some(Jump { demo_id: d.demo_id, tick })
        })
    };

    for ((round, raw), o) in detail.rounds.iter_mut().zip(&log.rounds).zip(&offsets) {
        let (Some(raw_start), Some(o)) = (raw.start_time, *o) else { continue };
        round.jump = jump_at(raw_start, o, 0.0);
        for ev in &mut round.events {
            ev.jump = jump_at(raw_start + ev.at_s, o, JUMP_LEAD_S);
        }
    }

    // Sidecar markers (killstreaks) are already demo ticks: place them in rounds.
    for d in &demos {
        let (Some(start), Some(rate)) = (d.start_utc, d.tick_rate) else { continue };
        for (tick, name, value) in &d.events {
            let raw = (start + *tick as f64 / rate).round() as i64 - offset;
            let hit = log.rounds.iter().zip(detail.rounds.iter_mut()).find(|(r, _)| {
                matches!((r.start_time, r.length_s), (Some(s), Some(len)) if raw >= s && raw <= s + len)
            });
            if let Some((r, round)) = hit {
                round.events.push(EventRow {
                    at_s: raw - r.start_time.unwrap_or(raw),
                    kind: name.to_ascii_lowercase(),
                    team: None,
                    player: None,
                    killer: None,
                    killer_is_me: false,
                    medigun: None,
                    point: None,
                    value: value.clone(),
                    jump: Some(Jump {
                        demo_id: d.demo_id,
                        tick: (*tick - (JUMP_LEAD_S * rate).round() as i64).max(0),
                    }),
                });
            }
        }
    }
    for round in &mut detail.rounds {
        round.events.sort_by_key(|e| e.at_s);
    }
    Ok(())
}

fn view(d: &LinkedDemo) -> DemoView {
    DemoView {
        demo_id: d.demo_id,
        file_name: d.file_name.clone(),
        playdemo_arg: d.playdemo_arg.clone(),
        kind: d.kind.clone(),
        recorder: d.recorder.clone(),
        duration_s: d.playback_s,
        recorded_at: d.start_utc.map(|s| s.round() as i64),
        size_bytes: d.size_bytes,
        method: d.method.clone(),
        log_share: d.log_share,
        markers: d.events.len(),
        approximate: d.kind == "stv",
        deleted: d.deleted,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StvFetched {
    /// The match it was fetched for, so a listener that has since moved on
    /// can tell which download finished.
    pub log_id: i64,
    pub demo_id: i64,
    pub file_name: String,
    pub bytes: u64,
    /// Share of the match inside this demo. A stopwatch match's combined log
    /// usually spans several STV demos, one per half, and demos.tf links one.
    pub log_share: f64,
}

/// Download the STV demo demos.tf holds for a match, index it and link it.
///
/// STV demos are placed on the clock from their demos.tf upload time, which
/// follows the end of the recording by seconds. That is an estimate rather
/// than the exact file times a local recording gives, so jumps into an STV
/// demo are flagged approximate on the match page.
pub async fn fetch_stv(
    db: &Db,
    sources: &crate::Sources,
    tf: &Path,
    log_id: i64,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<StvFetched> {
    use anyhow::Context;

    // demos.tf first; ETF2L's own demos for the matches it has none of (Q48).
    // A combined log's STV is one demo per part (Q52): all of them.
    let ids = db.stv_ids(log_id).await?;
    if ids.is_empty() {
        // Boxed: reading and linking a demo is a large future, and nested
        // here it overflowed the CLI's main-thread stack.
        return Box::pin(crate::etf2l_demos::fetch(db, sources, tf, log_id, progress))
            .await
            .context("demos.tf has no demo for this match, and ETF2L's could not be used");
    }
    // The other parts of its recording too (Flashy: "make it fetch the
    // missing STV parts automatically"): found before when the match is on
    // the real clock already, and again after the first download puts it
    // there. A part already on this machine is not downloaded again.
    let me = db.get_me().await?;
    let mut out: Option<StvFetched> = None;
    let mut tried: std::collections::HashSet<i64> = db.demos_tf_ids_here(log_id).await?.into_iter().collect();
    for pass in 0..2 {
        if let Some(me) = me {
            if let Err(e) = crate::stv_parts::find(db, sources, me, log_id).await {
                tracing::warn!(log_id, pass, error = %format!("{e:#}"), "looking for the other parts of the STV failed");
            }
        }
        let ids: Vec<i64> = db.stv_ids(log_id).await?.into_iter().filter(|id| !tried.contains(id)).collect();
        if ids.is_empty() {
            break;
        }
        for id in ids {
            tried.insert(id);
            let got = fetch_demostf(db, sources, tf, log_id, id, &mut progress).await?;
            out = Some(match out {
                None => got,
                Some(o) => StvFetched { bytes: o.bytes + got.bytes, log_share: ((o.log_share + got.log_share) * 100.0).round().min(100.0) / 100.0, ..o },
            });
        }
    }
    out.context("every part of this match's SourceTV demos.tf has is already on this machine")
}

/// One demos.tf demo, downloaded, indexed and linked to `log_id`.
async fn fetch_demostf(
    db: &Db,
    sources: &crate::Sources,
    tf: &Path,
    log_id: i64,
    demos_tf_id: i64,
    progress: impl FnMut(u64, Option<u64>),
) -> Result<StvFetched> {
    let meta = sources.demostf_meta(demos_tf_id).await?;
    // Q62: demos.tf keeps the listing of a demo whose file it no longer has.
    if !meta.available() {
        anyhow::bail!("demos.tf no longer has the file of demo {demos_tf_id}: its demos from before 14 October 2019 are gone");
    }

    // demos.tf names are already filesystem-safe; guard anyway.
    let safe: String = meta
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' })
        .collect();
    let dest = tf.join(hl_demos::scan::STV_DIR).join(&safe);
    let bytes = sources.download(&meta.url, &dest, progress).await?;

    let f = hl_demos::scan::read_demo(tf, &dest)?;
    let playback = f.header.playback_s as f64;
    let start_utc = meta.time.map(|t| t as f64 - playback);
    let path = dest.to_string_lossy().into_owned();
    let demo_id = db
        .upsert_demo(&DemoRow {
            path: &path,
            file_name: &f.file_name,
            playdemo_arg: &f.playdemo_arg,
            kind: "stv",
            size_bytes: f.size_bytes as i64,
            mtime: f.mtime,
            map: &f.header.map,
            server: &f.header.server,
            recorder: &f.header.recorder,
            playback_s: playback,
            ticks: f.header.ticks as i64,
            tick_rate: f.header.tick_rate(),
            start_utc,
            filename_time: None,
            demos_tf_id: Some(demos_tf_id),
            events: Vec::new(),
        })
        .await?;

    // How much of the match this demo covers, from the log's clock.
    let log_share = match (db.log_clock_offset(log_id).await?, start_utc) {
        (Some(_), Some(s)) => place_logs(db)
            .await?
            .into_iter()
            .find(|c| c.log_id == log_id)
            .map(|c| {
                let (ls, le) = (c.start_utc() as f64, c.end_utc() as f64);
                let overlap = (s + playback).min(le) - s.max(ls);
                ((overlap.max(0.0) / (le - ls)) * 100.0).round() / 100.0
            })
            .unwrap_or(0.0),
        _ => 0.0,
    };
    db.add_demo_link(demo_id, log_id, "demos.tf", log_share).await?;
    // The demo's header names its map: resolve again now, so a match whose
    // log never said which map it was on is drawn at once (Q30).
    if let Err(e) = crate::maps::resolve_all(db).await {
        tracing::warn!(log_id, error = %format!("{e:#}"), "round maps not resolved again after linking a demo");
    }

    Ok(StvFetched { log_id, demo_id, file_name: f.file_name, bytes, log_share })
}

/// Turns a moment in a log (logs.tf's round-time frame) into a demo tick, in
/// the first linked demo that holds it.
pub struct Jumper {
    demos: Vec<LinkedDemo>,
    offset: i64,
}

impl Jumper {
    /// `lead_s` seconds early, to show the lead-up.
    pub fn at(&self, log_time: i64, lead_s: f64) -> Option<Jump> {
        self.demos.iter().find_map(|d| {
            let tick = tick_for(log_time, self.offset, d.start_utc?, d.tick_rate?, d.ticks, lead_s)?;
            Some(Jump { demo_id: d.demo_id, tick })
        })
    }
}

/// `None` when the log has no linked demo or is not on the real clock.
pub async fn jumper(db: &Db, log_id: i64) -> Result<Option<Jumper>> {
    let demos = db.demos_for_log(log_id).await?;
    if demos.is_empty() {
        return Ok(None);
    }
    Ok(db.log_clock_offset(log_id).await?.map(|offset| Jumper { demos, offset }))
}
