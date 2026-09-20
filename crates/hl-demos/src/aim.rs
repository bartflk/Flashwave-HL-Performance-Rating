//! Aim, read from a demo (PLAN §14).
//!
//! The demo carries its own kill events, each stamped with the exact tick, so
//! the shot itself needs no guessing from log times (those are only good to
//! the second, and a demo's start is itself an estimate). For every kill by
//! the player, three numbers come out:
//!
//! - **Crosshair error**: the angle between where they looked and the line to
//!   the victim's head, in degrees. Small means the crosshair was already
//!   there; large means it had to travel.
//! - **Flick**: how far the view turned in the half second before the shot.
//!   A small error after a large flick is a reaction; a small error after no
//!   flick is placement.
//! - **Range**: the distance to the victim at the shot, in map units, and the
//!   height difference. Exact here, unlike the log's rounded positions.
//!
//! Angles use Source's convention: yaw counts anticlockwise from +x, pitch is
//! positive looking **down**. A player's origin is at their feet, so the head
//! is [`HEAD_HEIGHT`] above it.

use crate::parse::view_dir;
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use tf_demo_parser::demo::data::game_state::{PlayerCondition, PlayerState};
use tf_demo_parser::demo::parser::gamestateanalyser::GameStateAnalyser;
use tf_demo_parser::{Demo, DemoParser};

/// A teammate this close is covering you: a Scout's fight is about this wide,
/// and a Medic on you is far closer.
pub const MATE_NEAR_UNITS: f32 = 900.0;

/// Eyes above the origin for a standing player. TF2's standing view offset.
pub const HEAD_HEIGHT: f32 = 68.0;

/// How long before the shot the "before" reading is taken.
pub const LEAD_S: f64 = 1.0;
/// The window the flick is measured over.
pub const FLICK_S: f64 = 0.5;

/// What the demo says about one kill.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    /// The demo tick of the kill, from the demo's own kill event.
    pub tick: u32,
    /// The victim, as the demo writes SteamIDs (`[U:1:139131191]`).
    pub victim: String,
    /// The weapon the demo names, which is not always the log's name.
    pub weapon: String,
    /// Degrees between the view and the victim's head at the shot, and
    /// [`LEAD_S`] before it.
    pub error_deg: f32,
    pub error_before_deg: f32,
    /// The same miss split into sideways and vertical degrees, so it can be
    /// drawn on a target: positive is right of the head and above it.
    pub dx_deg: f32,
    pub dy_deg: f32,
    pub before_dx_deg: f32,
    pub before_dy_deg: f32,
    /// Degrees the view turned over the [`FLICK_S`] before the shot.
    pub flick_deg: f32,
    /// Distance to the victim, and how far above the shooter they stood.
    pub range: f32,
    pub height: f32,
    /// Both players were carried by the demo for the whole window. A POV demo
    /// drops players it never showed, and their numbers would be stale.
    pub victim_seen: bool,
}

/// How you died, as the demo saw it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Death {
    pub tick: u32,
    /// Who killed you, as the demo writes SteamIDs, and how far away they were
    /// (`None` when the demo never carried them: the usual case for a Spy).
    pub killer: String,
    pub killer_range: Option<f32>,
    /// The nearest living teammate, and how many were within
    /// [`MATE_NEAR_UNITS`]. `None` when the demo carried no teammate at all.
    pub nearest_mate: Option<f32>,
    pub mates_near: u8,
    /// You were scoped in at some point in the second before it. The tick of
    /// the death itself is no good: dying clears the condition.
    pub scoped: bool,
}

/// One pass over a demo: the kills, the deaths, and how the time was spent.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pass {
    pub shots: Vec<Shot>,
    pub deaths: Vec<Death>,
    /// Ticks you were alive, and of those, ticks you were scoped in.
    pub alive_ticks: u32,
    pub scoped_ticks: u32,
}

impl Pass {
    /// The share of your living time spent scoped, where there was any.
    pub fn scoped_share(&self) -> Option<f64> {
        (self.alive_ticks > 0).then(|| f64::from(self.scoped_ticks) / f64::from(self.alive_ticks))
    }
}

