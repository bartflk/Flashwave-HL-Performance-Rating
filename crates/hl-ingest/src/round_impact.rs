//! Impact: the round-win chance a player's kills added and their deaths gave
//! away (the MVP research, October 2026; HLTV's "Round Swing").
//!
//! Every counted kill moves the killer's side from `own v enemy` alive to
//! `own v enemy-1`. How much that is worth is read from a table of how often
//! a side at those numbers, with or without the uber advantage, went on to
//! win the round: [`WinTable`], measured on the league sample's officials
//! (`hl impact measure`). The killer is credited with the change (three
//! quarters of it when someone assisted, the assister the rest) and the
//! player who died loses it, so a side's impact over a round sums to its
//! share of the result.
//!
//! This is not the rating's fight swing (`kills.rs`): that values a kill by
//! winning the *fight*, counts kills only, and is one part of the rating.
//! Impact is shown beside the rating and never feeds it.
//!
//! Measured on 6,157 officials (21 October 2026): a season's impact agrees
//! with itself between odd and even officials better than the rating does
//! (0.71 against 0.61 at 8+ maps), and tracks the rating within each class
//! (0.77 to 0.87; Spy 0.40).

use crate::rawlog::RawLog;
use crate::state::{GameState, Round};
use hl_core::matchdata::Team;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The most players a side is counted with: Highlander is 9.
const MAX: usize = 9;
/// Kills of one player this close together are one burst (a multi-kill).
const BURST_S: i64 = 5;
/// Share of a kill's value an assister takes.
const ASSIST_SHARE: f64 = 0.25;

/// `P(the side wins the round | own alive, enemy alive, uber advantage)`.
/// Index `[adv][own][enemy]`, adv 0 = ours, 1 = nobody's, 2 = theirs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WinTable {
    pub kills: u64,
    pub p: Vec<Vec<Vec<f64>>>,
}

impl WinTable {
    /// The measured table shipped with the app.
    pub fn shipped() -> &'static WinTable {
        static T: OnceLock<WinTable> = OnceLock::new();
        T.get_or_init(|| serde_json::from_str(include_str!("round_win.json")).expect("round_win.json"))
    }

    pub fn p(&self, adv: usize, own: usize, enemy: usize) -> f64 {
        self.p[adv][own.min(MAX)][enemy.min(MAX)]
    }

    /// The table from tallies: each cell is pooled with its mirror (a side
    /// at 7v6 with the advantage is the other side at 6v7 without it) and
    /// pulled towards a logistic curve on the difference, so a rare cell
    /// cannot swing on a handful of rounds.
    pub fn from_tally(t: &Tally) -> WinTable {
        let prior_n = 20.0;
        let mut p = vec![vec![vec![0.5; MAX + 1]; MAX + 1]; 3];
        for adv in 0..3 {
            let mirror = 2 - adv;
            for own in 0..=MAX {
                for enemy in 0..=MAX {
                    let (w1, n1) = t.cells[adv][own][enemy];
                    let (w2, n2) = t.cells[mirror][enemy][own];
                    let wins = w1 as f64 + (n2 - w2) as f64;
                    let n = (n1 + n2) as f64;
                    let tilt = match adv {
                        0 => 0.35,
                        2 => -0.35,
                        _ => 0.0,
                    };
                    let prior = 1.0 / (1.0 + (-(0.45 * (own as f64 - enemy as f64) + tilt)).exp());
                    p[adv][own][enemy] = (wins + prior_n * prior) / (n + prior_n);
                }
            }
        }
        WinTable { kills: t.kills, p }
    }
}

/// Counts for measuring a [`WinTable`]: `(won, seen)` per state.
#[derive(Debug, Clone)]
pub struct Tally {
    pub kills: u64,
    pub cells: Vec<Vec<Vec<(u64, u64)>>>,
}

impl Default for Tally {
    fn default() -> Self {
        Tally { kills: 0, cells: vec![vec![vec![(0, 0); MAX + 1]; MAX + 1]; 3] }
    }
}

/// One counted kill in context.
#[derive(Debug, Clone, Copy)]
struct Moment {
    at: i64,
    killer: u32,
    victim: u32,
    assister: Option<u32>,
    killer_team: Team,
    /// Alive before the kill, from the killer's side.
    own: usize,
    enemy: usize,
    /// 0 the killer's side had the uber advantage, 1 nobody, 2 the victim's.
    adv: usize,
    winner: Option<Team>,
}

