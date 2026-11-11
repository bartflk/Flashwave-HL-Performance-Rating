//! A demo, kept (Q3).
//!
//! Every pass that reads a demo -- aim, deaths, routes, spychecks -- asks its
//! own question of the file and keeps only the answer. The next question means
//! reading the file again, and a demo deleted to save space (Q23) can never be
//! asked anything new. A timeline is the other way round: one pass records
//! what the demo holds, compactly, and later questions are answered from the
//! record without the file.
//!
//! What is kept:
//!
//! - **Samples**, every `stride` ticks, for every player alive and carried by
//!   the demo: position and view angles. This is the bulk, so it is stored as
//!   per-player deltas (a few bytes a sample) and deflated.
//! - **Changes**, at the exact tick they happen: health, class, team, alive,
//!   carried (`in_pvs`), the full condition bits (scoped, cloaked, ubered...),
//!   Medic charge, medigun and heal target, Spy cloak and disguise. State that
//!   changes rarely costs nothing between changes, and nothing is sampled away.
//! - **Objects**, on change: the payload cart's position and every building's
//!   level, health, state and place; and every reflect (v2): who sent which
//!   projectile back, from where, which way it had been flying, and where it
//!   ended.
//! - **Ping** (v2), per player on change: the scoreboard's, about once a
//!   second.
//! - **Events**: every game event the demo carried (hurts, deaths, captures,
//!   spawns, charges), exactly as the parser decoded them.
//!
//! Time is one counter, `t`, that keeps counting when the demo's own tick
//! restarts (a recording holding two matches). `seams` maps it back: each is
//! `(t, tick)` where a stretch starts, so `tick = seam.tick + (t - seam.t)`.

use crate::deep::{Conds, DeepAnalyser};
use anyhow::{bail, Context, Result};
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Write};
use tf_demo_parser::demo::data::game_state::{Building, PlayerClassData, PlayerState};
// What a timeline's readers need to ask it questions, without depending on
// the parser themselves.
pub use tf_demo_parser::demo::data::game_state::PlayerCondition;
pub use tf_demo_parser::demo::gamevent::GameEvent;

/// Bump when what is recorded, or how, changes: stored timelines below it
/// are recorded again the next time their demo is read.
///
/// 2: ping per player, and reflects (Q44, Q45).
pub const TIMELINE_VERSION: i64 = 2;

/// The oldest stored format [`Timeline::decode`] still reads. Versions so far
/// only added fields and event kinds, so 1 reads under the 2 layout (it just
/// lacks ping and reflects). Raise this the day a bump changes a layout:
/// older rows then fail with "needs reading again" instead of being misread
/// as the new one, and the aim pass records them again (audit SY-6).
pub const MIN_READABLE_VERSION: i64 = 1;

/// Ticks between samples: every one. Measured on three demos (PLAN §14b):
/// recomputing "a second before" from a timeline matched the demo to 0.05°
/// on average at every tick, and drifted to 1-2° with outliers past 100° at
/// every second or fourth, for a saving of a quarter to a half of 2-3 MB. The
/// questions still to come -- reaction time, flicks -- live in exactly those
/// ticks.
pub const DEFAULT_STRIDE: u32 = 1;

/// A player, once per demo however many times they reconnected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Person {
    pub steamid: String,
    pub name: String,
}

/// One sample of where a player stood and looked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub t: u32,
    /// Map units, rounded to the unit.
    pub pos: [f32; 3],
    /// Degrees, to within 0.006.
    pub yaw: f32,
    /// Degrees, positive looking down, to within 0.01.
    pub pitch: f32,
}

/// One piece of a player's state, from tick `t` until the next change of the
/// same kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Field {
    Health(u16),
    MaxHealth(u16),
    /// `tf_demo_parser`'s class number: 1 Scout ... 9 Engineer.
    Class(u8),
    /// 2 RED, 3 BLU.
    Team(u8),
    Alive(bool),
    /// Whether the demo is carrying this player: always for an STV, only in
    /// view for a POV demo's other players.
    Here(bool),
    Conds(Conds),
    /// Uber percentage.
    Charge(u8),
    Medigun(u8),
    /// Slot of the player being healed.
    HealTarget(Option<u16>),
    /// Spy cloak, percent.
    Cloak(u8),
    DisguiseClass(u8),
    DisguiseTeam(u8),
    /// Milliseconds, as the scoreboard shows it (v2).
    Ping(u16),
}

/// Kinds of [`Field`]: the size of the recorder's "last value" table.
const FIELD_KINDS: usize = 14;

