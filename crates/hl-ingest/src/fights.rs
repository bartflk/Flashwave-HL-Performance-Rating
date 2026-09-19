//! Kills in context (PLAN §11 B and D): which kill opened a fight, which
//! was traded straight back, which only cleaned up, and how kills and
//! deaths sit around the ubers.
//!
//! Pure: a [`RawLog`] and its [`GameState`] in, a label per kill and counts
//! per player out. The windows were measured on this account's 195,095 kills:
//!
//! - **Fight gap, 10 s.** 89% of gaps between consecutive kills in a round
//!   are 10 s or less; the long tail beyond is the lull between fights.
//! - **Trade window, 3 s.** After a kill, the killing team's next loss peaks
//!   one second later and has halved by about three.
//!
//! Colours are the ones worn at the moment, which is all "same team" needs.

use crate::rawlog::RawLog;
use crate::state::{Charge, ChargeKind, GameState};
use hl_core::matchdata::Team;
use hl_core::TfClass;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// A kill more than this after the previous one starts a new fight.
pub const FIGHT_GAP_S: i64 = 10;
/// A loss within this long after a kill trades it back.
pub const TRADE_S: i64 = 3;
/// "Before" and "after" a team's uber: this long either side of it.
pub const UBER_WINDOW_S: i64 = 10;
/// A pop is forced when the Medic took this much damage in the seconds before it...
pub const FORCE_DAMAGE: i64 = 90;
pub const FORCE_WINDOW_S: i64 = 3;
/// ...and a player is credited when they dealt at least this much of it.
pub const FORCE_SHARE: i64 = 40;

/// The classes that take and hold space together (PLAN §11, "four groups").
pub fn is_combo(c: TfClass) -> bool {
    matches!(c, TfClass::Medic | TfClass::Demoman | TfClass::Heavy | TfClass::Pyro)
}

/// What one kill meant. Not exclusive: an opening pick can also be traded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KillTags {
    /// The first kill of a fight.
    pub opening: bool,
    /// The first kill of the round.
    pub first_of_round: bool,
    /// The killer's team lost someone within [`TRADE_S`].
    pub traded: bool,
    /// The killer themselves died within [`TRADE_S`].
    pub died_after: bool,
    /// Avenged a teammate who died within [`TRADE_S`] before.
    pub trade: bool,
    /// The killer's team already had more players alive.
    pub cleanup: bool,
    /// The victim was on the combo while their team held a ready charge.
    pub into_charge: bool,
    /// The victim was a Medic holding a ready charge.
    pub drop: bool,
}

/// One player's counts for one match.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FightStats {
    pub account_id: u32,
    /// Rounds the player was alive in at some point.
    pub rounds: u32,
    pub kills: u32,
    pub deaths: u32,
    pub opening_kills: u32,
    pub opening_deaths: u32,
    pub first_picks: u32,
    pub first_deaths: u32,
    pub traded_kills: u32,
    pub died_after_kill: u32,
    pub trade_kills: u32,
    pub cleanup_kills: u32,
    pub charged_picks: u32,
    pub drops: u32,
    /// Enemy ubers popped under this player's damage on the Medic.
    pub forces: u32,
    /// Deaths in the [`UBER_WINDOW_S`] before the player's team popped, while
    /// it was in use, and in the window after it ended.
    pub deaths_before_uber: u32,
    pub deaths_during_uber: u32,
    pub deaths_after_uber: u32,
}

/// The first kill of each round, for the match page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstPick {
    /// The round's number in the raw log's order.
    pub round: u32,
    /// Seconds from the round going live: the end of setup in stopwatch.
    pub after_s: i64,
    pub killer: u32,
    pub victim: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Fights {
    /// Aligned with `raw.kills`; `None` for kills that are not counted
    /// (outside a round, feigns, team kills, duplicated lines).
    pub tags: Vec<Option<KillTags>>,
    pub players: Vec<FightStats>,
    pub first_picks: Vec<FirstPick>,
}

