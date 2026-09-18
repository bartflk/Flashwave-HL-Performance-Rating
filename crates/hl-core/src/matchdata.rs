//! Normalized match data: the shape a logs.tf log takes once parsed.
//!
//! Produced by `hl-ingest`, written by `hl-db`. Lives here so neither of those
//! crates has to depend on the other for it.

use crate::{SteamId, TfClass};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Team {
    Red,
    Blue,
}

impl Team {
    pub fn parse(s: &str) -> Option<Team> {
        match s {
            "Red" | "red" | "RED" => Some(Team::Red),
            "Blue" | "blue" | "BLU" | "BLUE" => Some(Team::Blue),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Team::Red => "Red",
            Team::Blue => "Blue",
        }
    }

    pub fn other(self) -> Team {
        match self {
            Team::Red => Team::Blue,
            Team::Blue => Team::Red,
        }
    }
}

/// Which stats a log actually recorded. A `false` flag means the matching
/// field is missing, not zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogFlags {
    pub real_damage: bool,
    pub accuracy: bool,
    pub hs: bool,
    pub hs_hit: bool,
    pub bs: bool,
    pub cp: bool,
    pub dt: bool,
    pub airshots: bool,
    pub hr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedLog {
    pub log_id: i64,
    pub title: Option<String>,
    pub map: Option<String>,
    pub played_at: Option<i64>,
    pub duration_s: i64,
    pub red_score: i64,
    pub blue_score: i64,
    pub flags: LogFlags,
    pub players: Vec<PlayerLine>,
    pub rounds: Vec<RoundLine>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerStats {
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub suicides: i64,
    pub dmg: i64,
    pub dmg_real: i64,
    pub dt: i64,
    pub dt_real: i64,
    pub hr: i64,
    pub heal: i64,
    pub ubers: i64,
    pub drops: i64,
    pub headshots: i64,
    pub headshots_hit: i64,
    pub backstabs: i64,
    pub medkits: i64,
    pub medkits_hp: i64,
    pub sentries: i64,
    pub cpc: i64,
    pub ic: i64,
    pub lks: i64,
    pub airshots: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerLine {
    pub id: SteamId,
    pub name: Option<String>,
    pub team: Team,
    pub stats: PlayerStats,
    pub classes: Vec<ClassLine>,
    pub vs: Vec<VsLine>,
}

impl PlayerLine {
    pub fn total_time(&self) -> i64 {
        self.classes.iter().map(|c| c.time_s).sum()
    }

    /// The class played longest. `None` when the log recorded no class time.
    pub fn main_class(&self) -> Option<TfClass> {
        self.classes
            .iter()
            .filter(|c| c.time_s > 0)
            .max_by_key(|c| c.time_s)
            .map(|c| c.class)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassLine {
    pub class: TfClass,
    pub time_s: i64,
    pub kills: i64,
    pub assists: i64,
    pub deaths: i64,
    pub dmg: i64,
}

/// One row of the class-vs-class breakdown for a single player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsLine {
    pub other_class: TfClass,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundLine {
    pub round_num: i64,
    pub start_time: Option<i64>,
    pub length_s: Option<i64>,
    pub winner: Option<Team>,
    pub firstcap: Option<Team>,
    pub red: RoundTeam,
    pub blue: RoundTeam,
    pub events: Vec<EventLine>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoundTeam {
    pub kills: Option<i64>,
    pub dmg: Option<i64>,
    pub ubers: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLine {
    pub at_s: i64,
    pub kind: String,
    pub team: Option<Team>,
    pub player: Option<SteamId>,
    pub killer: Option<SteamId>,
    pub medigun: Option<String>,
    pub point: Option<i64>,
}

/// Format as trends.tf names it, plus what our own classifier can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Highlander,
    Sixes,
    Prolander,
    Other,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Highlander => "highlander",
            Format::Sixes => "sixes",
            Format::Prolander => "prolander",
            Format::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Format {
        match s {
            "highlander" => Format::Highlander,
            "sixes" => Format::Sixes,
            "prolander" => Format::Prolander,
            _ => Format::Other,
        }
    }
}