impl Field {
    fn kind(&self) -> u8 {
        match self {
            Field::Health(_) => 0,
            Field::MaxHealth(_) => 1,
            Field::Class(_) => 2,
            Field::Team(_) => 3,
            Field::Alive(_) => 4,
            Field::Here(_) => 5,
            Field::Conds(_) => 6,
            Field::Charge(_) => 7,
            Field::Medigun(_) => 8,
            Field::HealTarget(_) => 9,
            Field::Cloak(_) => 10,
            Field::DisguiseClass(_) => 11,
            Field::DisguiseTeam(_) => 12,
            Field::Ping(_) => 13,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    pub t: u32,
    pub field: Field,
}

/// Everything recorded about one player.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub samples: Vec<Sample>,
    pub changes: Vec<Change>,
}

/// A cart or a building, as it stood from `t` until its next row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectRow {
    pub t: u32,
    /// The demo's entity index: the same object across rows.
    pub entity: u32,
    /// "cart", "sentry", "dispenser", "teleporter", or "gone". Borrowed
    /// while recording, so a row per object per tick allocates nothing.
    pub kind: Cow<'static, str>,
    pub pos: [i32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<u16>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub building: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sapped: bool,
    /// A reflect's player, by slot ("reflect" rows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<u16>,
    /// What was reflected: "rocket", "pipe", "sticky", "flare"...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what: Option<Cow<'static, str>>,
    /// Units a second, the way it was flying before it was reflected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vel: Option<[i32; 3]>,
}

impl ObjectRow {
    /// Everything but the time matches: the object has not changed.
    fn same_as(&self, other: &ObjectRow) -> bool {
        self.entity == other.entity
            && self.kind == other.kind
            && self.pos == other.pos
            && self.builder == other.builder
            && self.team == other.team
            && self.level == other.level
            && self.health == other.health
            && self.building == other.building
            && self.sapped == other.sapped
            && self.by == other.by
            && self.what == other.what
            && self.vel == other.vel
    }
}

/// A demo, decoded from what was stored.
#[derive(Debug, Clone, Default)]
pub struct Timeline {
    pub version: i64,
    pub tick_rate: f64,
    pub stride: u32,
    /// `(t, tick)` where each stretch of the demo's own ticks starts.
    pub seams: Vec<(u32, u32)>,
    /// Slot `i` is `people[i]`.
    pub people: Vec<Person>,
    pub tracks: Vec<Track>,
    /// The demo's user ids, which events use, to slots.
    pub user_ids: Vec<(u16, u16)>,
    /// Entity indices to slots, every pairing seen: a capture names its
    /// cappers by entity (Q18). Empty for timelines recorded before.
    pub entities: Vec<(u32, u16)>,
    pub objects: Vec<ObjectRow>,
    pub events: Vec<(u32, GameEvent)>,
}

impl Timeline {
    pub fn slot_of(&self, steamid: &str) -> Option<usize> {
        self.people.iter().position(|p| p.steamid == steamid)
    }

    /// The slots that have played on entity `entity`.
    pub fn slots_of_entity(&self, entity: u32) -> impl Iterator<Item = usize> + '_ {
        self.entities.iter().filter(move |(e, _)| *e == entity).map(|(_, s)| usize::from(*s))
    }

    pub fn slot_of_user(&self, user_id: u16) -> Option<usize> {
        self.user_ids.iter().find(|(u, _)| *u == user_id).map(|(_, s)| usize::from(*s))
    }

    /// The demo's own tick for `t`.
    pub fn tick_of(&self, t: u32) -> u32 {
        let seam = self.seams.iter().rev().find(|(st, _)| *st <= t).copied().unwrap_or((0, 0));
        seam.1 + (t - seam.0)
    }

    /// `t` for the demo's own `tick`, in the stretch that holds it. A tick
    /// that two stretches both hold (a demo with a restart) resolves to the
    /// first; pass the stretch explicitly with [`Timeline::t_in`] when that
    /// matters.
    pub fn t_of(&self, tick: u32) -> Option<u32> {
        (0..self.seams.len()).find_map(|i| self.t_in(i, tick))
    }

    pub fn t_in(&self, stretch: usize, tick: u32) -> Option<u32> {
        let (st, sk) = *self.seams.get(stretch)?;
        let end = self.seams.get(stretch + 1).map(|(t, _)| *t).unwrap_or(u32::MAX);
        let t = st.checked_add(tick.checked_sub(sk)?)?;
        (t < end).then_some(t)
    }

    /// The last change of `kind`'s sort at or before `t`, for `slot`.
    pub fn at(&self, slot: usize, t: u32, kind: fn(&Field) -> bool) -> Option<Field> {
        let changes = &self.tracks.get(slot)?.changes;
        let end = changes.partition_point(|c| c.t <= t);
        changes[..end].iter().rev().find(|c| kind(&c.field)).map(|c| c.field)
    }

    /// The sample nearest `t` for `slot`, if one is within `within` ticks.
    pub fn sample_near(&self, slot: usize, t: u32, within: u32) -> Option<Sample> {
        let s = &self.tracks.get(slot)?.samples;
        let i = s.partition_point(|x| x.t < t);
        [i.checked_sub(1), Some(i)]
            .into_iter()
            .flatten()
            .filter_map(|j| s.get(j))
            .filter(|x| x.t.abs_diff(t) <= within)
            .min_by_key(|x| x.t.abs_diff(t))
            .copied()
    }
}

/// The stored form: what goes in the `demo_timeline` row.
#[derive(Debug, Clone, Default)]
pub struct Stored {
    pub version: i64,
    pub tick_rate: f64,
    pub stride: u32,
    /// JSON: people, user ids and seams. Small, and readable in the database.
    pub head: String,
    /// Deflated.
    pub samples: Vec<u8>,
    pub changes: Vec<u8>,
    pub objects: Vec<u8>,
    pub events: Vec<u8>,
    /// What the four blobs came to before deflating, for the Settings count.
    pub raw_bytes: usize,
}

