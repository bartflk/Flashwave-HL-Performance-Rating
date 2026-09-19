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
    /// Overrides when the victim is defending on an attack/defence map.
    defending: HashMap<TfClass, f64>,
    /// Per-map overrides, keyed by map-name prefix.
    maps: Vec<MapValues>,
    /// Control-point maps with an attacking and a defending side. Every
    /// payload map is one without being listed.
    attack_defend: Vec<String>,
    pub general: General,
    models: HashMap<ModelKey, Vec<(Component, f64)>>,
}

#[derive(Debug, Clone)]
struct MapValues {
    prefix: String,
    both: HashMap<TfClass, f64>,
    defending: HashMap<TfClass, f64>,
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
    /// Class values, plus the `defending` and `map` sub-tables.
    victim_value: toml::Table,
    #[serde(default)]
    attack_defend: AttackDefend,
    general: General,
    model: HashMap<String, HashMap<String, f64>>,
}

#[derive(Deserialize, Default)]
struct AttackDefend {
    #[serde(default)]
    maps: Vec<String>,
}

/// Class values in one table. Keys in `allow` (sub-tables) are left for the
/// caller; anything else that is not a known class is an error.
fn class_values(table: &toml::Table, at: &str, allow: &[&str]) -> Result<HashMap<TfClass, f64>> {
    let mut out = HashMap::new();
    for (key, value) in table {
        if allow.contains(&key.as_str()) {
            continue;
        }
        let class = TfClass::parse(key).map_err(|_| anyhow::anyhow!("[{at}]: unknown class `{key}`"))?;
        let v = value
            .as_float()
            .or_else(|| value.as_integer().map(|i| i as f64))
            .with_context(|| format!("[{at}]: `{key}` must be a number"))?;
        if v < 0.0 {
            bail!("[{at}]: `{key}` is negative");
        }
        out.insert(class, v);
    }
    Ok(out)
}

