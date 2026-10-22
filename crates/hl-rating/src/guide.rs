//! The rating explained, from the live model (Flashy).
//!
//! The "How ratings work" page is built from this, not written by hand: every
//! class's components and weights come from the same [`Weights`] the rating
//! uses, so a retuned model changes the page with it. What each component
//! means is written once, here, in an exhaustive `match` -- a component added
//! to the model does not compile until it has been explained.

use crate::model::{Component, RATING_SPREAD};
use crate::weights::Weights;
use hl_core::TfClass;
use serde::Serialize;

/// What part of a player's game a component describes. Drawn as one colour
/// per group on the page, so a class's model reads at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Group {
    /// Kills, their value, and the damage behind them.
    Fragging,
    /// Dying, and how.
    Survival,
    /// Being in the fight and making it count for the team.
    Teamplay,
    /// Points and the cart.
    Objective,
    /// The Medic's own job.
    Medic,
    /// One class's speciality: the Sniper duel, headshots, backstabs.
    Speciality,
}

impl Group {
    pub const ALL: [Group; 6] = [Group::Fragging, Group::Survival, Group::Teamplay, Group::Objective, Group::Medic, Group::Speciality];

    pub fn key(self) -> &'static str {
        match self {
            Group::Fragging => "fragging",
            Group::Survival => "survival",
            Group::Teamplay => "teamplay",
            Group::Objective => "objective",
            Group::Medic => "medic",
            Group::Speciality => "speciality",
        }
    }
}

/// One game's standing in each component group, 0-100: the group's
/// components' percentiles weighted by their share of the rating. A group
/// the class's model has nothing in is left out. The stat bars of a player
/// profile (Q36), HLTV's "Firepower, Entrying, ..." for Highlander.
pub fn group_scores(parts: &[crate::model::Part]) -> Vec<(Group, f64)> {
    Group::ALL
        .into_iter()
        .filter_map(|g| {
            let (sum, weight) = parts
                .iter()
                .filter(|p| p.component.group() == g && p.weight > 0.0)
                .fold((0.0, 0.0), |(s, w), p| (s + p.percentile * p.weight, w + p.weight));
            (weight > 0.0).then(|| (g, sum / weight))
        })
        .collect()
}

/// The same as a JSON object keyed by group: `{"fragging": 71.2, ...}`.
pub fn group_scores_json(parts: &[crate::model::Part]) -> String {
    let fields: Vec<String> = group_scores(parts).into_iter().map(|(g, v)| format!("\"{}\":{:.1}", g.key(), v)).collect();
    format!("{{{}}}", fields.join(","))
}

/// Where a component's numbers come from, which decides which games have it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// logs.tf's summary: every log has it.
    Log,
    /// The raw server log, kill by kill: most logs; the oldest ~200 do not.
    ServerLog,
}

