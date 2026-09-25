//! `hl validate <class>`: does a rating pick the team that won?
//!
//! Every kept, decided match with one rated player of that class a side gives
//! a pair — and Highlander guarantees one of each class a side, so every
//! class has as many pairs as the Sniper does. For each component the
//! question is how often the team whose player was better at it won; for a
//! set of weights, how often the higher-rated player's team won; and for a logistic model fitted on the pairs, which components carry
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

/// What every class is measured on: output, deaths in context, and the
/// fights pass. None of these is about a particular class — they are what
/// a log says about anyone who shot at someone.
const CORE: [Component; 14] = [
    Component::ImpactKills,
    Component::ImpactAssists,
    Component::MedicPicks,
    Component::Deaths,
    Component::Dpm,
    Component::Caps,
    Component::Opening,
    Component::Untraded,
    Component::UntradedDeaths,
    Component::FlankDeaths,
    Component::StationaryDeaths,
    Component::FightKast,
    Component::FightKastEngaged,
    Component::SituationKills,
];

/// Every component that means something for this class, used by the live
/// model or not. The validator scores all of them; the model is what comes
/// out of that, which is the whole of Q8.
///
/// A class's extras are the things only it does. A Medic's healing, ubers
/// and drops are his job and nobody else's; backstabs are the Spy's; the
/// Sniper duel and headshot share belong to the two classes that can win
/// a fight at range with one shot.
pub fn components_for(class: TfClass) -> Vec<Component> {
    let mut v = CORE.to_vec();
    v.push(Component::FightSwing);
    match class {
        TfClass::Sniper => v.extend([Component::Duel, Component::HeadshotShare]),
        TfClass::Spy => v.extend([Component::Backstabs, Component::HeadshotShare, Component::Duel]),
        TfClass::Medic => v.extend([Component::Heal, Component::Ubers, Component::Drops]),
        _ => {}
    }
    v.sort_by_key(|c| Component::ALL.iter().position(|x| x == c));
    v
}

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
    /// A model proposed from every pair, rounded to weights a person can
    /// read (Q8). Fitted on everything, because that is the model that
    /// ships; what it is worth is `cv_proposed`, not its own `all` column.
    pub proposed: Vec<(Component, f64)>,
    /// What proposing a model this way is worth, cross-validated: five
    /// blocks of time, each scored by a model proposed from the other four.
    /// Every pair is held out exactly once, so this is comparable to the
    /// `all` column of a model that was not fitted here.
    pub cv_proposed: Accuracy,
    /// The same five held-out blocks, scored by the live and generic models,
    /// which is just their `all` accuracy — kept beside `cv_proposed` so the
    /// two numbers are read off the same pairs.
    pub cv_live: Accuracy,
    pub cv_generic: Accuracy,
}

/// How many blocks the cross-validation splits the pairs into.
///
/// Blocks are contiguous in time rather than random. Highlander drifts —
/// maps, the meta, who is playing — and a random fold lets a model peek at
/// the season it is being scored on. Five blocks over ~690 pairs leaves
/// about 140 pairs held out at a time and trains on the rest.
pub const FOLDS: usize = 5;

/// Score the proposal procedure honestly: propose from four blocks, score on
/// the fifth, five times over.
pub fn cross_validate(pairs: &[Pair], comps: &[Component]) -> Accuracy {
    let mut acc = Accuracy::default();
    for k in 0..FOLDS {
        let lo = pairs.len() * k / FOLDS;
        let hi = pairs.len() * (k + 1) / FOLDS;
        let train: Vec<&Pair> = pairs[..lo].iter().chain(&pairs[hi..]).collect();
        let test: Vec<&Pair> = pairs[lo..hi].iter().collect();
        if train.len() < 50 || test.is_empty() {
            continue;
        }
        let model = propose(comps, &fit(&train, comps.len(), 3000));
        if model.is_empty() {
            continue;
        }
        let a = accuracy(test, comps, &model);
        acc.correct += a.correct;
        acc.n += a.n;
    }
    acc
}