/// Read the demo at `path` once: every kill by `me` with the aim behind it,
/// every death with who was nearby, and how much of the time was scoped.
///
/// Only the last [`LEAD_S`] of ticks is held, so memory stays flat however
/// long the demo is.
pub fn pass(path: &Path, me: &str, tick_rate: f64) -> Result<Pass> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let demo = Demo::new(&bytes);
    let (_, mut ticker) = DemoParser::new_with_analyser(demo.get_stream(), GameStateAnalyser::new())
        .ticker()
        .with_context(|| format!("opening {}", path.display()))?;

    let depth = ((LEAD_S * tick_rate).round() as usize).max(2);
    let flick_back = (FLICK_S * tick_rate).round() as usize;
    // The last `depth` ticks: the shooter's view, and where everyone stood.
    let mut recent: VecDeque<Frame> = VecDeque::with_capacity(depth + 1);
    let mut out = Pass::default();
    let mut done = 0usize;
    let mut last = u32::MAX;

    while ticker.tick().with_context(|| format!("parsing {}", path.display()))? {
        let state = ticker.state();
        let tick = u32::from(state.tick);
        if tick == last {
            continue;
        }
        last = tick;

        // This tick: the shooter's view, and every player's position by user id.
        let mut frame = Frame {
            tick,
            me: None,
            my_team: None,
            scoped: false,
            others: HashMap::new(),
            mates: Vec::new(),
            user_ids: HashMap::new(),
        };
        for p in &state.players {
            let Some(info) = &p.info else { continue };
            let pos = [p.position.x, p.position.y, p.position.z];
            frame.user_ids.insert(u16::from(info.user_id), info.steam_id.clone());
            if info.steam_id == me {
                frame.me = Some((pos, p.view_angle, p.pitch_angle));
                frame.my_team = Some(p.team);
                frame.scoped = p.has_condition(PlayerCondition::Zoomed);
                if p.state == PlayerState::Alive {
                    out.alive_ticks += 1;
                    out.scoped_ticks += u32::from(frame.scoped);
                }
            }
            frame.others.insert(u16::from(info.user_id), (pos, p.in_pvs));
        }
        // Living teammates, for "was anyone watching my flank".
        for p in &state.players {
            if p.info.as_ref().is_some_and(|i| i.steam_id != me)
                && Some(p.team) == frame.my_team
                && p.state == PlayerState::Alive
                && p.in_pvs
            {
                frame.mates.push([p.position.x, p.position.y, p.position.z]);
            }
        }
        if recent.len() == depth {
            recent.pop_front();
        }
        recent.push_back(frame);

        // Kills are appended to the state as they happen, so anything new
        // since the last tick belongs to this moment, with the view still fresh.
        for kill in state.kills.iter().skip(done) {
            if let Some(shot) = shot_for(&recent, me, kill.attacker_id, kill.victim_id, &kill.weapon, flick_back) {
                out.shots.push(shot);
            }
            if let Some(death) = death_for(&recent, me, kill.attacker_id, kill.victim_id) {
                out.deaths.push(death);
            }
        }
        done = state.kills.len();
    }
    Ok(out)
}

/// Your death, from the tick it happened on.
fn death_for(recent: &VecDeque<Frame>, me: &str, attacker: u16, victim: u16) -> Option<Death> {
    let now = recent.back()?;
    if now.user_ids.get(&victim).map(String::as_str) != Some(me) {
        return None;
    }
    let (pos, ..) = now.me?;
    let killer = now.user_ids.get(&attacker).cloned().unwrap_or_default();
    let killer_range = now
        .others
        .get(&attacker)
        .filter(|(_, seen)| *seen)
        .map(|&(p, _)| dist(pos, p));
    let mut nearest: Option<f32> = None;
    let mut near = 0u8;
    for &m in &now.mates {
        let d = dist(pos, m);
        nearest = Some(nearest.map_or(d, |n: f32| n.min(d)));
        if d <= MATE_NEAR_UNITS {
            near += 1;
        }
    }
    let scoped = recent.iter().any(|f| f.scoped);
    Some(Death { tick: now.tick, killer, killer_range, nearest_mate: nearest, mates_near: near, scoped })
}

struct Frame {
    tick: u32,
    /// The shooter: position, yaw, pitch.
    me: Option<(Pos, f32, f32)>,
    my_team: Option<tf_demo_parser::demo::parser::analyser::Team>,
    /// The shooter was scoped in on this tick.
    scoped: bool,
    /// Everyone by user id: position, and whether the demo carried them.
    others: HashMap<u16, (Pos, bool)>,
    /// Living teammates the demo carried, for the distance to the nearest.
    mates: Vec<Pos>,
    user_ids: HashMap<u16, String>,
}

type Pos = [f32; 3];

