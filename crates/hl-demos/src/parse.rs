//! Reading a demo's packets (PLAN §14): where every player stood and looked,
//! tick by tick.
//!
//! The rest of this crate reads only a demo's 1072-byte header. This module
//! reads the body, through `tf-demo-parser` (the crate behind demos.tf). It
//! is deliberately a thin layer: it walks the ticks, samples them, and hands
//! back plain numbers. Nothing here knows about logs, kills or ratings.
//!
//! A POV demo holds only what that player's client was sent, so other players
//! are missing while they are out of view (`in_pvs`). Samples say so rather
//! than pretending a position is known.

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use tf_demo_parser::demo::data::game_state::PlayerState;
use tf_demo_parser::demo::parser::gamestateanalyser::GameStateAnalyser;
use tf_demo_parser::{Demo, DemoParser};

/// Sample a tick every this many by default: about four a second at 66 tick,
/// which is enough for aim and movement without keeping every frame.
pub const DEFAULT_STRIDE: u32 = 16;

/// One player at one tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub tick: u32,
    /// Where they stood, in map units.
    pub pos: [f32; 3],
    /// Where they looked: yaw 0-360 and pitch, degrees.
    pub yaw: f32,
    pub pitch: f32,
    pub health: u16,
    pub alive: bool,
    /// False when the demo did not carry this player at this tick (a POV demo
    /// out of view): the position is the last one known, not a current one.
    pub known: bool,
}

/// A player the demo named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoPlayer {
    pub steamid: String,
    pub name: String,
    pub team: String,
    pub class: String,
    /// Ticks they were in the demo at all.
    pub ticks: u32,
}

/// What one pass over a demo found.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub map: String,
    /// The demo's own tick count, and the ticks actually walked.
    pub header_ticks: u32,
    pub ticks: u32,
    pub interval_per_tick: f32,
    pub players: Vec<DemoPlayer>,
    /// Samples for the player asked for, oldest first.
    pub samples: Vec<Sample>,
}

impl Scan {
    /// Seconds of play the demo covers, from its own tick interval.
    pub fn seconds(&self) -> f64 {
        f64::from(self.header_ticks) * f64::from(self.interval_per_tick)
    }
}

/// A view direction as a unit-ish vector, from Source's yaw and pitch in
/// degrees. Pitch is positive looking down, so its sign flips here.
pub fn view_dir(yaw: f32, pitch: f32) -> [f32; 3] {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    [p.cos() * y.cos(), p.cos() * y.sin(), -p.sin()]
}

/// Walk `path`, sampling every `stride` ticks. `steamid64` picks whose
/// samples come back; `None` collects none, which is the fastest way to see
/// who is in a demo.
///
/// Reads the whole file into memory: a 40-minute STV demo is about 50 MB.
pub fn scan(path: &Path, steamid64: Option<&str>, stride: u32) -> Result<Scan> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let demo = Demo::new(&bytes);
    let (header, mut ticker) = DemoParser::new_with_analyser(demo.get_stream(), GameStateAnalyser::new())
        .ticker()
        .with_context(|| format!("opening {}", path.display()))?;

    let stride = stride.max(1);
    let mut out = Scan {
        map: header.map.clone(),
        header_ticks: header.ticks,
        interval_per_tick: 0.0,
        ..Scan::default()
    };
    // Per player: how many ticks they appeared in, and their last name/class.
    let mut seen: HashMap<String, DemoPlayer> = HashMap::new();
    let mut last_tick = 0u32;

    while ticker.tick().with_context(|| format!("parsing {}", path.display()))? {
        let state = ticker.state();
        let tick = u32::from(state.tick);
        if tick == last_tick {
            continue;
        }
        last_tick = tick;
        out.ticks += 1;
        out.interval_per_tick = state.interval_per_tick;

        for p in &state.players {
            let Some(info) = &p.info else { continue };
            let e = seen.entry(info.steam_id.clone()).or_insert_with(|| DemoPlayer {
                steamid: info.steam_id.clone(),
                name: info.name.clone(),
                team: format!("{:?}", p.team),
                class: format!("{:?}", p.class),
                ticks: 0,
            });
            e.ticks += 1;
            e.name.clone_from(&info.name);
            e.team = format!("{:?}", p.team);
            if format!("{:?}", p.class) != "Other" {
                e.class = format!("{:?}", p.class);
            }

            if tick % stride == 0 && steamid64.is_some_and(|id| id == info.steam_id) {
                out.samples.push(Sample {
                    tick,
                    pos: [p.position.x, p.position.y, p.position.z],
                    yaw: p.view_angle,
                    pitch: p.pitch_angle,
                    health: p.health,
                    alive: p.state == PlayerState::Alive,
                    known: p.in_pvs,
                });
            }
        }
    }

    out.players = seen.into_values().collect();
    out.players.sort_by(|a, b| b.ticks.cmp(&a.ticks).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}
