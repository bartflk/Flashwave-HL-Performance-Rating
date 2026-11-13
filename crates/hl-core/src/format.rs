//! The formats the app covers, and the roles a 6v6 player plays.
//!
//! The app is growing from Highlander to all of competitive TF2 (Flashy, 13
//! Nov 2026: "format neutral ... it will become all encompassing for tf2
//! competitive"). 6v6 comes first, planned in phases (the 6v6 plan artifact,
//! P0 to P8); this is P0's shared vocabulary.
//!
//! Formats are never mixed: a Soldier's Highlander logs never feed a 6v6
//! number and the other way round (boSe.'s answer to question 2).

use crate::tfclass::TfClass;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Whether 6v6 is switched on in this build. 6v6 ships only once its rating
/// is ready (question 15), so until then a release build hides every 6v6
/// entry; a debug build (`tauri dev`) shows them to build against.
pub const SIXES_ENABLED: bool = cfg!(debug_assertions);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameFormat {
    Highlander,
    Sixes,
}

impl GameFormat {
    pub const ALL: [GameFormat; 2] = [GameFormat::Highlander, GameFormat::Sixes];

    /// The name stored in the database: the same words trends.tf and the
    /// app's classifier use for a log's format.
    pub fn as_str(self) -> &'static str {
        match self {
            GameFormat::Highlander => "highlander",
            GameFormat::Sixes => "sixes",
        }
    }

    pub fn parse(s: &str) -> Option<GameFormat> {
        match s.trim().to_ascii_lowercase().as_str() {
            "highlander" | "hl" => Some(GameFormat::Highlander),
            "sixes" | "6v6" | "6s" => Some(GameFormat::Sixes),
            _ => None,
        }
    }

    /// ETF2L's competition type for the format.
    pub fn etf2l_type(self) -> &'static str {
        match self {
            GameFormat::Highlander => "Highlander",
            GameFormat::Sixes => "6v6",
        }
    }

    /// Players a side.
    pub fn team_size(self) -> usize {
        match self {
            GameFormat::Highlander => 9,
            GameFormat::Sixes => 6,
        }
    }
}

impl fmt::Display for GameFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The formats someone plays, as setup stores them: "highlander,sixes".
///
/// Highlander alone when nothing is stored, nothing readable is, or 6v6 is
/// switched off in this build, so a Highlander player sees no change. Never
/// empty: a list with no format in it falls back to Highlander too.
pub fn formats_played(stored: Option<&str>) -> Vec<GameFormat> {
    let mut out: Vec<GameFormat> = stored
        .unwrap_or("")
        .split(',')
        .filter_map(GameFormat::parse)
        .filter(|f| SIXES_ENABLED || *f == GameFormat::Highlander)
        .collect();
    out.sort();
    out.dedup();
    if out.is_empty() {
        out.push(GameFormat::Highlander);
    }
    out
}

/// The stored form of [`formats_played`].
pub fn formats_stored(formats: &[GameFormat]) -> String {
    let mut v = formats.to_vec();
    v.sort();
    v.dedup();
    v.iter().map(|f| f.as_str()).collect::<Vec<_>>().join(",")
}

/// A 6v6 player's role in one match (the 6v6 plan, "The 6v6 models").
///
/// Two players share a class in 6v6 and play it differently, so the rating
/// is by role (question 5). Off-classes are rated by their own model each
/// (question 6), so an off-class stint carries its class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "role", content = "class")]
pub enum Role {
    /// The Soldier who stays with the Medic.
    Pocket,
    /// The Soldier who plays ahead and hunts picks.
    Roamer,
    /// The Scout who plays off the side.
    FlankScout,
    /// The Scout who plays with the combo.
    PocketScout,
    Demoman,
    Medic,
    /// A class outside the six: Pyro, Heavy, Engineer, Sniper or Spy.
    OffClass(TfClass),
}

impl Role {
    pub const MAIN: [Role; 6] = [Role::Pocket, Role::Roamer, Role::PocketScout, Role::FlankScout, Role::Demoman, Role::Medic];

    /// The class the role is played on.
    pub fn class(self) -> TfClass {
        match self {
            Role::Pocket | Role::Roamer => TfClass::Soldier,
            Role::FlankScout | Role::PocketScout => TfClass::Scout,
            Role::Demoman => TfClass::Demoman,
            Role::Medic => TfClass::Medic,
            Role::OffClass(c) => c,
        }
    }