/// The aim behind one kill, from the ticks leading up to it.
fn shot_for(recent: &VecDeque<Frame>, me: &str, attacker: u16, victim: u16, weapon: &str, flick_back: usize) -> Option<Shot> {
    let now = recent.back()?;
    // Ours only, and not a suicide.
    if now.user_ids.get(&attacker).map(String::as_str) != Some(me) || attacker == victim {
        return None;
    }
    let (pos, yaw, pitch) = now.me?;
    let &(victim_pos, in_pvs) = now.others.get(&victim)?;
    let error = angle_to(pos, yaw, pitch, victim_pos);
    let (dx, dy) = offset_to(pos, yaw, pitch, victim_pos);

    // A second earlier, against where the victim was then.
    let then = recent.front()?;
    let earlier = then.me.zip(then.others.get(&victim));
    let before = earlier.map_or(error, |((p, y, pi), &(vp, _))| angle_to(p, y, pi, vp));
    let (before_dx, before_dy) = earlier.map_or((dx, dy), |((p, y, pi), &(vp, _))| offset_to(p, y, pi, vp));
    let before_seen = then.others.get(&victim).is_some_and(|&(_, seen)| seen);

    // The flick: how far the view turned over the last FLICK_S.
    let flick_from = recent.len().saturating_sub(flick_back + 1);
    let flick = recent
        .get(flick_from)
        .and_then(|f| f.me)
        .map_or(0.0, |(_, y, p)| angle_between(view_dir(y, p), view_dir(yaw, pitch)));

    Some(Shot {
        tick: now.tick,
        victim: now.user_ids.get(&victim).cloned().unwrap_or_default(),
        weapon: weapon.to_string(),
        error_deg: error,
        error_before_deg: before,
        dx_deg: dx,
        dy_deg: dy,
        before_dx_deg: before_dx,
        before_dy_deg: before_dy,
        flick_deg: flick,
        range: dist(pos, victim_pos),
        height: victim_pos[2] - pos[2],
        victim_seen: in_pvs && before_seen,
    })
}

/// The angle between where a player looks and their line to a head, degrees.
fn angle_to(from: Pos, yaw: f32, pitch: f32, target: Pos) -> f32 {
    let eye = [from[0], from[1], from[2] + HEAD_HEIGHT];
    let head = [target[0], target[1], target[2] + HEAD_HEIGHT];
    let to = [head[0] - eye[0], head[1] - eye[1], head[2] - eye[2]];
    angle_between(view_dir(yaw, pitch), to)
}

/// Where the head sat relative to the crosshair, in degrees: sideways first
/// (positive to the right of the view), then vertical (positive above it).
/// Their combination is [`angle_to`], give or take the usual rounding.
fn offset_to(from: Pos, yaw: f32, pitch: f32, target: Pos) -> (f32, f32) {
    let eye = [from[0], from[1], from[2] + HEAD_HEIGHT];
    let head = [target[0], target[1], target[2] + HEAD_HEIGHT];
    let (dx, dy, dz) = (head[0] - eye[0], head[1] - eye[1], head[2] - eye[2]);
    let flat = (dx * dx + dy * dy).sqrt();
    if flat == 0.0 && dz == 0.0 {
        return (0.0, 0.0);
    }
    // Where the head is, in the same angles the view uses.
    let head_yaw = dy.atan2(dx).to_degrees();
    let head_pitch = -dz.atan2(flat).to_degrees();
    (wrap180(head_yaw - yaw), pitch - head_pitch)
}

/// An angle difference folded into -180..180 degrees.
fn wrap180(deg: f32) -> f32 {
    let d = (deg + 180.0).rem_euclid(360.0) - 180.0;
    if d == -180.0 {
        180.0
    } else {
        d
    }
}