/// Every counted kill with the numbers before it. `counted` is aligned with
/// `raw.kills` (the fights pass's tags: duplicates and feigns left out).
fn moments(raw: &RawLog, gs: &GameState, counted: &dyn Fn(usize) -> bool) -> Vec<Moment> {
    let mut out = Vec::new();
    // Deaths already taken at the same second, per colour, so two kills in
    // one second do not both see the full numbers.
    let mut same_second: (i64, [usize; 2]) = (i64::MIN, [0, 0]);
    for (i, k) in raw.kills.iter().enumerate() {
        if !counted(i) || !k.counts() {
            continue;
        }
        let (Some(kt), Some(vt)) = (k.killer.team, k.victim.team) else { continue };
        if kt == vt {
            continue;
        }
        let Some(round) = gs.round_at(k.at) else { continue };
        if same_second.0 != k.at {
            same_second = (k.at, [0, 0]);
        }
        let n = gs.numbers_at(k.at - 1);
        let alive = |t: Team| (n[idx(t)] as usize).saturating_sub(same_second.1[idx(t)]);
        let (own, enemy) = (alive(kt), alive(vt));
        same_second.1[idx(vt)] += 1;
        if enemy == 0 {
            continue;
        }
        let adv = match gs.advantage_at(k.at - 1) {
            Some(t) if t == kt => 0,
            Some(_) => 2,
            None => 1,
        };
        out.push(Moment { at: k.at, killer: k.killer.account, victim: k.victim.account, assister: k.assister, killer_team: kt, own, enemy, adv, winner: round.winner });
    }
    out
}

fn idx(t: Team) -> usize {
    match t {
        Team::Red => 0,
        Team::Blue => 1,
    }
}

/// Adds one log's kills to a tally (rounds with a winner only).
pub fn tally(raw: &RawLog, gs: &GameState, counted: &dyn Fn(usize) -> bool, t: &mut Tally) {
    for m in moments(raw, gs, counted) {
        let Some(w) = m.winner else { continue };
        let c = &mut t.cells[m.adv][m.own.min(MAX)][m.enemy.min(MAX)];
        c.0 += (w == m.killer_team) as u64;
        c.1 += 1;
        t.kills += 1;
    }
}

/// One player's impact in one log, and what goes with it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerImpact {
    /// Round-win chance added, in rounds: +1.0 is one round's worth.
    pub impact: f64,
    /// The same, on attack and on defence (payload and attack/defend maps:
    /// the attackers are always BLU). Zero on symmetric maps.
    pub impact_attack: f64,
    pub impact_defence: f64,
    pub kills_attack: u32,
    pub deaths_attack: u32,
    pub kills_defence: u32,
    pub deaths_defence: u32,
    /// Seconds of rounds played on each side.
    pub secs_attack: i64,
    pub secs_defence: i64,
    /// Kills made with the own side a player or more down.
    pub down_kills: u32,
    /// Bursts of 2 kills, and of 3 or more, each within 5 s of the last.
    pub multi2: u32,
    pub multi3: u32,
}

/// Every player's impact in one log.
pub fn impacts(raw: &RawLog, gs: &GameState, counted: &dyn Fn(usize) -> bool, attack_defend: bool, table: &WinTable) -> HashMap<u32, PlayerImpact> {
    let mut out: HashMap<u32, PlayerImpact> = HashMap::new();
    let ms = moments(raw, gs, counted);
    let mut times: HashMap<u32, Vec<i64>> = HashMap::new();
    for m in &ms {
        let before = table.p(m.adv, m.own, m.enemy);
        let after = table.p(m.adv, m.own, m.enemy - 1);
        let gain = after - before;
        let share = if m.assister.is_some() { 1.0 - ASSIST_SHARE } else { 1.0 };
        let attacking = m.killer_team == Team::Blue;
        {
            let k = out.entry(m.killer).or_default();
            k.impact += gain * share;
            if attack_defend {
                if attacking {
                    k.impact_attack += gain * share;
                    k.kills_attack += 1;
                } else {
                    k.impact_defence += gain * share;
                    k.kills_defence += 1;
                }
            }
            if m.own < m.enemy {
                k.down_kills += 1;
            }
        }
        if let Some(a) = m.assister {
            let a = out.entry(a).or_default();
            a.impact += gain * ASSIST_SHARE;
            if attack_defend {
                if attacking {
                    a.impact_attack += gain * ASSIST_SHARE;
                } else {
                    a.impact_defence += gain * ASSIST_SHARE;
                }
            }
        }
        let v = out.entry(m.victim).or_default();
        v.impact -= gain;
        if attack_defend {
            // The victim was on the other side of the attack.
            if attacking {
                v.impact_defence -= gain;
                v.deaths_defence += 1;
            } else {
                v.impact_attack -= gain;
                v.deaths_attack += 1;
            }
        }
        times.entry(m.killer).or_default().push(m.at);
    }
    for (p, ts) in times {
        let e = out.entry(p).or_default();
        let mut run = 1;
        for w in ts.windows(2) {
            if w[1] - w[0] <= BURST_S {
                run += 1;
            } else {
                count_burst(e, run);
                run = 1;
            }
        }
        count_burst(e, run);
    }
    if attack_defend {
        side_time(gs, &mut out);
    }
    out
}

