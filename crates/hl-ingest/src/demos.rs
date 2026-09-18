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

pub async fn index_demos(db: &Db, tf: &Path) -> Result<DemoIndexSummary> {
    let (files, failed) = hl_demos::scan(tf);
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
    let removed = db.prune_demos(&keep).await?;

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

/// Add linked demos, jump ticks and sidecar markers to a built match page.
pub async fn enrich(db: &Db, log: &NormalizedLog, detail: &mut MatchDetail) -> Result<()> {
    let demos = db.demos_for_log(log.log_id).await?;
    if demos.is_empty() {
        return Ok(());
    }
    detail.demos = demos.iter().map(view).collect();

    let Some(offset) = db.log_clock_offset(log.log_id).await? else {
        return Ok(());
    };

    // The first linked demo that contains a moment gets the jump.
    let jump_at = |raw_time: i64, lead_s: f64| -> Option<Jump> {
        demos.iter().find_map(|d| {
            let tick = tick_for(raw_time, offset, d.start_utc?, d.tick_rate?, d.ticks, lead_s)?;
            Some(Jump { demo_id: d.demo_id, tick })
        })
    };

    for (round, raw) in detail.rounds.iter_mut().zip(&log.rounds) {
        let Some(raw_start) = raw.start_time else { continue };
        round.jump = jump_at(raw_start, 0.0);
        for ev in &mut round.events {
            ev.jump = jump_at(raw_start + ev.at_s, JUMP_LEAD_S);
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
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StvFetched {
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
    progress: impl FnMut(u64, Option<u64>),
) -> Result<StvFetched> {
    use anyhow::Context;

    let demos_tf_id = db
        .index_info(log_id)
        .await?
        .and_then(|i| i.demos_tf_id)
        .context("demos.tf has no demo for this match")?;
    let meta = sources.demostf_meta(demos_tf_id).await?;

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

    Ok(StvFetched { demo_id, file_name: f.file_name, bytes, log_share })
}