/// Turn fitted coefficients into weights fit to write down.
///
/// The fit answers "how much does being better at this predict winning, once
/// the others are known". That is the right question, and its answer is not
/// yet a model: coefficients come with a sign, a scale nobody reads, and a
/// long tail of components that carry a hundredth of a point. So:
///
/// * a negative or zero coefficient is dropped — the component is already
///   flipped so that higher is better, and one that still points down is
///   telling us it is collinear with something else, not that being worse at
///   it wins games;
/// * anything under a fifth of the largest is dropped as noise;
/// * what is left is normalised and rounded to the nearest 0.05, which is as
///   fine as a weight in this file has ever been meaningful.
///
/// Rounding is deliberately coarse. A model written to three decimals is a
/// fit pretending to be an opinion, and it will not survive the next season.
pub fn propose(comps: &[Component], coefs: &[f64]) -> Vec<(Component, f64)> {
    const FLOOR: f64 = 0.2;
    const STEP: f64 = 0.05;
    let max = coefs.iter().copied().fold(0.0_f64, f64::max);
    if max <= 0.0 {
        return Vec::new();
    }
    let kept: Vec<(Component, f64)> = comps
        .iter()
        .zip(coefs)
        .filter(|(_, &c)| c > 0.0 && c >= FLOOR * max)
        .map(|(&c, &v)| (c, v))
        .collect();
    let total: f64 = kept.iter().map(|(_, v)| v).sum();
    let mut out: Vec<(Component, f64)> = kept
        .iter()
        .map(|&(c, v)| (c, (((v / total) / STEP).round() * STEP * 100.0).round() / 100.0))
        .filter(|(_, w)| *w > 0.0)
        .collect();
    // Rounding rarely lands on 1.00; the largest weight takes the difference
    // so the numbers in the file add up when read.
    let sum: f64 = out.iter().map(|(_, w)| w).sum();
    if let Some(top) = out.iter_mut().max_by(|a, b| a.1.total_cmp(&b.1)) {
        top.1 = ((top.1 + 1.0 - sum) * 100.0).round() / 100.0;
    }
    out.sort_by_key(|(c, _)| Component::ALL.iter().position(|x| x == c));
    out
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
    let comps: Vec<Component> = components_for(class);

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
                let pct = baseline.percentile(class, c, p.map.as_deref(), raw)?;
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
        bail!(
            "only {} matches with a rated {} a side: too few to validate",
            pairs.len(),
            class.display_name()
        );
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

    // What this class is rated by now, and what it would be rated by with
    // no model of its own: the bar a new model has to clear.
    let mut schemes: Vec<Candidate> = vec![("live".to_string(), live.model_for(class).to_vec())];
    if live.has_own_model(class) {
        schemes.push(("generic".to_string(), live.generic_model().to_vec()));
    }
    if class == TfClass::Sniper {
        schemes.push(("v1".to_string(), SNIPER_V1.to_vec()));
    }
    schemes.extend(candidates);
    let schemes: Vec<SchemeRow> = schemes
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

    // The model that would ship: fitted on everything and rounded. What it
    // is worth is the cross-validation below, not its own accuracy here.
    let proposed = propose(&comps, &w);
    let cv_proposed = cross_validate(&pairs, &comps);
    let generic = live.generic_model().to_vec();

    Ok(Report {
        proposed,
        cv_proposed,
        cv_live: accuracy(&pairs, &comps, live.model_for(class)),
        cv_generic: accuracy(&pairs, &comps, &generic),
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
        writeln!(
            f,
            "
Over every pair, each held out once ({} blocks of time):
  {:<24} {}
  {:<24} {}
  {:<24} {}",
            FOLDS,
            "proposed (cross-validated)",
            pct(self.cv_proposed),
            "live",
            pct(self.cv_live),
            "generic",
            pct(self.cv_generic),
        )?;
        if !self.proposed.is_empty() {
            writeln!(f, "
Proposed, fitted on every pair:
")?;
            writeln!(f, "[model.{}]", self.class.as_str())?;
            let pad = self.proposed.iter().map(|(c, _)| c.key().len()).max().unwrap_or(0);
            for (c, w) in &self.proposed {
                writeln!(f, "{:<pad$} = {w:.2}", c.key())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: [Component; 2] = [Component::ImpactKills, Component::Deaths];

    #[test]
    fn a_class_is_measured_on_what_only_it_does() {
        assert!(components_for(TfClass::Medic).contains(&Component::Drops));
        assert!(!components_for(TfClass::Heavy).contains(&Component::Drops));
        assert!(components_for(TfClass::Spy).contains(&Component::Backstabs));
        for c in TfClass::ALL {
            let v = components_for(c);
            assert!(v.contains(&Component::Deaths), "{} is measured on dying", c.as_str());
            let mut sorted = v.clone();
            sorted.dedup();
            assert_eq!(sorted.len(), v.len(), "{}: no component twice", c.as_str());
        }
    }

    #[test]
    fn a_proposal_drops_the_noise_and_adds_up() {
        let comps = [
            Component::ImpactKills,
            Component::Deaths,
            Component::Dpm,
            Component::Caps,
        ];
        // Kills and deaths decide it; DPM is a tenth of the largest and caps
        // point the wrong way.
        let out = propose(&comps, &[2.0, 1.0, 0.2, -0.5]);
        assert_eq!(out, [(Component::ImpactKills, 0.65), (Component::Deaths, 0.35)]);
        let sum: f64 = out.iter().map(|(_, w)| w).sum();
        assert!((sum - 1.0).abs() < 1e-9, "{sum}");
        // Nothing helps: no model, rather than a model of noise.
        assert!(propose(&comps, &[-1.0, -0.5, 0.0, -0.2]).is_empty());
    }

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