impl Component {
    /// One or two sentences a player can read: what is counted, and why it
    /// matters.
    pub fn description(self) -> &'static str {
        match self {
            Component::ImpactKills => "Your kills, each worth what the class you killed is worth: a Medic or a Demoman counts for more than a Scout. Adjusted for the map, and on attack/defend for which side the victim was on.",
            Component::ImpactAssists => "Your assists, valued the same way as kills but at a share of a kill's worth: the damage or the spam that made someone else's kill.",
            Component::MedicPicks => "Enemy Medics you killed. A dead Medic loses his team the uber, and usually the next fight.",
            Component::Duel => "Kills on the enemy Sniper minus deaths to him: who won the Sniper duel.",
            Component::HeadshotShare => "The share of your kills that were headshots.",
            Component::Backstabs => "Backstabs, per 10 minutes.",
            Component::Heal => "Healing done, per minute.",
            Component::Ubers => "Ubers used, per 10 minutes.",
            Component::Drops => "Ubers you died holding, per 10 minutes. Fewer is better.",
            Component::Caps => "Points you capped, per 10 minutes.",
            Component::Deaths => "How often you died, per 10 minutes. Fewer is better: every death is a player down for a respawn.",
            Component::Dpm => "Damage per minute. Measured twice and found to add nothing on top of what your kills and assists already count, so only the fallback model uses it.",
            Component::Opening => "The first kill of a fight, got minus given away. Opening a fight a player up decides most fights.",
            Component::Untraded => "The share of your kills after which your team did not lose someone within 3 seconds. A kill that is instantly traded back won nothing.",
            Component::UntradedDeaths => "Deaths your team did not avenge within 3 seconds. A death that was traded at least took someone with it. Fewer is better.",
            Component::FlankDeaths => "Deaths to a flanker: a Scout, a Spy or a Soldier. Fewer is better.",
            Component::StationaryDeaths => "Deaths near a spot you had already been killed from twice in the same life: holding an angle that had stopped working. Fewer is better.",
            Component::FightKast => "The share of fights you were alive for in which you got a kill or an assist, survived, or had your death traded.",
            Component::FightKastEngaged => "Fight KAST, but surviving only counts if you fired a shot: hiding through a fight does not.",
            Component::SituationKills => "Your kills, each scaled by the numbers when it happened: a kill at even numbers is worth more than a clean-up four players up.",
            Component::FightSwing => "How much your kills raised your team's chance of winning the fight, from the numbers and ubers at the moment of each kill.",
            Component::CapsContested => "Points you capped, each counted by the enemies alive to stop it. Walking onto a point after a wipe adds nothing.",
            Component::CapsMatesDead => "Points you capped, counted by your own team's dead at the time.",
            Component::FightSwingShared => "Fight swing, with each kill's worth shared among everyone who damaged the victim in the 5 seconds before: the damage that set up a kill earns part of it, and the last shot keeps at least half.",
            Component::RoundImpact => "How much your kills added to your team's chance of winning the round, less what your deaths gave away, from how many were alive on each side and who had the uber advantage. Half of every class's rating since v12: a player who takes more fights and wins them comes out ahead even when they die more.",
            Component::CapsSpawnDelay => "Seconds your caps cost your own dead teammates by pushing back their respawn a whole wave (8 s or more). Only on KOTH and 5CP: on payload and attack/defend a capture moves the attackers' spawn up. Fewer is better.",
        }
    }

    pub fn group(self) -> Group {
        match self {
            Component::ImpactKills | Component::Dpm | Component::SituationKills | Component::FightSwing | Component::FightSwingShared | Component::Opening | Component::RoundImpact => Group::Fragging,
            Component::Deaths | Component::UntradedDeaths | Component::FlankDeaths | Component::StationaryDeaths => Group::Survival,
            Component::ImpactAssists | Component::Untraded | Component::FightKast | Component::FightKastEngaged | Component::MedicPicks => Group::Teamplay,
            Component::Caps | Component::CapsContested | Component::CapsMatesDead | Component::CapsSpawnDelay => Group::Objective,
            Component::Heal | Component::Ubers | Component::Drops => Group::Medic,
            Component::Duel | Component::HeadshotShare | Component::Backstabs => Group::Speciality,
        }
    }

    pub fn source(self) -> Source {
        match self {
            Component::MedicPicks
            | Component::Duel
            | Component::HeadshotShare
            | Component::Backstabs
            | Component::Heal
            | Component::Ubers
            | Component::Drops
            | Component::Caps
            | Component::Deaths
            | Component::Dpm => Source::Log,
            Component::ImpactKills
            | Component::ImpactAssists
            | Component::Opening
            | Component::Untraded
            | Component::UntradedDeaths
            | Component::FlankDeaths
            | Component::StationaryDeaths
            | Component::FightKast
            | Component::FightKastEngaged
            | Component::SituationKills
            | Component::FightSwing
            | Component::CapsContested
            | Component::CapsMatesDead
            | Component::FightSwingShared
            | Component::CapsSpawnDelay
            | Component::RoundImpact => Source::ServerLog,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentGuide {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub higher_is_better: bool,
    /// Its share of the class's rating, 0 to 1: the weight over the model's
    /// total, which is how the rating renormalises them.
    pub share: f64,
    pub group: Group,
    pub source: Source,
    pub description: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassGuide {
    /// `scout` ... `spy`, as the rest of the app spells classes.
    pub class: &'static str,
    /// Rated by a model of its own, or by the shared fallback.
    pub own_model: bool,
    /// Biggest share first.
    pub components: Vec<ComponentGuide>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelGuide {
    pub model_version: &'static str,
    /// One standard deviation of the pool, in rating points: 0.25.
    pub rating_spread: f64,
    /// A class played for less than this is not rated.
    pub min_minutes: f64,
    pub classes: Vec<ClassGuide>,
    /// What a kill of each class is worth before map and side: the value
    /// behind impact kills and assists.
    pub victim_values: Vec<(&'static str, f64)>,
    /// Every component any model can use, for the glossary.
    pub glossary: Vec<ComponentGuide>,
}

fn component(c: Component, share: f64) -> ComponentGuide {
    ComponentGuide {
        key: c.key(),
        label: c.label(),
        unit: c.unit(),
        higher_is_better: c.higher_is_better(),
        share,
        group: c.group(),
        source: c.source(),
        description: c.description(),
    }
}

/// The live model, laid out for the page.
pub fn guide(w: &Weights) -> ModelGuide {
    let classes = TfClass::ALL
        .into_iter()
        .map(|class| {
            let model = w.model_for(class);
            let total: f64 = model.iter().map(|(_, x)| x).sum();
            let mut components: Vec<ComponentGuide> =
                model.iter().map(|(c, x)| component(*c, if total > 0.0 { x / total } else { 0.0 })).collect();
            components.sort_by(|a, b| b.share.total_cmp(&a.share));
            ClassGuide { class: class.as_str(), own_model: w.has_own_model(class), components }
        })
        .collect();
    ModelGuide {
        model_version: crate::model::MODEL_VERSION,
        rating_spread: RATING_SPREAD,
        min_minutes: w.general.min_minutes,
        classes,
        victim_values: TfClass::ALL.into_iter().map(|c| (c.as_str(), w.victim(c))).collect(),
        glossary: Component::ALL.into_iter().map(|c| component(c, 0.0)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_is_laid_out_from_the_live_model() {
        let w = Weights::default_weights();
        let g = guide(&w);
        assert_eq!(g.classes.len(), 9);
        for c in &g.classes {
            let class = TfClass::parse(c.class).unwrap();
            assert_eq!(c.components.len(), w.model_for(class).len(), "{}: every component shown", c.class);
            let sum: f64 = c.components.iter().map(|x| x.share).sum();
            assert!((sum - 1.0).abs() < 1e-9, "{}: shares add up to the whole rating, got {sum}", c.class);
            assert!(c.components.windows(2).all(|p| p[0].share >= p[1].share), "{}: biggest first", c.class);
        }
        assert_eq!(g.glossary.len(), Component::ALL.len());
        assert!(g.glossary.iter().all(|c| c.description.len() > 20), "every component explained");
    }

    #[test]
    fn a_group_is_its_components_percentiles_weighted_by_share() {
        use crate::model::Part;
        let part = |c: Component, pct: f64, weight: f64| Part { component: c, label: String::new(), unit: String::new(), raw: 0.0, percentile: pct, weight };
        let parts = [part(Component::Deaths, 80.0, 0.3), part(Component::UntradedDeaths, 40.0, 0.1), part(Component::MedicPicks, 90.0, 0.2)];
        let g: std::collections::HashMap<&str, f64> = group_scores(&parts).into_iter().map(|(g, v)| (g.key(), v)).collect();
        assert!((g["survival"] - 70.0).abs() < 1e-9, "(80 x .3 + 40 x .1) / .4");
        assert!((g["teamplay"] - 90.0).abs() < 1e-9);
        assert!(!g.contains_key("medic"), "a group with nothing in it is left out");
        assert_eq!(group_scores_json(&parts), r#"{"survival":70.0,"teamplay":90.0}"#);
    }
}
