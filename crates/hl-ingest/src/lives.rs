//! A player's lives on one map, across every match a demo followed them
//! through (Q57, Emiel): every Spy cross they made on Product, split by
//! playing RED and BLU.
//!
//! The routes are stored per life already (`demo_path`); what they lack is
//! the class and the side, which the demo's timeline has. Those are read
//! from the timeline when asked, so nothing has to be read from the demo
//! files again. A life whose demo has no timeline keeps its route with the
//! class and side unknown.

use crate::aim::account_of;
use anyhow::Result;
use hl_core::maps::map_base;
use hl_db::Db;
use hl_demos::timeline::{Field, Stored, Timeline};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// A map the player has lives on, with how many matches hold them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapWithLives {
    /// Without its version: `product`.
    pub map: String,
    /// The full name of the newest version played, for the map's picture.
    pub name: String,
    pub matches: usize,
    pub lives: usize,
}

/// The maps `account_id` has routes on, most matches first.
pub async fn maps(db: &Db, account_id: u32) -> Result<Vec<MapWithLives>> {
    let mut by: HashMap<String, (String, Option<i64>, HashSet<i64>, usize)> = HashMap::new();
    for r in db.lives_index(account_id).await? {
        let Some(map) = r.map.filter(|m| !m.is_empty()) else { continue };
        let e = by.entry(map_base(&map)).or_insert_with(|| (map.clone(), r.played_at, HashSet::new(), 0));
        if r.played_at > e.1 {
            e.0 = map.clone();
            e.1 = r.played_at;
        }
        e.2.insert(r.log_id);
        e.3 += 1;
    }
    let mut out: Vec<MapWithLives> = by.into_iter().map(|(map, (name, _, logs, lives))| MapWithLives { map, name, matches: logs.len(), lives }).collect();
    out.sort_by(|a, b| b.matches.cmp(&a.matches).then_with(|| a.map.cmp(&b.map)));
    Ok(out)
}

/// One life on the map.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeOnMap {
    pub log_id: i64,
    pub played_at: Option<i64>,
    pub demo_id: i64,
    /// The recording was SourceTV's rather than the player's own.
    pub stv: bool,
    /// `scout` ... `spy`, as it was when the life began; `None` where the
    /// demo has no timeline.
    pub class: Option<String>,
    /// `Red` or `Blue`, likewise.
    pub team: Option<String>,
    pub round_num: Option<i64>,
    pub died: bool,
    pub seconds: f64,
    /// `(x, y)` in map units, about four a second.
    pub points: Vec<(i32, i32)>,
    /// Q67: their team captured a point during the life.
    pub capped: bool,
    /// Q67: the life began within [`AFTER_CAP_S`] of the other team taking a
    /// point in the same round: the respawn after losing one.
    pub after_enemy_cap: bool,
    /// Q67: others were killing each other both in the [`FIGHT_BEFORE_S`]
    /// before the life began and the [`FIGHT_AFTER_S`] after: spawned into a
    /// fight in progress.
    pub mid_fight: bool,
}

/// How soon after the other team's cap a life counts as the one after it.
pub const AFTER_CAP_S: f64 = 20.0;
/// The window around a life's start in which other players' kills mean a
/// fight was going on.
pub const FIGHT_BEFORE_S: f64 = 5.0;
pub const FIGHT_AFTER_S: f64 = 10.0;

/// A life's start on the log's clock, and whether those three hold. Pure, for
/// testing: `caps` are `(at, team, point)` on the log's clock, `rounds`
/// `(number, start, end)`, `kills` `(at, killer, victim)`.
pub fn situation(
    start: Option<f64>,
    team: Option<&str>,
    round: Option<i64>,
    me: u32,
    caps: &[(i64, String, i64)],
    rounds: &[(i64, f64, f64)],
    kills: &[(i64, u32, u32)],
) -> (bool, bool) {
    let Some(start) = start else { return (false, false) };
    let round_start = round.and_then(|r| rounds.iter().find(|(n, ..)| *n == r)).map(|(_, s, _)| *s).unwrap_or(f64::MIN);
    let after_cap = team.is_some_and(|team| {
        caps.iter().any(|(at, by, _)| {
            let at = *at as f64;
            by != team && at >= round_start && at <= start && start - at <= AFTER_CAP_S
        })
    });
    // A fight going on across the moment the life began: others killing
    // each other both just before it and just after. Any kill nearby alone
    // was nearly every life (139 of 143 on Product): with 18 players there
    // is always one somewhere.
    let others = |from: f64, to: f64| {
        kills.iter().any(|&(at, killer, victim)| {
            let at = at as f64;
            killer != me && victim != me && at >= from && at <= to
        })
    };
    let fight = others(start - FIGHT_BEFORE_S, start) && others(start, start + FIGHT_AFTER_S);
    (after_cap, fight)
}

