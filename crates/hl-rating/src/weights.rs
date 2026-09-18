//! Tunable weights, loaded from TOML.
//!
//! Defaults are compiled in. A `weights.toml` in the app data directory
//! overrides them, so tuning never needs a rebuild — the whole point of
//! keeping these out of Rust source.

use crate::model::Component;
use anyhow::{bail, Context, Result};
use hl_core::TfClass;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

pub const DEFAULT_TOML: &str = include_str!("weights.default.toml");

#[derive(Debug, Clone)]
pub struct Weights {
    victim_value: HashMap<TfClass, f64>,
    pub general: General,
    models: HashMap<ModelKey, Vec<(Component, f64)>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct General {
    pub assist_share: f64,
    pub min_minutes: f64,
    pub even_margin: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ModelKey {
    Sniper,
    Medic,
    Spy,
    Generic,
}

impl ModelKey {
    fn for_class(c: TfClass) -> Self {
        match c {
            TfClass::Sniper => ModelKey::Sniper,
            TfClass::Medic => ModelKey::Medic,
            TfClass::Spy => ModelKey::Spy,
            _ => ModelKey::Generic,
        }
    }

    fn name(self) -> &'static str {
        match self {
            ModelKey::Sniper => "sniper",
            ModelKey::Medic => "medic",
            ModelKey::Spy => "spy",
            ModelKey::Generic => "generic",
        }
    }
}

/// The file as written; validated and converted into [`Weights`].
#[derive(Deserialize)]
struct Raw {
    victim_value: HashMap<String, f64>,
    general: General,
    model: HashMap<String, HashMap<String, f64>>,
}

impl Weights {
    pub fn default_weights() -> Self {
        Self::parse(DEFAULT_TOML).expect("the compiled-in default weights must parse")
    }

    /// Parse and validate. Every mistake is an error rather than a silent zero:
    /// a typo'd class or component name would otherwise just vanish from the
    /// rating with nothing to say why.
    pub fn parse(toml_text: &str) -> Result<Self> {
        let raw: Raw = toml::from_str(toml_text).context("parsing weights TOML")?;

        let mut victim_value = HashMap::new();
        for c in TfClass::ALL {
            let v = *raw
                .victim_value
                .get(c.as_str())
                .with_context(|| format!("[victim_value] is missing `{}`", c.as_str()))?;
            victim_value.insert(c, v);
        }

        let mut models = HashMap::new();
        for key in [ModelKey::Sniper, ModelKey::Medic, ModelKey::Spy, ModelKey::Generic] {
            let table = raw
                .model
                .get(key.name())
                .with_context(|| format!("missing [model.{}]", key.name()))?;
            let mut comps = Vec::new();
            for (name, &w) in table {
                let c = Component::parse(name)
                    .with_context(|| format!("[model.{}]: unknown component `{name}`", key.name()))?;
                if w < 0.0 {
                    bail!("[model.{}]: `{name}` has a negative weight", key.name());
                }
                comps.push((c, w));
            }
            // Stable order, so ratings and their breakdowns never reshuffle.
            comps.sort_by_key(|(c, _)| Component::ALL.iter().position(|x| x == c));
            models.insert(key, comps);
        }
        if let Some(unknown) = raw.model.keys().find(|k| {
            ![ModelKey::Sniper, ModelKey::Medic, ModelKey::Spy, ModelKey::Generic]
                .iter()
                .any(|m| m.name() == k.as_str())
        }) {
            bail!("unknown model [model.{unknown}]; expected sniper, medic, spy or generic");
        }

        Ok(Weights { victim_value, general: raw.general, models })
    }

    /// The user's override if one exists and parses, otherwise the defaults.
    /// A broken override is reported rather than silently ignored.
    pub fn load(override_path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(override_path) {
            Ok(text) => match Self::parse(&text) {
                Ok(w) => (w, None),
                Err(e) => (
                    Self::default_weights(),
                    Some(format!("{} is invalid, using defaults: {e:#}", override_path.display())),
                ),
            },
            Err(_) => (Self::default_weights(), None),
        }
    }

    pub fn victim(&self, class: TfClass) -> f64 {
        self.victim_value[&class]
    }

    /// The components and weights that rate this class.
    pub fn model_for(&self, class: TfClass) -> &[(Component, f64)] {
        &self.models[&ModelKey::for_class(class)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_rank_medic_highest() {
        let w = Weights::default_weights();
        let top = TfClass::ALL
            .iter()
            .max_by(|a, b| w.victim(**a).total_cmp(&w.victim(**b)))
            .unwrap();
        assert_eq!(*top, TfClass::Medic);
    }

    #[test]
    fn a_missing_class_is_an_error_not_a_zero() {
        let broken = DEFAULT_TOML.replace("spy      = 0.9", "");
        assert!(Weights::parse(&broken).is_err());
    }

    #[test]
    fn a_typo_in_a_component_is_an_error() {
        let broken = DEFAULT_TOML.replace("duel           = 0.20", "dual = 0.20");
        let err = Weights::parse(&broken).unwrap_err();
        assert!(format!("{err:#}").contains("dual"));
    }

    #[test]
    fn sniper_rates_the_duel_and_heavy_does_not() {
        let w = Weights::default_weights();
        let has = |c: TfClass| w.model_for(c).iter().any(|(x, _)| *x == Component::Duel);
        assert!(has(TfClass::Sniper));
        assert!(!has(TfClass::Heavy));
    }
}
