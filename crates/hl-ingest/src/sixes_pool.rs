//! The 6v6 rating's league pool, built in (the 6v6 plan, P5: "a summary
//! snapshot shipped with the release").
//!
//! Each 6v6 role's measurements over every 6v6 official in the logs.tf
//! database (`hl logsdump-sixes-pool`, from `F:\logs.tf database\
//! sixes.sqlite3`): the value at each percentile 0 to 100, per role and
//! component, and the role's score average and spread. A game's percentile
//! on a component is read from these instead of the few players in the
//! owner's logs, so a preview rating is against the whole competitive field
//! from the first sync. A component the dump cannot give (round impact,
//! kills valued by victim) is read against the stored pool as before.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const BUILT_IN: &str = include_str!("sixes_pool.json");

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SixesPool {
    /// Where it was built from, for the notes.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub logs: usize,
    #[serde(default)]
    pub roles: BTreeMap<String, RolePool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RolePool {
    pub games: usize,
    /// The role's weighted-percentile score over the pool: its average and spread.
    pub score_mean: f64,
    pub score_sd: f64,
    /// Per component, the value at each percentile 0..=100, rising.
    pub components: BTreeMap<String, Vec<f64>>,
}

/// The pool built into this build; `None` when it holds no role.
pub fn built_in() -> Option<&'static SixesPool> {
    static POOL: OnceLock<Option<SixesPool>> = OnceLock::new();
    POOL.get_or_init(|| serde_json::from_str::<SixesPool>(BUILT_IN).ok().filter(|p| !p.roles.is_empty())).as_ref()
}

/// Where `x` falls among `q` (the values at percentiles 0..=100), 0-100,
/// read between the two points around it; flipped when lower is better.
pub fn percentile(q: &[f64], x: f64, higher_is_better: bool) -> f64 {
    if q.len() < 2 || !x.is_finite() {
        return 50.0;
    }
    let last = (q.len() - 1) as f64;
    let rank = if x <= q[0] {
        0.0
    } else if x >= q[q.len() - 1] {
        last
    } else {
        // The first point at or above x, and the share of the way to it.
        let i = q.partition_point(|v| *v < x);
        let (lo, hi) = (q[i - 1], q[i]);
        let f = if hi > lo { (x - lo) / (hi - lo) } else { 0.5 };
        (i - 1) as f64 + f
    };
    let pct = rank / last * 100.0;
    if higher_is_better {
        pct
    } else {
        100.0 - pct
    }
}

/// The value at each percentile 0..=100 of `values` (sorted here).
pub fn quantiles(mut values: Vec<f64>) -> Vec<f64> {
    values.retain(|v| v.is_finite());
    if values.is_empty() {
        return Vec::new();
    }
    values.sort_by(f64::total_cmp);
    let n = values.len();
    (0..=100)
        .map(|p| {
            let pos = p as f64 / 100.0 * (n - 1) as f64;
            let (i, f) = (pos.floor() as usize, pos.fract());
            let v = if i + 1 < n { values[i] + (values[i + 1] - values[i]) * f } else { values[i] };
            (v * 1e4).round() / 1e4
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_placed_between_the_percentiles_around_it_and_flipped_when_less_is_better() {
        let q = quantiles((0..=100).map(f64::from).collect());
        assert_eq!(q.len(), 101);
        assert_eq!(percentile(&q, 25.0, true), 25.0);
        assert_eq!(percentile(&q, 25.5, true), 25.5);
        assert_eq!(percentile(&q, -5.0, true), 0.0);
        assert_eq!(percentile(&q, 500.0, true), 100.0);
        assert_eq!(percentile(&q, 25.0, false), 75.0, "fewer deaths is better");
    }

    #[test]
    fn the_built_in_pool_reads() {
        // Empty in a build without one; every role it has, complete.
        if let Some(p) = built_in() {
            for (role, r) in &p.roles {
                assert!(r.score_sd > 0.0, "{role}");
                assert!(r.components.values().all(|q| q.len() == 101), "{role}");
            }
        }
    }
}
