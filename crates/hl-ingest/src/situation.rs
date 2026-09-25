//! What a kill is worth in its situation (PLAN §12 step 3, TF2's version of
//! HLTV's economy adjustment).
//!
//! A kill's worth is how much it raises its team's chance of winning. For
//! every counted kill, the state just before it is the numbers difference
//! (the killer's team alive minus the victim's, -4 to +4) and the uber
//! advantage (the killer's team has it, nobody does, the victim's team has
//! it). Two chances are measured per state:
//!
//! - `base`: how often a team in that state went on to win, whichever side
//!   got the next kill;
//! - `after`: how often it won when it got the kill.
//!
//! The kill's worth is `after - base`, and the situation factor is that worth
//! divided by the worth of a kill at even numbers with no uber advantage.
//! Winning is measured two ways: the fight (more kills in it; a tie goes to
//! the next cap in the round, or is left out) and the round.

use crate::fights::{analyse, KillTags};
use crate::rawlog::RawLog;
use crate::state::GameState;
use hl_core::matchdata::Team;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;

/// Numbers differences beyond this are counted with it.
pub const MAX_DIFF: i8 = 4;
/// A factor is never below or above these: no kill is worth three kills.
pub const FACTOR_MIN: f64 = 0.5;
pub const FACTOR_MAX: f64 = 1.5;
/// States seen fewer times than this take the factor of the next state
/// toward even numbers.
pub const MIN_SAMPLES: u32 = 200;

/// The state of one kill, from the killer's side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KillState {
    /// Killer's team alive minus the victim's, just before, clamped to ±[`MAX_DIFF`].
    pub diff: i8,
    /// Uber advantage just before: 1 the killer's team, -1 the victim's, 0 neither.
    pub adv: i8,
}

impl KillState {
    fn flip(self) -> Self {
        KillState { diff: -self.diff, adv: -self.adv }
    }
}

