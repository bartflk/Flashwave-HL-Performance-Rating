//! Which map every round was played on, including rounds inside logs that
//! were combined from several maps under a made-up name.
//!
//! Pure. Each round takes the first source that answers:
//!
//! 1. `log`: the log names one real map, so every round is on it.
//! 2. `meta`: the raw log's own `meta_data (map ...)` line before the round.
//! 3. `part`: a part the log was combined from has a round starting at the
//!    same second (logs.tf copies rounds verbatim when combining).
//! 4. `window`: the round falls in exactly one part's upload window on the
//!    real clock. Never wrong where checked, but windows leave gaps.
//! 5. `geometry`: the round's kill positions scored against every map's
//!    outline (see [`Geometry`]). On 310 held-out single-map rounds it picks
//!    the right map 97% of the time from 18 maps, and 99.7% from three. On
//!    this account it agrees with `window` on 680 of 692 rounds; the 12 others
//!    were on maps with no outline yet (Bagel, Valor), which the geometry
//!    cannot pick, so a candidate without an outline makes it unsure.
//!
//! Candidates narrow the geometry: the parts' maps, ETF2L's map list, and
//! map names spelled out in the log's name (`proot + proplant`, `upw/casc`).
//! Last, `neighbour` smoothing: maps come in unbroken blocks, so a round the
//! geometry was unsure of, between two rounds of one other map, takes theirs.

use hl_demos::map_base;
use std::collections::{HashMap, HashSet};

/// Game units per geometry cell. Coarser than the drawn outline: the model
/// wants "is this part of that map", not detail.
pub const GEO_CELL: f64 = 60.0;
/// Rounds with fewer kill positions than this are left to other sources.
const GEO_MIN_POINTS: usize = 10;
/// Smoothing pseudo-count for cells a map has never seen.
const GEO_PRIOR: f64 = 0.1;
/// A geometry answer this far ahead of the runner-up (mean log-likelihood per
/// position) is not overturned by its neighbours.
const GEO_CONFIDENT: f64 = 0.5;

/// Where the rounds' maps come from, most reliable first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Log,
    Meta,
    Part,
    Window,
    Geometry,
    Neighbour,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Log => "log",
            Source::Meta => "meta",
            Source::Part => "part",
            Source::Geometry => "geometry",
            Source::Window => "window",
            Source::Neighbour => "neighbour",
        }
    }
}

/// Top-down position counts per map, from logs that name one map.
#[derive(Debug, Default)]
pub struct Geometry {
    cells: HashMap<String, HashMap<(i64, i64), u32>>,
    totals: HashMap<String, u32>,
}

impl Geometry {
    pub fn add(&mut self, base: &str, x: i32, y: i32) {
        *self.cells.entry(base.to_string()).or_default().entry(cell(x, y)).or_default() += 1;
        *self.totals.entry(base.to_string()).or_default() += 1;
    }

    pub fn bases(&self) -> impl Iterator<Item = &str> {
        self.cells.keys().map(String::as_str)
    }

    /// Mean log-likelihood of these positions under a map's outline. Higher
    /// is a better fit; comparable across maps because each is normalised by
    /// its own total.
    fn fit(&self, base: &str, points: &[(i32, i32)]) -> Option<f64> {
        let cells = self.cells.get(base)?;
        let total = *self.totals.get(base)? as f64;
        let sum: f64 = points
            .iter()
            .map(|&(x, y)| ((cells.get(&cell(x, y)).copied().unwrap_or(0) as f64 + GEO_PRIOR) / total).ln())
            .sum();
        Some(sum / points.len() as f64)
    }

    /// The best-fitting base among `candidates` (all maps when empty), and its
    /// margin over the runner-up.
    pub fn best(&self, points: &[(i32, i32)], candidates: &HashSet<String>) -> Option<(String, f64)> {
        if points.len() < GEO_MIN_POINTS {
            return None;
        }
        // A candidate the model has never seen could be the answer, and would
        // never be chosen: report no confidence, so neighbours may overrule.
        let blind = candidates.iter().any(|c| !self.cells.contains_key(c));
        let mut scored: Vec<(f64, &str)> = self
            .bases()
            .filter(|b| candidates.is_empty() || candidates.contains(*b))
            .filter_map(|b| Some((self.fit(b, points)?, b)))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (top, base) = *scored.first()?;
        let margin = if blind { 0.0 } else { scored.get(1).map_or(f64::INFINITY, |s| top - s.0) };
        Some((base.to_string(), margin))
    }
}