fn sub_table<'a>(table: &'a toml::Table, key: &str, at: &str) -> Result<Option<&'a toml::Table>> {
    match table.get(key) {
        None => Ok(None),
        Some(v) => v.as_table().map(Some).with_context(|| format!("[{at}.{key}] must be a table")),
    }
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

        let victim_value = class_values(&raw.victim_value, "victim_value", &["defending", "map"])?;
        for c in TfClass::ALL {
            if !victim_value.contains_key(&c) {
                bail!("[victim_value] is missing `{}`", c.as_str());
            }
        }
        let defending = match sub_table(&raw.victim_value, "defending", "victim_value")? {
            Some(t) => class_values(t, "victim_value.defending", &[])?,
            None => HashMap::new(),
        };
        let mut maps = Vec::new();
        if let Some(t) = sub_table(&raw.victim_value, "map", "victim_value")? {
            for (prefix, v) in t {
                let at = format!("victim_value.map.{prefix}");
                let table = v.as_table().with_context(|| format!("[{at}] must be a table"))?;
                let both = class_values(table, &at, &["defending"])?;
                let defending = match sub_table(table, "defending", &at)? {
                    Some(d) => class_values(d, &format!("{at}.defending"), &[])?,
                    None => HashMap::new(),
                };
                maps.push(MapValues { prefix: prefix.to_ascii_lowercase(), both, defending });
            }
        }
        // Longest prefix first, so `pl_upward_f12` beats `pl_upward`.
        maps.sort_by_key(|m| std::cmp::Reverse(m.prefix.len()));

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

        Ok(Weights {
            victim_value,
            defending,
            maps,
            attack_defend: raw.attack_defend.maps.iter().map(|m| m.to_ascii_lowercase()).collect(),
            general: raw.general,
            models,
        })
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

    /// The general value of killing this class, with no map or side.
    pub fn victim(&self, class: TfClass) -> f64 {
        self.victim_value[&class]
    }

    /// Whether this map has an attacking and a defending side: every payload
    /// map, and the control-point maps listed in `[attack_defend]`.
    pub fn attack_defend(&self, map: &str) -> bool {
        let map = map.to_ascii_lowercase();
        map.starts_with("pl_") || self.attack_defend.iter().any(|p| map.starts_with(p.as_str()))
    }

    /// The value of a kill in context. `defending` is whether the victim was
    /// on the defending side; it only matters on attack/defence maps. Most
    /// specific first: map and side, map, side, then the general value.
    pub fn victim_in(&self, class: TfClass, map: Option<&str>, defending: bool) -> f64 {
        let map = map.map(str::to_ascii_lowercase);
        let defending = defending && map.as_deref().is_some_and(|m| self.attack_defend(m));
        let per_map = map.as_deref().and_then(|m| self.maps.iter().find(|v| m.starts_with(v.prefix.as_str())));
        if let Some(m) = per_map {
            if defending {
                if let Some(v) = m.defending.get(&class) {
                    return *v;
                }
            }
            if let Some(v) = m.both.get(&class) {
                return *v;
            }
        }
        if defending {
            if let Some(v) = self.defending.get(&class) {
                return *v;
            }
        }
        self.victim(class)
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
    fn v2_values_are_the_defaults() {
        let w = Weights::default_weights();
        assert_eq!(w.victim(TfClass::Pyro), 1.5);
        assert_eq!(w.victim(TfClass::Spy), 1.3);
        assert!(w.victim(TfClass::Scout) > w.victim(TfClass::Engineer));
    }

    const LAYERS: &str = r#"
[victim_value.defending]
engineer = 2.0

[victim_value.map.pl_vigil]
sniper = 1.0

[victim_value.map.pl_vigil.defending]
engineer = 3.0

"#;

    fn layered() -> Weights {
        // Swap the default layers for ones the test controls.
        let base = DEFAULT_TOML.split("[victim_value.defending]").next().unwrap();
        let rest = &DEFAULT_TOML[DEFAULT_TOML.find("
[attack_defend]").unwrap()..];
        Weights::parse(&format!("{base}{LAYERS}{rest}")).unwrap()
    }

    #[test]
    fn victim_values_layer_from_specific_to_general() {
        let w = layered();
        let general = w.victim(TfClass::Engineer);
        // Payload: defending counts, attacking does not.
        assert_eq!(w.victim_in(TfClass::Engineer, Some("pl_upward_f12"), true), 2.0);
        assert_eq!(w.victim_in(TfClass::Engineer, Some("pl_upward_f12"), false), general);
        // A symmetric map has no defending side.
        assert_eq!(w.victim_in(TfClass::Engineer, Some("koth_product_final"), true), general);
        // Listed control-point maps are attack/defence; 5CP is not.
        assert!(w.attack_defend("cp_steel_f12"));
        assert!(!w.attack_defend("cp_gullywash_final1"));
        // Map and side beat map, which beats side.
        assert_eq!(w.victim_in(TfClass::Engineer, Some("pl_vigil_rc10"), true), 3.0);
        assert_eq!(w.victim_in(TfClass::Sniper, Some("pl_vigil_rc10"), true), 1.0);
        assert_eq!(w.victim_in(TfClass::Sniper, Some("PL_VIGIL_RC10"), false), 1.0, "case does not matter");
        assert_eq!(w.victim_in(TfClass::Medic, Some("pl_vigil_rc10"), true), w.victim(TfClass::Medic));
        assert_eq!(w.victim_in(TfClass::Engineer, None, true), general, "no map, no side");
    }

    #[test]
    fn a_typo_in_a_layer_is_an_error() {
        let bad = DEFAULT_TOML.replacen("engineer = 1.6", "enginer = 1.6", 1);
        assert!(bad != DEFAULT_TOML, "the default must have a defending layer to break");
        assert!(format!("{:#}", Weights::parse(&bad).unwrap_err()).contains("enginer"));
    }

    #[test]
    fn a_missing_class_is_an_error_not_a_zero() {
        let line = DEFAULT_TOML.lines().find(|l| l.starts_with("spy ")).expect("spy has a victim value");
        let broken = DEFAULT_TOML.replace(line, "");
        assert!(Weights::parse(&broken).is_err());
    }

    #[test]
    fn a_typo_in_a_component_is_an_error() {
        let broken = DEFAULT_TOML.replace("duel           = 0.05", "dual = 0.05");
        assert_ne!(broken, DEFAULT_TOML, "the test must break something");
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