impl Stored {
    pub fn stored_bytes(&self) -> usize {
        self.head.len() + self.samples.len() + self.changes.len() + self.objects.len() + self.events.len()
    }
}

#[derive(Serialize, Deserialize)]
struct Head {
    people: Vec<Person>,
    user_ids: Vec<(u16, u16)>,
    seams: Vec<(u32, u32)>,
    /// Entity index to slot, every pairing seen. Older timelines have none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    entities: Vec<(u32, u16)>,
}

// ---- Recording ------------------------------------------------------------

/// One player's samples as delta-encoded bytes, written as they arrive.
///
/// The recorder used to keep every sample as floats -- 2.4 million of them in
/// an hour-long demo, 60 MB -- and encode them all at the end, holding both at
/// once. Writing the deltas as they come keeps a fifth of that, and
/// [`Timeline::encode`] uses this same writer, so recording and re-encoding
/// produce the same bytes by construction.
#[derive(Default)]
struct SampleWriter {
    buf: Vec<u8>,
    count: u64,
    t: i64,
    p: [i64; 3],
    yaw: i64,
    pitch: i64,
}

impl SampleWriter {
    fn push(&mut self, x: &Sample) {
        put_var(&mut self.buf, (i64::from(x.t) - self.t) as u64);
        self.t = i64::from(x.t);
        for (last, pos) in self.p.iter_mut().zip(x.pos) {
            let v = pos.round() as i64;
            put_zig(&mut self.buf, v - *last);
            *last = v;
        }
        let y = yaw_step(x.yaw);
        // The short way round, so turning through 0 is a small step.
        let mut dy = y - self.yaw;
        if dy > 32768 {
            dy -= 65536;
        } else if dy < -32768 {
            dy += 65536;
        }
        put_zig(&mut self.buf, dy);
        self.yaw = y;
        let pi = (x.pitch * PITCH_STEPS).round() as i64;
        put_zig(&mut self.buf, pi - self.pitch);
        self.pitch = pi;
        self.count += 1;
    }
}

/// One player's changes as `(dt, kind, value)` bytes, written as they arrive.
#[derive(Default)]
struct ChangeWriter {
    buf: Vec<u8>,
    count: u64,
    t: u32,
}

impl ChangeWriter {
    fn push(&mut self, ch: &Change) {
        let c = &mut self.buf;
        put_var(c, u64::from(ch.t - self.t));
        self.t = ch.t;
        c.push(ch.field.kind());
        match ch.field {
            Field::Health(v) | Field::MaxHealth(v) | Field::Ping(v) => put_var(c, u64::from(v)),
            Field::Class(v)
            | Field::Team(v)
            | Field::Charge(v)
            | Field::Medigun(v)
            | Field::Cloak(v)
            | Field::DisguiseClass(v)
            | Field::DisguiseTeam(v) => c.push(v),
            Field::Alive(v) | Field::Here(v) => c.push(u8::from(v)),
            Field::Conds(bits) => c.extend_from_slice(&bits),
            Field::HealTarget(v) => put_var(c, v.map_or(0, |s| u64::from(s) + 1)),
        }
        self.count += 1;
    }
}

/// The sample or change blob: how many players, then each one's count and
/// bytes.
fn assemble<'a>(n: usize, tracks: impl Iterator<Item = (u64, &'a [u8])>) -> Vec<u8> {
    let mut out = Vec::new();
    put_var(&mut out, n as u64);
    for (count, bytes) in tracks {
        put_var(&mut out, count);
        out.extend_from_slice(bytes);
    }
    out
}

/// Builds a timeline one tick at a time, from inside a pass that is already
/// walking the demo, writing the stored form as it goes.
pub struct Recorder {
    stride: u32,
    tick_rate: f64,
    t: u32,
    /// The demo tick seen last, to spot a restart.
    last_tick: Option<u32>,
    seams: Vec<(u32, u32)>,
    people: Vec<Person>,
    slots: HashMap<String, u16>,
    user_ids: HashMap<u16, u16>,
    entities: std::collections::BTreeSet<(u32, u16)>,
    samples: Vec<SampleWriter>,
    changes: Vec<ChangeWriter>,
    /// The last value of each field kind per slot, to record only changes.
    last: Vec<[Option<Field>; FIELD_KINDS]>,
    /// Reflects and their ends already written, from the analyser's lists.
    reflects_done: usize,
    ends_done: usize,
    events_done: usize,
    /// Events as JSON lines, written straight from the parser's copy: no
    /// clone of each event is kept.
    events: Vec<u8>,
    event_count: usize,
    events_unwritten: usize,
    objects: Vec<u8>,
    object_count: usize,
    object_last: HashMap<u32, ObjectRow>,
    // Scratch reused every tick instead of allocated: the players this tick
    // as (slot, index into the parser's list), entity -> slot for heal
    // targets, and which objects were seen.
    rows: Vec<(u16, usize)>,
    by_entity: HashMap<u32, u16>,
    seen: Vec<u32>,
}