fn cell(x: i32, y: i32) -> (i64, i64) {
    ((x as f64 / GEO_CELL).floor() as i64, (y as f64 / GEO_CELL).floor() as i64)
}

/// One round, in logs.tf's round-time frame.
#[derive(Debug, Clone)]
pub struct RoundIn {
    pub round_num: i64,
    pub start: i64,
    pub length: i64,
    /// Kill positions (both players) inside the round.
    pub points: Vec<(i32, i32)>,
}

/// A part the log was combined from.
#[derive(Debug, Clone)]
pub struct PartIn {
    pub map: String,
    /// Its rounds' start times, when its logs.tf JSON is stored.
    pub round_starts: Vec<i64>,
    /// Upload time and length on the real clock, from trends.tf.
    pub uploaded: Option<i64>,
    pub duration: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct LogIn {
    /// logs.tf's map field: a real map, free text, or nothing.
    pub map_field: Option<String>,
    pub title: Option<String>,
    pub rounds: Vec<RoundIn>,
    pub parts: Vec<PartIn>,
    /// `(time in logs.tf's frame, map)` from the raw log's map lines.
    pub meta_maps: Vec<(i64, String)>,
    /// ETF2L's maps for an official, in order.
    pub etf2l_maps: Vec<String>,
    /// Shift from logs.tf's frame to real time (the M4 log clock).
    pub clock_offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoundOut {
    pub round_num: i64,
    pub map: Option<String>,
    pub source: Option<Source>,
}

/// A real map name, as the server writes it: a short game-mode prefix, an
/// underscore, then the name (`pl_upward_f12`, `tow_tetsudo_b10a`). Anything
/// with spaces, dashes or slashes, or no prefix, is a name someone typed.
pub fn is_map_name(s: &str) -> bool {
    let s = s.trim().to_ascii_lowercase();
    let Some((prefix, rest)) = s.split_once('_') else { return false };
    (2..=6).contains(&prefix.len())
        && prefix.bytes().all(|b| b.is_ascii_lowercase())
        && !rest.is_empty()
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Map bases spelled out in free text: `proot + proplant`, `upw/casc`,
/// `vigilx2`. A token names a map when it is at least three letters and the
/// start of exactly one known base.
pub fn named_bases(text: &str, known: &HashSet<String>) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    for raw in lower.split(|c: char| !c.is_ascii_alphanumeric()) {
        // "vigilx2" -> "vigil"
        let token = raw.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end_matches('x');
        if token.len() < 3 {
            continue;
        }
        // An exact name wins over longer ones it prefixes (product, product_rcx).
        let hits: Vec<&String> = match known.get(token) {
            Some(exact) => vec![exact],
            None => known.iter().filter(|b| b.starts_with(token)).collect(),
        };
        if let [one] = hits.as_slice() {
            if !out.contains(*one) {
                out.push((*one).clone());
            }
        }
    }
    out
}

/// Resolve every round's map. `full_name` turns a base back into the map name
/// to store (`vigil` -> `pl_vigil_rc10`).
pub fn resolve(log: &LogIn, geo: &Geometry, full_name: &dyn Fn(&str) -> Option<String>) -> Vec<RoundOut> {
    let mut rounds: Vec<&RoundIn> = log.rounds.iter().collect();
    rounds.sort_by_key(|r| r.start);

    // 1. One real map for the whole log.
    if let Some(m) = log.map_field.as_deref().filter(|m| is_map_name(m)) {
        return rounds
            .iter()
            .map(|r| RoundOut { round_num: r.round_num, map: Some(m.trim().to_ascii_lowercase()), source: Some(Source::Log) })
            .collect();
    }

    // Candidate maps for the geometry, from everything that names one.
    let known: HashSet<String> = geo.bases().map(str::to_string).collect();
    let mut candidates: HashSet<String> = HashSet::new();
    let mut full: HashMap<String, String> = HashMap::new();
    for m in log.parts.iter().map(|p| p.map.as_str()).chain(log.etf2l_maps.iter().map(String::as_str)) {
        if is_map_name(m) {
            let b = map_base(m);
            full.entry(b.clone()).or_insert_with(|| m.to_ascii_lowercase());
            candidates.insert(b);
        }
    }
    for text in [log.map_field.as_deref(), log.title.as_deref()].into_iter().flatten() {
        candidates.extend(named_bases(text, &known));
    }
    let name_of = |b: &str| full.get(b).cloned().or_else(|| full_name(b));

    let mut out: Vec<(RoundOut, f64)> = Vec::new();
    for r in &rounds {
        let answer: Option<(String, Source, f64)> = meta_map(log, r)
            .map(|m| (m, Source::Meta, f64::INFINITY))
            .or_else(|| part_exact(log, r).map(|m| (m, Source::Part, f64::INFINITY)))
            .or_else(|| part_window(log, r).map(|m| (m, Source::Window, f64::INFINITY)))
            .or_else(|| {
                geo.best(&r.points, &candidates)
                    .and_then(|(b, margin)| Some((name_of(&b)?, Source::Geometry, margin)))
            });
        out.push(match answer {
            Some((m, s, margin)) => (RoundOut { round_num: r.round_num, map: Some(m), source: Some(s) }, margin),
            None => (RoundOut { round_num: r.round_num, map: None, source: None }, 0.0),
        });
    }

    // Neighbours: an unanswered round, or an unsure geometry answer, between
    // two rounds of one map takes that map. At the ends, the one neighbour.
    for i in 0..out.len() {
        let unsure = out[i].0.map.is_none() || (out[i].0.source == Some(Source::Geometry) && out[i].1 < GEO_CONFIDENT);
        if !unsure {
            continue;
        }
        let prev = i.checked_sub(1).and_then(|j| out[j].0.map.clone());
        let next = out.get(i + 1).and_then(|o| o.0.map.clone());
        let pick = match (prev, next) {
            (Some(a), Some(b)) if a == b => Some(a),
            (Some(a), None) if i + 1 == out.len() => Some(a),
            (None, Some(b)) if i == 0 => Some(b),
            _ => None,
        };
        if let Some(m) = pick {
            if out[i].0.map.as_deref() != Some(m.as_str()) {
                out[i].0 = RoundOut { round_num: out[i].0.round_num, map: Some(m), source: Some(Source::Neighbour) };
            }
        }
    }
    out.into_iter().map(|(o, _)| o).collect()
}

/// The last map line at or before the round's start.
fn meta_map(log: &LogIn, r: &RoundIn) -> Option<String> {
    log.meta_maps
        .iter()
        .filter(|(t, _)| *t <= r.start)
        .max_by_key(|(t, _)| *t)
        .map(|(_, m)| m.to_ascii_lowercase())
        .filter(|m| is_map_name(m))
}

fn part_exact(log: &LogIn, r: &RoundIn) -> Option<String> {
    log.parts
        .iter()
        .find(|p| is_map_name(&p.map) && p.round_starts.contains(&r.start))
        .map(|p| p.map.to_ascii_lowercase())
}

/// A part whose upload window holds the round, when exactly one map fits.
fn part_window(log: &LogIn, r: &RoundIn) -> Option<String> {
    let t = r.start + log.clock_offset?;
    let maps: HashSet<String> = log
        .parts
        .iter()
        .filter(|p| is_map_name(&p.map))
        .filter(|p| match (p.uploaded, p.duration) {
            (Some(up), Some(d)) => t >= up - d - 120 && t <= up,
            _ => false,
        })
        .map(|p| p.map.to_ascii_lowercase())
        .collect();
    (maps.len() == 1).then(|| maps.into_iter().next().unwrap())
}

/// Consecutive rounds on one map: `(map, round numbers)` in play order.
pub fn segments(rounds: &[RoundOut]) -> Vec<(Option<String>, Vec<i64>)> {
    let mut out: Vec<(Option<String>, Vec<i64>)> = Vec::new();
    for r in rounds {
        match out.last_mut() {
            Some((m, rs)) if *m == r.map => rs.push(r.round_num),
            _ => out.push((r.map.clone(), vec![r.round_num])),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two toy maps: "left" lives at x < 0, "right" at x > 0.
    fn geo() -> Geometry {
        let mut g = Geometry::default();
        for i in 0..400 {
            g.add("left", -2000 + (i % 20) * 60, (i / 20) * 60);
            g.add("right", 800 + (i % 20) * 60, (i / 20) * 60);
        }
        g
    }

    fn pts(x0: i32) -> Vec<(i32, i32)> {
        (0..20).map(|i| (x0 + (i % 5) * 60, (i / 5) * 60)).collect()
    }

    fn round(n: i64, start: i64, points: Vec<(i32, i32)>) -> RoundIn {
        RoundIn { round_num: n, start, length: 300, points }
    }

    fn names(b: &str) -> Option<String> {
        Some(format!("koth_{b}"))
    }

    #[test]
    fn recognises_real_map_names() {
        assert!(is_map_name("pl_upward_f12"));
        assert!(is_map_name("koth_product_final"));
        assert!(is_map_name("tow_tetsudo_b10a"), "any game-mode prefix");
        assert!(!is_map_name("steel"));
        assert!(!is_map_name("proot-product"));
        assert!(!is_map_name("proot + proplant"));
        assert!(!is_map_name("русские не победили :("));
        assert!(!is_map_name("vigilx2"));
    }

    #[test]
    fn reads_maps_out_of_free_text() {
        let known: HashSet<String> =
            ["upward", "cascade", "proot", "proplant", "product", "product_rcx", "vigil"].iter().map(|s| s.to_string()).collect();
        assert_eq!(named_bases("upw/casc", &known), ["upward", "cascade"]);
        assert_eq!(named_bases("vigilx2", &known), ["vigil"]);
        assert_eq!(named_bases("proot + proplant", &known), ["proot", "proplant"]);
        assert!(named_bases("pro", &known).is_empty(), "ambiguous: proot, proplant, product");
        assert_eq!(named_bases("product", &known), ["product"], "exact beats product_rcx");
        assert!(named_bases("how did we win", &known).is_empty());
    }

    #[test]
    fn a_real_map_name_covers_every_round() {
        let log = LogIn { map_field: Some("pl_upward_f12".into()), rounds: vec![round(1, 0, vec![])], ..Default::default() };
        let out = resolve(&log, &geo(), &names);
        assert_eq!(out[0].map.as_deref(), Some("pl_upward_f12"));
        assert_eq!(out[0].source, Some(Source::Log));
    }

    #[test]
    fn parts_match_rounds_to_the_second() {
        let log = LogIn {
            map_field: Some("both maps".into()),
            rounds: vec![round(1, 1000, pts(900)), round(2, 5000, pts(-1900))],
            parts: vec![
                PartIn { map: "koth_right".into(), round_starts: vec![1000], uploaded: None, duration: None },
                PartIn { map: "koth_left".into(), round_starts: vec![5000], uploaded: None, duration: None },
            ],
            ..Default::default()
        };
        let out = resolve(&log, &geo(), &names);
        assert_eq!(out.iter().map(|o| o.map.as_deref().unwrap()).collect::<Vec<_>>(), ["koth_right", "koth_left"]);
        assert!(out.iter().all(|o| o.source == Some(Source::Part)));
    }

    #[test]
    fn geometry_answers_when_nothing_else_does() {
        let log = LogIn {
            map_field: Some("how did we win".into()),
            rounds: vec![round(1, 0, pts(-1900)), round(2, 400, pts(900))],
            ..Default::default()
        };
        let out = resolve(&log, &geo(), &names);
        assert_eq!(out[0].map.as_deref(), Some("koth_left"));
        assert_eq!(out[1].map.as_deref(), Some("koth_right"));
        assert_eq!(out[0].source, Some(Source::Geometry));
    }

    #[test]
    fn a_round_without_kills_takes_its_neighbours_map() {
        let log = LogIn {
            map_field: None,
            rounds: vec![round(1, 0, pts(-1900)), round(2, 400, vec![]), round(3, 800, pts(-1900))],
            ..Default::default()
        };
        let out = resolve(&log, &geo(), &names);
        assert_eq!(out[1].map.as_deref(), Some("koth_left"));
        assert_eq!(out[1].source, Some(Source::Neighbour));
    }

    #[test]
    fn segments_group_consecutive_rounds() {
        let r = |n, m: &str| RoundOut { round_num: n, map: Some(m.into()), source: Some(Source::Part) };
        let s = segments(&[r(1, "a"), r(2, "a"), r(3, "b"), r(4, "a")]);
        assert_eq!(s.len(), 3, "a map played again later is its own segment");
        assert_eq!(s[0].1, vec![1, 2]);
    }
}
