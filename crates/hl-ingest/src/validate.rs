//! `hl validate sniper`: does a rating pick the team that won?
//!
//! Every kept, decided match with one rated Sniper a side gives a pair. For
//! each component the question is how often the team whose Sniper was better
//! at it won; for a set of weights, how often the higher-rated Sniper's team
//! won; and for a logistic model fitted on the pairs, which components carry
//! weight once the others are known (PLAN §3, "Model v2"; §12, step 0).
//!
//! Components are extracted afresh for every performance, including ones the
//! live model does not use, and measured against a pool of every Sniper
//! performance. The live rating leaves the owner out of the pool; here that
//! would favour one player's games, so nobody is left out.
//!
//! **Out of sample.** Pairs are split by date. The fitted model is trained on
//! the earlier pairs and scored on the later ones, and every weighting is
//! scored on both halves, so a weighting that only wins on the games it was
//! tuned on shows it. The default split is the start of Season 34, the
//! newest ~40% of this account's pairs.

use crate::rating::collect_performances;
use anyhow::{bail, Result};
use hl_core::TfClass;
use hl_db::Db;
use hl_rating::{Baseline, Component, Weights};
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;

/// Every component that means something for a Sniper, used or not.
pub const SNIPER_COMPONENTS: [Component; 9] = [
    Component::ImpactKills,
    Component::ImpactAssists,
    Component::MedicPicks,
    Component::Duel,
    Component::HeadshotShare,
    Component::Deaths,
    Component::Dpm,
    Component::Opening,
    Component::Untraded,
];

/// Model v1's Sniper weights, for comparison.
pub const SNIPER_V1: [(Component, f64); 7] = [
    (Component::ImpactKills, 0.30),
    (Component::Duel, 0.20),
    (Component::MedicPicks, 0.15),
    (Component::Deaths, 0.15),
    (Component::HeadshotShare, 0.10),
    (Component::Dpm, 0.05),
    (Component::ImpactAssists, 0.05),
];