impl Recorder {
    pub fn new(stride: u32, tick_rate: f64) -> Self {
        Recorder {
            stride: stride.max(1),
            tick_rate,
            t: 0,
            last_tick: None,
            seams: Vec::new(),
            people: Vec::new(),
            slots: HashMap::new(),
            entities: std::collections::BTreeSet::new(),
            user_ids: HashMap::new(),
            samples: Vec::new(),
            changes: Vec::new(),
            last: Vec::new(),
            events_done: 0,
            events: Vec::new(),
            event_count: 0,
            events_unwritten: 0,
            objects: Vec::new(),
            object_count: 0,
            object_last: HashMap::new(),
            reflects_done: 0,
            ends_done: 0,
            rows: Vec::new(),
            by_entity: HashMap::new(),
            seen: Vec::new(),
        }
    }

    /// The slot for this player. Almost always answered from their user id
    /// with one string compare, not a hash of the SteamID per player per
    /// tick.
    fn slot(&mut self, user_id: u16, steamid: &str, name: &str) -> u16 {
        if let Some(&s) = self.user_ids.get(&user_id) {
            if self.people[usize::from(s)].steamid == steamid {
                return s;
            }
        }
        let s = match self.slots.get(steamid) {
            Some(&s) => s,
            None => {
                let s = self.people.len() as u16;
                self.people.push(Person { steamid: steamid.to_string(), name: name.to_string() });
                self.slots.insert(steamid.to_string(), s);
                self.samples.push(SampleWriter::default());
                self.changes.push(ChangeWriter::default());
                self.last.push([None; FIELD_KINDS]);
                s
            }
        };
        self.user_ids.insert(user_id, s);
        s
    }

    fn set(&mut self, slot: u16, field: Field) {
        let i = usize::from(slot);
        let k = usize::from(field.kind());
        if self.last[i][k] != Some(field) {
            self.last[i][k] = Some(field);
            self.changes[i].push(&Change { t: self.t, field });
        }
    }

    /// Record the tick the analyser is on. `owner` is a POV demo's recorder,
    /// who counts as carried whatever `in_pvs` says; empty for an STV.
    pub fn observe(&mut self, a: &DeepAnalyser, owner: &str) {
        let state = a.state();
        let tick = u32::from(state.tick);
        match self.last_tick {
            Some(last) if tick == last => return,
            Some(last) if tick > last => self.t += tick - last,
            // A restart: a new stretch, `t` moving on by one so no two
            // stretches share a moment. Except when the stretch before never
            // got past its first tick -- a demo opens with one sign-on packet
            // stamped with some unrelated tick before play starts at 1 -- in
            // which case that stretch was not one and this replaces it.
            Some(_) => {
                if self.seams.last().is_some_and(|(st, _)| *st == self.t) {
                    self.seams.pop();
                } else {
                    self.t += 1;
                }
                self.seams.push((self.t, tick));
            }
            None => self.seams.push((0, tick)),
        }
        self.last_tick = Some(tick);

        // Everyone this tick, by entity, to resolve heal targets to slots.
        let mut rows = std::mem::take(&mut self.rows);
        rows.clear();
        self.by_entity.clear();
        for (i, p) in state.players.iter().enumerate() {
            let Some(info) = &p.info else { continue };
            if info.steam_id.is_empty() || info.steam_id == "BOT" {
                continue;
            }
            let slot = self.slot(u16::from(info.user_id), &info.steam_id, &info.name);
            self.by_entity.insert(u32::from(p.entity), slot);
            self.entities.insert((u32::from(p.entity), slot));
            rows.push((slot, i));
        }
        let sample_now = self.t.is_multiple_of(self.stride);
        for &(slot, i) in &rows {
            let p = &state.players[i];
            let here = p.in_pvs || p.info.as_ref().is_some_and(|i| i.steam_id == owner);
            let alive = p.state == PlayerState::Alive;
            self.set(slot, Field::Here(here));
            self.set(slot, Field::Alive(alive));
            // The scoreboard reaches every client: true for anyone, carried
            // or not, and 0 for nobody real.
            if p.ping > 0 {
                self.set(slot, Field::Ping(p.ping));
            }
            if !here {
                // Whatever the demo says about someone it is not carrying is
                // stale; recording it would only record the staleness.
                continue;
            }
            self.set(slot, Field::Team(p.team as u8));
            self.set(slot, Field::Class(p.class as u8));
            self.set(slot, Field::Health(p.health));
            self.set(slot, Field::MaxHealth(p.max_health));
            self.set(slot, Field::Conds(a.conds_of(p.entity)));
            match &p.class_data {
                PlayerClassData::Medic { charge, medigun, target, .. } => {
                    self.set(slot, Field::Charge(*charge));
                    self.set(slot, Field::Medigun(*medigun as u8));
                    let t = target.and_then(|e| self.by_entity.get(&u32::from(e)).copied());
                    self.set(slot, Field::HealTarget(t));
                }
                PlayerClassData::Spy { disguise_team, disguise_class, cloak } => {
                    self.set(slot, Field::Cloak(cloak.clamp(0.0, 100.0).round() as u8));
                    self.set(slot, Field::DisguiseClass(*disguise_class as u8));
                    self.set(slot, Field::DisguiseTeam(*disguise_team as u8));
                }
                PlayerClassData::None => {}
            }
            if sample_now && alive {
                self.samples[usize::from(slot)].push(&Sample {
                    t: self.t,
                    pos: [p.position.x, p.position.y, p.position.z],
                    yaw: p.view_angle,
                    pitch: p.pitch_angle,
                });
            }
        }
        self.rows = rows;

        for (_, event) in state.events.iter().skip(self.events_done) {
            let at = self.events.len();
            let written = write!(self.events, "{}\t", self.t).is_ok()
                && serde_json::to_writer(&mut self.events, event).is_ok();
            if written {
                self.events.push(b'\n');
                self.event_count += 1;
            } else {
                // Never expected; counted and reported rather than lost quietly.
                self.events.truncate(at);
                self.events_unwritten += 1;
            }
        }
        self.events_done = state.events.len();

        // Reflects, written as they happen whatever the sampling stride: a
        // handful a match, and each one counts.
        for r in &a.reflects[self.reflects_done..] {
            let row = ObjectRow {
                t: self.t,
                entity: r.entity,
                kind: Cow::Borrowed("reflect"),
                pos: round3(r.pos),
                builder: None,
                team: Some(r.team),
                level: None,
                health: None,
                building: false,
                sapped: false,
                by: r.by_entity.and_then(|e| self.by_entity.get(&e).copied()),
                what: Some(Cow::Borrowed(r.what)),
                vel: r.before.map(round3),
            };
            if serde_json::to_writer(&mut self.objects, &row).is_ok() {
                self.objects.push(b'\n');
                self.object_count += 1;
            }
        }
        self.reflects_done = a.reflects.len();
        for e in &a.reflect_ends[self.ends_done..] {
            let row = ObjectRow {
                t: self.t,
                entity: e.entity,
                kind: Cow::Borrowed(if e.seen { "reflect_end" } else { "reflect_lost" }),
                pos: round3(e.pos),
                builder: None,
                team: None,
                level: None,
                health: None,
                building: false,
                sapped: false,
                by: None,
                what: None,
                vel: None,
            };
            if serde_json::to_writer(&mut self.objects, &row).is_ok() {
                self.objects.push(b'\n');
                self.object_count += 1;
            }
        }
        self.ends_done = a.reflect_ends.len();

        if sample_now {
            self.seen.clear();
            for (entity, objective) in &state.objectives {
                // The cart is the only objective the parser tracks.
                let Some(cart) = objective.as_cart() else { continue };
                let row = ObjectRow {
                    t: self.t,
                    entity: u32::from(*entity),
                    kind: Cow::Borrowed("cart"),
                    pos: round3([cart.position.x, cart.position.y, cart.position.z]),
                    builder: None,
                    team: None,
                    level: None,
                    health: None,
                    building: false,
                    sapped: false,
                    by: None,
                    what: None,
                    vel: None,
                };
                self.seen.push(row.entity);
                self.object(row);
            }
            for (entity, b) in &state.buildings {
                let Some(row) = building_row(self.t, u32::from(*entity), b) else { continue };
                self.seen.push(row.entity);
                self.object(row);
            }
            // Anything that was here and is not any more: destroyed, or a
            // round reset.
            let gone: Vec<u32> = self
                .object_last
                .iter()
                .filter(|(e, r)| r.kind != "gone" && !self.seen.contains(e))
                .map(|(e, _)| *e)
                .collect();
            for e in gone {
                let mut row = self.object_last[&e].clone();
                row.t = self.t;
                row.kind = Cow::Borrowed("gone");
                self.object(row);
            }
        }
    }

