//! The game state at every moment of a match, rebuilt from its raw log:
//! who is alive, each Medic's charge, which points are taken, and which
//! sentries are up. The base for everything in PLAN §11 (pick context, uber
//! timing, holds); on its own it answers "was it 9 v 8 when this happened"
//! and "who had the uber advantage".
//!
//! Pure: a [`RawLog`] in, a [`GameState`] out, in whatever clock the raw log
//! is in. Nothing is stored; a log rebuilds in about a millisecond.
//!
//! **Lives.** A player is alive from a `spawned as` line until the kill line
//! with them as victim (a Dead Ringer feign is not a death), a suicide, a
//! disconnect or team change, or their next spawn, whichever comes first.
//! Lines of the same second are taken in log order: each event records how
//! many kills preceded it.
//!
//! **Charge.** Each Medic life is cut into spans: building, ready, deployed.
//! The log marks every boundary (`chargeready`, `chargedeployed`,
//! `chargeended`) and the charge held at death (`medic_death_ex`), so the
//! percentage while building is interpolated between two known points rather
//! than modelled from build rates. That makes it hindsight: it is right about
//! the past, which is all a match review needs. A span with no known end (the
//! round ended mid-build) extrapolates at the stock rate, 2.5% a second.
//!
//! **Combined logs.** logs.tf appends the parts of a combined log in any
//! order, so the clock can jump back hours mid-file, and one part can be
//! played in the gap between two others. So everything open is closed at
//! `Game_Over`, where time goes backwards, and across a silent gap; each part
//! starts clean and no life spans another part's time.
//!
//! **Colours.** Everything here is in the colours worn at the time, which in
//! stopwatch swap between halves. Map to stable teams outside.

use crate::rawlog::{Actor, EventKind, RawLog};
use hl_core::matchdata::Team;
use hl_core::TfClass;
use serde::Serialize;
use std::collections::HashMap;

/// Stock Medigun build rate with no overheal penalty, % a second (40 s to full).
const STOCK_RATE: f64 = 2.5;

/// Time going back more than this starts a new part of a combined log. Lines
/// within one part can be a second or two out of order.
const PART_JUMP_S: i64 = 30;

