//! Running the round-map resolver ([`crate::mapres`]) over every kept log,
//! and fetching the parts it matches rounds against.
//!
//! ```text
//! fetch:    combined logs -> their parts (recursively) -> logs.tf JSON -> part_raw
//! resolve:  rounds + kills + parts + ETF2L + raw map lines -> round_map + log_segment
//! ```

use crate::mapres::{self, is_map_name, DemoIn, Geometry, GeometryFile, LogIn, PartIn, RoundIn};
use crate::rawlog;
use crate::sources::Sources;
use crate::Progress;
use anyhow::Result;
use hl_db::{Db, RoundMapRow, SegmentRow};
use hl_demos::map_base;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Stop fetching after this many requests in a row fail to reach logs.tf:
/// when it is down or refusing us, 289 timeouts would take hours.
const GIVE_UP_AFTER: usize = 3;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartsSummary {
    pub wanted: usize,
    pub fetched: usize,
    pub failed: usize,
    /// logs.tf stopped answering, so the rest waits for the next sync.
    pub gave_up: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveSummary {
    pub logs: usize,
    pub rounds: usize,
    pub unresolved: usize,
    pub multi_map_logs: usize,
    pub by_source: Vec<(String, usize)>,
}

fn parse_ids(json: Option<&str>) -> Vec<i64> {
    json.and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default()
}

/// Every part with a real map name behind the kept logs that lack one,
/// walking through parts that are themselves combined.
async fn wanted_parts(db: &Db) -> Result<Vec<i64>> {
    let mut wanted = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<i64> = db
        .resolver_logs()
        .await?
        .into_iter()
        .filter(|l| !l.map_field.as_deref().is_some_and(is_map_name))
        .flat_map(|l| parse_ids(l.duplicate_of.as_deref()))
        .collect();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        for p in db.part_rows(&[id]).await? {
            if p.map.as_deref().is_some_and(is_map_name) {
                wanted.push(p.log_id);
            } else {
                let inner = parse_ids(p.duplicate_of.as_deref());
                if inner.is_empty() {
                    wanted.push(p.log_id);
                }
                stack.extend(inner);
            }
        }
    }
    wanted.sort_unstable();
    Ok(wanted)
}

pub async fn fetch_parts(db: &Db, sources: &Sources, mut progress: impl FnMut(Progress)) -> Result<PartsSummary> {
    let have: HashSet<i64> = db.part_raw_ids().await?.into_iter().collect();
    let todo: Vec<i64> = wanted_parts(db)
        .await?
        .into_iter()
        .filter(|id| !have.contains(id))
        .take(crate::BULK_PER_SYNC)
        .collect();
    let mut s = PartsSummary { wanted: todo.len(), ..Default::default() };
    let mut failing = 0;
    for (i, id) in todo.iter().enumerate() {
        progress(Progress::Parts { done: i, total: todo.len() });
        // drops.tf, and logs.tf only for a log under an hour old.
        match sources.sync_log_json(*id, db.log_played_at(*id).await?).await {
            Ok(None) => failing = 0,
            Ok(Some((json, _))) => {
                db.store_part_raw(*id, &json).await?;
                s.fetched += 1;
                failing = 0;
            }
            Err(e) => {
                tracing::warn!(log_id = id, error = %format!("{e:#}"), "part fetch failed");
                s.failed += 1;
                failing += 1;
                // A refusal ends the pass at once (see `http::Refused`).
                if failing >= GIVE_UP_AFTER || crate::http::refused(&e) {
                    s.gave_up = true;
                    progress(Progress::GaveUp { source: "logs.tf", done: i, total: todo.len(), what: "combined logs' parts" });
                    break;
                }
            }
        }
    }
    progress(Progress::Parts { done: todo.len(), total: todo.len() });
    Ok(s)
}

/// Base -> full map name -> logs that named it.
type NameCounts = HashMap<String, HashMap<String, usize>>;

/// Add this install's logs that name one map to `geo`, counting the full
/// names as it goes.
fn own_geometry(
    logs: &[hl_db::ResolverLog],
    points: &HashMap<i64, Vec<(i64, i32, i32, i32, i32)>>,
    mut geo: Geometry,
) -> (Geometry, NameCounts) {
    let mut names: NameCounts = HashMap::new();
    for l in logs {
        let Some(m) = l.map_field.as_deref().filter(|m| is_map_name(m)) else { continue };
        let base = map_base(m);
        *names.entry(base.clone()).or_default().entry(m.trim().to_ascii_lowercase()).or_default() += 1;
        for &(_, kx, ky, vx, vy) in points.get(&l.log_id).into_iter().flatten() {
            geo.add(&base, kx, ky);
            geo.add(&base, vx, vy);
        }
    }
    (geo, names)
}