    /// Record an object's row if anything but its time differs from its last.
    fn object(&mut self, row: ObjectRow) {
        if self.object_last.get(&row.entity).is_some_and(|last| last.same_as(&row)) {
            return;
        }
        if serde_json::to_writer(&mut self.objects, &row).is_ok() {
            self.objects.push(b'\n');
            self.object_count += 1;
        }
        self.object_last.insert(row.entity, row);
    }

    /// The stored form, ready for the database.
    pub fn finish(self) -> Result<Stored> {
        if self.events_unwritten > 0 {
            tracing::warn!(unwritten = self.events_unwritten, "some demo events could not be written to the timeline");
        }
        let mut user_ids: Vec<(u16, u16)> = self.user_ids.into_iter().collect();
        user_ids.sort_unstable();
        let n = self.people.len();
        let s = assemble(n, self.samples.iter().map(|w| (w.count, w.buf.as_slice())));
        let c = assemble(n, self.changes.iter().map(|w| (w.count, w.buf.as_slice())));
        let head = serde_json::to_string(&Head { people: self.people, user_ids, seams: self.seams, entities: self.entities.into_iter().collect() })?;
        Ok(Stored {
            version: TIMELINE_VERSION,
            tick_rate: self.tick_rate,
            stride: self.stride,
            raw_bytes: s.len() + c.len() + self.objects.len() + self.events.len(),
            head,
            samples: deflate(&s)?,
            changes: deflate(&c)?,
            objects: deflate(&self.objects)?,
            events: deflate(&self.events)?,
        })
    }
}

fn round3(v: [f32; 3]) -> [i32; 3] {
    [v[0].round() as i32, v[1].round() as i32, v[2].round() as i32]
}

