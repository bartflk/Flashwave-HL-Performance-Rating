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

/// Someone else with routes on a map, to compare with (Flashy: "where both
/// snipers rollout").
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapPlayer {
    pub account_id: u32,
    pub name: String,
    /// The classes they mainly played in those matches, most matches first.
    pub classes: Vec<(String, usize)>,
    pub matches: usize,
    /// Of those, matches `me` played too.
    pub shared: usize,
}

/// Everyone a demo on this machine followed on `map`, but `me`: those who
/// shared the most matches with `me` first, then those with the most.
pub async fn players_on_map(db: &Db, map: &str, me: u32) -> Result<Vec<MapPlayer>> {
    let want = map_base(map);
    let rows: Vec<_> = db.map_players(&want).await?.into_iter().filter(|r| map_base(&r.map) == want).collect();
    let mine: HashSet<i64> = rows.iter().filter(|r| r.account_id == me).map(|r| r.log_id).collect();
    let mut by: HashMap<u32, (String, Option<i64>, HashMap<String, usize>, usize, usize)> = HashMap::new();
    for r in rows.iter().filter(|r| r.account_id != me) {
        let e = by.entry(r.account_id).or_insert_with(|| (String::new(), None, HashMap::new(), 0, 0));
        if let Some(name) = r.name.as_ref().filter(|_| r.played_at >= e.1 || e.0.is_empty()) {
            e.0 = name.clone();
            e.1 = r.played_at;
        }
        if let Some(c) = &r.main_class {
            *e.2.entry(c.clone()).or_default() += 1;
        }
        e.3 += 1;
        if mine.contains(&r.log_id) {
            e.4 += 1;
        }
    }
    let mut out: Vec<MapPlayer> = by
        .into_iter()
        .map(|(account_id, (name, _, classes, matches, shared))| {
            let mut classes: Vec<(String, usize)> = classes.into_iter().collect();
            classes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            MapPlayer { account_id, name, classes, matches, shared }
        })
        .collect();
    out.sort_by(|a, b| b.shared.cmp(&a.shared).then(b.matches.cmp(&a.matches)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
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
    /// Whose life it is.
    pub account_id: u32,
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
    /// The round's opening (PGL's heatmaps): for the `points` from
    /// `opening_skip` on that fall in the round's first [`OPENING_S`], each
    /// one's seconds since the round began. Empty for a life that was not
    /// there when a round began.
    pub opening_t: Vec<f32>,
    /// The points before the round began: the pre-round freeze, or the
    /// pregame for round 1's.
    pub opening_skip: usize,
    /// The round whose opening it is: a life alive across a round's start
    /// (the pregame's) may have been counted to none.
    pub opening_round: Option<i64>,
    /// Seconds since the round began of its first kill, for a life that
    /// began with it (the view can stop there).
    pub first_kill: Option<f32>,
}

/// How long a round's opening lasts at most.
pub const OPENING_S: f64 = 40.0;
/// A life that began this soon after the round did is its first one.
pub const OPENING_GRACE_S: f64 = 10.0;

/// For a life from `start` to `end` that was there when a round began (alive
/// at its start, as players respawn in the pre-round freeze before the log's
/// Round_Start and round 1's are alive from the pregame, or spawned within
/// [`OPENING_GRACE_S`] of it): the round, the life's start in seconds since
/// the round began (negative when before it), and when the round's first
/// kill came. `None` for any other life. Pure, for testing: `rounds` are
/// `(number, start, end)` and `kills` `(at, killer, victim)` on the log's
/// clock.
pub fn opening(start: Option<f64>, end: Option<f64>, rounds: &[(i64, f64, f64)], kills: &[(i64, u32, u32)]) -> Option<(i64, f64, Option<f64>)> {
    let (start, end) = (start?, end?);
    let &(round, from, to) = rounds.iter().find(|&&(_, from, _)| start <= from + OPENING_GRACE_S && end > from)?;
    let first_kill = kills.iter().map(|&(at, ..)| at as f64).filter(|&at| at >= from && at <= to).fold(f64::INFINITY, f64::min);
    // A kill before the life began: it spawned into the fight, not the opening.
    if first_kill < start {
        return None;
    }
    Some((round, start - from, first_kill.is_finite().then_some(first_kill - from)))
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
        let which: Vec<(i64, bool)> = demos.iter().map(|(&d, &(stv, _))| (d, stv)).collect();
        let keep = |p: &hl_db::PathRow| p.account_id == account_id && demos.get(&p.demo_id).is_some_and(|(_, seqs)| seqs.contains(&p.seq));
        out.lives.extend(lives_in_log(db, log_id, played_at, &which, &keep).await?);
    }
    Ok(out)
}

/// The lives of whoever played `class` on the other team in the rounds
/// `account_id` played it on `map` (Flashy: "where both snipers rollout"):
/// their opposite number, match by match. `None` mirrors whatever class they
/// were on in each round.
pub async fn opposite(db: &Db, account_id: u32, map: &str, class: Option<&str>) -> Result<LivesOnMap> {
    let mine = on_map(db, account_id, map).await?;
    // Their class and side in each round, by match.
    let mut rounds: BTreeMap<i64, (Option<i64>, HashMap<i64, Vec<(String, String)>>, HashMap<i64, bool>)> = BTreeMap::new();
    for l in &mine.lives {
        let (Some(c), Some(team), Some(round)) = (&l.class, &l.team, l.opening_round.or(l.round_num)) else { continue };
        if class.is_some_and(|want| want != c) {
            continue;
        }
        let e = rounds.entry(l.log_id).or_insert_with(|| (l.played_at, HashMap::new(), HashMap::new()));
        let in_round = e.1.entry(round).or_default();
        if !in_round.iter().any(|(cc, tt)| cc == c && tt == team) {
            in_round.push((c.clone(), team.clone()));
        }
        e.2.insert(l.demo_id, l.stv);
    }
    let mut out = LivesOnMap { map: mine.map, ..Default::default() };
    for (log_id, (played_at, by_round, demos)) in rounds {
        let which = one_view(demos.into_iter().collect());
        let keep = |p: &hl_db::PathRow| p.account_id != account_id;
        let theirs: Vec<LifeOnMap> = lives_in_log(db, log_id, played_at, &which, &keep)
            .await?
            .into_iter()
            .filter(|l| {
                let (Some(c), Some(team), Some(round)) = (&l.class, &l.team, l.opening_round.or(l.round_num)) else { return false };
                by_round.get(&round).is_some_and(|mine| mine.iter().any(|(mc, mt)| mc == c && mt != team))
            })
            .collect();
        if theirs.is_empty() {
            continue;
        }
        out.matches += 1;
        if theirs.iter().any(|l| l.stv) {
            out.stv_matches += 1;
        }
        out.lives.extend(theirs);
    }
    Ok(out)
}

/// Every life in one match that began with its round, everyone's: the match
/// page's round openings (Flashy: "i want to see this per match on the match
/// page too").
pub async fn match_openings(db: &Db, log_id: i64) -> Result<Vec<LifeOnMap>> {
    let linked = db.demos_for_log(log_id).await?;
    let which = one_view(linked.iter().map(|d| (d.demo_id, d.kind == "stv")).collect());
    let played_at = db.match_played_at(log_id).await?;
    let keep = |_: &hl_db::PathRow| true;
    Ok(lives_in_log(db, log_id, played_at, &which, &keep).await?.into_iter().filter(|l| !l.opening_t.is_empty()).collect())
}

/// SourceTV's demos when there are any, as they see everyone; a player's own
/// recording besides one would draw the same lives twice.
fn one_view(demos: Vec<(i64, bool)>) -> Vec<(i64, bool)> {
    if demos.iter().any(|&(_, stv)| stv) {
        demos.into_iter().filter(|&(_, stv)| stv).collect()
    } else {
        demos
    }
}

/// The lives `keep` picks in one match's `demos` (`(id, SourceTV's)`), each
/// with its class and side from the demo's timeline, its situations and its
/// opening.
async fn lives_in_log(db: &Db, log_id: i64, played_at: Option<i64>, demos: &[(i64, bool)], keep: &(dyn Fn(&hl_db::PathRow) -> bool + Sync)) -> Result<Vec<LifeOnMap>> {
    let mut out = Vec::new();
    let paths = db.paths_for_log(log_id).await?;
    // For the situation filters (Q67): the log's caps, rounds and kills,
    // and where each demo starts on the log's clock.
    let offset = db.log_clock_offset(log_id).await?;
    let caps = db.cap_events(log_id).await?;
    let rounds: Vec<(i64, f64, f64)> = db.round_spans(log_id).await?.into_iter().map(|(n, s, len)| (n, s as f64, (s + len) as f64)).collect();
    let kills: Vec<(i64, u32, u32)> = db.kills_for_log(log_id).await?.into_iter().filter(|k| k.live).map(|k| (k.at_raw, k.killer, k.victim)).collect();
    let linked = db.demos_for_log(log_id).await?;
    for &(demo_id, stv) in demos {
        let clock = linked.iter().find(|d| d.demo_id == demo_id).and_then(|d| Some((d.start_utc?, d.tick_rate?)));
        let lives: Vec<&hl_db::PathRow> = paths.iter().filter(|p| p.demo_id == demo_id && keep(p)).collect();
        if lives.is_empty() {
            continue;
        }
        let rate = 66.67;
        // Class and side from the timeline, at each life's middle.
        let who = match db.timeline(demo_id).await? {
            Some(row) => {
                let s = stored(row);
                let asks: Vec<(u32, u32)> = lives.iter().map(|l| (l.account_id, ((l.from_tick + l.to_tick) / 2).max(0) as u32)).collect();
                tokio::task::spawn_blocking(move || -> Result<Vec<(Option<u8>, Option<u8>)>> {
                    let tl = Timeline::decode(&s)?;
                    let slots: HashMap<u32, usize> = tl.people.iter().enumerate().filter_map(|(i, p)| Some((account_of(&p.steamid)?, i))).collect();
                    Ok(asks
                        .iter()
                        .map(|&(account, tick)| {
                            let (Some(&slot), Some(t)) = (slots.get(&account), tl.t_of(tick)) else { return (None, None) };
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
            let (after_enemy_cap, mid_fight) = situation(start, team, l.round_num, l.account_id, &caps, &rounds, &kills);
            let end = start.map(|s| s + (l.to_tick - l.from_tick) as f64 / rate);
            let (opening_t, opening_skip, opening_round, first_kill) = match opening(start, end, &rounds, &kills) {
                Some((round, offset, first_kill)) => {
                    let t: Vec<f64> = l.points.iter().map(|&(tick, ..)| offset + (tick - l.from_tick) as f64 / rate).collect();
                    let skip = t.iter().take_while(|&&t| t < 0.0).count();
                    let opening_t: Vec<f32> = t[skip..].iter().take_while(|&&t| t <= OPENING_S).map(|t| ((t * 10.0).round() / 10.0) as f32).collect();
                    (opening_t, skip, Some(round), first_kill.map(|k| ((k * 10.0).round() / 10.0) as f32))
                }
                None => (Vec::new(), 0, None, None),
            };
            out.push(LifeOnMap {
                log_id,
                played_at,
                demo_id,
                stv,
                account_id: l.account_id,
                class: class.and_then(class_name).map(str::to_string),
                team: team.map(str::to_string),
                round_num: l.round_num,
                died: l.died,
                seconds: ((l.to_tick - l.from_tick).max(0) as f64 / rate * 10.0).round() / 10.0,
                points: l.points.iter().map(|&(_, x, y, _)| (x, y)).collect(),
                capped: !l.caps.is_empty(),
                after_enemy_cap,
                mid_fight,
                opening_t,
                opening_skip,
                opening_round,
                first_kill,
            });
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
    fn a_life_there_when_its_round_began_and_the_rounds_first_kill() {
        let rounds = [(1, 100.0, 400.0), (2, 400.0, 700.0)];
        // Begun with the round, or 3 s late; the first kill 100 s in, or none.
        assert_eq!(opening(Some(100.0), Some(150.0), &rounds, &[(200, 1, 2)]), Some((1, 0.0, Some(100.0))));
        assert_eq!(opening(Some(103.0), Some(150.0), &rounds, &[]), Some((1, 3.0, None)));
        // Respawned in the pre-round freeze, 5 s before the log's start; alive
        // from the pregame: both the round's.
        assert_eq!(opening(Some(395.0), Some(450.0), &rounds, &[(390, 1, 2), (425, 3, 4)]), Some((2, -5.0, Some(25.0))));
        assert_eq!(opening(Some(20.0), Some(130.0), &rounds, &[]), Some((1, -80.0, None)));
        // A life begun mid-round, one over before the round began, a kill
        // before it began, no clock: none.
        assert_eq!(opening(Some(150.0), Some(200.0), &rounds, &[]), None);
        assert_eq!(opening(Some(20.0), Some(90.0), &rounds, &[]), None);
        assert_eq!(opening(Some(105.0), Some(150.0), &rounds, &[(102, 1, 2)]), None);
        assert_eq!(opening(None, None, &rounds, &[]), None);
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
