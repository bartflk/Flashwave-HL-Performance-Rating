//! Where a game's damage per minute and accuracy stand on their class
//! (Flashy, October 2026: "a colour-coded damage per minute, like osu has
//! colours for difficulties ... and this for accuracy"): each class's
//! values at fixed percentiles of the games held, the top 1% the last. An
//! official counts fully, a scrim half and a pug a quarter, so the scale is
//! what serious games look like.

use anyhow::Result;
use hl_db::Db;
use serde::Serialize;
use std::collections::BTreeMap;

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
    let mut dpm: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for (class, dmg, secs, w) in db.dpm_sample().await? {
        dpm.entry(class).or_default().push((dmg as f64 * 60.0 / secs as f64, w));
    }
    let mut acc: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for (class, shots, hits, w) in db.accuracy_sample().await? {
        acc.entry(class).or_default().push((hits as f64 / shots as f64, w));
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
    Ok(StatScales { percentiles: PERCENTILES.to_vec(), classes })
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
