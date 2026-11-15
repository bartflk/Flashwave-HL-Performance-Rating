//! 6v6 measures read from a raw log, from boSe.'s review of the weights
//! (15 Nov 2026, model s2):
//!
//! - **Both flanks down**: "It's okay to die in flank, as long as your other
//!   flank is still alive ... they should be penalized way more if they both
//!   die at the same time though (not on mids, only second, last & on
//!   Product, Bagel is different since flank scout runs everywhere)". A
//!   roamer's or flank Scout's death counts when the team's other flank died
//!   within [`FLANK_WINDOW_S`] of it, on 5CP after the round's first capture
//!   and on Product; on any other map it is not measured.
//! - **Kills in an über** for the pocket Scout: "track the number of uber
//!   taken, and how many picks or flashes have been done in it, this HAS to
//!   be in the pocket scout rating". An über's target is whoever its Medic
//!   healed most in its first [`TARGET_WINDOW_S`] seconds.
//! - **Caps contested** for the Medic: "only ... if he's under ubercharge or
//!   he's the last one alive, else it shouldn't count since he should never
//!   be on point". A capture or block of his counts only then.
//! - **Über forces, backcaps and Sniper pressure**, as proposed on the 6v6
//!   numbers panel (`sixes_stats::add_proposed`); boSe.: "All 3 should stay"
//!   (model s3). Forces: an enemy über deployed after this player dealt
//!   [`FORCE_MIN_DAMAGE`] or more to the Medic's team in the
//!   [`FORCE_WINDOW_S`] seconds before. Backcaps: a capture without the
//!   capping team's Medic on the point. Sniper pressure: damage as Sniper on
//!   the enemy Medic, Demoman and pocket.

use crate::rawlog::{EventKind, RawLog};
use hl_core::matchdata::Team;
use std::collections::{HashMap, HashSet};

/// Two flanks' deaths this close together are both down at once.
pub const FLANK_WINDOW_S: i64 = 10;
/// The heals that say who an über went on: its first seconds.
pub const TARGET_WINDOW_S: i64 = 2;
/// An über with no `chargeended` line is taken to last this long.
pub const UBER_S: i64 = 8;
/// How long before an enemy über a player's damage counts as forcing it.
pub const FORCE_WINDOW_S: i64 = 4;
/// Damage on the Medic's team in that window that counts as a force.
pub const FORCE_MIN_DAMAGE: i64 = 50;

/// Counts per account, for one log.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RawParts {
    /// Deaths with the other flank down too, where the map counts them.
    pub flank_double_deaths: HashMap<u32, u32>,
    /// Whether this log's map counts flank deaths at all.
    pub flanks_measured: bool,
    pub ubers_taken: HashMap<u32, u32>,
    pub uber_picks: HashMap<u32, u32>,
    pub caps_contested: HashMap<u32, u32>,
    pub forces: HashMap<u32, u32>,
    pub backcaps: HashMap<u32, u32>,
    /// Damage as Sniper on the enemy Medic, Demoman and pocket.
    pub sniper_combo_damage: HashMap<u32, i64>,
}

/// Where both flanks dying counts: 5CP after the round's first capture
/// (mids left out), Product throughout, nowhere else.
#[derive(Debug, Clone, Copy, PartialEq)]
enum FlankMap {
    FiveCp,
    Product,
    Other,
}

fn flank_map(map: &str) -> FlankMap {
    let m = map.to_ascii_lowercase();
    if m.starts_with("cp_") {
        FlankMap::FiveCp
    } else if m.starts_with("koth_product") {
        FlankMap::Product
    } else {
        FlankMap::Other
    }
}

struct Charge {
    medic: u32,
    start: i64,
    end: i64,
}

