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
use tf_demo_parser::demo::parser::gamestateanalyser::GameStateAnalyser;
use tf_demo_parser::{Demo, DemoParser};

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
    /// Degrees the view turned over the [`FLICK_S`] before the shot.
    pub flick_deg: f32,
    /// Distance to the victim, and how far above the shooter they stood.
    pub range: f32,
    pub height: f32,
    /// Both players were carried by the demo for the whole window. A POV demo
    /// drops players it never showed, and their numbers would be stale.
    pub victim_seen: bool,
}

/// Every kill by `me` in the demo at `path`, with the aim behind it.
///
/// One pass. Only the last [`LEAD_S`] of ticks is held, so memory stays flat
/// however long the demo is.
pub fn shots(path: &Path, me: &str, tick_rate: f64) -> Result<Vec<Shot>> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let demo = Demo::new(&bytes);
    let (_, mut ticker) = DemoParser::new_with_analyser(demo.get_stream(), GameStateAnalyser::new())
        .ticker()
        .with_context(|| format!("opening {}", path.display()))?;

    let depth = ((LEAD_S * tick_rate).round() as usize).max(2);
    let flick_back = (FLICK_S * tick_rate).round() as usize;
    // The last `depth` ticks: the shooter's view, and where everyone stood.
    let mut recent: VecDeque<Frame> = VecDeque::with_capacity(depth + 1);
    let mut out: Vec<Shot> = Vec::new();
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
        let mut frame = Frame { tick, me: None, others: HashMap::new(), user_ids: HashMap::new() };
        for p in &state.players {
            let Some(info) = &p.info else { continue };
            let pos = [p.position.x, p.position.y, p.position.z];
            frame.user_ids.insert(u16::from(info.user_id), info.steam_id.clone());
            if info.steam_id == me {
                frame.me = Some((pos, p.view_angle, p.pitch_angle));
            }
            frame.others.insert(u16::from(info.user_id), (pos, p.in_pvs));
        }
        if recent.len() == depth {
            recent.pop_front();
        }
        recent.push_back(frame);

        // Kills are appended to the state as they happen, so anything new
        // since the last tick belongs to this moment, with the view still fresh.
        for kill in state.kills.iter().skip(done) {
            if let Some(shot) = shot_for(&recent, me, kill.attacker_id, kill.victim_id, &kill.weapon, flick_back) {
                out.push(shot);
            }
        }
        done = state.kills.len();
    }
    Ok(out)
}

struct Frame {
    tick: u32,
    /// The shooter: position, yaw, pitch.
    me: Option<(Pos, f32, f32)>,
    /// Everyone by user id: position, and whether the demo carried them.
    others: HashMap<u16, (Pos, bool)>,
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

    // A second earlier, against where the victim was then.
    let then = recent.front()?;
    let before = then
        .me
        .zip(then.others.get(&victim))
        .map_or(error, |((p, y, pi), &(vp, _))| angle_to(p, y, pi, vp));
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
            recent.push_back(Frame { tick, me: Some(([0.0, 0.0, 0.0], yaw, 0.0)), others, user_ids });
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
}
