//! Rating model v1: per-class components, scored as percentiles.
//!
//! A performance is one player's time on their main class in one match. Each
//! component (impact kills, sniper duel, deaths, ...) is compared against every
//! other performance on that class in the stored history, giving a percentile.
//! The rating is the weighted average of those percentiles, 0-100.
//!
//! Only a player's **main class** is rated. `classkills` is recorded per
//! player, not per class played, so a flexer's kills cannot be split across
//! classes honestly; rating only the main class keeps every number attributable.

use crate::weights::Weights;
use hl_core::matchdata::{LogFlags, PlayerLine};
use hl_core::TfClass;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MODEL_VERSION: &str = "v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    ImpactKills,
    ImpactAssists,
    MedicPicks,
    Duel,
    HeadshotShare,
    Backstabs,
    Heal,
    Ubers,
    Drops,
    Caps,
    Deaths,
    Dpm,
}

impl Component {
    pub const ALL: [Component; 12] = [
        Component::ImpactKills,
        Component::ImpactAssists,
        Component::MedicPicks,
        Component::Duel,
        Component::HeadshotShare,
        Component::Backstabs,
        Component::Heal,
        Component::Ubers,
        Component::Drops,
        Component::Caps,
        Component::Deaths,
        Component::Dpm,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Component::ImpactKills => "impact_kills",
            Component::ImpactAssists => "impact_assists",
            Component::MedicPicks => "medic_picks",
            Component::Duel => "duel",
            Component::HeadshotShare => "headshot_share",
            Component::Backstabs => "backstabs",
            Component::Heal => "heal",
            Component::Ubers => "ubers",
            Component::Drops => "drops",
            Component::Caps => "caps",
            Component::Deaths => "deaths",
            Component::Dpm => "dpm",
        }
    }

    pub fn parse(s: &str) -> Option<Component> {
        Component::ALL.into_iter().find(|c| c.key() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Component::ImpactKills => "Impact kills",
            Component::ImpactAssists => "Impact assists",
            Component::MedicPicks => "Medic picks",
            Component::Duel => "Sniper duel",
            Component::HeadshotShare => "Headshot share",
            Component::Backstabs => "Backstabs",
            Component::Heal => "Healing",
            Component::Ubers => "Ubers",
            Component::Drops => "Drops",
            Component::Caps => "Caps",
            Component::Deaths => "Deaths",
            Component::Dpm => "Damage / min",
        }
    }

    /// The unit the raw value is shown in.
    pub fn unit(self) -> &'static str {
        match self {
            Component::HeadshotShare => "% of kills",
            Component::Heal | Component::Dpm => "per min",
            _ => "per 10 min",
        }
    }

    /// Fewer deaths and fewer drops are better; everything else, more is.
    pub fn higher_is_better(self) -> bool {
        !matches!(self, Component::Deaths | Component::Drops)
    }
}

/// One rateable performance and its raw component values.
#[derive(Debug, Clone)]
pub struct Performance {
    pub account_id: u32,
    pub class: TfClass,
    pub minutes: f64,
    pub values: Vec<(Component, f64)>,
}