/// The angle between two vectors, degrees. Zero-length vectors give 0.
fn angle_between(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt() * (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    if len == 0.0 {
        return 0.0;
    }
    (dot / len).clamp(-1.0, 1.0).acos().to_degrees()
}

fn dist(a: Pos, b: Pos) -> f32 {
    let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looking_straight_at_a_head_is_no_error() {
        // Standing at the origin, looking along +x at someone 500 units away.
        let e = angle_to([0.0, 0.0, 0.0], 0.0, 0.0, [500.0, 0.0, 0.0]);
        assert!(e < 0.01, "{e}");
    }

    #[test]
    fn looking_past_them_is_the_angle_past_them() {
        // They are 500 units along +x; the view is 30 degrees off.
        let e = angle_to([0.0, 0.0, 0.0], 30.0, 0.0, [500.0, 0.0, 0.0]);
        assert!((e - 30.0).abs() < 0.01, "{e}");
        // Above them: the target is 500 up and 500 along, so 45 degrees.
        let e = angle_to([0.0, 0.0, 0.0], 0.0, 0.0, [500.0, 0.0, 500.0]);
        assert!((e - 45.0).abs() < 0.01, "{e}");
    }

    #[test]
    fn pitch_is_positive_looking_down() {
        // Someone 500 along and 500 below: looking 45 degrees down is exact.
        let e = angle_to([0.0, 0.0, 0.0], 0.0, 45.0, [500.0, 0.0, -500.0]);
        assert!(e < 0.01, "{e}");
    }

    #[test]
    fn the_miss_splits_into_sideways_and_vertical_degrees() {
        // Looking along +x; the head is 30 degrees to the left (+y is left of
        // +x in Source's anticlockwise yaw), so the offset is +30 sideways.
        let (dx, dy) = offset_to([0.0, 0.0, 0.0], 0.0, 0.0, [500.0, 289.0, 0.0]);
        assert!((dx - 30.0).abs() < 0.2, "{dx}");
        assert!(dy.abs() < 0.01, "{dy}");
        // Level view, head 500 above: the crosshair sits 45 degrees below it.
        let (dx, dy) = offset_to([0.0, 0.0, 0.0], 0.0, 0.0, [500.0, 0.0, 500.0]);
        assert!(dx.abs() < 0.01, "{dx}");
        assert!((dy - 45.0).abs() < 0.2, "{dy}");
        // Looking right at them: no miss either way.
        let (dx, dy) = offset_to([0.0, 0.0, 0.0], 0.0, 45.0, [500.0, 0.0, -500.0]);
        assert!(dx.abs() < 0.01 && dy.abs() < 0.2, "{dx} {dy}");
    }

    #[test]
    fn angles_wrap_the_short_way_round() {
        assert!((wrap180(350.0) + 10.0).abs() < 1e-6);
        assert!((wrap180(-350.0) - 10.0).abs() < 1e-6);
        assert!((wrap180(10.0) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn a_flick_is_the_turn_between_two_views() {
        assert!((angle_between(view_dir(0.0, 0.0), view_dir(90.0, 0.0)) - 90.0).abs() < 0.01);
        assert!(angle_between(view_dir(10.0, -5.0), view_dir(10.0, -5.0)) < 0.01);
    }

    /// Two ticks a second apart: the shooter turns onto a Sniper and kills
    /// them. The kill is ours, the view is on the head, and the turn is the
    /// flick.
    #[test]
    fn a_kill_reads_its_ticks() {
        let me = "[U:1:1]";
        let mut recent = VecDeque::new();
        for (tick, yaw) in [(0u32, 60.0f32), (66, 0.0)] {
            let mut others = HashMap::new();
            others.insert(2, ([500.0, 0.0, 0.0], true));
            let mut user_ids = HashMap::new();
            user_ids.insert(1, me.to_string());
            user_ids.insert(2, "[U:1:2]".to_string());
            recent.push_back(Frame {
                tick,
                me: Some(([0.0, 0.0, 0.0], yaw, 0.0)),
                my_team: None,
                scoped: false,
                others,
                mates: vec![[300.0, 0.0, 0.0], [2_000.0, 0.0, 0.0]],
                user_ids,
            });
        }
        let shot = shot_for(&recent, me, 1, 2, "sniperrifle", 1).expect("our kill");
        assert!(shot.error_deg < 0.01, "on the head: {}", shot.error_deg);
        assert!((shot.error_before_deg - 60.0).abs() < 0.01, "a second earlier: {}", shot.error_before_deg);
        assert!((shot.flick_deg - 60.0).abs() < 0.01, "the turn: {}", shot.flick_deg);
        assert!((shot.range - 500.0).abs() < 0.01);
        assert!(shot.victim_seen);
        // Someone else's kill, and our own death, are not ours.
        assert!(shot_for(&recent, me, 2, 1, "x", 1).is_none());
        assert!(shot_for(&recent, me, 1, 1, "x", 1).is_none());
    }

    /// The same two ticks, read as a death instead: who killed us, how far
    /// away they were, and who was near enough to help.
    #[test]
    fn a_death_reads_who_was_nearby() {
        let me = "[U:1:1]";
        let mut recent = VecDeque::new();
        let mut others = HashMap::new();
        others.insert(2, ([500.0, 0.0, 0.0], true));
        let mut user_ids = HashMap::new();
        user_ids.insert(1, me.to_string());
        user_ids.insert(2, "[U:1:2]".to_string());
        recent.push_back(Frame {
            tick: 10,
            me: Some(([0.0, 0.0, 0.0], 0.0, 0.0)),
            my_team: None,
            scoped: true,
            others,
            mates: vec![[300.0, 0.0, 0.0], [2_000.0, 0.0, 0.0]],
            user_ids,
        });
        let d = death_for(&recent, me, 2, 1).expect("our death");
        assert_eq!(d.killer, "[U:1:2]");
        assert_eq!(d.killer_range, Some(500.0));
        assert_eq!(d.nearest_mate, Some(300.0));
        assert_eq!(d.mates_near, 1, "the other teammate is 2,000 units away");
        assert!(d.scoped);
        // Our own kill is not a death.
        assert!(death_for(&recent, me, 1, 2).is_none());
    }
}