/// Both Snipers of one decided match. Percentiles are 0-1 with lower-is-better
/// components flipped, in the order of the component list; `None` where the
/// log could not measure one (no raw log, no headshot tracking).
#[derive(Debug, Clone)]
pub struct Pair {
    pub log_id: i64,
    pub played_at: i64,
    /// Sniper A's team won.
    pub a_won: bool,
    pub a: Vec<Option<f64>>,
    pub b: Vec<Option<f64>>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Accuracy {
    pub correct: usize,
    pub n: usize,
}

impl Accuracy {
    pub fn pct(&self) -> Option<f64> {
        (self.n > 0).then(|| self.correct as f64 / self.n as f64 * 100.0)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRow {
    pub component: Component,
    pub label: String,
    /// The better Sniper's team won this share of the pairs that differ.
    pub better_won: Accuracy,
    /// Standard errors from a coin flip.
    pub z: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemeRow {
    pub name: String,
    pub weights: Vec<(Component, f64)>,
    pub all: Accuracy,
    pub before: Accuracy,
    pub after: Accuracy,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coef {
    pub component: Component,
    pub value: f64,
    /// Bootstrap 5th and 95th percentiles.
    pub low: f64,
    pub high: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub class: TfClass,
    pub pairs: usize,
    pub split_at: i64,
    pub before: usize,
    pub after: usize,
    pub components: Vec<ComponentRow>,
    pub schemes: Vec<SchemeRow>,
    /// Fitted on every pair, with bootstrap spread.
    pub fitted: Vec<Coef>,
    /// The same model fitted on the earlier pairs only, scored on the later ones.
    pub fitted_out_of_sample: Accuracy,
    pub fitted_in_sample: Accuracy,
}

/// How often the better Sniper at each component was on the winning team.
pub fn component_rows(pairs: &[Pair], comps: &[Component]) -> Vec<ComponentRow> {
    let mut rows: Vec<ComponentRow> = comps
        .iter()
        .enumerate()
        .map(|(j, &c)| {
            let mut acc = Accuracy::default();
            for p in pairs {
                let (Some(a), Some(b)) = (p.a[j], p.b[j]) else { continue };
                if a == b {
                    continue;
                }
                acc.n += 1;
                acc.correct += usize::from((a > b) == p.a_won);
            }
            let z = if acc.n > 0 { (acc.correct as f64 / acc.n as f64 - 0.5) / (0.25 / acc.n as f64).sqrt() } else { 0.0 };
            ComponentRow { component: c, label: c.label().to_string(), better_won: acc, z }
        })
        .collect();
    rows.sort_by(|x, y| y.better_won.pct().unwrap_or(0.0).total_cmp(&x.better_won.pct().unwrap_or(0.0)));
    rows
}

/// A weighted rating from percentiles, renormalised over the components the
/// log measured, as the live rating does.
pub fn score(pcts: &[Option<f64>], comps: &[Component], weights: &[(Component, f64)]) -> Option<f64> {
    let (mut sum, mut total) = (0.0, 0.0);
    for (j, c) in comps.iter().enumerate() {
        let Some(&(_, w)) = weights.iter().find(|(x, _)| x == c) else { continue };
        let Some(p) = pcts[j] else { continue };
        if w > 0.0 {
            sum += w * p;
            total += w;
        }
    }
    (total > 0.0).then(|| sum / total)
}

/// How often the higher-rated Sniper's team won.
pub fn accuracy<'a>(pairs: impl IntoIterator<Item = &'a Pair>, comps: &[Component], weights: &[(Component, f64)]) -> Accuracy {
    let mut acc = Accuracy::default();
    for p in pairs {
        let (Some(a), Some(b)) = (score(&p.a, comps, weights), score(&p.b, comps, weights)) else { continue };
        if a == b {
            continue;
        }
        acc.n += 1;
        acc.correct += usize::from((a > b) == p.a_won);
    }
    acc
}

/// The pair's percentile differences, a missing component counting as even.
fn diffs(p: &Pair) -> Vec<f64> {
    p.a.iter().zip(&p.b).map(|(a, b)| a.zip(*b).map_or(0.0, |(a, b)| a - b)).collect()
}

/// Logistic regression of "A's team won" on the percentile differences, with
/// no intercept (which Sniper is A is arbitrary) and a light L2 penalty, by
/// gradient descent. Coefficients are per unit of percentile (0-1).
pub fn fit(pairs: &[&Pair], dims: usize, iterations: usize) -> Vec<f64> {
    const RATE: f64 = 0.5;
    const L2: f64 = 0.5;
    let data: Vec<(f64, Vec<f64>)> = pairs.iter().map(|p| (if p.a_won { 1.0 } else { 0.0 }, diffs(p))).collect();
    let n = data.len().max(1) as f64;
    let mut w = vec![0.0; dims];
    for _ in 0..iterations {
        let mut g: Vec<f64> = w.iter().map(|wi| L2 * wi / n).collect();
        for (y, x) in &data {
            let z: f64 = w.iter().zip(x).map(|(a, b)| a * b).sum();
            let e = 1.0 / (1.0 + (-z).exp()) - y;
            for (gj, xj) in g.iter_mut().zip(x) {
                *gj += e * xj / n;
            }
        }
        for (wj, gj) in w.iter_mut().zip(&g) {
            *wj -= RATE * gj;
        }
    }
    w
}

/// Tiny deterministic generator for the bootstrap: no dependency, same
/// numbers every run.
struct XorShift(u64);

impl XorShift {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn bootstrap(pairs: &[&Pair], dims: usize, reps: usize) -> Vec<(f64, f64)> {
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut runs: Vec<Vec<f64>> = (0..reps)
        .map(|_| {
            let sample: Vec<&Pair> = (0..pairs.len()).map(|_| pairs[rng.below(pairs.len())]).collect();
            fit(&sample, dims, 800)
        })
        .collect();
    (0..dims)
        .map(|j| {
            let mut v: Vec<f64> = runs.iter_mut().map(|r| r[j]).collect();
            v.sort_by(f64::total_cmp);
            let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
            (at(0.05), at(0.95))
        })
        .collect()
}

/// One Sniper of a match: their team won, when, and their percentiles.
type Side = (bool, i64, Vec<Option<f64>>);

/// A candidate model to score: a name and its weights.
pub type Candidate = (String, Vec<(Component, f64)>);

/// Build the pairs and score the live weights, v1 and any candidates.
pub async fn run(db: &Db, class: TfClass, live: &Weights, candidates: Vec<Candidate>, split: Option<i64>) -> Result<Report> {
    if class != TfClass::Sniper {
        bail!("only the Sniper is validated for now");
    }
    let comps: Vec<Component> = SNIPER_COMPONENTS.to_vec();

    // Every component, whatever the live model uses.
    let every = live.with_model(class, comps.iter().map(|c| (*c, 1.0)).collect());
    let (_, perfs) = collect_performances(db, &every, |_| {}).await?;
    let perfs: Vec<(i64, hl_rating::Performance)> = perfs.into_iter().filter(|(_, p)| p.class == class).collect();
    let baseline = Baseline::build(perfs.iter().map(|(_, p)| p), None);

    let results = db.decided_results().await?;
    let mut by_log: HashMap<i64, Vec<Side>> = HashMap::new();
    for (log_id, p) in &perfs {
        let Some(&(won, played_at)) = results.get(&(*log_id, p.account_id)) else { continue };
        let pcts = comps
            .iter()
            .map(|&c| {
                let raw = p.values.iter().find(|(x, _)| *x == c)?.1;
                let pct = baseline.percentile(class, c, raw)?;
                Some(if c.higher_is_better() { pct } else { 1.0 - pct })
            })
            .collect();
        by_log.entry(*log_id).or_default().push((won, played_at.unwrap_or(0), pcts));
    }
    let mut pairs: Vec<Pair> = by_log
        .into_iter()
        .filter_map(|(log_id, mut v)| {
            if v.len() != 2 || v[0].0 == v[1].0 {
                return None;
            }
            let (b, a) = (v.pop()?, v.pop()?);
            Some(Pair { log_id, played_at: a.1, a_won: a.0, a: a.2, b: b.2 })
        })
        .collect();
    pairs.sort_by_key(|p| (p.played_at, p.log_id));
    if pairs.len() < 50 {
        bail!("only {} matches with a rated Sniper a side: too few to validate", pairs.len());
    }

    let split_at = match split {
        Some(t) => t,
        None => crate::seasons::list(db)
            .await?
            .into_iter()
            .find(|s| s.key == "s34")
            .map(|s| s.from)
            .unwrap_or_else(|| pairs[pairs.len() * 3 / 5].played_at),
    };
    let (before, after): (Vec<&Pair>, Vec<&Pair>) = pairs.iter().partition(|p| p.played_at < split_at);

    let mut schemes: Vec<Candidate> = vec![
        ("live".to_string(), live.model_for(class).to_vec()),
        ("v1".to_string(), SNIPER_V1.to_vec()),
    ];
    schemes.extend(candidates);
    let schemes = schemes
        .into_iter()
        .map(|(name, weights)| SchemeRow {
            all: accuracy(&pairs, &comps, &weights),
            before: accuracy(before.iter().copied(), &comps, &weights),
            after: accuracy(after.iter().copied(), &comps, &weights),
            name,
            weights,
        })
        .collect();

    let all: Vec<&Pair> = pairs.iter().collect();
    let w = fit(&all, comps.len(), 3000);
    let spread = bootstrap(&all, comps.len(), 30);
    let fitted = comps
        .iter()
        .enumerate()
        .map(|(j, &c)| Coef { component: c, value: w[j], low: spread[j].0, high: spread[j].1 })
        .collect();

    // Trained on the earlier pairs, its positive coefficients as weights.
    let early = fit(&before, comps.len(), 3000);
    let as_weights: Vec<(Component, f64)> = comps.iter().zip(&early).map(|(c, w)| (*c, w.max(0.0))).collect();

    Ok(Report {
        class,
        pairs: pairs.len(),
        split_at,
        before: before.len(),
        after: after.len(),
        components: component_rows(&pairs, &comps),
        schemes,
        fitted,
        fitted_in_sample: accuracy(before.iter().copied(), &comps, &as_weights),
        fitted_out_of_sample: accuracy(after.iter().copied(), &comps, &as_weights),
    })
}

fn pct(a: Accuracy) -> String {
    a.pct().map_or("–".to_string(), |p| format!("{p:5.1}%"))
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let date = |t: i64| {
            let d = t.div_euclid(86_400);
            // Days since 1970 to a civil date (Howard Hinnant's algorithm).
            let z = d + 719_468;
            let era = z.div_euclid(146_097);
            let doe = z - era * 146_097;
            let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let day = doy - (153 * mp + 2) / 5 + 1;
            let month = if mp < 10 { mp + 3 } else { mp - 9 };
            let year = yoe + era * 400 + i64::from(month <= 2);
            format!("{year}-{month:02}-{day:02}")
        };
        writeln!(
            f,
            "{} matches with one rated {} a side: {} before {}, {} from then on\n",
            self.pairs,
            self.class.display_name(),
            self.before,
            date(self.split_at),
            self.after
        )?;
        writeln!(f, "Better at it, and their team won:")?;
        for r in &self.components {
            writeln!(f, "  {:<20} {}  of {:>4}  (z {:>5.1})", r.label, pct(r.better_won), r.better_won.n, r.z)?;
        }
        writeln!(f, "\nFitted together (bootstrap 5-95%):")?;
        for c in &self.fitted {
            let sign = if c.low > 0.0 {
                "helps"
            } else if c.high < 0.0 {
                "hurts"
            } else {
                "unclear"
            };
            writeln!(f, "  {:<20} {:>6.2}  [{:>5.2}, {:>5.2}]  {sign}", c.component.label(), c.value, c.low, c.high)?;
        }
        writeln!(f, "\nPicks the winner:            all   before  after")?;
        for s in &self.schemes {
            writeln!(f, "  {:<24} {}  {}  {}", s.name, pct(s.all), pct(s.before), pct(s.after))?;
        }
        writeln!(
            f,
            "  {:<24}    –     {}  {}   (trained before, scored after)",
            "fitted",
            pct(self.fitted_in_sample),
            pct(self.fitted_out_of_sample)
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: [Component; 2] = [Component::ImpactKills, Component::Deaths];

    fn pair(a_won: bool, a: [f64; 2], b: [f64; 2]) -> Pair {
        Pair { log_id: 0, played_at: 0, a_won, a: a.map(Some).to_vec(), b: b.map(Some).to_vec() }
    }

    #[test]
    fn the_better_sniper_winning_is_counted_per_component() {
        let pairs = [pair(true, [0.9, 0.2], [0.1, 0.8]), pair(false, [0.9, 0.2], [0.1, 0.8]), pair(true, [0.6, 0.9], [0.4, 0.1])];
        let rows = component_rows(&pairs, &C);
        let kills = rows.iter().find(|r| r.component == Component::ImpactKills).unwrap();
        assert_eq!((kills.better_won.correct, kills.better_won.n), (2, 3));
        let deaths = rows.iter().find(|r| r.component == Component::Deaths).unwrap();
        assert_eq!((deaths.better_won.correct, deaths.better_won.n), (2, 3));
    }

    #[test]
    fn a_score_renormalises_over_what_was_measured() {
        let w = [(Component::ImpactKills, 0.75), (Component::Deaths, 0.25)];
        assert!((score(&[Some(0.8), Some(0.4)], &C, &w).unwrap() - 0.7).abs() < 1e-12);
        let kills_only = score(&[Some(0.8), None], &C, &w).unwrap();
        assert!((kills_only - 0.8).abs() < 1e-12, "missing deaths: kills carry it all");
        assert_eq!(accuracy(&[pair(true, [0.8, 0.4], [0.2, 0.9])], &C, &w).correct, 1);
    }

    #[test]
    fn the_fit_finds_the_component_that_decides() {
        // Kills decide every match; deaths are noise.
        let pairs: Vec<Pair> = (0..200)
            .map(|i| {
                let k = (i % 10) as f64 / 10.0;
                let d = ((i * 7) % 10) as f64 / 10.0;
                pair(k > 0.45, [k, d], [0.45, 0.5])
            })
            .collect();
        let refs: Vec<&Pair> = pairs.iter().collect();
        let w = fit(&refs, 2, 3000);
        assert!(w[0] > 1.0 && w[0] > 5.0 * w[1].abs(), "{w:?}");
    }
}