/// Raw component values for `player` on their main class, or `None` when they
/// did not play it long enough to rate.
pub fn extract(player: &PlayerLine, flags: &LogFlags, w: &Weights) -> Option<Performance> {
    let class = player.main_class()?;
    let line = player.classes.iter().find(|c| c.class == class)?;
    let minutes = line.time_s as f64 / 60.0;
    if minutes < w.general.min_minutes {
        return None;
    }
    let per10 = 10.0 / minutes;
    let s = &player.stats;
    let vs = |c: TfClass| player.vs.iter().find(|v| v.other_class == c);

    let mut values = Vec::new();
    for (component, _) in w.model_for(class) {
        let v = match component {
            Component::ImpactKills => Some(
                player.vs.iter().map(|v| v.kills as f64 * w.victim(v.other_class)).sum::<f64>() * per10,
            ),
            Component::ImpactAssists => Some(
                player.vs.iter().map(|v| v.assists as f64 * w.victim(v.other_class)).sum::<f64>()
                    * w.general.assist_share
                    * per10,
            ),
            Component::MedicPicks => Some(vs(TfClass::Medic).map_or(0, |v| v.kills) as f64 * per10),
            Component::Duel => {
                let v = vs(TfClass::Sniper);
                let diff = v.map_or(0, |v| v.kills) - v.map_or(0, |v| v.deaths);
                Some(diff as f64 * per10)
            }
            // Undefined without kills, and meaningless where the log did not
            // record headshots: skipped rather than scored as zero.
            Component::HeadshotShare => {
                (flags.hs && line.kills > 0).then(|| s.headshots as f64 / line.kills as f64 * 100.0)
            }
            Component::Backstabs => flags.bs.then_some(s.backstabs as f64 * per10),
            Component::Heal => Some(s.heal as f64 / minutes),
            Component::Ubers => Some(s.ubers as f64 * per10),
            Component::Drops => Some(s.drops as f64 * per10),
            Component::Caps => flags.cp.then_some(s.cpc as f64 * per10),
            Component::Deaths => Some(line.deaths as f64 * per10),
            Component::Dpm => Some(line.dmg as f64 / minutes),
        };
        if let Some(v) = v {
            values.push((*component, v));
        }
    }

    Some(Performance {
        account_id: player.id.account_id(),
        class,
        minutes,
        values,
    })
}

/// Sorted component values per class: the pool every rating is measured against.
#[derive(Debug, Clone, Default)]
pub struct Baseline {
    by: HashMap<(TfClass, Component), Vec<f64>>,
}

impl Baseline {
    /// Build from performances, leaving out `exclude` — the owner — so they are
    /// measured against the players they face rather than against themselves.
    pub fn build<'a>(perfs: impl IntoIterator<Item = &'a Performance>, exclude: Option<u32>) -> Self {
        let mut by: HashMap<(TfClass, Component), Vec<f64>> = HashMap::new();
        for p in perfs {
            if Some(p.account_id) == exclude {
                continue;
            }
            for &(c, v) in &p.values {
                if v.is_finite() {
                    by.entry((p.class, c)).or_default().push(v);
                }
            }
        }
        for vals in by.values_mut() {
            vals.sort_by(f64::total_cmp);
        }
        Baseline { by }
    }

    pub fn from_parts(parts: impl IntoIterator<Item = (TfClass, Component, Vec<f64>)>) -> Self {
        let mut by = HashMap::new();
        for (class, c, mut vals) in parts {
            vals.sort_by(f64::total_cmp);
            by.insert((class, c), vals);
        }
        Baseline { by }
    }

    pub fn parts(&self) -> impl Iterator<Item = (TfClass, Component, &[f64])> {
        self.by.iter().map(|((class, c), v)| (*class, *c, v.as_slice()))
    }

    pub fn is_empty(&self) -> bool {
        self.by.is_empty()
    }

    /// Size of the pool for a class (its largest component).
    pub fn pool_size(&self, class: TfClass) -> usize {
        self.by
            .iter()
            .filter(|((c, _), _)| *c == class)
            .map(|(_, v)| v.len())
            .max()
            .unwrap_or(0)
    }

    /// Mid-rank percentile in 0..=1: the share of the pool below `value`,
    /// counting ties as half. `None` when there is no pool to compare against.
    pub fn percentile(&self, class: TfClass, c: Component, value: f64) -> Option<f64> {
        let vals = self.by.get(&(class, c)).filter(|v| !v.is_empty())?;
        let below = vals.partition_point(|x| *x < value);
        let not_above = vals.partition_point(|x| *x <= value);
        Some((below as f64 + (not_above - below) as f64 / 2.0) / vals.len() as f64)
    }
}