fn building_row(t: u32, entity: u32, b: &Building) -> Option<ObjectRow> {
    // A teleporter's end, for the map (Emiel): which way players went.
    let what = match b {
        Building::Teleporter(tp) => Some(Cow::Borrowed(if tp.is_entrance { "entrance" } else { "exit" })),
        _ => None,
    };
    let (kind, builder, pos, level, health, team, building, sapped) = match b {
        Building::Sentry(s) => ("sentry", s.builder, s.position, s.level, s.health, s.team, s.building, s.sapped),
        Building::Dispenser(d) => ("dispenser", d.builder, d.position, d.level, d.health, d.team, d.building, d.sapped),
        Building::Teleporter(tp) => ("teleporter", tp.builder, tp.position, tp.level, tp.health, tp.team, tp.building, tp.sapped),
        // The parser may learn new kinds; until this does, they are skipped.
        _ => return None,
    };
    Some(ObjectRow {
        t,
        entity,
        kind: Cow::Borrowed(kind),
        pos: round3([pos.x, pos.y, pos.z]),
        builder: Some(u16::from(builder)),
        team: Some(team as u8),
        level: Some(level),
        health: Some(health),
        building,
        sapped,
        by: None,
        what,
        vel: None,
    })
}

// ---- Reading -----------------------------------------------------------------

/// A player's whole state at one moment, rebuilt from their changes: what
/// later passes ask questions of.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Now {
    pub alive: bool,
    pub here: bool,
    pub health: u16,
    pub max_health: u16,
    pub class: u8,
    pub team: u8,
    pub conds: Conds,
    pub charge: u8,
    pub medigun: u8,
    pub heal_target: Option<u16>,
    pub cloak: u8,
    pub disguise_class: u8,
    pub disguise_team: u8,
    /// Milliseconds; 0 where the timeline has none (before v2).
    pub ping: u16,
}

impl Now {
    pub fn apply(&mut self, f: Field) {
        match f {
            Field::Health(v) => self.health = v,
            Field::MaxHealth(v) => self.max_health = v,
            Field::Class(v) => self.class = v,
            Field::Team(v) => self.team = v,
            Field::Alive(v) => self.alive = v,
            Field::Here(v) => self.here = v,
            Field::Conds(v) => self.conds = v,
            Field::Charge(v) => self.charge = v,
            Field::Medigun(v) => self.medigun = v,
            Field::HealTarget(v) => self.heal_target = v,
            Field::Cloak(v) => self.cloak = v,
            Field::DisguiseClass(v) => self.disguise_class = v,
            Field::DisguiseTeam(v) => self.disguise_team = v,
            Field::Ping(v) => self.ping = v,
        }
    }

    /// Whether condition `c` holds (read correctly; see `deep`).
    pub fn has(&self, c: tf_demo_parser::demo::data::game_state::PlayerCondition) -> bool {
        crate::deep::has(&self.conds, c)
    }

    /// Alive and carried by the demo: the only time anything about a player
    /// is a measurement.
    pub fn live(&self) -> bool {
        self.alive && self.here
    }
}

impl Timeline {
    /// The last moment anything was recorded.
    pub fn end(&self) -> u32 {
        self.tracks
            .iter()
            .flat_map(|tr| tr.changes.last().map(|c| c.t).into_iter().chain(tr.samples.last().map(|s| s.t)))
            .chain(self.events.last().map(|(t, _)| *t))
            .chain(self.objects.last().map(|o| o.t))
            .max()
            .unwrap_or(0)
    }

    /// `slot`'s demo as stretches of unchanging state, `(from, to, state)`,
    /// from their first change to [`Timeline::end`]. Every change at one
    /// moment is applied before the stretch starting there.
    pub fn stretches(&self, slot: usize) -> Vec<(u32, u32, Now)> {
        let Some(track) = self.tracks.get(slot) else { return Vec::new() };
        let changes = &track.changes;
        let end = self.end();
        let mut out = Vec::new();
        let mut now = Now::default();
        let mut i = 0;
        while i < changes.len() {
            let from = changes[i].t;
            while let Some(c) = changes.get(i).filter(|c| c.t == from) {
                now.apply(c.field);
                i += 1;
            }
            let to = changes.get(i).map_or(end, |c| c.t);
            if to > from {
                out.push((from, to, now));
            }
        }
        out
    }

    /// Ticks `slot` spent alive, carried, and matching `test`.
    pub fn ticks_where(&self, slot: usize, test: impl Fn(&Now) -> bool) -> u32 {
        self.stretches(slot).iter().filter(|(_, _, n)| n.live() && test(n)).map(|(a, b, _)| b - a).sum()
    }

    /// `slot`'s state at `t`.
    pub fn now(&self, slot: usize, t: u32) -> Now {
        let mut now = Now::default();
        if let Some(track) = self.tracks.get(slot) {
            for c in track.changes.iter().take_while(|c| c.t <= t) {
                now.apply(c.field);
            }
        }
        now
    }

    /// Seconds for a number of ticks.
    pub fn seconds(&self, ticks: u32) -> f64 {
        f64::from(ticks) / self.tick_rate.max(1.0)
    }
}

// ---- Encoding -------------------------------------------------------------

fn put_var(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn put_zig(out: &mut Vec<u8>, v: i64) {
    put_var(out, ((v << 1) ^ (v >> 63)) as u64);
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn var(&mut self) -> Result<u64> {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let Some(&b) = self.b.get(self.i) else { bail!("timeline ended mid-number") };
            self.i += 1;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
            if shift > 63 {
                bail!("timeline number too long");
            }
        }
    }
    fn zig(&mut self) -> Result<i64> {
        let v = self.var()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }
    fn byte(&mut self) -> Result<u8> {
        let Some(&b) = self.b.get(self.i) else { bail!("timeline ended mid-record") };
        self.i += 1;
        Ok(b)
    }
    fn done(&self) -> bool {
        self.i >= self.b.len()
    }
}