/// The counts for one raw log. `roles` holds each player's main 6v6 role
/// ("pocket", "roamer", "flank_scout", "pocket_scout", "demoman", "medic").
pub fn measure(raw: &RawLog, roles: &HashMap<u32, String>, map: &str) -> RawParts {
    let role = |a: u32| roles.get(&a).map(String::as_str);
    let flank = |a: u32| matches!(role(a), Some("roamer" | "flank_scout"));
    let mut out = RawParts { flanks_measured: flank_map(map) != FlankMap::Other, ..Default::default() };

    // Übers and heals, then each über's target.
    let mut charges: Vec<Charge> = Vec::new();
    let mut heals: Vec<(u32, u32, i64, i64)> = Vec::new();
    for e in &raw.events {
        match &e.kind {
            EventKind::ChargeDeployed { medic, .. } => charges.push(Charge { medic: medic.account, start: e.at, end: e.at + UBER_S }),
            EventKind::ChargeEnded(m) => {
                if let Some(c) = charges.iter_mut().rev().find(|c| c.medic == m.account && c.start <= e.at) {
                    c.end = c.end.min(e.at.max(c.start));
                }
            }
            EventKind::Healed { medic, target, amount } => heals.push((medic.account, *target, e.at, *amount)),
            _ => {}
        }
    }
    for c in &charges {
        let mut by: HashMap<u32, i64> = HashMap::new();
        for (m, t, at, amount) in &heals {
            if *m == c.medic && *at >= c.start && *at <= c.start + TARGET_WINDOW_S {
                *by.entry(*t).or_default() += amount;
            }
        }
        let Some((&target, _)) = by.iter().max_by_key(|(a, n)| (**n, std::cmp::Reverse(**a))) else { continue };
        if role(target) != Some("pocket_scout") {
            continue;
        }
        *out.ubers_taken.entry(target).or_default() += 1;
        let kills = raw
            .kills
            .iter()
            .filter(|k| k.killer.account == target && k.counts() && k.killer.team != k.victim.team && k.at >= c.start && k.at <= c.end)
            .count() as u32;
        if kills > 0 {
            *out.uber_picks.entry(target).or_default() += kills;
        }
    }
    let ubered = |medic: u32, at: i64| charges.iter().any(|c| c.medic == medic && at >= c.start && at <= c.end);

    // Über forces: who pressured the Medic's team just before each über.
    for e in &raw.events {
        let EventKind::ChargeDeployed { medic, .. } = &e.kind else { continue };
        let Some(team) = medic.team else { continue };
        let mut dealt: HashMap<u32, i64> = HashMap::new();
        for d in raw.damage.iter().filter(|d| d.live && d.at <= e.at && d.at >= e.at - FORCE_WINDOW_S) {
            if d.victim.team == Some(team) && d.attacker.team.is_some_and(|t| t != team) {
                *dealt.entry(d.attacker.account).or_default() += d.amount.min(crate::rawlog::HIT_CAP);
            }
        }
        for (a, dmg) in dealt {
            if dmg >= FORCE_MIN_DAMAGE {
                *out.forces.entry(a).or_default() += 1;
            }
        }
    }
    // Sniper pressure: damage on the combo.
    for d in raw.damage.iter().filter(|d| d.live && d.attacker.class == Some(hl_core::TfClass::Sniper) && d.attacker.team != d.victim.team) {
        if matches!(role(d.victim.account), Some("medic" | "demoman" | "pocket")) {
            *out.sniper_combo_damage.entry(d.attacker.account).or_default() += d.amount.min(crate::rawlog::HIT_CAP);
        }
    }
    let medics: HashSet<u32> = roles.iter().filter(|(_, r)| r.as_str() == "medic").map(|(a, _)| *a).collect();

    // The walk: kills and events in time order, with who is alive and
    // whether the round's first point has gone.
    let mut alive: HashMap<Team, HashSet<u32>> = HashMap::new();
    let mut team_of: HashMap<u32, Team> = HashMap::new();
    let mut capped = false;
    let mut flank_deaths: Vec<(i64, Team, u32)> = Vec::new();
    let mut ei = 0;
    let mut events_upto = |upto: usize, ei: &mut usize, alive: &mut HashMap<Team, HashSet<u32>>, capped: &mut bool, out: &mut RawParts| {
        while *ei < raw.events.len() && raw.events[*ei].after_kills <= upto {
            let e = &raw.events[*ei];
            let medic_counts = |m: u32, team: Option<Team>, alive: &HashMap<Team, HashSet<u32>>| {
                let last_alive = team.and_then(|t| alive.get(&t)).is_some_and(|s| s.len() == 1 && s.contains(&m));
                role(m) == Some("medic") && (ubered(m, e.at) || last_alive)
            };
            match &e.kind {
                EventKind::RoundStart => {
                    alive.clear();
                    *capped = false;
                }
                EventKind::Spawn(a) => {
                    if let Some(t) = a.team {
                        for s in alive.values_mut() {
                            s.remove(&a.account);
                        }
                        alive.entry(t).or_default().insert(a.account);
                        team_of.insert(a.account, t);
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
                EventKind::PointCaptured { team, cappers, .. } => {
                    for &m in cappers {
                        if medic_counts(m, *team, alive) {
                            *out.caps_contested.entry(m).or_default() += 1;
                        }
                    }
                    // A backcap: taken without the team's Medic on the point.
                    if !cappers.is_empty() && !medics.is_empty() && !cappers.iter().any(|c| medics.contains(c)) {
                        for &c in cappers {
                            *out.backcaps.entry(c).or_default() += 1;
                        }
                    }
                    *capped = true;
                }
                EventKind::CaptureBlocked { by, .. } if medic_counts(by.account, by.team, alive) => {
                    *out.caps_contested.entry(by.account).or_default() += 1;
                }
                _ => {}
            }
            *ei += 1;
        }
    };
    let mode = flank_map(map);
    for (i, k) in raw.kills.iter().enumerate() {
        events_upto(i, &mut ei, &mut alive, &mut capped, &mut out);
        let (Some(side), Some(vside)) = (k.killer.team, k.victim.team) else { continue };
        if side == vside || !k.counts() {
            continue;
        }
        let counts_here = match mode {
            FlankMap::FiveCp => capped,
            FlankMap::Product => true,
            FlankMap::Other => false,
        };
        if counts_here && flank(k.victim.account) {
            flank_deaths.push((k.at, vside, k.victim.account));
        }
        if let Some(s) = alive.get_mut(&vside) {
            s.remove(&k.victim.account);
        }
    }
    events_upto(usize::MAX, &mut ei, &mut alive, &mut capped, &mut out);

    // A flank's death counts when the team's other flank died close by.
    for (at, team, who) in &flank_deaths {
        let partner = flank_deaths.iter().any(|(a2, t2, w2)| t2 == team && w2 != who && (a2 - at).abs() <= FLANK_WINDOW_S);
        if partner {
            *out.flank_double_deaths.entry(*who).or_default() += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "L 11/15/2026 - 20:00:";

    fn line(sec: u32, body: &str) -> String {
        format!("{T}{sec:02}: {body}")
    }
    fn p(name: &str, id: u32, team: &str) -> String {
        format!("\"{name}<{id}><[U:1:{id}]><{team}>\"")
    }

    /// Red: 1 roamer, 2 flank Scout, 3 pocket Scout, 4 Medic; Blue: 11 to 14.
    fn roles() -> HashMap<u32, String> {
        [(1, "roamer"), (2, "flank_scout"), (3, "pocket_scout"), (4, "medic"), (11, "roamer"), (12, "flank_scout"), (13, "pocket_scout"), (14, "medic")]
            .into_iter()
            .map(|(a, r)| (a, r.to_string()))
            .collect()
    }

    fn start() -> Vec<String> {
        let mut l = vec![line(0, "World triggered \"Round_Start\"")];
        for (id, team, class) in [(1, "Red", "soldier"), (2, "Red", "scout"), (3, "Red", "scout"), (4, "Red", "medic"), (11, "Blue", "soldier"), (12, "Blue", "scout"), (13, "Blue", "scout"), (14, "Blue", "medic")] {
            l.push(line(1, &format!("{} spawned as \"{class}\"", p("x", id, team))));
        }
        l
    }

    fn kill(sec: u32, k: u32, kt: &str, v: u32, vt: &str) -> String {
        line(sec, &format!("{} killed {} with \"scattergun\" (attacker_position \"0 0 0\") (victim_position \"1 1 1\")", p("k", k, kt), p("v", v, vt)))
    }

    #[test]
    fn both_flanks_down_counts_after_mids_on_5cp_and_not_during_them() {
        let mut l = start();
        // Mids: both Red flanks die at once. Not counted.
        l.push(kill(5, 11, "Blue", 1, "Red"));
        l.push(kill(6, 12, "Blue", 2, "Red"));
        l.push(line(10, &format!("Team \"Blue\" triggered \"pointcaptured\" (cp \"2\") (cpname \"mid\") (numcappers \"1\") (player1 {}) (position1 \"0 0 0\")", p("c", 11, "Blue"))));
        l.push(line(12, &format!("{} spawned as \"soldier\"", p("x", 1, "Red"))));
        l.push(line(12, &format!("{} spawned as \"scout\"", p("x", 2, "Red"))));
        // Second: the roamer dies, the flank Scout 15 s later: not together.
        l.push(kill(20, 11, "Blue", 1, "Red"));
        l.push(kill(35, 12, "Blue", 2, "Red"));
        l.push(line(42, &format!("{} spawned as \"soldier\"", p("x", 1, "Red"))));
        l.push(line(42, &format!("{} spawned as \"scout\"", p("x", 2, "Red"))));
        // Then both within 5 s: counted for each.
        l.push(kill(50, 11, "Blue", 1, "Red"));
        l.push(kill(55, 12, "Blue", 2, "Red"));
        let raw = crate::rawlog::parse(&l.join("\n"));
        let m = measure(&raw, &roles(), "cp_process_f12");
        assert!(m.flanks_measured);
        assert_eq!((m.flank_double_deaths.get(&1), m.flank_double_deaths.get(&2)), (Some(&1), Some(&1)));
        let bagel = measure(&raw, &roles(), "koth_bagel_rc5");
        assert!(!bagel.flanks_measured && bagel.flank_double_deaths.is_empty(), "Bagel: the flank Scout runs everywhere");
        assert_eq!(measure(&raw, &roles(), "koth_product_final").flank_double_deaths.get(&1), Some(&2), "Product counts mids too");
    }

    #[test]
    fn a_pocket_scout_s_kills_in_an_uber_he_took_are_counted() {
        let mut l = start();
        l.push(line(20, &format!("{} triggered \"chargedeployed\" (medigun \"medigun\")", p("m", 4, "Red"))));
        l.push(line(20, &format!("{} triggered \"healed\" against {} (healing \"40\")", p("m", 4, "Red"), p("s", 3, "Red"))));
        l.push(line(21, &format!("{} triggered \"healed\" against {} (healing \"30\")", p("m", 4, "Red"), p("r", 1, "Red"))));
        l.push(kill(23, 3, "Red", 12, "Blue"));
        l.push(kill(25, 3, "Red", 14, "Blue"));
        l.push(line(27, &format!("{} triggered \"chargeended\"", p("m", 4, "Red"))));
        l.push(kill(30, 3, "Red", 11, "Blue"));
        let m = measure(&crate::rawlog::parse(&l.join("\n")), &roles(), "cp_gullywash_f9");
        assert_eq!((m.ubers_taken.get(&3), m.uber_picks.get(&3)), (Some(&1), Some(&2)), "the kill after it ended is not counted");
    }

    #[test]
    fn forces_backcaps_and_sniper_pressure_are_counted_as_bose_agreed() {
        let mut l = start();
        let dmg = |sec: u32, a: u32, at: &str, v: u32, vt: &str, n: u32| line(sec, &format!("{} triggered \"damage\" against {} (damage \"{n}\")", p("a", a, at), p("v", v, vt)));
        // Blue's roamer 11 deals 60 to Red, then Red's Medic übers: a force.
        // Blue's 12 deals 30: not enough. Damage 10 s before: too early.
        l.push(dmg(10, 12, "Blue", 1, "Red", 200));
        l.push(dmg(18, 11, "Blue", 1, "Red", 60));
        l.push(dmg(19, 12, "Blue", 3, "Red", 30));
        l.push(line(20, &format!("{} triggered \"chargedeployed\" (medigun \"medigun\")", p("m", 4, "Red"))));
        // A capture by Red's flank Scout without the Medic: a backcap.
        l.push(line(25, &format!("Team \"Red\" triggered \"pointcaptured\" (cp \"3\") (cpname \"second\") (numcappers \"1\") (player1 {}) (position1 \"0 0 0\")", p("c", 2, "Red"))));
        // Blue's 13 as Sniper on Red's Medic: pressure; on the flank Scout: not.
        l.push(line(26, &format!("{} spawned as \"sniper\"", p("x", 13, "Blue"))));
        l.push(dmg(30, 13, "Blue", 4, "Red", 150));
        l.push(dmg(31, 13, "Blue", 2, "Red", 150));
        let m = measure(&crate::rawlog::parse(&l.join("\n")), &roles(), "cp_process_f12");
        assert_eq!((m.forces.get(&11), m.forces.get(&12)), (Some(&1), None));
        assert_eq!(m.backcaps.get(&2), Some(&1));
        assert_eq!(m.sniper_combo_damage.get(&13), Some(&150));
    }

    #[test]
    fn a_medic_s_cap_counts_only_ubered_or_last_alive() {
        let mut l = start();
        let cap = |sec: u32| line(sec, &format!("Team \"Red\" triggered \"pointcaptured\" (cp \"2\") (cpname \"mid\") (numcappers \"1\") (player1 {}) (position1 \"0 0 0\")", p("m", 4, "Red")));
        // On the point with his team alive: not counted.
        l.push(cap(5));
        // His team dead but him: counted.
        l.push(kill(10, 11, "Blue", 1, "Red"));
        l.push(kill(10, 11, "Blue", 2, "Red"));
        l.push(kill(11, 11, "Blue", 3, "Red"));
        l.push(cap(12));
        let m = measure(&crate::rawlog::parse(&l.join("\n")), &roles(), "cp_process_f12");
        assert_eq!(m.caps_contested.get(&4), Some(&1));
    }
}
