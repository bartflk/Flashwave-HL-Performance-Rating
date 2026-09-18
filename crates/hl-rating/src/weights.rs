//! Tunable weights, loaded from TOML.
//!
//! Defaults are compiled in. A `weights.toml` in the app data directory
//! overrides them, so tuning never needs a rebuild — the whole point of
//! keeping these out of Rust source.

use anyhow::{Context, Result};
use hl_core::TfClass;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

pub const DEFAULT_TOML: &str = include_str!("weights.default.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Weights {
    victim_value: HashMap<String, f64>,
    pub generic: Generic,
    pub medic: Medic,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Generic {
    pub assist_share: f64,
    pub death_cost_share: f64,
    pub even_margin: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Medic {
    pub heal_per_point: f64,
    pub uber_value: f64,
    pub drop_cost: f64,
}

impl Weights {
    pub fn default_weights() -> Self {
        Self::parse(DEFAULT_TOML).expect("the compiled-in default weights must parse")
    }

    pub fn parse(toml_text: &str) -> Result<Self> {
        let w: Weights = toml::from_str(toml_text).context("parsing weights TOML")?;
        // Every class must have a value; a typo would otherwise silently score as zero.
        for c in TfClass::ALL {
            if !w.victim_value.contains_key(c.as_str()) {
                anyhow::bail!("weights: [victim_value] is missing `{}`", c.as_str());
            }
        }
        Ok(w)
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
        self.victim_value[class.as_str()]
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
}
