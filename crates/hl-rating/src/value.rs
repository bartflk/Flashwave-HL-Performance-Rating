//! The provisional class value score (model `v0`).
//!
//! Deliberately simple and fully itemised: the point of v0 is to give the
//! matchup view a winner *and* show exactly why, so the formula can be argued
//! with before M3 replaces it with per-class models. Every term is per 10
//! minutes on the class, so a sub who played five minutes is comparable to a
//! starter who played thirty.

use crate::weights::Weights;
use hl_core::matchdata::{ClassLine, PlayerLine};
use hl_core::TfClass;
use serde::Serialize;

pub const MODEL_VERSION: &str = "v0-generic";

/// A class value with its working shown.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Value {
    pub score: f64,
    pub minutes: f64,
    /// Kills weighted by victim class.
    pub impact_kills: f64,
    /// Assists weighted by victim class, scaled by `assist_share`.
    pub impact_assists: f64,
    /// Deaths weighted by the player's own class.
    pub death_cost: f64,
    /// Medic only: healing, ubers and drops.
    pub medic_term: f64,
    /// True when kills could not be split by victim class for this class
    /// (the player's time here is not their main class), so every kill was
    /// weighted as average.
    pub approximate: bool,
}

/// Value of `player`'s time on `line.class`.
///
/// `classkills` in the log is per player, not per class played. When this
/// class is the player's main class that is close enough to attribute the
/// weighted kills here; otherwise the class line's plain kill count is used
/// at average weight, and the result is flagged approximate.
pub fn value(player: &PlayerLine, line: &ClassLine, w: &Weights) -> Value {
    let minutes = line.time_s as f64 / 60.0;
    if minutes <= 0.0 {
        return Value::default();
    }
    let per10 = 10.0 / minutes;
    let is_main = player.main_class() == Some(line.class);

    let (impact_kills, impact_assists, approximate) = if is_main {
        let k: f64 = player.vs.iter().map(|v| v.kills as f64 * w.victim(v.other_class)).sum();
        let a: f64 = player.vs.iter().map(|v| v.assists as f64 * w.victim(v.other_class)).sum();
        (k, a * w.generic.assist_share, false)
    } else {
        let avg = average_victim_value(w);
        (
            line.kills as f64 * avg,
            line.assists as f64 * avg * w.generic.assist_share,
            true,
        )
    };

    let death_cost = line.deaths as f64 * w.victim(line.class) * w.generic.death_cost_share;

    let medic_term = if line.class == TfClass::Medic && is_main {
        let s = &player.stats;
        s.heal as f64 / w.medic.heal_per_point + s.ubers as f64 * w.medic.uber_value
            - s.drops as f64 * w.medic.drop_cost
    } else {
        0.0
    };

    Value {
        score: round2((impact_kills + impact_assists - death_cost + medic_term) * per10),
        minutes: round2(minutes),
        impact_kills: round2(impact_kills * per10),
        impact_assists: round2(impact_assists * per10),
        death_cost: round2(death_cost * per10),
        medic_term: round2(medic_term * per10),
        approximate,
    }
}

fn average_victim_value(w: &Weights) -> f64 {
    TfClass::ALL.iter().map(|c| w.victim(*c)).sum::<f64>() / TfClass::ALL.len() as f64
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