/// Everything the view draws, and the counts it states.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivesOnMap {
    pub map: String,
    /// Matches on this map with a demo that followed the player.
    pub matches: usize,
    /// Of those, with a SourceTV demo; the rest are their own recordings.
    pub stv_matches: usize,
    pub lives: Vec<LifeOnMap>,
}

/// TF2's class numbers, as the timeline stores them.
fn class_name(n: u8) -> Option<&'static str> {
    Some(match n {
        1 => "scout",
        2 => "sniper",
        3 => "soldier",
        4 => "demoman",
        5 => "medic",
        6 => "heavy",
        7 => "pyro",
        8 => "spy",
        9 => "engineer",
        _ => return None,
    })
}

fn team_name(n: u8) -> Option<&'static str> {
    match n {
        2 => Some("Red"),
        3 => Some("Blue"),
        _ => None,
    }
}

/// `account_id`'s lives on `map` (any version of it).
pub async fn on_map(db: &Db, account_id: u32, map: &str) -> Result<LivesOnMap> {
    let want = map_base(map);
    // Which lives, by match and demo.
    let mut picked: BTreeMap<i64, (Option<i64>, HashMap<i64, (bool, HashSet<i64>)>)> = BTreeMap::new();
    for r in db.lives_index(account_id).await? {
        if r.map.as_deref().map(map_base) != Some(want.clone()) {
            continue;
        }
        let log = picked.entry(r.log_id).or_insert_with(|| (r.played_at, HashMap::new()));
        log.1.entry(r.demo_id).or_insert_with(|| (r.kind == "stv", HashSet::new())).1.insert(r.seq);
    }

    let mut out = LivesOnMap { map: want, ..Default::default() };
    for (log_id, (played_at, demos)) in picked {
        out.matches += 1;
        if demos.values().any(|(stv, _)| *stv) {
            out.stv_matches += 1;
        }
        let paths = db.paths_for_log(log_id).await?;
        // For the situation filters (Q67): the log's caps, rounds and kills,
        // and where each demo starts on the log's clock.
        let offset = db.log_clock_offset(log_id).await?;
        let caps = db.cap_events(log_id).await?;
        let rounds: Vec<(i64, f64, f64)> = db.round_spans(log_id).await?.into_iter().map(|(n, s, len)| (n, s as f64, (s + len) as f64)).collect();
        let kills: Vec<(i64, u32, u32)> = db.kills_for_log(log_id).await?.into_iter().filter(|k| k.live).map(|k| (k.at_raw, k.killer, k.victim)).collect();
        let linked = db.demos_for_log(log_id).await?;
        for (demo_id, (stv, seqs)) in demos {
            let clock = linked.iter().find(|d| d.demo_id == demo_id).and_then(|d| Some((d.start_utc?, d.tick_rate?)));
            let lives: Vec<&hl_db::PathRow> = paths.iter().filter(|p| p.demo_id == demo_id && p.account_id == account_id && seqs.contains(&p.seq)).collect();
            if lives.is_empty() {
                continue;
            }
            let rate = 66.67;
            // Class and side from the timeline, at each life's middle.
            let who = match db.timeline(demo_id).await? {
                Some(row) => {
                    let s = stored(row);
                    let ticks: Vec<u32> = lives.iter().map(|l| ((l.from_tick + l.to_tick) / 2).max(0) as u32).collect();
                    tokio::task::spawn_blocking(move || -> Result<Vec<(Option<u8>, Option<u8>)>> {
                        let tl = Timeline::decode(&s)?;
                        let slot = tl.people.iter().position(|p| account_of(&p.steamid) == Some(account_id));
                        Ok(ticks
                            .iter()
                            .map(|&tick| {
                                let (Some(slot), Some(t)) = (slot, tl.t_of(tick)) else { return (None, None) };
                                let class = match tl.at(slot, t, |f| matches!(f, Field::Class(_))) {
                                    Some(Field::Class(c)) => Some(c),
                                    _ => None,
                                };
                                let team = match tl.at(slot, t, |f| matches!(f, Field::Team(_))) {
                                    Some(Field::Team(c)) => Some(c),
                                    _ => None,
                                };
                                (class, team)
                            })
                            .collect())
                    })
                    .await??
                }
                None => vec![(None, None); lives.len()],
            };
            for (l, (class, team)) in lives.into_iter().zip(who) {
                let team = team.and_then(team_name);
                // The life's start on the log's clock, as the routes pass places it.
                let start = match (clock, offset) {
                    (Some((s, r)), Some(o)) => Some(s + l.from_tick as f64 / r - o as f64),
                    _ => None,
                };
                let (after_enemy_cap, mid_fight) = situation(start, team, l.round_num, account_id, &caps, &rounds, &kills);
                out.lives.push(LifeOnMap {
                    log_id,
                    played_at,
                    demo_id,
                    stv,
                    class: class.and_then(class_name).map(str::to_string),
                    team: team.map(str::to_string),
                    round_num: l.round_num,
                    died: l.died,
                    seconds: ((l.to_tick - l.from_tick).max(0) as f64 / rate * 10.0).round() / 10.0,
                    points: l.points.iter().map(|&(_, x, y, _)| (x, y)).collect(),
                    capped: !l.caps.is_empty(),
                    after_enemy_cap,
                    mid_fight,
                });
            }
        }
    }
    Ok(out)
}

