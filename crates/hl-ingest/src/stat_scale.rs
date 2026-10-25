//! Where a game's damage per minute and accuracy stand on their class
//! (Flashy, October 2026: "a colour-coded damage per minute, like osu has
//! colours for difficulties ... and this for accuracy"): each class's
//! values at fixed percentiles of the games held, the top 1% the last. An
//! official counts fully, a scrim half and a pug a quarter, so the scale is
//! what serious games look like.
//!
//! Each class also has a scale per map where enough games were played on
//! it (Flashy: "300 dpm on steel on sniper is crazy vs 300 dpm on
//! koth_product"): a stat is placed among the class's games on that map,
//! and on the class as a whole where the map has too few.

use anyhow::Result;
use hl_db::Db;
use serde::Serialize;
use std::collections::BTreeMap;

/// Games a class needs on a map before the map has a scale of its own.
pub const MIN_MAP_GAMES: usize = 200;

/// The percentiles each class's values are given at.
pub const PERCENTILES: [f64; 10] = [0.0, 10.0, 25.0, 40.0, 55.0, 70.0, 80.0, 90.0, 95.0, 99.0];

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClassScale {
    /// Damage per minute at each of [`PERCENTILES`].
    pub dpm: Vec<f64>,
    /// Hits over shots, 0-1, at each of [`PERCENTILES`].
    pub accuracy: Vec<f64>,
    /// Games each was measured on.
    pub dpm_games: usize,
    pub accuracy_games: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatScales {
    pub percentiles: Vec<f64>,
    pub classes: BTreeMap<String, ClassScale>,
    /// Class, then map (as [`hl_core::maps::map_base`] names it): the same
    /// scales on one map. A stat with too few games there is left empty.
    pub maps: BTreeMap<String, BTreeMap<String, ClassScale>>,
}

/// The value at each percentile of `(value, weight)`s.
pub fn weighted_percentiles(mut v: Vec<(f64, f64)>) -> Vec<f64> {
    v.retain(|(x, w)| x.is_finite() && *w > 0.0);
    if v.is_empty() {
        return Vec::new();
    }
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: f64 = v.iter().map(|(_, w)| w).sum();
    PERCENTILES
        .iter()
        .map(|p| {
            let target = p / 100.0 * total;
            let mut cum = 0.0;
            for (x, w) in &v {
                cum += w;
                if cum >= target {
                    return *x;
                }
            }
            v[v.len() - 1].0
        })
        .collect()
}

pub async fn stat_scales(db: &Db) -> Result<StatScales> {
    type ByMap = BTreeMap<(String, String), Vec<(f64, f64)>>;
    let mut dpm: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    let mut dpm_map: ByMap = BTreeMap::new();
    for (class, dmg, secs, w, map) in db.dpm_sample().await? {
        let x = (dmg as f64 * 60.0 / secs as f64, w);
        if let Some(m) = map.as_deref().map(hl_core::maps::map_base).filter(|m| !m.is_empty()) {
            dpm_map.entry((class.clone(), m)).or_default().push(x);
        }
        dpm.entry(class).or_default().push(x);
    }
    let mut acc: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    let mut acc_map: ByMap = BTreeMap::new();
    for (class, shots, hits, w, map) in db.accuracy_sample().await? {
        let x = (hits as f64 / shots as f64, w);
        if let Some(m) = map.as_deref().map(hl_core::maps::map_base).filter(|m| !m.is_empty()) {
            acc_map.entry((class.clone(), m)).or_default().push(x);
        }
        acc.entry(class).or_default().push(x);
    }
    let mut maps: BTreeMap<String, BTreeMap<String, ClassScale>> = BTreeMap::new();
    for ((c, m), v) in dpm_map {
        if v.len() >= MIN_MAP_GAMES {
            let e = maps.entry(c).or_default().entry(m).or_default();
            e.dpm_games = v.len();
            e.dpm = weighted_percentiles(v);
        }
    }
    for ((c, m), v) in acc_map {
        if v.len() >= MIN_MAP_GAMES {
            let e = maps.entry(c).or_default().entry(m).or_default();
            e.accuracy_games = v.len();
            e.accuracy = weighted_percentiles(v);
        }
    }
    let mut classes: BTreeMap<String, ClassScale> = BTreeMap::new();
    for (c, v) in dpm {
        let e = classes.entry(c).or_default();
        e.dpm_games = v.len();
        e.dpm = weighted_percentiles(v);
    }
    for (c, v) in acc {
        let e = classes.entry(c).or_default();
        e.accuracy_games = v.len();
        e.accuracy = weighted_percentiles(v);
    }
    Ok(StatScales { percentiles: PERCENTILES.to_vec(), classes, maps })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pug_counts_for_less_than_an_official() {
        // 100 officials at 200 and 100 pugs at 400: the pugs weigh a quarter,
        // so the median is an official's.
        let v: Vec<(f64, f64)> = (0..100).map(|_| (200.0, 1.0)).chain((0..100).map(|_| (400.0, 0.25))).collect();
        let p = weighted_percentiles(v);
        assert_eq!(p[4], 200.0, "55th");
        assert_eq!(p[9], 400.0, "the top 1% is the pugs'");
    }
}