/// Per counted kill (aligned with `raw.kills`): its state, and the index of
/// the fight it belongs to within the log.
pub fn kill_states(raw: &RawLog, gs: &GameState, tags: &[Option<KillTags>]) -> Vec<Option<(KillState, usize)>> {
    let mut fight = 0usize;
    let mut first = true;
    tags.iter()
        .enumerate()
        .map(|(i, t)| {
            let t = t.as_ref()?;
            if t.opening && !first {
                fight += 1;
            }
            first = false;
            let k = &raw.kills[i];
            let (kt, vt) = (k.killer.team?, k.victim.team?);
            let n = gs.numbers_at(k.at - 1);
            let alive = |team: Team| i16::from(if team == Team::Red { n[0] } else { n[1] });
            let diff = (alive(kt) - alive(vt)).clamp(-i16::from(MAX_DIFF), i16::from(MAX_DIFF)) as i8;
            let adv = match gs.advantage_at(k.at - 1) {
                Some(t) if t == kt => 1,
                Some(_) => -1,
                None => 0,
            };
            Some((KillState { diff, adv }, fight))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tally {
    /// Team-moments in this state, and how many of those teams won.
    pub seen: u32,
    pub won: u32,
    /// Kills made from this state, and how many of those killers' teams won.
    pub kills: u32,
    pub kills_won: u32,
}

impl Tally {
    pub fn base(&self) -> Option<f64> {
        (self.seen > 0).then(|| f64::from(self.won) / f64::from(self.seen))
    }
    pub fn after(&self) -> Option<f64> {
        (self.kills > 0).then(|| f64::from(self.kills_won) / f64::from(self.kills))
    }
    pub fn worth(&self) -> Option<f64> {
        Some(self.after()? - self.base()?)
    }
}

/// Tallies per state, for winning the fight and winning the round.
#[derive(Debug, Clone, Default)]
pub struct Tallies {
    pub fight: HashMap<KillState, Tally>,
    pub round: HashMap<KillState, Tally>,
    pub logs: usize,
    pub kills: usize,
}

impl Tallies {
    /// Add one log's kills.
    pub fn add(&mut self, raw: &RawLog, gs: &GameState) {
        let tags = analyse(raw, gs).tags;
        let states = kill_states(raw, gs, &tags);
        self.logs += 1;

        // Fight winners: more kills, or the next cap in the round on a tie.
        let mut per_fight: HashMap<usize, (u32, u32, i64, u32)> = HashMap::new();
        for (i, s) in states.iter().enumerate() {
            let Some((_, f)) = s else { continue };
            let k = &raw.kills[i];
            let e = per_fight.entry(*f).or_insert((0, 0, k.at, k.round));
            match k.killer.team {
                Some(Team::Red) => e.0 += 1,
                Some(Team::Blue) => e.1 += 1,
                None => {}
            }
            e.2 = e.2.max(k.at);
        }
        let fight_winner: HashMap<usize, Team> = per_fight
            .into_iter()
            .filter_map(|(f, (red, blue, last, round))| {
                let w = match red.cmp(&blue) {
                    std::cmp::Ordering::Greater => Some(Team::Red),
                    std::cmp::Ordering::Less => Some(Team::Blue),
                    std::cmp::Ordering::Equal => {
                        let end = gs.rounds.iter().find(|r| r.start <= last && r.end.is_none_or(|e| last <= e)).and_then(|r| r.end);
                        gs.caps
                            .iter()
                            .filter(|c| c.at > last && end.is_none_or(|e| c.at <= e) && c.round == round)
                            .min_by_key(|c| c.at)
                            .and_then(|c| c.team)
                    }
                };
                Some((f, w?))
            })
            .collect();

        for (i, s) in states.iter().enumerate() {
            let Some((state, f)) = *s else { continue };
            let k = &raw.kills[i];
            let Some(kt) = k.killer.team else { continue };
            self.kills += 1;
            if let Some(&w) = fight_winner.get(&f) {
                record(&mut self.fight, state, w == kt);
            }
            if let Some(w) = gs.round_at(k.at).and_then(|r| r.winner) {
                record(&mut self.round, state, w == kt);
            }
        }
    }
}

/// One kill: the killer's side saw `state` and got the kill; the victim's
/// side saw the flipped state and did not.
fn record(t: &mut HashMap<KillState, Tally>, state: KillState, killer_won: bool) {
    let k = t.entry(state).or_default();
    k.seen += 1;
    k.won += u32::from(killer_won);
    k.kills += 1;
    k.kills_won += u32::from(killer_won);
    let v = t.entry(state.flip()).or_default();
    v.seen += 1;
    v.won += u32::from(!killer_won);
}

/// The factor per state: each state's worth over the worth at even numbers
/// with no uber advantage, clamped. Thin states drop the advantage, then
/// fall back toward even numbers.
pub fn factors(t: &HashMap<KillState, Tally>) -> HashMap<KillState, f64> {
    let reference = t.get(&KillState { diff: 0, adv: 0 }).and_then(Tally::worth).filter(|w| *w > 0.0);
    let Some(reference) = reference else { return HashMap::new() };
    let mut out = HashMap::new();
    for adv in -1..=1 {
        for diff in -MAX_DIFF..=MAX_DIFF {
            // Drop the uber advantage, then walk toward even numbers, until a state has enough kills.
            let mut s = KillState { diff, adv };
            let f = loop {
                match t.get(&s) {
                    Some(x) if x.kills >= MIN_SAMPLES => break x.worth().map_or(1.0, |w| w / reference),
                    _ if s.adv != 0 => s.adv = 0,
                    _ if s.diff != 0 => s.diff -= s.diff.signum(),
                    _ => break 1.0,
                }
            };
            out.insert(KillState { diff, adv }, f.clamp(FACTOR_MIN, FACTOR_MAX));
        }
    }
    out
}

/// The factors as a `[situation]` table for `weights.default.toml`.
pub fn toml_table(f: &HashMap<KillState, f64>, source: &str) -> String {
    table("situation", f, source, 2, 1.0)
}

/// What a kill in each state is worth as a change in win chance, rather than
/// as a factor against the even-numbers kill (PLAN §12 step 5).
///
/// The same measurement the factors come from, left in the units it was made
/// in: at even numbers with no uber a kill takes its team from 50.0% to
/// 69.2%, so it is worth 0.192. Nothing is normalised and nothing is clamped
/// — a clean-up at four up really is worth 0.047, and saying so out loud is
/// the point of the component.
pub fn swings(t: &HashMap<KillState, Tally>) -> HashMap<KillState, f64> {
    let mut out = HashMap::new();
    for adv in -1..=1 {
        for diff in -MAX_DIFF..=MAX_DIFF {
            // A thin state borrows from the next one toward even numbers, as
            // the factors do: a handful of kills is not a win chance.
            let mut at = KillState { diff, adv };
            let worth = loop {
                match t.get(&at) {
                    Some(x) if x.kills >= MIN_SAMPLES => break x.worth().unwrap_or(0.0),
                    _ if at.diff != 0 => at.diff -= at.diff.signum(),
                    _ => break 0.0,
                }
            };
            out.insert(KillState { diff, adv }, worth);
        }
    }
    out
}

/// The swings as a `[swing]` table for `weights.default.toml`.
pub fn swing_table(f: &HashMap<KillState, f64>, source: &str) -> String {
    table("swing", f, source, 3, 0.0)
}

fn table(name: &str, f: &HashMap<KillState, f64>, source: &str, dp: usize, default: f64) -> String {
    let row = |adv: i8| {
        (-MAX_DIFF..=MAX_DIFF)
            .map(|diff| format!("{:.dp$}", f.get(&KillState { diff, adv }).copied().unwrap_or(default)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "[{name}]\n# {source}\n# Columns: the killer's team alive minus the victim's, -4 to +4.\ntheirs = [{}]\nnone   = [{}]\nours   = [{}]\n",
        row(-1),
        row(0),
        row(1)
    )
}

/// Read every stored raw log.
pub async fn measure(db: &hl_db::Db) -> anyhow::Result<Tallies> {
    let mut t = Tallies::default();
    for log_id in db.rawlog_ids().await? {
        let Some(zip) = db.rawlog(log_id).await? else { continue };
        let raw = crate::rawlog::parse(&crate::rawlog::unzip(&zip)?);
        let gs = GameState::build(&raw);
        t.add(&raw, &gs);
    }
    Ok(t)
}

impl fmt::Display for Tallies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} logs, {} kills", self.logs, self.kills)?;
        for (name, t) in [("fight", &self.fight), ("round", &self.round)] {
            let fac = factors(t);
            writeln!(f, "\nWinning the {name}:")?;
            writeln!(f, "{:>5} {:>4} {:>8} {:>7} {:>7} {:>7} {:>7} {:>6}", "diff", "adv", "kills", "base", "after", "worth", "raw", "factor")?;
            let reference = t.get(&KillState { diff: 0, adv: 0 }).and_then(Tally::worth);
            for adv in -1..=1 {
                for diff in -MAX_DIFF..=MAX_DIFF {
                    let s = KillState { diff, adv };
                    let x = t.get(&s).copied().unwrap_or_default();
                    let pct = |v: Option<f64>| v.map_or("-".into(), |v| format!("{:.1}%", v * 100.0));
                    let raw = x.worth().zip(reference).map_or("-".into(), |(w, r)| format!("{:.2}", w / r));
                    writeln!(
                        f,
                        "{:>+5} {:>+4} {:>8} {:>7} {:>7} {:>7} {:>7} {:>6.2}",
                        diff,
                        adv,
                        x.kills,
                        pct(x.base()),
                        pct(x.after()),
                        pct(x.worth()),
                        raw,
                        fac.get(&s).copied().unwrap_or(1.0)
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(kills: u32, kills_won: u32, seen: u32, won: u32) -> Tally {
        Tally { seen, won, kills, kills_won }
    }

    #[test]
    fn a_kill_records_both_sides() {
        let mut t = HashMap::new();
        record(&mut t, KillState { diff: 2, adv: 1 }, true);
        let k = t[&KillState { diff: 2, adv: 1 }];
        let v = t[&KillState { diff: -2, adv: -1 }];
        assert_eq!((k.seen, k.won, k.kills, k.kills_won), (1, 1, 1, 1));
        assert_eq!((v.seen, v.won, v.kills), (1, 0, 0), "the victim's side saw the flipped state and lost");
    }

    #[test]
    fn factors_are_relative_to_an_even_kill_clamped_and_fall_back_when_thin() {
        let mut t = HashMap::new();
        // Even: a kill lifts the win chance from 50% to 70%.
        t.insert(KillState { diff: 0, adv: 0 }, tally(1000, 700, 2000, 1000));
        // Up two: 80% to 85%, a quarter of an even kill: clamped to 0.5.
        t.insert(KillState { diff: 2, adv: 0 }, tally(1000, 850, 2000, 1600));
        // Down one: 35% to 60%, 1.25 of an even kill.
        t.insert(KillState { diff: -1, adv: 0 }, tally(1000, 600, 2000, 700));
        // Down two, only 50 kills: takes down one's factor.
        t.insert(KillState { diff: -2, adv: 0 }, tally(50, 50, 100, 0));
        let f = factors(&t);
        assert!((f[&KillState { diff: 0, adv: 0 }] - 1.0).abs() < 1e-9);
        assert_eq!(f[&KillState { diff: 2, adv: 0 }], FACTOR_MIN);
        assert!((f[&KillState { diff: -1, adv: 0 }] - 1.25).abs() < 1e-9);
        assert!((f[&KillState { diff: -2, adv: 0 }] - 1.25).abs() < 1e-9);
        // Nothing at all with an uber advantage: back to even, no advantage.
        assert!((f[&KillState { diff: 3, adv: 1 }] - 0.5).abs() < 1e-9, "walks to +2, no advantage, then clamps");
        assert!(toml_table(&f, "test").contains("none   = [1.25, 1.25, 1.25, 1.25, 1.00, 1.00, 0.50, 0.50, 0.50]"));
    }
}

// ---------------------------------------------------------------------------
// What killing each class is worth, per kind of map (Q4, second attempt).
//
// boSe's claim is that a Scout pick on KOTH is worth more than elsewhere:
// on KOTH the Scout is in the übered push rather than off on a flank, so
// killing him costs the enemy the push. The first attempt tested it the long
// way round, by changing `victim_value` and asking whether the *Sniper*
// rating picked more winners. It could not have worked: Scout kills are a
// ninth of a Sniper's, reaching the rating through one component.
//
// This measures the claim directly. For every counted kill, the state just
// before it says what the killer's team's chance of winning the round was.
// Whether they went on to win it is known. The difference, averaged over
// every kill on a class, is what killing that class is worth *over what the
// situation alone predicted* — so a class that tends to die in clean-ups is
// not credited with the clean-up.
// ---------------------------------------------------------------------------

/// The kinds of map a Highlander season is played on. KOTH and stopwatch are
/// different games, which is the whole of boSe's point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Koth,
    Stopwatch,
    Other,
}

impl Mode {
    pub fn of(map: &str) -> Mode {
        let m = map.to_ascii_lowercase();
        if m.starts_with("koth_") {
            Mode::Koth
        } else if m.starts_with("pl_") || m.starts_with("cp_steel") || m.starts_with("cp_gravelpit") {
            Mode::Stopwatch
        } else {
            Mode::Other
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Koth => "koth",
            Mode::Stopwatch => "stopwatch",
            Mode::Other => "other",
        }
    }
}

/// Kills on one class on one kind of map, and how they went.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worth {
    pub kills: u32,
    /// Of those, the ones whose killer's team won the round.
    pub won: u32,
    /// Summed chance of winning the round that the state alone gave them.
    pub expected: f64,
}

impl Worth {
    /// How often the killer's team won, less how often the situation said
    /// they would: the part of the outcome the kill can be credited with.
    pub fn excess(&self) -> Option<f64> {
        (self.kills >= 200).then(|| {
            (f64::from(self.won) - self.expected) / f64::from(self.kills)
        })
    }
}

/// Per victim class and kind of map.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VictimWorth {
    pub by: HashMap<(hl_core::TfClass, Mode), Worth>,
    pub logs: usize,
    pub unmapped: usize,
}

/// Measure it over every stored raw log.
///
/// The baseline is per mode, not pooled. A kill converts into a round win far
/// more often on KOTH than on stopwatch whoever it lands on — KOTH rounds are
/// short and decided by a fight, stopwatch rounds are long and decided by a
/// clock — and a pooled baseline turns that into a fake premium on every
/// class at once. Measured against its own mode, what is left is the class.
pub async fn victim_worth(db: &hl_db::Db) -> anyhow::Result<VictimWorth> {
    let maps = db.all_round_map_names().await?;
    let mut out = VictimWorth::default();

    // One pass over the logs; the baseline needs every kill before any
    // excess can be worked out, and parsing them twice costs eight seconds.
    let mut seen: Vec<(Mode, KillState, hl_core::TfClass, bool)> = Vec::new();
    for log_id in db.rawlog_ids().await? {
        let Some(zip) = db.rawlog(log_id).await? else { continue };
        let raw = crate::rawlog::parse(&crate::rawlog::unzip(&zip)?);
        let gs = GameState::build(&raw);
        let tags = analyse(&raw, &gs).tags;
        let states = kill_states(&raw, &gs, &tags);
        let rounds = maps.get(&log_id);
        out.logs += 1;

        for (i, s) in states.iter().enumerate() {
            let Some((state, _)) = *s else { continue };
            let k = &raw.kills[i];
            let (Some(kt), Some(vc)) = (k.killer.team, k.victim.class) else { continue };
            let Some(round) = gs.round_at(k.at) else { continue };
            let Some(won) = round.winner else { continue };
            // The map this kill happened on, which in a combined log is the
            // round's map and not the log's name. Matched by round number:
            // the raw log's clock is the server's and does not line up with
            // the round windows logs.tf reports.
            let Some(map) = rounds.and_then(|m| m.get(&i64::from(round.num))) else {
                out.unmapped += 1;
                continue;
            };
            seen.push((Mode::of(map), state, vc, won == kt));
        }
    }

    // The baseline, per mode: of every team-moment in a state, how often that
    // team won the round. Each kill is two moments — the killer's, and the
    // victim's seeing the state flipped and not getting the kill.
    let mut base: HashMap<(Mode, KillState), (u32, u32)> = HashMap::new();
    for &(mode, state, _, won) in &seen {
        let a = base.entry((mode, state)).or_default();
        a.0 += 1;
        a.1 += u32::from(won);
        let b = base.entry((mode, state.flip())).or_default();
        b.0 += 1;
        b.1 += u32::from(!won);
    }

    for (mode, state, vc, won) in seen {
        let Some(&(n, w)) = base.get(&(mode, state)) else { continue };
        let e = out.by.entry((vc, mode)).or_default();
        e.kills += 1;
        e.won += u32::from(won);
        e.expected += f64::from(w) / f64::from(n);
    }
    Ok(out)
}

impl fmt::Display for VictimWorth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} logs; what killing each class was worth over what the situation predicted\n\
             ({} kills skipped: the round's map is unknown)\n",
            self.logs, self.unmapped
        )?;
        writeln!(f, "{:<10} {:>18} {:>18} {:>10}", "victim", "KOTH", "stopwatch", "KOTH -")?;
        writeln!(f, "{:<10} {:>18} {:>18} {:>10}", "", "kills   excess", "kills   excess", "stopwatch")?;
        let mut rows: Vec<_> = hl_core::TfClass::ALL.to_vec();
        rows.sort_by_key(|c| c.as_str());
        for c in rows {
            let k = self.by.get(&(c, Mode::Koth)).copied().unwrap_or_default();
            let s = self.by.get(&(c, Mode::Stopwatch)).copied().unwrap_or_default();
            let cell = |w: Worth| match w.excess() {
                Some(e) => format!("{:>6}  {:>+7.2}%", w.kills, e * 100.0),
                None => format!("{:>6}  {:>8}", w.kills, "–"),
            };
            let diff = match (k.excess(), s.excess()) {
                (Some(a), Some(b)) => format!("{:>+9.2}%", (a - b) * 100.0),
                _ => format!("{:>10}", "–"),
            };
            writeln!(f, "{:<10} {} {} {}", c.as_str(), cell(k), cell(s), diff)?;
        }
        Ok(())
    }
}