    /// The name stored in the database: "pocket", "roamer", ..., or
    /// "off:engineer" for an off-class stint.
    pub fn key(self) -> String {
        match self {
            Role::Pocket => "pocket".into(),
            Role::Roamer => "roamer".into(),
            Role::FlankScout => "flank_scout".into(),
            Role::PocketScout => "pocket_scout".into(),
            Role::Demoman => "demoman".into(),
            Role::Medic => "medic".into(),
            Role::OffClass(c) => format!("off:{}", c.as_str()),
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        Some(match s {
            "pocket" => Role::Pocket,
            "roamer" => Role::Roamer,
            "flank_scout" => Role::FlankScout,
            "pocket_scout" => Role::PocketScout,
            "demoman" => Role::Demoman,
            "medic" => Role::Medic,
            _ => {
                let c = TfClass::parse(s.strip_prefix("off:")?).ok()?;
                return Role::off_class(c);
            }
        })
    }

    /// The role for a class played outside the six, or `None` for one of
    /// 6v6's own classes (Scout, Soldier, Demoman, Medic), whose role needs
    /// the match to tell.
    pub fn off_class(c: TfClass) -> Option<Role> {
        match c {
            TfClass::Pyro | TfClass::Heavy | TfClass::Engineer | TfClass::Sniper | TfClass::Spy => Some(Role::OffClass(c)),
            _ => None,
        }
    }

    /// Combo (Medic, Demoman, pocket, pocket Scout) or flank (roamer, flank
    /// Scout); an off-class is neither.
    pub fn is_combo(self) -> Option<bool> {
        match self {
            Role::Pocket | Role::PocketScout | Role::Demoman | Role::Medic => Some(true),
            Role::Roamer | Role::FlankScout => Some(false),
            Role::OffClass(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_round_trip_and_read_every_spelling() {
        for f in GameFormat::ALL {
            assert_eq!(GameFormat::parse(f.as_str()), Some(f));
        }
        assert_eq!(GameFormat::parse("6v6"), Some(GameFormat::Sixes));
        assert_eq!(GameFormat::parse(" HL "), Some(GameFormat::Highlander));
        assert_eq!(GameFormat::parse("prolander"), None);
    }

    #[test]
    fn formats_played_is_never_empty_and_highlander_by_default() {
        assert_eq!(formats_played(None), [GameFormat::Highlander]);
        assert_eq!(formats_played(Some("")), [GameFormat::Highlander]);
        assert_eq!(formats_played(Some("nonsense")), [GameFormat::Highlander]);
        let both = formats_played(Some("sixes,highlander,sixes"));
        if SIXES_ENABLED {
            assert_eq!(both, [GameFormat::Highlander, GameFormat::Sixes]);
            assert_eq!(formats_played(Some("sixes")), [GameFormat::Sixes]);
        } else {
            // A release build hides 6v6 until its rating ships.
            assert_eq!(both, [GameFormat::Highlander]);
            assert_eq!(formats_played(Some("sixes")), [GameFormat::Highlander]);
        }
        assert_eq!(formats_stored(&[GameFormat::Sixes, GameFormat::Highlander]), "highlander,sixes");
    }

    #[test]
    fn roles_round_trip_and_know_their_class() {
        let mut all: Vec<Role> = Role::MAIN.to_vec();
        all.extend([TfClass::Pyro, TfClass::Heavy, TfClass::Engineer, TfClass::Sniper, TfClass::Spy].map(|c| Role::off_class(c).unwrap()));
        for r in all {
            assert_eq!(Role::parse(&r.key()), Some(r), "{}", r.key());
        }
        assert_eq!(Role::Pocket.class(), TfClass::Soldier);
        assert_eq!(Role::FlankScout.class(), TfClass::Scout);
        assert_eq!(Role::off_class(TfClass::Soldier), None, "a Soldier's role needs the match to tell");
        assert_eq!(Role::parse("off:soldier"), None);
        assert_eq!(Role::OffClass(TfClass::Engineer).key(), "off:engineer");
        assert_eq!((Role::Pocket.is_combo(), Role::Roamer.is_combo()), (Some(true), Some(false)));
    }
}
