//! Our own format classifier, for logs trends.tf does not index.
//!
//! trends.tf is the primary source for format. This is the fallback, and it
//! works from class coverage rather than headcount: a Highlander log with a
//! mid-match sub has 19+ players, but each team still fields nine classes.

use hl_core::matchdata::{Format, NormalizedLog, Team};
use std::collections::HashSet;

/// A class counts as fielded once someone played it for this long. Short
/// enough to survive a class swap, long enough to ignore a spawn-room switch.
const MIN_CLASS_TIME_S: i64 = 90;

/// A player counts towards the lineup once they played this share of the match.
const CORE_PLAYER_SHARE: f64 = 0.4;

pub fn classify(log: &NormalizedLog) -> Format {
    if log.duration_s <= 0 || log.players.is_empty() {
        return Format::Other;
    }

    let min_classes = [Team::Red, Team::Blue]
        .iter()
        .map(|&t| distinct_classes(log, t))
        .min()
        .unwrap_or(0);
    let min_core = [Team::Red, Team::Blue]
        .iter()
        .map(|&t| core_players(log, t))
        .min()
        .unwrap_or(0);

    if min_classes >= 8 {
        Format::Highlander
    } else if min_core == 7 {
        Format::Prolander
    } else if (5..=6).contains(&min_core) && min_classes <= 6 {
        Format::Sixes
    } else {
        Format::Other
    }
}

fn distinct_classes(log: &NormalizedLog, team: Team) -> usize {
    log.players
        .iter()
        .filter(|p| p.team == team)
        .flat_map(|p| p.classes.iter())
        .filter(|c| c.time_s >= MIN_CLASS_TIME_S)
        .map(|c| c.class)
        .collect::<HashSet<_>>()
        .len()
}

fn core_players(log: &NormalizedLog, team: Team) -> usize {
    let threshold = (log.duration_s as f64 * CORE_PLAYER_SHARE) as i64;
    log.players
        .iter()
        .filter(|p| p.team == team && p.total_time() >= threshold)
        .count()
}