pub fn analyse(raw: &RawLog, gs: &GameState) -> Fights {
    // The kills that count, once each. Some raw logs write lines twice or
    // hold the same match twice; a repeat is the same second, pair and weapon.
    let mut seen = HashSet::new();
    let idx: Vec<usize> = raw
        .kills
        .iter()
        .enumerate()
        .filter(|(_, k)| {
            k.counts()
                && k.killer.account != k.victim.account
                && k.killer.team.is_some()
                && k.victim.team.is_some()
                && k.killer.team != k.victim.team
        })
        .filter(|(_, k)| seen.insert((k.at, k.killer.account, k.victim.account, k.weapon.clone())))
        .map(|(i, _)| i)
        .collect();

    let mut stats: HashMap<u32, FightStats> = HashMap::new();
    let mut tags: Vec<Option<KillTags>> = vec![None; raw.kills.len()];
    let mut first_picks = Vec::new();

    // Kills grouped by round, in log order within it (time order, within one part).
    let mut rounds: Vec<(u32, Vec<usize>)> = Vec::new();
    for &i in &idx {
        let r = raw.kills[i].round;
        match rounds.last_mut() {
            Some((cur, v)) if *cur == r => v.push(i),
            _ => rounds.push((r, vec![i])),
        }
    }

    for (round, ks) in &rounds {
        let at = |j: usize| raw.kills[ks[j]].at;
        for j in 0..ks.len() {
            let k = &raw.kills[ks[j]];
            let (kt, vt) = (k.killer.team.unwrap_or(Team::Red), k.victim.team.unwrap_or(Team::Red));
            let mut t = KillTags {
                first_of_round: j == 0,
                opening: j == 0 || at(j) - at(j - 1) > FIGHT_GAP_S,
                ..KillTags::default()
            };
            // What came straight after, and straight before.
            for &n in &ks[j + 1..] {
                let next = &raw.kills[n];
                if next.at - k.at > TRADE_S {
                    break;
                }
                if next.victim.team == Some(kt) {
                    t.traded = true;
                    if next.victim.account == k.killer.account {
                        t.died_after = true;
                    }
                }
            }
            for &p in ks[..j].iter().rev() {
                let prev = &raw.kills[p];
                if k.at - prev.at > TRADE_S {
                    break;
                }
                if prev.victim.team == Some(kt) {
                    t.trade = true;
                    break;
                }
            }
            // Numbers and charge just before the kill.
            let n = gs.numbers_at(k.at - 1);
            let alive = |team: Team| if team == Team::Red { n[0] } else { n[1] };
            t.cleanup = alive(kt) > alive(vt);
            let charged = gs.charge_at(vt, k.at - 1) == Charge::Ready;
            let vclass = k.victim.class;
            t.into_charge = charged && vclass.is_some_and(is_combo);
            t.drop = charged && vclass == Some(TfClass::Medic);
            tags[ks[j]] = Some(t);

            let s = stats.entry(k.killer.account).or_insert_with(|| FightStats { account_id: k.killer.account, ..Default::default() });
            s.kills += 1;
            s.opening_kills += u32::from(t.opening);
            s.first_picks += u32::from(t.first_of_round);
            s.traded_kills += u32::from(t.traded);
            s.died_after_kill += u32::from(t.died_after);
            s.trade_kills += u32::from(t.trade);
            s.cleanup_kills += u32::from(t.cleanup);
            s.charged_picks += u32::from(t.into_charge);
            s.drops += u32::from(t.drop);

            let v = stats.entry(k.victim.account).or_insert_with(|| FightStats { account_id: k.victim.account, ..Default::default() });
            v.deaths += 1;
            v.opening_deaths += u32::from(t.opening);
            v.first_deaths += u32::from(t.first_of_round);
            match uber_phase(gs, vt, k.at) {
                Some(UberPhase::Before) => v.deaths_before_uber += 1,
                Some(UberPhase::During) => v.deaths_during_uber += 1,
                Some(UberPhase::After) => v.deaths_after_uber += 1,
                None => {}
            }

            if j == 0 {
                let live = live_from(raw, gs, *round);
                first_picks.push(FirstPick {
                    round: *round,
                    after_s: (k.at - live.unwrap_or(k.at)).max(0),
                    killer: k.killer.account,
                    victim: k.victim.account,
                });
            }
        }
    }

    for (account, n) in forces(raw, gs) {
        stats.entry(account).or_insert_with(|| FightStats { account_id: account, ..Default::default() }).forces += n;
    }
    for (account, n) in rounds_alive(gs) {
        stats.entry(account).or_insert_with(|| FightStats { account_id: account, ..Default::default() }).rounds = n;
    }

    let mut players: Vec<FightStats> = stats.into_values().collect();
    players.sort_by_key(|s| s.account_id);
    Fights { tags, players, first_picks }
}

/// Bump to recompute every stored log's fights on the next pass.
pub const VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeriveSummary {
    pub derived: usize,
    pub total: usize,
}