fn count_burst(e: &mut PlayerImpact, run: u32) {
    match run {
        2 => e.multi2 += 1,
        r if r >= 3 => e.multi3 += 1,
        _ => {}
    }
}

/// Seconds of rounds each player spent on attack (BLU) and defence (RED):
/// a round counts for the colour the player had most of their time alive in it.
fn side_time(gs: &GameState, out: &mut HashMap<u32, PlayerImpact>) {
    for r in &gs.rounds {
        let Some(len) = round_length(r) else { continue };
        let mut by: HashMap<u32, [i64; 2]> = HashMap::new();
        let end = r.end.unwrap_or(r.start);
        for l in gs.lives.iter().filter(|l| l.from < end && l.to > r.start) {
            let t = l.to.min(end) - l.from.max(r.start);
            by.entry(l.account).or_default()[idx(l.team)] += t.max(0);
        }
        for (p, [red, blue]) in by {
            let e = out.entry(p).or_default();
            if blue >= red {
                e.secs_attack += len;
            } else {
                e.secs_defence += len;
            }
        }
    }
}

/// Measures the table on every league sample log with a raw server log.
pub async fn measure(db: &hl_db::Db, mut progress: impl FnMut(usize, usize)) -> anyhow::Result<WinTable> {
    let ids = db.league_rateable().await?;
    let mut t = Tally::default();
    for (i, log_id) in ids.iter().enumerate() {
        let (_, Some(zip)) = db.league_log_files(*log_id).await? else { continue };
        let part = tokio::task::spawn_blocking(move || -> anyhow::Result<Tally> {
            let raw = crate::rawlog::parse(&crate::rawlog::unzip(&zip)?);
            let gs = GameState::build(&raw);
            let f = crate::fights::analyse(&raw, &gs);
            let mut t = Tally::default();
            tally(&raw, &gs, &|i| f.tags.get(i).is_some_and(Option::is_some), &mut t);
            Ok(t)
        })
        .await?;
        if let Ok(p) = part {
            t.kills += p.kills;
            for a in 0..3 {
                for o in 0..=MAX {
                    for e in 0..=MAX {
                        t.cells[a][o][e].0 += p.cells[a][o][e].0;
                        t.cells[a][o][e].1 += p.cells[a][o][e].1;
                    }
                }
            }
        }
        progress(i + 1, ids.len());
    }
    Ok(WinTable::from_tally(&t))
}

fn round_length(r: &Round) -> Option<i64> {
    Some(r.end? - r.start).filter(|l| *l > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_table_is_sane() {
        let t = WinTable::shipped();
        assert!(t.kills > 100_000, "measured on the league sample");
        // Even numbers, nobody's advantage: a coin flip.
        assert!((t.p(1, 9, 9) - 0.5).abs() < 0.02);
        // More alive is better, and so is the advantage.
        assert!(t.p(1, 9, 8) > t.p(1, 9, 9));
        assert!(t.p(1, 5, 4) > t.p(1, 9, 8), "a late pick is worth more");
        assert!(t.p(0, 9, 9) > t.p(1, 9, 9) && t.p(1, 9, 9) > t.p(2, 9, 9));
        // Mirrors agree.
        assert!((t.p(0, 7, 6) + t.p(2, 6, 7) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_kill_moves_the_killer_up_and_the_victim_down_by_the_same() {
        let raw = crate::rawlog::parse(SAMPLE);
        let gs = GameState::build(&raw);
        let m = impacts(&raw, &gs, &|_| true, true, WinTable::shipped());
        let k = m[&1];
        let v = m[&2];
        assert!(k.impact > 0.0);
        assert!((k.impact + v.impact).abs() < 1e-9, "zero-sum without an assist");
        // BLU killed RED on an attack/defend map: an attack kill, a defence death.
        assert_eq!((k.kills_attack, v.deaths_defence), (1, 1));
    }

    const SAMPLE: &str = r#"L 10/01/2026 - 20:00:00: World triggered "Round_Start"
L 10/01/2026 - 20:00:00: "a<1><[U:1:1]><Blue>" spawned as "Scout"
L 10/01/2026 - 20:00:00: "b<2><[U:1:2]><Red>" spawned as "Scout"
L 10/01/2026 - 20:00:00: "c<3><[U:1:3]><Red>" spawned as "Soldier"
L 10/01/2026 - 20:00:10: "a<1><[U:1:1]><Blue>" killed "b<2><[U:1:2]><Red>" with "scattergun" (attacker_position "0 0 0") (victim_position "0 0 0")
L 10/01/2026 - 20:01:00: World triggered "Round_Win" (winner "Blue")
"#;
}
