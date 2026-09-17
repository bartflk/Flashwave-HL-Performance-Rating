//! The nine classes, in Highlander lineup order.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TfClass {
    Scout,
    Soldier,
    Pyro,
    Demoman,
    Heavy,
    Engineer,
    Medic,
    Sniper,
    Spy,
}

impl TfClass {
    /// Highlander lineup order — used for stable sorting in every roster view.
    pub const ALL: [TfClass; 9] = [
        TfClass::Scout,
        TfClass::Soldier,
        TfClass::Pyro,
        TfClass::Demoman,
        TfClass::Heavy,
        TfClass::Engineer,
        TfClass::Medic,
        TfClass::Sniper,
        TfClass::Spy,
    ];

    /// The identifier used in the database and in logs.tf JSON.
    ///
    /// Note logs.tf calls Heavy `heavyweapons`; [`TfClass::parse`] accepts both
    /// spellings but we always store the short one.
    pub fn as_str(self) -> &'static str {
        match self {
            TfClass::Scout => "scout",
            TfClass::Soldier => "soldier",
            TfClass::Pyro => "pyro",
            TfClass::Demoman => "demoman",
            TfClass::Heavy => "heavy",
            TfClass::Engineer => "engineer",
            TfClass::Medic => "medic",
            TfClass::Sniper => "sniper",
            TfClass::Spy => "spy",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            TfClass::Scout => "Scout",
            TfClass::Soldier => "Soldier",
            TfClass::Pyro => "Pyro",
            TfClass::Demoman => "Demoman",
            TfClass::Heavy => "Heavy",
            TfClass::Engineer => "Engineer",
            TfClass::Medic => "Medic",
            TfClass::Sniper => "Sniper",
            TfClass::Spy => "Spy",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "scout" => Ok(TfClass::Scout),
            "soldier" => Ok(TfClass::Soldier),
            "pyro" => Ok(TfClass::Pyro),
            "demoman" | "demo" => Ok(TfClass::Demoman),
            "heavyweapons" | "heavy" => Ok(TfClass::Heavy),
            "engineer" | "engie" => Ok(TfClass::Engineer),
            "medic" => Ok(TfClass::Medic),
            "sniper" => Ok(TfClass::Sniper),
            "spy" => Ok(TfClass::Spy),
            other => Err(Error::UnknownClass(other.to_string())),
        }
    }
}

impl fmt::Display for TfClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_logstf_spelling_of_heavy() {
        assert_eq!(TfClass::parse("heavyweapons").unwrap(), TfClass::Heavy);
        assert_eq!(TfClass::parse("Heavy").unwrap(), TfClass::Heavy);
        assert_eq!(TfClass::Heavy.as_str(), "heavy");
    }

    #[test]
    fn every_class_round_trips_through_as_str() {
        for c in TfClass::ALL {
            assert_eq!(TfClass::parse(c.as_str()).unwrap(), c);
        }
    }

    #[test]
    fn highlander_lineup_is_nine_distinct_classes() {
        let mut seen = TfClass::ALL.to_vec();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 9);
    }
}