/// Read every stored raw log the current [`VERSION`] has not read yet, or
/// all of them with `all`, and store each player's counts. About 5 ms a log.
pub async fn derive_all(db: &hl_db::Db, all: bool) -> anyhow::Result<DeriveSummary> {
    let ids = db.rawlog_ids().await?;
    let done: HashSet<i64> = if all { HashSet::new() } else { db.fight_logs(VERSION).await?.into_iter().collect() };
    let mut derived = 0;
    for &log_id in ids.iter().filter(|id| !done.contains(id)) {
        let Some(zip) = db.rawlog(log_id).await? else { continue };
        let raw = crate::rawlog::parse(&crate::rawlog::unzip(&zip)?);
        let gs = GameState::build(&raw);
        let rows: Vec<hl_db::FightRow> = analyse(&raw, &gs).players.iter().map(row).collect();
        db.replace_fight_stats(log_id, VERSION, &rows).await?;
        derived += 1;
    }
    Ok(DeriveSummary { derived, total: ids.len() })
}

/// One player's counts in `hl_db::FIGHT_COLUMNS` order.
fn row(s: &FightStats) -> hl_db::FightRow {
    let v = [
        s.rounds,
        s.kills,
        s.deaths,
        s.opening_kills,
        s.opening_deaths,
        s.first_picks,
        s.first_deaths,
        s.traded_kills,
        s.died_after_kill,
        s.trade_kills,
        s.cleanup_kills,
        s.charged_picks,
        s.drops,
        s.forces,
        s.deaths_before_uber,
        s.deaths_during_uber,
        s.deaths_after_uber,
    ];
    hl_db::FightRow { account_id: s.account_id, values: v.map(i64::from) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UberPhase {
    Before,
    During,
    After,
}

/// Charges that count as a team's uber: a Vaccinator's many small pops do not.
fn ubers(gs: &GameState, team: Team) -> impl Iterator<Item = &crate::state::ChargeSpan> {
    gs.charges.iter().filter(move |c| {
        c.team == team && c.kind == ChargeKind::Deployed && c.medigun.as_deref() != Some("vaccinator")
    })
}

/// Where a death of `team` at `t` falls against that team's own ubers.
fn uber_phase(gs: &GameState, team: Team, t: i64) -> Option<UberPhase> {
    if ubers(gs, team).any(|c| c.from <= t && t <= c.to) {
        Some(UberPhase::During)
    } else if ubers(gs, team).any(|c| c.from > t && c.from - t <= UBER_WINDOW_S) {
        Some(UberPhase::Before)
    } else if ubers(gs, team).any(|c| c.to < t && t - c.to <= UBER_WINDOW_S) {
        Some(UberPhase::After)
    } else {
        None
    }
}

/// Pops the Medic was pushed into: [`FORCE_DAMAGE`] on the Medic in the
/// [`FORCE_WINDOW_S`] before it. Credit to each enemy who dealt
/// [`FORCE_SHARE`] of it. A deliberate push is popped at full health, so this
/// is the usual sign of a force; it misses pops forced to save someone else.
fn forces(raw: &RawLog, gs: &GameState) -> HashMap<u32, u32> {
    let mut out: HashMap<u32, u32> = HashMap::new();
    for team in [Team::Red, Team::Blue] {
        for c in ubers(gs, team) {
            let mut by: HashMap<u32, i64> = HashMap::new();
            for d in raw.damage.iter().filter(|d| {
                d.victim.account == c.medic && d.attacker.team.is_some_and(|t| t != team) && d.at <= c.from && c.from - d.at <= FORCE_WINDOW_S
            }) {
                *by.entry(d.attacker.account).or_default() += d.amount;
            }
            if by.values().sum::<i64>() >= FORCE_DAMAGE {
                for (a, dmg) in by {
                    if dmg >= FORCE_SHARE {
                        *out.entry(a).or_default() += 1;
                    }
                }
            }
        }
    }
    out
}

/// When a round went live: the end of setup, or the round start.
fn live_from(raw: &RawLog, gs: &GameState, round: u32) -> Option<i64> {
    let start = *raw.round_starts.get((round as usize).checked_sub(1)?)?;
    let r = gs.rounds.iter().find(|r| r.start == start);
    Some(r.and_then(|r| r.setup_end).unwrap_or(start))
}

/// Rounds each player was alive in at some point.
fn rounds_alive(gs: &GameState) -> HashMap<u32, u32> {
    let mut out: HashMap<u32, u32> = HashMap::new();
    for r in &gs.rounds {
        let end = r.end.unwrap_or(i64::MAX);
        let who: HashSet<u32> = gs
            .lives
            .iter()
            .filter(|l| l.to > l.from && l.from < end && l.to > r.start)
            .map(|l| l.account)
            .collect();
        for a in who {
            *out.entry(a).or_default() += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rawlog::parse;

    fn line(sec: u32, body: &str) -> String {
        format!("L 09/15/2026 - 20:{:02}:{:02}: {body}\n", sec / 60, sec % 60)
    }
    fn p(name: &str, id: u32, team: &str) -> String {
        format!("\"{name}<{id}><[U:1:{id}]><{team}>\"")
    }
    fn kill(sec: u32, k: &str, v: &str) -> String {
        line(sec, &format!("{k} killed {v} with \"x\" (attacker_position \"0 0 0\") (victim_position \"0 0 0\")"))
    }

    /// Red: sniper 1, medic 2, demo 3. Blue: sniper 11, medic 12, demo 13.
    fn log() -> String {
        let (rs, rm, rd) = (p("rs", 1, "Red"), p("rm", 2, "Red"), p("rd", 3, "Red"));
        let (bs, bm, bd) = (p("bs", 11, "Blue"), p("bm", 12, "Blue"), p("bd", 13, "Blue"));
        let mut s = line(0, "World triggered \"Round_Start\"");
        for (who, class) in [(&rs, "sniper"), (&rm, "medic"), (&rd, "demoman"), (&bs, "sniper"), (&bm, "medic"), (&bd, "demoman")] {
            s += &line(0, &format!("{who} spawned as \"{class}\""));
        }
        s += &line(40, &format!("{bm} triggered \"chargeready\""));
        // Fight 1: Red's Sniper picks the charged Blue Medic (a drop), and
        // Blue's Demo trades him two seconds later.
        s += &kill(50, &rs, &bm);
        s += &kill(52, &bd, &rs);
        // Red's Demo avenges him at 2 v 2, then Red's Medic cleans up 2 v 1.
        s += &kill(53, &rd, &bd);
        s += &kill(55, &rm, &bs);
        // Fight 2, 30 s later: Blue's Sniper opens on Red's Medic.
        s += &line(80, &format!("{rm} triggered \"chargeready\""));
        s += &line(83, &format!("{bs} triggered \"damage\" against {rm} (damage \"100\") (weapon \"sniperrifle\")"));
        s += &line(84, &format!("{rm} triggered \"chargedeployed\" (medigun \"medigun\")"));
        s += &line(92, &format!("{rm} triggered \"chargeended\" (duration \"8\")"));
        s += &kill(95, &bs, &rd);
        s += &line(120, "World triggered \"Round_Win\" (winner \"Red\")");
        s
    }

    fn fights() -> Fights {
        let raw = parse(&log());
        let gs = GameState::build(&raw);
        analyse(&raw, &gs)
    }

    fn of(f: &Fights, id: u32) -> FightStats {
        f.players.iter().find(|s| s.account_id == id).cloned().unwrap_or_default()
    }

    #[test]
    fn a_pick_on_a_charged_medic_is_an_opening_drop_that_got_traded() {
        let f = fights();
        let t = f.tags[0].unwrap();
        assert!(t.opening && t.first_of_round && t.drop && t.into_charge);
        assert!(t.traded && t.died_after, "the Sniper died 2 s later");
        assert!(!t.cleanup, "3 v 3 before it");
        let rs = of(&f, 1);
        assert_eq!((rs.kills, rs.opening_kills, rs.first_picks, rs.drops, rs.died_after_kill), (1, 1, 1, 1, 1));
        assert_eq!(of(&f, 12).first_deaths, 1);
    }

    #[test]
    fn trades_and_cleanups() {
        let f = fights();
        let trade = f.tags[1].unwrap();
        assert!(trade.trade, "Blue's Demo avenged the Medic within 3 s");
        assert!(!trade.opening);
        let avenged = f.tags[2].unwrap();
        assert!(avenged.trade && !avenged.cleanup, "2 v 2 when Red's Demo got the kill");
        let cleanup = f.tags[3].unwrap();
        assert!(cleanup.cleanup, "2 v 1 when Red's Medic got the kill");
    }

    #[test]
    fn a_new_fight_after_the_gap_and_deaths_around_ubers() {
        let f = fights();
        let t = f.tags[4].unwrap();
        assert!(t.opening && !t.first_of_round);
        // Red's Demo died 3 s after Red's uber ended.
        assert_eq!(of(&f, 3).deaths_after_uber, 1);
        // Blue's Sniper hit the Medic for 100 a second before the pop.
        assert_eq!(of(&f, 11).forces, 1);
        assert_eq!(f.first_picks.len(), 1);
        assert_eq!(f.first_picks[0].after_s, 50);
        assert_eq!(of(&f, 1).rounds, 1);
    }
}