fn stored(row: hl_db::TimelineRow) -> Stored {
    Stored {
        version: row.version,
        tick_rate: row.tick_rate,
        stride: row.stride as u32,
        head: row.head,
        samples: row.samples,
        changes: row.changes,
        objects: row.objects,
        events: row.events,
        raw_bytes: row.raw_bytes as usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_life_after_the_other_team_capped_and_one_into_a_fight() {
        let rounds = [(1, 100.0, 400.0), (2, 400.0, 700.0)];
        let caps = vec![(200, "Blue".to_string(), 2), (395, "Red".to_string(), 3)];
        let kills = [(300, 7, 8), (305, 9, 1)];
        // RED, spawning 12 s after BLU took a point: after a cap.
        assert!(situation(Some(212.0), Some("Red"), Some(1), 1, &caps, &rounds, &kills).0);
        // Too long after, or their own team's cap, or the cap in the round before.
        assert!(!situation(Some(240.0), Some("Red"), Some(1), 1, &caps, &rounds, &kills).0);
        assert!(!situation(Some(212.0), Some("Blue"), Some(1), 1, &caps, &rounds, &kills).0);
        assert!(!situation(Some(402.0), Some("Blue"), Some(2), 1, &caps, &rounds, &kills).0);
        // Others fighting 4 s before the life began and 1 s after: mid-fight.
        // Only before is not; the kill on themselves does not count, nor one
        // long after.
        assert!(situation(Some(304.0), Some("Red"), Some(1), 1, &caps, &rounds, &[(300, 7, 8), (305, 9, 4)]).1);
        assert!(!situation(Some(304.0), Some("Red"), Some(1), 1, &caps, &rounds, &[(300, 7, 8)]).1);
        assert!(!situation(Some(308.0), Some("Red"), Some(1), 1, &caps, &rounds, &[(305, 9, 1)]).1);
        assert!(!situation(Some(250.0), Some("Red"), Some(1), 1, &caps, &rounds, &kills).1);
        // No clock, no answer.
        assert_eq!(situation(None, Some("Red"), Some(1), 1, &caps, &rounds, &kills), (false, false));
    }

    #[test]
    fn class_and_team_numbers_read_as_names() {
        assert_eq!(class_name(8), Some("spy"));
        assert_eq!(class_name(6), Some("heavy"));
        assert_eq!(class_name(0), None);
        assert_eq!(team_name(2), Some("Red"));
        assert_eq!(team_name(3), Some("Blue"));
        assert_eq!(team_name(1), None);
    }
}