fn most_named(names: &NameCounts, base: &str) -> Option<String> {
    names.get(base).and_then(|m| m.iter().max_by_key(|(_, n)| **n).map(|(name, _)| name.clone()))
}

/// The map shapes this database knows, as the file that ships with the app
/// (`maps/geometry.json`): this database's own logs only, not the shipped
/// shapes, and only maps with `min_points` positions or more.
pub async fn export_geometry(db: &Db, min_points: u32) -> Result<GeometryFile> {
    let logs = db.resolver_logs().await?;
    let points = db.kill_points().await?;
    let (geo, names) = own_geometry(&logs, &points, Geometry::default());
    Ok(geo.to_file(&|b| most_named(&names, b), min_points))
}

/// The maps a player can choose from when saying which map rounds were on:
/// every map this database has resolved rounds to, and every map the app
/// ships a shape for. Full names, one per base (the most played version).
pub async fn known_maps(db: &Db) -> Result<Vec<String>> {
    let mut best: HashMap<String, (i64, String)> = HashMap::new();
    for (map, n) in db.round_map_counts().await? {
        let e = best.entry(map_base(&map)).or_insert((0, map.clone()));
        if n > e.0 {
            *e = (n, map);
        }
    }
    for (base, m) in GeometryFile::built_in().maps {
        best.entry(base).or_insert((0, m.name));
    }
    let mut out: Vec<(String, String)> = best.into_iter().map(|(b, (_, m))| (b, m)).collect();
    out.sort();
    Ok(out.into_iter().map(|(_, m)| m).collect())
}

/// Say which map these rounds of a log were on (`None` takes it back), and
/// resolve again so every page shows it.
pub async fn set_round_map(db: &Db, log_id: i64, rounds: &[i64], map: Option<&str>) -> Result<()> {
    if let Some(m) = map {
        anyhow::ensure!(is_map_name(m), "`{m}` is not a map name");
    }
    db.set_manual_round_map(log_id, rounds, map.map(|m| m.trim().to_ascii_lowercase()).as_deref()).await?;
    resolve_all(db).await?;
    Ok(())
}

