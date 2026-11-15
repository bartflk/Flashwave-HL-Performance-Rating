//! 6v6's round-win table and each kill's round impact (the 6v6 plan, P6).
//!
//! Highlander's rating is half round impact: the round-win chance each
//! kill added and each death gave away, measured on a million league kills
//! (`round_impact.rs`). 6v6 needs its own table -- six a side, not nine --
//! measured on the 6v6 league sample's raw logs.
//!
//! **The table** ([`measure`]): at every kill, the players alive on each side
//! just after it, from the raw log's spawns and deaths; `W[d]` is the share
//! of those moments, from either side's view, where the side `d` players up
//! went on to win the round. [`MIN_ROUNDS`] rounds before it is trusted.
//!
//! **A kill's worth** ([`impacts`]): `W[after] - W[before]` for the killer's
//! side, and the same taken from the victim; summed per player.
//!
//! **What killing each class is worth** (`WinTable::victims`): how much more
//! often the killer's side went on to win the round than the state alone
//! says, per victim class, scaled so the average kill is 1 (the plan: "a
//! Medic pick even more decisive than in Highlander", measured, not set).
//!
//! Pure; the table is stored as a setting once measured.

use crate::rawlog::{EventKind, RawLog};
use hl_core::matchdata::Team;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Rounds the table needs before the rating uses it.
pub const MIN_ROUNDS: usize = 300;
/// Players up or down, -5 to +5.
const SPAN: i32 = 5;
pub const SETTING: &str = "sixes_round_win";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WinTable {
    /// `W[d + 5]`: the chance the side `d` players up wins the round.
    pub by_diff: Vec<f64>,
    pub rounds: usize,
    pub kills: usize,
    /// Each victim class's value, the average kill 1; empty until measured.
    #[serde(default)]
    pub victims: std::collections::BTreeMap<String, f64>,
}

impl WinTable {
    pub fn w(&self, diff: i32) -> f64 {
        self.by_diff[(diff.clamp(-SPAN, SPAN) + SPAN) as usize]
    }
}

/// One counted kill with the alive counts around it: `(killer, victim,
/// killer's side, round, alive before for the killer's side, for the
/// victim's)`.
struct KillState {
    killer: u32,
    victim: u32,
    victim_class: Option<hl_core::TfClass>,
    side: Team,
    round: u32,
    us: i32,
    them: i32,
}

/// Every counted kill between the two sides, with who was alive, and each
/// round's winner.
fn walk(raw: &RawLog) -> (Vec<KillState>, HashMap<u32, Team>) {
    let mut alive: HashMap<Team, HashSet<u32>> = HashMap::new();
    let mut winners: HashMap<u32, Team> = HashMap::new();
    let mut round = 0u32;
    // Kills and events in time order: events carry how many kills came
    // before them, so the two lists interleave exactly.
    let mut ei = 0;
    let mut out = Vec::new();
    let mut apply_events = |upto: usize, ei: &mut usize, alive: &mut HashMap<Team, HashSet<u32>>, round: &mut u32| {
        while *ei < raw.events.len() && raw.events[*ei].after_kills <= upto {
            match &raw.events[*ei].kind {
                EventKind::RoundStart => {
                    alive.clear();
                    *round += 1;
                }
                EventKind::Spawn(a) => {
                    if let Some(t) = a.team {
                        for s in alive.values_mut() {
                            s.remove(&a.account);
                        }
                        alive.entry(t).or_default().insert(a.account);
                    }
                }
                EventKind::Suicide(a) => {
                    for s in alive.values_mut() {
                        s.remove(&a.account);
                    }
                }
                EventKind::Left(acc) => {
                    for s in alive.values_mut() {
                        s.remove(acc);
                    }
                }
                EventKind::RoundWin(Some(t)) => {
                    winners.insert(*round, *t);
                }
                _ => {}
            }
            *ei += 1;
        }
    };
    for (i, k) in raw.kills.iter().enumerate() {
        apply_events(i, &mut ei, &mut alive, &mut round);
        let (Some(side), Some(vside)) = (k.killer.team, k.victim.team) else { continue };
        if side == vside || !k.counts() {
            continue;
        }
        let us = alive.get(&side).map_or(0, |s| s.len()) as i32;
        let them = alive.get(&vside).map_or(0, |s| s.len()) as i32;
        out.push(KillState { killer: k.killer.account, victim: k.victim.account, victim_class: k.victim.class, side, round, us, them });
        if let Some(s) = alive.get_mut(&vside) {
            s.remove(&k.victim.account);
        }
    }
    apply_events(usize::MAX, &mut ei, &mut alive, &mut round);
    (out, winners)
}

/// The table from many raw logs; `None` until [`MIN_ROUNDS`] rounds.
pub fn measure<'a>(raws: impl IntoIterator<Item = &'a RawLog> + Clone) -> Option<WinTable> {
    let mut won = vec![0f64; (2 * SPAN + 1) as usize];
    let mut seen = vec![0f64; (2 * SPAN + 1) as usize];
    let (mut rounds, mut kills) = (0, 0);
    for raw in raws.clone() {
        let (states, winners) = walk(raw);
        rounds += winners.len();
        for k in states {
            let Some(winner) = winners.get(&k.round) else { continue };
            kills += 1;
            // Just after the kill, from both sides' view.
            let d = (k.us - (k.them - 1)).clamp(-SPAN, SPAN);
            let i = (d + SPAN) as usize;
            let j = (-d + SPAN) as usize;
            seen[i] += 1.0;
            seen[j] += 1.0;
            if *winner == k.side {
                won[i] += 1.0;
            } else {
                won[j] += 1.0;
            }
        }
    }
    if rounds < MIN_ROUNDS {
        return None;
    }
    // A state never seen takes its neighbour's, then the even half.
    let mut by: Vec<f64> = won.iter().zip(&seen).map(|(w, s)| if *s > 0.0 { w / s } else { f64::NAN }).collect();
    let mid = SPAN as usize;
    if by[mid].is_nan() {
        by[mid] = 0.5;
    }
    for i in (0..mid).rev() {
        if by[i].is_nan() {
            by[i] = by[i + 1];
        }
    }
    for i in mid + 1..by.len() {
        if by[i].is_nan() {
            by[i] = by[i - 1];
        }
    }
    let mut table = WinTable { by_diff: by.into_iter().map(|x| (x * 1000.0).round() / 1000.0).collect(), rounds, kills, victims: Default::default() };
    table.victims = victim_values(raws, &table);
    Some(table)
}