/// A live server logs shots and heals every second; a silence this long is a
/// break between parts.
const GAP_S: i64 = 300;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    pub lives: Vec<Life>,
    pub charges: Vec<ChargeSpan>,
    pub rounds: Vec<Round>,
    pub caps: Vec<Cap>,
    pub sentries: Vec<Sentry>,
    /// The log's own `lost_uber_advantage` lines: a check on [`GameState::advantage_at`].
    pub lost_advantages: Vec<LostAdvantage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Life {
    pub account: u32,
    pub team: Team,
    pub class: TfClass,
    /// Alive over `[from, to)`.
    pub from: i64,
    pub to: i64,
    pub end: LifeEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum LifeEnd {
    Killed { by: u32 },
    Suicide,
    /// Disconnected or changed team.
    Left,
    /// Spawned again without a logged death: a class change in the spawn room.
    Respawned,
    /// A new round started. Everyone in the game respawns then, so this
    /// also closes the lives of players who left unlogged (at a map change).
    NewRound,
    /// The log, or its part of a combined log, ended.
    LogEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChargeKind {
    Building,
    Ready,
    Deployed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChargeSpan {
    pub medic: u32,
    pub team: Team,
    pub kind: ChargeKind,
    /// Over `[from, to)`.
    pub from: i64,
    pub to: i64,
    /// Charge at `from` and at `to`, 0-100. `to_pct` is `None` when the span
    /// was cut off with no reading (the round ended while building).
    pub from_pct: f64,
    pub to_pct: Option<f64>,
    /// The medigun, known once it is deployed.
    pub medigun: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    /// 1-based, in log order.
    pub num: u32,
    pub start: i64,
    /// Stopwatch: when the attackers were let out.
    pub setup_end: Option<i64>,
    pub end: Option<i64>,
    pub winner: Option<Team>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cap {
    pub at: i64,
    pub round: u32,
    pub team: Option<Team>,
    pub cp: u32,
    pub name: String,
    pub cappers: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sentry {
    pub owner: u32,
    pub team: Team,
    /// Up over `[from, to)`.
    pub from: i64,
    pub to: i64,
    pub end: SentryEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SentryEnd {
    Destroyed { by: u32 },
    /// Blown up by its own Engineer.
    Detonated,
    /// Picked up to move it.
    Carried,
    /// The round ended, or the Engineer left or changed class.
    Cleared,
    LogEnd,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LostAdvantage {
    pub at: i64,
    pub medic: u32,
    pub team: Option<Team>,
    pub secs: i64,
}

/// A team's charge at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Charge {
    /// No Medic alive.
    NoMedic,
    Building { pct: f64 },
    Ready,
    Deployed,
}

impl Charge {
    /// Can pop now.
    pub fn ready(self) -> bool {
        matches!(self, Charge::Ready)
    }

    fn pct(self) -> f64 {
        match self {
            Charge::NoMedic => -1.0,
            Charge::Building { pct } => pct,
            Charge::Ready | Charge::Deployed => 100.0,
        }
    }
}

impl ChargeSpan {
    /// The charge at `t`, which must lie in the span.
    pub fn pct_at(&self, t: i64) -> f64 {
        match self.kind {
            ChargeKind::Ready | ChargeKind::Deployed => 100.0,
            ChargeKind::Building => {
                let dt = (t - self.from) as f64;
                match self.to_pct {
                    Some(end) if self.to > self.from => self.from_pct + (end - self.from_pct) * dt / (self.to - self.from) as f64,
                    Some(end) => end,
                    None => (self.from_pct + STOCK_RATE * dt).min(99.0),
                }
            }
        }
    }
}

pub fn other(team: Team) -> Team {
    match team {
        Team::Red => Team::Blue,
        Team::Blue => Team::Red,
    }
}

fn idx(team: Team) -> usize {
    match team {
        Team::Red => 0,
        Team::Blue => 1,
    }
}

impl GameState {
    pub fn build(raw: &RawLog) -> Self {
        // Kills and events in file order: kills written before an event's
        // line happened before it.
        let mut lines: Vec<Line> = Vec::with_capacity(raw.kills.len() + raw.events.len());
        let mut kills = raw.kills.iter().enumerate().peekable();
        for e in &raw.events {
            while let Some((_, k)) = kills.next_if(|(i, _)| *i < e.after_kills) {
                lines.push(Line::Kill(k));
            }
            lines.push(Line::Event(e.at, &e.kind));
        }
        lines.extend(kills.map(|(_, k)| Line::Kill(k)));

        let skip = repeated_parts(&lines);
        let mut b = Builder::default();
        for (line, part) in lines.iter().zip(parts(&lines)) {
            if skip.contains(&part) {
                continue;
            }
            match line {
                Line::Kill(k) => b.kill(k),
                Line::Event(at, kind) => b.event(*at, kind),
            }
        }
        b.finish()
    }

    /// Everyone alive at `t`. A player killed at `t` is not.
    pub fn alive_at(&self, t: i64) -> impl Iterator<Item = &Life> {
        self.lives.iter().filter(move |l| l.from <= t && t < l.to)
    }

    /// Players alive per colour at `t`: `[red, blue]`.
    pub fn numbers_at(&self, t: i64) -> [u8; 2] {
        let mut n = [0u8; 2];
        for l in self.alive_at(t) {
            n[idx(l.team)] += 1;
        }
        n
    }

    /// Whether a class is alive for a colour at `t`.
    pub fn class_alive(&self, team: Team, class: TfClass, t: i64) -> bool {
        self.alive_at(t).any(|l| l.team == team && l.class == class)
    }

    /// A colour's charge at `t`: its best, if two Medics are up.
    pub fn charge_at(&self, team: Team, t: i64) -> Charge {
        self.charges
            .iter()
            .filter(|c| c.team == team && c.from <= t && t < c.to)
            .map(|c| match c.kind {
                ChargeKind::Building => Charge::Building { pct: c.pct_at(t) },
                ChargeKind::Ready => Charge::Ready,
                ChargeKind::Deployed => Charge::Deployed,
            })
            .max_by(|a, b| a.pct().total_cmp(&b.pct()))
            .unwrap_or(Charge::NoMedic)
    }

    /// The colour with the uber advantage at `t`: it can pop and the other
    /// cannot answer (building, or no Medic). Two ready charges, or a charge
    /// already in use, is no advantage.
    pub fn advantage_at(&self, t: i64) -> Option<Team> {
        let (red, blue) = (self.charge_at(Team::Red, t), self.charge_at(Team::Blue, t));
        let answers = |c: Charge| matches!(c, Charge::Ready | Charge::Deployed);
        match (red.ready() && !answers(blue), blue.ready() && !answers(red)) {
            (true, false) => Some(Team::Red),
            (false, true) => Some(Team::Blue),
            _ => None,
        }
    }

    /// How many seconds sooner this colour will have a charge than the other,
    /// at stock build rate: MedicStats' "uber advantage", which logs.tf's
    /// "advantage lost" measures. Negative when behind. A dead Medic, or a
    /// charge in use, is a full build away.
    pub fn uber_lead_s(&self, team: Team, t: i64) -> f64 {
        self.to_full_s(other(team), t) - self.to_full_s(team, t)
    }

    /// Seconds until a colour has a charge at the stock rate, as MedicStats
    /// counts it. (Each span's own rate is known in hindsight, but using it
    /// matched logs.tf's "advantage lost" sizes worse: median 7 s off, not 2.)
    pub fn to_full_s(&self, team: Team, t: i64) -> f64 {
        match self.charge_at(team, t) {
            Charge::Ready => 0.0,
            Charge::Building { pct } => (100.0 - pct) / STOCK_RATE,
            Charge::NoMedic | Charge::Deployed => 100.0 / STOCK_RATE,
        }
    }

    /// Sentries up per colour at `t`: `[red, blue]`.
    pub fn sentries_at(&self, t: i64) -> [u8; 2] {
        let mut n = [0u8; 2];
        for s in self.sentries.iter().filter(|s| s.from <= t && t < s.to) {
            n[idx(s.team)] += 1;
        }
        n
    }

    /// The round `t` falls in, by log order.
    pub fn round_at(&self, t: i64) -> Option<&Round> {
        self.rounds.iter().rev().find(|r| r.start <= t)
    }
}

enum Line<'a> {
    Kill(&'a crate::rawlog::Kill),
    Event(i64, &'a EventKind),
}

impl Line<'_> {
    fn at(&self) -> i64 {
        match self {
            Line::Kill(k) => k.at,
            Line::Event(at, _) => *at,
        }
    }
}

/// The part of a combined log each line belongs to: a new part starts where
/// time goes backwards.
fn parts(lines: &[Line]) -> Vec<usize> {
    let (mut part, mut last) = (0, i64::MIN);
    lines
        .iter()
        .map(|l| {
            if l.at() + PART_JUMP_S < last {
                part += 1;
            }
            last = last.max(l.at());
            if part > 0 && l.at() + PART_JUMP_S < last {
                last = l.at();
            }
            part
        })
        .collect()
}

/// Parts whose every round also starts in an earlier part: the same match
/// appended twice, which some raw logs hold. Replaying it would double
/// everyone.
fn repeated_parts(lines: &[Line]) -> Vec<usize> {
    let mut starts: Vec<Vec<i64>> = Vec::new();
    for (l, part) in lines.iter().zip(parts(lines)) {
        if starts.len() <= part {
            starts.resize(part + 1, Vec::new());
        }
        if let Line::Event(at, EventKind::RoundStart) = l {
            starts[part].push(*at);
        }
    }
    (1..starts.len())
        .filter(|&p| !starts[p].is_empty() && starts[p].iter().all(|s| starts[..p].iter().any(|e| e.contains(s))))
        .collect()
}

#[derive(Default)]
struct Builder {
    out: GameState,
    /// Open life per account: index into `out.lives`.
    alive: HashMap<u32, usize>,
    /// Open charge span per Medic: index into `out.charges`.
    charging: HashMap<u32, usize>,
    /// Open sentry per Engineer: index into `out.sentries`.
    sentry: HashMap<u32, usize>,
    /// The latest time seen, for closing whatever is open at the end.
    last: i64,
    round: u32,
}

impl Builder {
    /// Note the time of the next line. Time going backwards means the next
    /// part of a combined log: close everything the last part left open.
    fn tick(&mut self, at: i64) {
        if at + PART_JUMP_S < self.last || at > self.last + GAP_S && self.last > 0 {
            self.close_all(self.last + 1);
            self.last = at;
        }
        self.last = self.last.max(at);
    }

    fn close_all(&mut self, at: i64) {
        for (_, i) in self.alive.drain() {
            self.out.lives[i].to = at;
        }
        for (_, i) in self.charging.drain() {
            self.out.charges[i].to = at;
        }
        for (_, i) in self.sentry.drain() {
            self.out.sentries[i].to = at;
        }
    }

    fn kill(&mut self, k: &crate::rawlog::Kill) {
        self.tick(k.at);
        if k.custom.as_deref() == Some("feign_death") {
            return;
        }
        self.end_life(k.victim.account, k.at, LifeEnd::Killed { by: k.killer.account });
    }

    fn event(&mut self, at: i64, kind: &EventKind) {
        self.tick(at);
        match kind {
            EventKind::RoundStart => {
                self.round += 1;
                self.out.rounds.push(Round { num: self.round, start: at, setup_end: None, end: None, winner: None });
                // Everyone respawns; buildings and charges do not carry over.
                for (_, i) in self.alive.drain() {
                    let l = &mut self.out.lives[i];
                    l.to = at;
                    l.end = LifeEnd::NewRound;
                }
                for (_, i) in self.charging.drain() {
                    self.out.charges[i].to = at;
                }
                for owner in self.sentry.keys().copied().collect::<Vec<_>>() {
                    self.end_sentry(owner, at, SentryEnd::Cleared);
                }
            }
            EventKind::SetupEnd => {
                if let Some(r) = self.out.rounds.last_mut() {
                    r.setup_end.get_or_insert(at);
                }
            }
            EventKind::RoundWin(winner) => {
                if let Some(r) = self.out.rounds.last_mut().filter(|r| r.end.is_none()) {
                    r.end = Some(at);
                    r.winner = *winner;
                }
            }
            EventKind::RoundStalemate => {
                if let Some(r) = self.out.rounds.last_mut().filter(|r| r.end.is_none()) {
                    r.end = Some(at);
                }
            }
            EventKind::GameOver => {
                if let Some(r) = self.out.rounds.last_mut().filter(|r| r.end.is_none()) {
                    r.end = Some(at);
                }
                // The map is over: nothing carries into whatever the file holds next.
                self.close_all(at);
            }
            EventKind::Spawn(a) => self.spawn(*a, at),
            EventKind::Suicide(a) => self.end_life(a.account, at, LifeEnd::Suicide),
            EventKind::Left(account) => {
                self.end_life(*account, at, LifeEnd::Left);
                self.end_sentry(*account, at, SentryEnd::Cleared);
            }
            EventKind::FirstHeal(m) => {
                // Charge builds from the first heal, not from the spawn: the
                // wait before it holds at 0.
                let fresh = self.charging.get(&m.account).is_some_and(|&i| {
                    let c = &self.out.charges[i];
                    c.kind == ChargeKind::Building && c.from_pct == 0.0 && c.from < at
                });
                if fresh {
                    self.next_charge(m.account, at, Some(0.0), ChargeKind::Building, 0.0, None);
                }
            }
            EventKind::ChargeReady(m) => self.next_charge(m.account, at, Some(100.0), ChargeKind::Ready, 100.0, None),
            EventKind::ChargeDeployed { medic, medigun } => {
                self.next_charge(medic.account, at, None, ChargeKind::Deployed, 100.0, Some(medigun.clone()));
            }
            EventKind::ChargeEnded(m) => self.next_charge(m.account, at, None, ChargeKind::Building, 0.0, None),
            EventKind::MedicDied { medic, pct } => self.medic_died(medic.account, at, f64::from(*pct)),
            EventKind::LostAdvantage { medic, secs } => {
                self.out.lost_advantages.push(LostAdvantage { at, medic: medic.account, team: medic.team, secs: *secs });
            }
            EventKind::PointCaptured { team, cp, name, cappers } => {
                self.out.caps.push(Cap { at, round: self.round, team: *team, cp: *cp, name: name.clone(), cappers: cappers.clone() });
            }
            EventKind::CaptureBlocked { .. } => {}
            EventKind::Built { by, object } => {
                if is_sentry(object) && !self.sentry.contains_key(&by.account) {
                    if let Some(team) = by.team {
                        self.sentry.insert(by.account, self.out.sentries.len());
                        self.out.sentries.push(Sentry { owner: by.account, team, from: at, to: at, end: SentryEnd::LogEnd });
                    }
                }
            }
            EventKind::ObjectKilled { by, owner, object } => {
                if let (true, Some(owner)) = (is_sentry(object), owner) {
                    self.end_sentry(*owner, at, SentryEnd::Destroyed { by: by.account });
                }
            }
            EventKind::Detonated { by, object } => {
                if is_sentry(object) {
                    self.end_sentry(by.account, at, SentryEnd::Detonated);
                }
            }
            EventKind::Carried { by, object, picked } => {
                if is_sentry(object) {
                    if *picked {
                        self.end_sentry(by.account, at, SentryEnd::Carried);
                    } else if let (false, Some(team)) = (self.sentry.contains_key(&by.account), by.team) {
                        self.sentry.insert(by.account, self.out.sentries.len());
                        self.out.sentries.push(Sentry { owner: by.account, team, from: at, to: at, end: SentryEnd::LogEnd });
                    }
                }
            }
        }
    }

    fn spawn(&mut self, a: Actor, at: i64) {
        self.end_life(a.account, at, LifeEnd::Respawned);
        let (Some(team), Some(class)) = (a.team, a.class) else { return };
        // An Engineer who comes back as another class loses their buildings.
        if class != TfClass::Engineer {
            self.end_sentry(a.account, at, SentryEnd::Cleared);
        }
        self.alive.insert(a.account, self.out.lives.len());
        self.out.lives.push(Life { account: a.account, team, class, from: at, to: at, end: LifeEnd::LogEnd });
        if class == TfClass::Medic {
            self.charging.insert(a.account, self.out.charges.len());
            self.out.charges.push(ChargeSpan {
                medic: a.account,
                team,
                kind: ChargeKind::Building,
                from: at,
                to: at,
                from_pct: 0.0,
                to_pct: None,
                medigun: None,
            });
        }
    }

    fn end_life(&mut self, account: u32, at: i64, end: LifeEnd) {
        if let Some(i) = self.alive.remove(&account) {
            let l = &mut self.out.lives[i];
            l.to = at;
            l.end = end;
        }
        // A dead Medic's charge is gone. `medic_death_ex` may follow with
        // the reading; see `medic_died`.
        if let Some(i) = self.charging.remove(&account) {
            self.out.charges[i].to = at;
        }
    }

    /// Close the Medic's open span at `at` and open the next one.
    fn next_charge(&mut self, medic: u32, at: i64, end_pct: Option<f64>, kind: ChargeKind, from_pct: f64, medigun: Option<String>) {
        let Some(&i) = self.charging.get(&medic) else { return };
        let prev = &mut self.out.charges[i];
        prev.to = at;
        if prev.kind == ChargeKind::Building {
            prev.to_pct = end_pct.or(Some(prev.pct_at(at)));
        }
        // The medigun is named at deploy; the ready span before it used the same one.
        if let (Some(m), ChargeKind::Ready) = (&medigun, prev.kind) {
            prev.medigun = Some(m.clone());
        }
        let team = prev.team;
        self.charging.insert(medic, self.out.charges.len());
        self.out.charges.push(ChargeSpan { medic, team, kind, from: at, to: at, from_pct, to_pct: None, medigun });
    }

    /// The charge a Medic held at death: ends the building span on the real
    /// reading. The line can come just before or just after the kill line.
    fn medic_died(&mut self, medic: u32, at: i64, pct: f64) {
        let open = self.charging.get(&medic).copied();
        let i = open.or_else(|| {
            self.out.charges.iter().rposition(|c| c.medic == medic && at - c.to <= 1)
        });
        let Some(i) = i else { return };
        let c = &mut self.out.charges[i];
        if c.kind == ChargeKind::Building {
            c.to_pct = Some(pct);
        }
        if open.is_some() {
            c.to = at;
            self.charging.remove(&medic);
        }
    }

    fn end_sentry(&mut self, owner: u32, at: i64, end: SentryEnd) {
        if let Some(i) = self.sentry.remove(&owner) {
            let s = &mut self.out.sentries[i];
            s.to = at;
            s.end = end;
        }
    }

    fn finish(mut self) -> GameState {
        self.close_all(self.last + 1);
        // A span cut by a death or a round end has no reading at its end;
        // `pct_at` extrapolates those.
        self.out.lives.retain(|l| l.to > l.from || !matches!(l.end, LifeEnd::Respawned | LifeEnd::NewRound));
        // A Vaccinator pops several times a second; those charges are real.
        self.out.charges.retain(|c| c.to > c.from || c.kind == ChargeKind::Deployed);
        self.out.sentries.retain(|s| s.to > s.from);
        self.out
    }
}

fn is_sentry(object: &str) -> bool {
    object.eq_ignore_ascii_case("OBJ_SENTRYGUN")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rawlog::parse;

    const P: &str = "L 09/15/2026 - 20:00:";
    fn line(sec: u32, body: &str) -> String {
        format!("{P}{sec:02}: {body}\n")
    }
    const RED_MED: &str = "\"med<1><[U:1:1]><Red>\"";
    const BLU_MED: &str = "\"bmed<2><[U:1:2]><Blue>\"";
    const RED_SNIPER: &str = "\"flashy<3><[U:1:3]><Red>\"";
    const BLU_ENGI: &str = "\"engi<4><[U:1:4]><Blue>\"";

    fn log() -> String {
        [
            line(0, "World triggered \"Round_Start\""),
            line(0, &format!("{RED_MED} spawned as \"medic\"")),
            line(0, &format!("{BLU_MED} spawned as \"medic\"")),
            line(0, &format!("{RED_SNIPER} spawned as \"sniper\"")),
            line(0, &format!("{BLU_ENGI} spawned as \"engineer\"")),
            line(2, &format!("{BLU_ENGI} triggered \"player_builtobject\" (object \"OBJ_SENTRYGUN\") (position \"0 0 0\")")),
            line(10, &format!("{RED_MED} triggered \"first_heal_after_spawn\" (time \"10\")")),
            line(40, &format!("{BLU_MED} triggered \"chargeready\"")),
            line(50, &format!("{RED_SNIPER} killed {BLU_MED} with \"sniperrifle\" (customkill \"headshot\") (attacker_position \"0 0 0\") (victim_position \"0 0 0\")")),
            line(50, &format!("{BLU_MED} triggered \"medic_death_ex\" (uberpct \"100\")")),
            line(50, &format!("{RED_MED} triggered \"chargeready\"")),
            line(55, &format!("{RED_MED} triggered \"chargedeployed\" (medigun \"kritzkrieg\")")),
            line(56, &format!("{RED_SNIPER} triggered \"killedobject\" (object \"OBJ_SENTRYGUN\") (weapon \"sniperrifle\") (objectowner \"{}\") (attacker_position \"0 0 0\")", &BLU_ENGI[1..BLU_ENGI.len() - 1])),
            line(58, &format!("{RED_SNIPER} committed suicide with \"world\" (attacker_position \"0 0 0\")")),
            line(59, &format!("{RED_MED} triggered \"chargeended\" (duration \"4\")")),
            line(59, "Team \"Red\" triggered \"pointcaptured\" (cp \"0\") (cpname \"#mid\") (numcappers \"1\") (player1 \"med<1><[U:1:1]><Red>\") (position1 \"0 0 0\")"),
            line(59, "World triggered \"Round_Win\" (winner \"Red\")"),
        ]
        .concat()
    }

    /// The test log, in seconds from its round start.
    fn state() -> GameState {
        let mut raw = parse(&log());
        raw.shift(-raw.round_starts[0]);
        GameState::build(&raw)
    }

    #[test]
    fn lives_end_on_kills_suicides_and_respawns() {
        let s = state();
        assert_eq!(s.numbers_at(0), [2, 2]);
        assert_eq!(s.numbers_at(49), [2, 2]);
        assert_eq!(s.numbers_at(50), [2, 1], "the Medic killed at :50 is not alive at :50");
        assert_eq!(s.numbers_at(58), [1, 1], "a suicide is a death");
        let med = s.lives.iter().find(|l| l.account == 2).unwrap();
        assert_eq!(med.end, LifeEnd::Killed { by: 3 });
        assert!(s.class_alive(Team::Blue, TfClass::Engineer, 58));
        assert!(!s.class_alive(Team::Blue, TfClass::Medic, 50));
    }

    #[test]
    fn charge_is_read_between_known_points() {
        let s = state();
        // Red builds from its first heal at :10 to ready at :50.
        assert_eq!(s.charge_at(Team::Red, 5), Charge::Building { pct: 0.0 }, "before the first heal it waits at 0");
        assert_eq!(s.charge_at(Team::Red, 30), Charge::Building { pct: 50.0 });
        assert_eq!(s.charge_at(Team::Red, 52), Charge::Ready);
        assert_eq!(s.charge_at(Team::Red, 56), Charge::Deployed);
        assert_eq!(s.charge_at(Team::Red, 59), Charge::Building { pct: 0.0 }, "a used charge starts again from 0");
        // Blue: ready at :40, dropped at :50.
        assert_eq!(s.charge_at(Team::Blue, 20), Charge::Building { pct: 50.0 });
        assert_eq!(s.charge_at(Team::Blue, 45), Charge::Ready);
        assert_eq!(s.charge_at(Team::Blue, 50), Charge::NoMedic);
        let deployed = s.charges.iter().find(|c| c.kind == ChargeKind::Deployed).unwrap();
        assert_eq!(deployed.medigun.as_deref(), Some("kritzkrieg"));
    }

    #[test]
    fn advantage_is_a_charge_the_other_side_cannot_answer() {
        let s = state();
        assert_eq!(s.advantage_at(30), None, "both building");
        assert_eq!(s.advantage_at(45), Some(Team::Blue), "Blue ready, Red at 87%");
        assert_eq!(s.advantage_at(52), Some(Team::Red), "Blue's Medic was dropped");
        assert_eq!(s.advantage_at(56), None, "a charge in use is spent, not held");
    }

    #[test]
    fn a_combined_log_part_from_earlier_starts_clean() {
        // The second part was played first, but logs.tf appended it after.
        let early = log().replace("20:00:", "19:00:");
        let s = GameState::build(&parse(&(log() + &early)));
        let t = |h: u32, sec: i64| s.rounds.iter().find(|r| r.start % 86_400 / 3600 == i64::from(h)).unwrap().start + sec;
        assert_eq!(s.numbers_at(t(20, 30)), [2, 2]);
        assert_eq!(s.numbers_at(t(19, 30)), [2, 2], "nothing from the later part is still alive in the earlier one");
        assert!(s.lives.iter().all(|l| l.to >= l.from));
    }

    #[test]
    fn a_part_appended_twice_counts_once() {
        let s = GameState::build(&parse(&(log() + &log())));
        let t0 = s.rounds[0].start;
        assert_eq!(s.numbers_at(t0 + 30), [2, 2]);
        assert_eq!(s.rounds.len(), 1);
    }

    #[test]
    fn sentries_caps_and_rounds() {
        let s = state();
        assert_eq!(s.sentries_at(10), [0, 1]);
        assert_eq!(s.sentries_at(56), [0, 0]);
        assert_eq!(s.sentries[0].end, SentryEnd::Destroyed { by: 3 });
        assert_eq!(s.caps.len(), 1);
        assert_eq!(s.caps[0].cappers, vec![1]);
        assert_eq!(s.caps[0].team, Some(Team::Red));
        assert_eq!(s.rounds.len(), 1);
        assert_eq!(s.rounds[0].winner, Some(Team::Red));
    }
}