// Angles are kept as whole steps: yaw in 1/65536 of a turn, pitch in
// hundredths of a degree.
const YAW_STEPS: f32 = 65536.0 / 360.0;
const PITCH_STEPS: f32 = 100.0;

fn yaw_step(deg: f32) -> i64 {
    (deg.rem_euclid(360.0) * YAW_STEPS).round() as i64 % 65536
}

fn deflate(raw: &[u8]) -> Result<Vec<u8>> {
    let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
    e.write_all(raw)?;
    Ok(e.finish()?)
}

fn inflate(z: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    DeflateDecoder::new(z).read_to_end(&mut out).context("inflating a timeline")?;
    Ok(out)
}

impl Timeline {
    pub fn encode(&self) -> Result<Stored> {
        // The same writers the recorder uses, so the two cannot disagree.
        let n = self.tracks.len();
        let samples: Vec<SampleWriter> = self
            .tracks
            .iter()
            .map(|tr| {
                let mut w = SampleWriter::default();
                tr.samples.iter().for_each(|x| w.push(x));
                w
            })
            .collect();
        let changes: Vec<ChangeWriter> = self
            .tracks
            .iter()
            .map(|tr| {
                let mut w = ChangeWriter::default();
                tr.changes.iter().for_each(|c| w.push(c));
                w
            })
            .collect();
        let s = assemble(n, samples.iter().map(|w| (w.count, w.buf.as_slice())));
        let c = assemble(n, changes.iter().map(|w| (w.count, w.buf.as_slice())));

        let mut o = Vec::new();
        for row in &self.objects {
            serde_json::to_writer(&mut o, row)?;
            o.push(b'\n');
        }
        let mut ev = Vec::new();
        for (t, e) in &self.events {
            write!(ev, "{t}\t")?;
            serde_json::to_writer(&mut ev, e)?;
            ev.push(b'\n');
        }

        let head = serde_json::to_string(&Head {
            people: self.people.clone(),
            user_ids: self.user_ids.clone(),
            seams: self.seams.clone(),
            entities: self.entities.clone(),
        })?;
        Ok(Stored {
            version: self.version,
            tick_rate: self.tick_rate,
            stride: self.stride,
            raw_bytes: s.len() + c.len() + o.len() + ev.len(),
            head,
            samples: deflate(&s)?,
            changes: deflate(&c)?,
            objects: deflate(&o)?,
            events: deflate(&ev)?,
        })
    }

    pub fn decode(stored: &Stored) -> Result<Timeline> {
        if !(MIN_READABLE_VERSION..=TIMELINE_VERSION).contains(&stored.version) {
            bail!(
                "this timeline was recorded at format {} and this version reads {} to {}: the demo needs reading again",
                stored.version,
                MIN_READABLE_VERSION,
                TIMELINE_VERSION
            );
        }
        let head: Head = serde_json::from_str(&stored.head).context("reading a timeline's head")?;
        let n = head.people.len();

        let raw = inflate(&stored.samples)?;
        let mut cur = Cursor { b: &raw, i: 0 };
        let tracks_n = cur.var()? as usize;
        if tracks_n != n {
            bail!("timeline has {tracks_n} sample tracks for {n} people");
        }
        let mut tracks = vec![Track::default(); n];
        for track in &mut tracks {
            let count = cur.var()? as usize;
            track.samples.reserve(count);
            let (mut t, mut p, mut yaw, mut pitch) = (0i64, [0i64; 3], 0i64, 0i64);
            for _ in 0..count {
                t += cur.var()? as i64;
                for v in &mut p {
                    *v += cur.zig()?;
                }
                yaw = (yaw + cur.zig()?).rem_euclid(65536);
                pitch += cur.zig()?;
                let mut y = yaw as f32 / YAW_STEPS;
                if y > 180.0 {
                    y -= 360.0;
                }
                track.samples.push(Sample {
                    t: t as u32,
                    pos: [p[0] as f32, p[1] as f32, p[2] as f32],
                    yaw: y,
                    pitch: pitch as f32 / PITCH_STEPS,
                });
            }
        }

        let raw = inflate(&stored.changes)?;
        let mut cur = Cursor { b: &raw, i: 0 };
        if cur.var()? as usize != n {
            bail!("timeline change tracks do not match its people");
        }
        for track in &mut tracks {
            let count = cur.var()? as usize;
            track.changes.reserve(count);
            let mut t = 0u32;
            for _ in 0..count {
                t += cur.var()? as u32;
                let field = match cur.byte()? {
                    0 => Field::Health(cur.var()? as u16),
                    1 => Field::MaxHealth(cur.var()? as u16),
                    2 => Field::Class(cur.byte()?),
                    3 => Field::Team(cur.byte()?),
                    4 => Field::Alive(cur.byte()? != 0),
                    5 => Field::Here(cur.byte()? != 0),
                    6 => {
                        let mut bits: Conds = [0; 20];
                        for b in &mut bits {
                            *b = cur.byte()?;
                        }
                        Field::Conds(bits)
                    }
                    7 => Field::Charge(cur.byte()?),
                    8 => Field::Medigun(cur.byte()?),
                    9 => Field::HealTarget(cur.var()?.checked_sub(1).map(|s| s as u16)),
                    10 => Field::Cloak(cur.byte()?),
                    11 => Field::DisguiseClass(cur.byte()?),
                    12 => Field::DisguiseTeam(cur.byte()?),
                    13 => Field::Ping(cur.var()? as u16),
                    k => bail!("timeline has a change of unknown kind {k}"),
                };
                track.changes.push(Change { t, field });
            }
        }
        if !cur.done() {
            bail!("timeline changes run past their count");
        }

        let raw = inflate(&stored.objects)?;
        let objects = raw
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_slice(l).context("reading a timeline object"))
            .collect::<Result<Vec<ObjectRow>>>()?;