/// Kills a class needs before its value is trusted.
const MIN_VICTIM_KILLS: usize = 200;

/// Each victim class's value: the killer's side's round wins over what the
/// state after the kill predicts, plus that prediction, over its average
/// across all kills -- so 1 is an ordinary kill. Classes with fewer than
/// [`MIN_VICTIM_KILLS`] kills are left out (counted as 1).
fn victim_values<'a>(raws: impl IntoIterator<Item = &'a RawLog>, table: &WinTable) -> std::collections::BTreeMap<String, f64> {
    let mut by: HashMap<hl_core::TfClass, (f64, usize)> = HashMap::new();
    let (mut all, mut n) = (0f64, 0usize);
    for raw in raws {
        let (states, winners) = walk(raw);
        for k in states {
            let (Some(c), Some(w)) = (k.victim_class, winners.get(&k.round)) else { continue };
            let after = k.us - (k.them - 1);
            // What the kill was worth: the round-win chance it bought, plus
            // how far the result beat the state's prediction.
            let worth = (table.w(after) - table.w(k.us - k.them)) + (if *w == k.side { 1.0 } else { 0.0 } - table.w(after));
            let e = by.entry(c).or_default();
            e.0 += worth;
            e.1 += 1;
            all += worth;
            n += 1;
        }
    }
    let mean = if n > 0 { all / n as f64 } else { 0.0 };
    if mean <= 0.0 {
        return Default::default();
    }
    by.into_iter()
        .filter(|(_, (_, k))| *k >= MIN_VICTIM_KILLS)
        .map(|(c, (sum, k))| (c.as_str().to_string(), (((sum / k as f64) / mean).clamp(0.25, 4.0) * 100.0).round() / 100.0))
        .collect()
}

/// Each player's round impact in one raw log: the round-win chance their
/// kills added, less what their deaths gave away.
pub fn impacts(raw: &RawLog, table: &WinTable) -> HashMap<u32, f64> {
    let (states, _) = walk(raw);
    let mut out: HashMap<u32, f64> = HashMap::new();
    for k in states {
        let before = k.us - k.them;
        let after = k.us - (k.them - 1);
        let gain = table.w(after) - table.w(before);
        *out.entry(k.killer).or_default() += gain;
        *out.entry(k.victim).or_default() -= gain;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A round: everyone spawns, then `kills` in order, then a winner.
    fn round(kills: &[(&str, &str, &str, &str)], winner: &str) -> String {
        let mut l = vec![r#"L 11/13/2026 - 20:00:00: World triggered "Round_Start""#.to_string()];
        for i in 1..=6 {
            l.push(format!(r#"L 11/13/2026 - 20:00:01: "r{i}<{i}><[U:1:{i}]><Red>" spawned as "soldier""#));
            l.push(format!(r#"L 11/13/2026 - 20:00:01: "b{i}<{0}><[U:1:{0}]><Blue>" spawned as "soldier""#, 100 + i));
        }
        for (k, kt, v, vt) in kills {
            l.push(format!(r#"L 11/13/2026 - 20:00:30: "{k}<1><[U:1:{k}]><{kt}>" killed "{v}<2><[U:1:{v}]><{vt}>" with "quake_rl" (attacker_position "0 0 0") (victim_position "1 1 1")"#));
        }
        l.push(format!(r#"L 11/13/2026 - 20:01:00: World triggered "Round_Win" (winner "{winner}")"#));
        l.join("\n")
    }

    #[test]
    fn the_side_that_gets_the_first_pick_and_wins_makes_up_its_chance_and_the_kill_is_worth_it() {
        // Red picks first and wins, 300 times: one up wins every time.
        let text: Vec<String> = (0..300).map(|_| round(&[("1", "Red", "101", "Blue")], "Red")).collect();
        let raw = crate::rawlog::parse(&text.join("\n"));
        let table = measure([&raw]).expect("300 rounds");
        assert_eq!((table.rounds, table.kills), (300, 300));
        assert_eq!(table.victims.get("soldier"), Some(&1.0), "one victim class: the average kill");
        assert_eq!((table.w(1), table.w(-1)), (1.0, 0.0), "one up won them all, one down lost them all");
        assert_eq!(table.w(0), 0.5, "never seen: even");
        let one = crate::rawlog::parse(&round(&[("1", "Red", "101", "Blue")], "Red"));
        let i = impacts(&one, &table);
        assert!((i[&1] - 0.5).abs() < 1e-9 && (i[&101] + 0.5).abs() < 1e-9, "{i:?}");
    }

    #[test]
    fn too_few_rounds_give_no_table() {
        let raw = crate::rawlog::parse(&round(&[("1", "Red", "101", "Blue")], "Red"));
        assert!(measure([&raw]).is_none());
    }
}