/// A rating with its working shown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rating {
    pub class: TfClass,
    /// 0-100: the weighted average of the component percentiles.
    pub score: f64,
    pub minutes: f64,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub component: Component,
    pub label: String,
    pub unit: String,
    pub raw: f64,
    /// 0-100, already flipped for lower-is-better components, so higher is
    /// always better here.
    pub percentile: f64,
    /// Share of the rating, after renormalising over the components present.
    pub weight: f64,
}

pub fn rate(perf: &Performance, baseline: &Baseline, w: &Weights) -> Option<Rating> {
    let weights: HashMap<Component, f64> = w.model_for(perf.class).iter().copied().collect();

    let mut parts = Vec::new();
    for &(c, raw) in &perf.values {
        let Some(&weight) = weights.get(&c).filter(|w| **w > 0.0) else { continue };
        let Some(p) = baseline.percentile(perf.class, c, raw) else { continue };
        let p = if c.higher_is_better() { p } else { 1.0 - p };
        parts.push(Part {
            component: c,
            label: c.label().to_string(),
            unit: c.unit().to_string(),
            raw: round2(raw),
            percentile: round1(p * 100.0),
            weight,
        });
    }

    let total: f64 = parts.iter().map(|p| p.weight).sum();
    if total <= 0.0 {
        return None;
    }
    for p in &mut parts {
        p.weight = round2(p.weight / total);
    }
    let score = parts.iter().map(|p| p.percentile * p.weight).sum::<f64>();

    Some(Rating {
        class: perf.class,
        score: round1(score),
        minutes: round1(perf.minutes),
        parts,
    })
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perf(account_id: u32, v: f64) -> Performance {
        Performance {
            account_id,
            class: TfClass::Sniper,
            minutes: 30.0,
            values: vec![(Component::Deaths, v), (Component::ImpactKills, v)],
        }
    }

    #[test]
    fn percentile_is_mid_rank() {
        let pool: Vec<_> = (1..=4).map(|i| perf(i, i as f64)).collect();
        let b = Baseline::build(&pool, None);
        // 2.0 sits above 1 value and ties 1: (1 + 0.5) / 4.
        assert_eq!(b.percentile(TfClass::Sniper, Component::ImpactKills, 2.0), Some(0.375));
        assert_eq!(b.percentile(TfClass::Sniper, Component::ImpactKills, 0.0), Some(0.0));
        assert_eq!(b.percentile(TfClass::Sniper, Component::ImpactKills, 9.0), Some(1.0));
    }

    #[test]
    fn the_owner_is_left_out_of_the_pool() {
        let pool = vec![perf(1, 1.0), perf(2, 2.0), perf(99, 50.0)];
        let b = Baseline::build(&pool, Some(99));
        assert_eq!(b.pool_size(TfClass::Sniper), 2);
    }

    #[test]
    fn fewer_deaths_rate_higher() {
        let w = Weights::default_weights();
        let pool: Vec<_> = (1..=10).map(|i| perf(i, i as f64)).collect();
        let b = Baseline::build(&pool, None);
        let few = Performance { values: vec![(Component::Deaths, 1.0)], ..perf(0, 0.0) };
        let many = Performance { values: vec![(Component::Deaths, 10.0)], ..perf(0, 0.0) };
        let few = rate(&few, &b, &w).unwrap().score;
        let many = rate(&many, &b, &w).unwrap().score;
        assert!(few > many, "few deaths {few} should beat many deaths {many}");
    }

    #[test]
    fn missing_components_renormalise_instead_of_scoring_zero() {
        let w = Weights::default_weights();
        let pool: Vec<_> = (1..=10).map(|i| perf(i, i as f64)).collect();
        let b = Baseline::build(&pool, None);
        let r = rate(&perf(0, 10.0), &b, &w).unwrap();
        let total: f64 = r.parts.iter().map(|p| p.weight).sum();
        assert!((total - 1.0).abs() < 0.02, "weights renormalise to 1, got {total}");
    }
}