        let raw = inflate(&stored.events)?;
        let mut events = Vec::new();
        for line in raw.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
            let tab = line.iter().position(|&b| b == b'\t').context("a timeline event without its time")?;
            let t: u32 = std::str::from_utf8(&line[..tab])?.parse()?;
            events.push((t, serde_json::from_slice(&line[tab + 1..]).context("reading a timeline event")?));
        }

        Ok(Timeline {
            version: stored.version,
            tick_rate: stored.tick_rate,
            stride: stored.stride,
            seams: head.seams,
            people: head.people,
            tracks,
            user_ids: head.user_ids,
            entities: head.entities,
            objects,
            events,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t: u32, x: f32, yaw: f32, pitch: f32) -> Sample {
        Sample { t, pos: [x, -x, 64.0], yaw, pitch }
    }

    #[test]
    fn a_timeline_from_a_format_this_build_does_not_read_is_refused_not_misread() {
        for version in [MIN_READABLE_VERSION - 1, TIMELINE_VERSION + 1] {
            let stored = Stored { version, ..Stored::default() };
            let err = Timeline::decode(&stored).unwrap_err().to_string();
            assert!(err.contains("needs reading again"), "{err}");
        }
    }

    #[test]
    fn a_timeline_survives_storing() {
        let mut bits: Conds = [0; 20];
        bits[0] = 0b10;
        let tl = Timeline {
            version: TIMELINE_VERSION,
            tick_rate: 66.67,
            stride: 2,
            seams: vec![(0, 100), (500, 3)],
            people: vec![
                Person { steamid: "[U:1:1]".into(), name: "a".into() },
                Person { steamid: "[U:1:2]".into(), name: "b".into() },
            ],
            tracks: vec![
                Track {
                    // Turning through 180/-180 must come back as a small turn.
                    samples: vec![sample(0, 10.0, 179.0, -10.5), sample(2, 12.0, -179.0, 3.25), sample(510, -4000.0, 0.0, 89.0)],
                    changes: vec![
                        Change { t: 0, field: Field::Health(125) },
                        Change { t: 0, field: Field::Conds(bits) },
                        Change { t: 4, field: Field::HealTarget(Some(1)) },
                        Change { t: 9, field: Field::HealTarget(None) },
                        Change { t: 9, field: Field::Alive(false) },
                    ],
                },
                Track::default(),
            ],
            user_ids: vec![(3, 0), (7, 1)],
            entities: vec![(2, 0), (5, 1)],
            objects: vec![ObjectRow {
                t: 4,
                entity: 99,
                kind: "cart".into(),
                pos: [1, 2, 3],
                builder: None,
                team: None,
                level: None,
                health: None,
                building: false,
                sapped: false,
                by: None,
                what: None,
                vel: None,
            }],
            events: vec![],
        };
        let back = Timeline::decode(&tl.encode().unwrap()).unwrap();
        assert_eq!(back.people, tl.people);
        assert_eq!(back.tracks[0].changes, tl.tracks[0].changes);
        assert_eq!(back.objects, tl.objects);
        for (a, b) in back.tracks[0].samples.iter().zip(&tl.tracks[0].samples) {
            assert_eq!(a.t, b.t);
            assert_eq!(a.pos, b.pos);
            assert!((a.yaw - b.yaw).abs() < 0.01, "{} vs {}", a.yaw, b.yaw);
            assert!((a.pitch - b.pitch).abs() < 0.006);
        }
        assert_eq!(back.tick_of(510), 13);
        assert_eq!(back.t_of(13), Some(510));
        assert_eq!(back.slot_of_user(7), Some(1));
    }

    #[test]
    fn state_is_read_as_of_a_moment() {
        let tl = Timeline {
            people: vec![Person { steamid: "x".into(), name: "x".into() }],
            tracks: vec![Track {
                samples: vec![],
                changes: vec![
                    Change { t: 0, field: Field::Health(150) },
                    Change { t: 10, field: Field::Health(40) },
                    Change { t: 20, field: Field::Alive(false) },
                ],
            }],
            ..Timeline::default()
        };
        let health = |f: &Field| matches!(f, Field::Health(_));
        assert_eq!(tl.at(0, 5, health), Some(Field::Health(150)));
        assert_eq!(tl.at(0, 10, health), Some(Field::Health(40)));
        assert_eq!(tl.at(0, 25, |f| matches!(f, Field::Alive(_))), Some(Field::Alive(false)));
    }
}
