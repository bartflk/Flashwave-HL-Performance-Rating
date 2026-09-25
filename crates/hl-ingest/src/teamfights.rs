//! Who turned up to the fight, and how together (Q7, Taiga's ask).
//!
//! The fights pass already says which kills belong to one fight. This says
//! who was *in* it and when they arrived — which is the thing Highlander
//! players argue about after a lost round and no stat has ever answered.
//!
//! A player joins a fight the first moment they are part of it: a kill, a
//! death, damage dealt, or damage taken. Damage matters most here. A Soldier
//! who lands two rockets and lives is in the fight; a scoreboard that only
//! counts kills says he was never there.
//!
//! Two numbers come out of it:
//!
//! * **Collapse** — how spread out one side's arrivals were. Everyone in
//!   within three seconds of each other is a collapse; a trickle over twelve
//!   is people arriving one at a time to die one at a time.
//! * **Late** — arrivals after the fight was already decided, which is the
//!   same mistake from the other end.
//!
//! Nothing here is stored yet: it is computed from a raw log on demand, the
//! way the match page's other panels are.

use crate::rawlog::RawLog;
use crate::state::GameState;
use hl_core::matchdata::Team;
use serde::Serialize;
use std::collections::HashMap;

/// Arrivals this far apart or less counted as arriving together.
pub const TOGETHER_S: i64 = 3;

/// A fight with fewer players than this a side is a skirmish, not a teamfight,
/// and saying who collapsed on it is noise.
pub const MIN_SIDE: usize = 3;

/// One player's part in one fight.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Arrival {
    pub account_id: u32,
    pub team: Team,
    /// Seconds after the fight's first kill that this player first did or took
    /// damage, killed, or died. Negative when they were already trading
    /// before the first kill landed, which is the usual case for whoever
    /// started it.
    pub joined_at: i64,
    /// Seconds after the first kill that they died, if they did.
    pub died_at: Option<i64>,
}

/// One fight, from both sides.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Teamfight {
    /// Index of the fight within the log, matching the fights pass.
    pub seq: usize,
    pub round_num: i64,
    /// The raw log's clock, at the first kill.
    pub at: i64,
    pub arrivals: Vec<Arrival>,
}

impl Teamfight {
    /// How spread out one side's arrivals were, in seconds: the gap between
    /// the first and the last. `None` for a side too small to say anything
    /// about.
    pub fn collapse(&self, team: Team) -> Option<i64> {
        let times: Vec<i64> =
            self.arrivals.iter().filter(|a| a.team == team).map(|a| a.joined_at).collect();
        (times.len() >= MIN_SIDE).then(|| {
            let (lo, hi) = (times.iter().min().copied(), times.iter().max().copied());
            hi.unwrap_or(0) - lo.unwrap_or(0)
        })
    }

    /// Whether this side arrived as one.
    pub fn together(&self, team: Team) -> Option<bool> {
        self.collapse(team).map(|spread| spread <= TOGETHER_S)
    }
}

/// Every fight in a log, with who joined each and when.
///
/// `fight_of` is the fights pass's answer for each kill: its index within the
/// log, or `None` for kills that do not count.
pub fn teamfights(raw: &RawLog, gs: &GameState, fight_of: &[Option<usize>]) -> Vec<Teamfight> {
    // When each fight opened, and in which round.
    let mut opened: HashMap<usize, (i64, i64)> = HashMap::new();
    for (i, k) in raw.kills.iter().enumerate() {
        let Some(f) = fight_of.get(i).copied().flatten() else { continue };
        let round = gs.round_at(k.at).map_or(0, |r| i64::from(r.num));
        opened.entry(f).or_insert((k.at, round));
    }
    if opened.is_empty() {
        return Vec::new();
    }
    let last = opened.keys().max().copied().unwrap_or(0);

    // A fight's window runs from its first kill to the next fight's.
    let mut starts: Vec<(usize, i64, i64)> =
        opened.iter().map(|(f, (at, r))| (*f, *at, *r)).collect();
    starts.sort_unstable();
    let window = |f: usize| -> (i64, i64) {
        let i = starts.binary_search_by_key(&f, |(x, _, _)| *x).unwrap_or(0);
        let from = starts[i].1;
        let to = starts.get(i + 1).map_or(from + 60, |(_, at, _)| *at);
        (from, to)
    };

    let mut out = Vec::new();
    for &(f, at, round_num) in &starts {
        let (from, to) = window(f);
        // Everyone who touched this fight, and the first moment they did.
        let mut first: HashMap<u32, (Team, i64)> = HashMap::new();
        let mut note = |who: &crate::rawlog::Actor, when: i64| {
            let Some(team) = who.team else { return };
            let e = first.entry(who.account).or_insert((team, when));
            if when < e.1 {
                *e = (team, when);
            }
        };
        // Damage is what makes this honest: a player who shot and lived was
        // in the fight, and the kill list alone would never say so.
        for d in &raw.damage {
            if d.live && (from - TOGETHER_S..to).contains(&d.at) {
                note(&d.attacker, d.at);
                note(&d.victim, d.at);
            }
        }
        let mut died: HashMap<u32, i64> = HashMap::new();
        for (i, k) in raw.kills.iter().enumerate() {
            if fight_of.get(i).copied().flatten() != Some(f) {
                continue;
            }
            note(&k.killer, k.at);
            note(&k.victim, k.at);
            died.entry(k.victim.account).or_insert(k.at - at);
        }

        let mut arrivals: Vec<Arrival> = first
            .into_iter()
            .map(|(account_id, (team, when))| Arrival {
                account_id,
                team,
                joined_at: when - at,
                died_at: died.get(&account_id).copied(),
            })
            .collect();
        arrivals.sort_unstable_by_key(|a| (a.team.as_str(), a.joined_at, a.account_id));
        out.push(Teamfight { seq: f, round_num, at, arrivals });
    }
    debug_assert!(out.len() <= last + 1);
    out
}