/// Resolve every kept log's rounds to maps and store them. No network.
pub async fn resolve_all(db: &Db) -> Result<ResolveSummary> {
    let logs = db.resolver_logs().await?;
    let rounds = db.all_rounds().await?;
    let points = db.kill_points().await?;
    let demos = db.linked_demo_maps().await?;
    let demos_tf = db.demos_tf_maps().await?;
    let manual = db.manual_round_maps().await?;

    // The geometry model: the shapes that ship with the app, plus this
    // install's logs that name one map; and the usual full name for each
    // base (`vigil` -> the version played most here, else the shipped one).
    let shipped = GeometryFile::built_in();
    let (geo, names) = own_geometry(&logs, &points, Geometry::from_file(&shipped));
    let full_name = |b: &str| -> Option<String> {
        most_named(&names, b).or_else(|| shipped.maps.get(b).map(|m| m.name.clone()))
    };

    let mut stored = Vec::with_capacity(logs.len());
    let mut s = ResolveSummary::default();
    let mut by_source: HashMap<&'static str, usize> = HashMap::new();
    for l in &logs {
        let Some(rs) = rounds.get(&l.log_id) else { continue };
        let pts = points.get(&l.log_id);
        let single = l.map_field.as_deref().is_some_and(is_map_name);

        let mut input = LogIn {
            map_field: l.map_field.clone(),
            title: l.title.clone(),
            clock_offset: l.clock_offset,
            etf2l_maps: l.etf2l_maps.as_deref().and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default(),
            rounds: rs
                .iter()
                .map(|r| RoundIn {
                    round_num: r.round_num,
                    start: r.start,
                    length: r.length,
                    points: pts
                        .into_iter()
                        .flatten()
                        .filter(|p| p.0 >= r.start && p.0 <= r.start + r.length)
                        .flat_map(|&(_, kx, ky, vx, vy)| [(kx, ky), (vx, vy)])
                        .collect(),
                })
                .collect(),
            demos: demos
                .get(&l.log_id)
                .into_iter()
                .flatten()
                .map(|(map, start, end)| DemoIn { map: map.clone(), start: *start, end: *end })
                // The map demos.tf lists for the matched demo, unplaced.
                .chain(demos_tf.get(&l.log_id).map(|m| DemoIn { map: m.clone(), start: None, end: None }))
                .collect(),
            manual: manual.get(&l.log_id).cloned().unwrap_or_default(),
            ..Default::default()
        };
        if !single {
            // The parts trends.tf listed, and those found on logs.tf and
            // folded into it since (Q84: the S36 High final's eight).
            let mut ids = parse_ids(l.duplicate_of.as_deref());
            ids.extend(db.log_and_parts(l.log_id).await?.into_iter().filter(|id| *id != l.log_id));
            input.parts = parts_of(db, ids).await?;
            input.meta_maps = meta_maps(db, l.log_id, &rs.iter().map(|r| r.start).collect::<Vec<_>>()).await?;
        }

        let out = mapres::resolve(&input, &geo, &full_name);
        let winner: HashMap<i64, Option<&str>> = rs.iter().map(|r| (r.round_num, r.winner.as_deref())).collect();
        let segments: Vec<SegmentRow> = mapres::segments(&out)
            .into_iter()
            .map(|(map, nums)| SegmentRow {
                first_round: *nums.first().unwrap_or(&0),
                last_round: *nums.last().unwrap_or(&0),
                rounds: nums.len() as i64,
                red_wins: nums.iter().filter(|n| winner.get(n).copied().flatten() == Some("Red")).count() as i64,
                blue_wins: nums.iter().filter(|n| winner.get(n).copied().flatten() == Some("Blue")).count() as i64,
                map,
            })
            .collect();

        s.logs += 1;
        s.rounds += out.len();
        s.unresolved += out.iter().filter(|o| o.map.is_none()).count();
        if segments.iter().filter_map(|x| x.map.as_deref().map(map_base)).collect::<HashSet<_>>().len() > 1 {
            s.multi_map_logs += 1;
        }
        for o in &out {
            *by_source.entry(o.source.map_or("unresolved", |x| x.as_str())).or_default() += 1;
        }
        let rows = out
            .into_iter()
            .map(|o| RoundMapRow { round_num: o.round_num, map: o.map, source: o.source.map(|x| x.as_str()) })
            .collect();
        stored.push((l.log_id, rows, segments));
    }
    db.replace_round_maps(&stored).await?;
    let mut by: Vec<(String, usize)> = by_source.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    by.sort_by_key(|x| std::cmp::Reverse(x.1));
    s.by_source = by;
    Ok(s)
}

/// A combined log's parts, walking through parts that are combined themselves.
async fn parts_of(db: &Db, ids: Vec<i64>) -> Result<Vec<PartIn>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = ids;
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        for p in db.part_rows(&[id]).await? {
            match p.map.as_deref().filter(|m| is_map_name(m)) {
                Some(map) => {
                    // Its JSON where the part fetch kept it, else where an
                    // import stored it as a log of its own.
                    let json = match db.part_raw(p.log_id).await? {
                        Some(json) => Some(json),
                        None => db.raw_log(p.log_id).await?,
                    };
                    let round_starts = json.as_deref().map(round_starts).unwrap_or_default();
                    out.push(PartIn { map: map.to_string(), round_starts, uploaded: p.uploaded, duration: p.duration });
                }
                None => stack.extend(parse_ids(p.duplicate_of.as_deref())),
            }
        }
    }
    Ok(out)
}

fn round_starts(json: &str) -> Vec<i64> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| {
            v.get("rounds").and_then(|r| r.as_array()).map(|rs| {
                rs.iter().filter_map(|r| r.get("start_time").and_then(serde_json::Value::as_i64)).collect()
            })
        })
        .unwrap_or_default()
}

/// The raw log's own map lines, moved into logs.tf's round-time frame.
/// Only newer uploads write them.
async fn meta_maps(db: &Db, log_id: i64, logstf_starts: &[i64]) -> Result<Vec<(i64, String)>> {
    let Some(zip) = db.rawlog(log_id).await? else { return Ok(Vec::new()) };
    let Ok(text) = rawlog::unzip(&zip) else { return Ok(Vec::new()) };
    if !text.contains("\"meta_data\" (map") {
        return Ok(Vec::new());
    }
    let mut raw = rawlog::parse(&text);
    if let Some(shift) = rawlog::frame_offset(&raw.round_starts, logstf_starts) {
        raw.shift(shift);
    }
    Ok(raw.map_loads)
}