/// What one player's fights say about them, over a log or a season.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FightHabits {
    /// Teamfights their side was in at all.
    pub fights: usize,
    /// Of those, the ones they joined.
    pub joined: usize,
    /// Median seconds after the first kill that they arrive.
    pub median_join_s: i64,
    /// Fights their side arrived at together, and how many of those they were
    /// part of: the difference is the player who was elsewhere.
    pub side_together: usize,
    pub together_with_them: usize,
}

/// Reduce a log's fights to one player's habits.
pub fn habits(fights: &[Teamfight], account_id: u32, team: Team) -> FightHabits {
    let mut h = FightHabits::default();
    let mut joins: Vec<i64> = Vec::new();
    for f in fights {
        // Only fights their side actually turned up to.
        if f.arrivals.iter().filter(|a| a.team == team).count() < MIN_SIDE {
            continue;
        }
        h.fights += 1;
        let together = f.together(team).unwrap_or(false);
        if together {
            h.side_together += 1;
        }
        if let Some(a) = f.arrivals.iter().find(|a| a.account_id == account_id) {
            h.joined += 1;
            joins.push(a.joined_at);
            if together {
                h.together_with_them += 1;
            }
        }
    }
    joins.sort_unstable();
    h.median_join_s = joins.get(joins.len() / 2).copied().unwrap_or(0);
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fight(seq: usize, arrivals: &[(u32, Team, i64)]) -> Teamfight {
        Teamfight {
            seq,
            round_num: 1,
            at: 0,
            arrivals: arrivals
                .iter()
                .map(|&(account_id, team, joined_at)| Arrival {
                    account_id,
                    team,
                    joined_at,
                    died_at: None,
                })
                .collect(),
        }
    }

    #[test]
    fn a_side_that_arrives_within_three_seconds_collapsed() {
        let f = fight(
            0,
            &[(1, Team::Blue, 0), (2, Team::Blue, 2), (3, Team::Blue, 3), (9, Team::Red, 0)],
        );
        assert_eq!(f.collapse(Team::Blue), Some(3));
        assert_eq!(f.together(Team::Blue), Some(true));
        // One player is not a side: nothing to say about RED.
        assert_eq!(f.collapse(Team::Red), None);
    }

    #[test]
    fn a_trickle_is_not_a_collapse() {
        let f = fight(0, &[(1, Team::Blue, 0), (2, Team::Blue, 6), (3, Team::Blue, 12)]);
        assert_eq!(f.collapse(Team::Blue), Some(12));
        assert_eq!(f.together(Team::Blue), Some(false));
    }

    #[test]
    fn habits_count_only_the_fights_their_side_turned_up_to() {
        let fights = vec![
            // Three BLU in, and our player among them.
            fight(0, &[(1, Team::Blue, 0), (2, Team::Blue, 1), (3, Team::Blue, 2)]),
            // Three BLU in, our player elsewhere.
            fight(1, &[(2, Team::Blue, 0), (3, Team::Blue, 1), (4, Team::Blue, 2)]),
            // Too few to count either way.
            fight(2, &[(1, Team::Blue, 0), (2, Team::Blue, 1)]),
        ];
        let h = habits(&fights, 1, Team::Blue);
        assert_eq!((h.fights, h.joined), (2, 1), "two fights, one of them theirs");
        assert_eq!(h.side_together, 2, "both were collapses");
        assert_eq!(h.together_with_them, 1, "they were only in one of them");
    }
}
