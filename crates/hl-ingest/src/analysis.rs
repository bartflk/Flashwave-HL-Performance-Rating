//! Everything the match page's analysis views draw, built on demand from the
//! stored raw log: every kill with positions, damage by class, a damage
//! timeline, and the play-by-play.
//!
//! Built per request rather than stored: the raw zip is already on disk, a
//! parse takes milliseconds, and damage alone is thousands of lines per match.
//!
//! **Game time.** Rounds are laid end to end with the gaps between them
//! removed, oldest first. Combined logs list their rounds out of order and can
//! span hours of breaks; game time keeps the timeline about the play.

use crate::demos::{jumper, Jumper, JUMP_LEAD_S};
use crate::normalize::normalize;
use crate::rawlog::{self, hits_capped, RawLog};
use crate::fights::{FightStats, KillTags};
use crate::state::{Charge, GameState};
use anyhow::{Context, Result};
use hl_core::matchdata::{NormalizedLog, Team};
use hl_core::{SteamId, TfClass};
use hl_db::{Db, RoundWindow, Segment};
use hl_rating::Jump;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// Damage timeline resolution, in game seconds.
pub const BUCKET_S: f64 = 30.0;
/// A killstreak is this many kills without dying.
const STREAK_MIN: usize = 3;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub log_id: i64,
    pub map: Option<String>,
    /// Game seconds: rounds end to end.
    pub duration_s: f64,
    pub rounds: Vec<RoundSpan>,
    pub players: Vec<Player>,
    pub kills: Vec<KillView>,
    /// Per player and other class: damage dealt to and taken from that class.
    pub damage: Vec<ClassDamage>,
    /// Per player: damage dealt in each `bucket_s` window of game time.
    pub damage_series: Vec<DamageSeries>,
    pub bucket_s: f64,
    /// Ubers, drops, caps, chat and killstreaks, in game-time order.
    pub events: Vec<PlayEvent>,
    /// Kills carry positions (every log since 2014 on this account does).
    pub has_positions: bool,
    /// logs.tf capped this log's hits at 450; damage here follows it.
    pub damage_capped: bool,
    /// The maps played, in order: one for most logs, two or three for a log
    /// combined after a scrim or official.
    pub segments: Vec<MapSegment>,
    /// Players alive and uber charge for each second of game time.
    pub state: StateSeries,
    /// Every player's kills in context (PLAN §11 B and D).
    pub fights: Vec<FightStats>,
    /// The first kill of each round.
    pub first_picks: Vec<FirstPickView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstPickView {
    /// Game seconds.
    pub t: f64,
    pub round_num: i64,
    /// Seconds after the round went live (the end of setup in stopwatch).
    pub after_s: i64,
    pub killer: u32,
    pub victim: u32,
}

/// The game state (`state.rs`) sampled once per game second, in stable
/// teams: a stopwatch half's colour swap is undone.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSeries {
    pub red_alive: Vec<u8>,
    pub blue_alive: Vec<u8>,
    /// 0-99 building, 100 ready, 101 in use, -1 no Medic alive.
    pub red_charge: Vec<i8>,
    pub blue_charge: Vec<i8>,
    /// Uber advantage: 1 Red, -1 Blue, 0 neither.
    pub advantage: Vec<i8>,
}

impl StateSeries {
    fn new(gs: &GameState, clock: &Clock, log: &NormalizedLog) -> Self {
        let n = clock.duration().ceil() as usize;
        let mut out = StateSeries {
            red_alive: vec![0; n],
            blue_alive: vec![0; n],
            red_charge: vec![-1; n],
            blue_charge: vec![-1; n],
            advantage: vec![0; n],
        };
        let code = |c: Charge| match c {
            Charge::NoMedic => -1,
            Charge::Building { pct } => pct.clamp(0.0, 99.0) as i8,
            Charge::Ready => 100,
            Charge::Deployed => 101,
        };
        for &(num, start, len, off) in &clock.rounds {
            let swapped = log.rounds.iter().any(|r| r.round_num == num && r.colours_swapped);
            // The colour each stable team wore this round.
            let (red, blue) = if swapped { (Team::Blue, Team::Red) } else { (Team::Red, Team::Blue) };
            for sec in 0..len {
                let i = (off + sec as f64) as usize;
                if i >= n {
                    break;
                }
                let t = start + sec;
                let alive = gs.numbers_at(t);
                let by_colour = |team: Team| if team == Team::Red { alive[0] } else { alive[1] };
                out.red_alive[i] = by_colour(red);
                out.blue_alive[i] = by_colour(blue);
                out.red_charge[i] = code(gs.charge_at(red, t));
                out.blue_charge[i] = code(gs.charge_at(blue, t));
                out.advantage[i] = match gs.advantage_at(t) {
                    Some(c) if c == red => 1,
                    Some(_) => -1,
                    None => 0,
                };
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapSegment {
    pub map: Option<String>,
    pub first_round: i64,
    pub last_round: i64,
    /// The segment's rounds in play order. Combined logs can number rounds
    /// out of order, so "first to last" is not a range of numbers.
    pub rounds: Vec<i64>,
    /// Game seconds.
    pub start_s: f64,
    pub end_s: f64,
    pub red_wins: i64,
    pub blue_wins: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundSpan {
    pub round_num: i64,
    pub start_s: f64,
    pub end_s: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub account_id: u32,
    pub name: String,
    /// The stable team, whatever colour a stopwatch half wore.
    pub team: Team,
    pub main_class: Option<TfClass>,
    pub is_me: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KillView {
    pub t: f64,
    pub round_num: i64,
    pub killer: u32,
    pub victim: u32,
    pub assister: Option<u32>,
    pub killer_class: Option<TfClass>,
    pub victim_class: Option<TfClass>,
    pub weapon: String,
    /// `headshot`, `backstab`, ...
    pub custom: Option<String>,
    pub killer_pos: Option<[i32; 3]>,
    pub victim_pos: Option<[i32; 3]>,
    /// Game units between the two players.
    pub distance: Option<f64>,
    /// The map of the kill's round.
    pub map: Option<String>,
    pub jump: Option<Jump>,
    /// Opening, traded, clean-up...: what the kill meant.
    pub tags: Option<KillTags>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassDamage {
    pub account_id: u32,
    pub other_class: TfClass,
    /// The round it happened in, so a round filter can narrow it. `0` for
    /// damage outside every round (warmup, or between rounds).
    pub round_num: i64,
    pub dealt: i64,
    pub taken: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageSeries {
    pub account_id: u32,
    pub buckets: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayEvent {
    pub t: f64,
    pub round_num: i64,
    /// `charge`, `drop`, `pointcap`, `chat`, `streak`.
    pub kind: String,
    pub team: Option<Team>,
    pub player: Option<u32>,
    /// Chat text, the medigun, the point number, or the streak length.
    pub text: Option<String>,
    /// A streak's victims, in order.
    pub victims: Vec<u32>,
    pub team_chat: bool,
    pub jump: Option<Jump>,
}

/// `None` when the log or its raw log is not stored.
pub async fn load(db: &Db, log_id: i64, me: Option<SteamId>) -> Result<Option<Analysis>> {
    let (Some(json), Some(zip)) = (db.raw_log(log_id).await?, db.rawlog(log_id).await?) else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(&json).with_context(|| format!("log {log_id} JSON"))?;
    let log = normalize(log_id, &value)?;
    let uploaded = value.pointer("/info/date").and_then(serde_json::Value::as_i64);

    let mut raw = rawlog::parse(&rawlog::unzip(&zip)?);
    let starts: Vec<i64> = log.rounds.iter().filter_map(|r| r.start_time).collect();
    if let Some(shift) = rawlog::frame_offset(&raw.round_starts, &starts) {
        raw.shift(shift);
    }
    let jumps = jumper(db, log_id).await?;
    let windows = db.round_windows(log_id).await?;
    let segments = db.segments(log_id).await?;
    Ok(Some(build(&log, &raw, me, hits_capped(uploaded), jumps.as_ref(), &windows, &segments)))
}

/// Maps a moment in the log's frame to game time.
struct Clock {
    /// (round number, start in the log frame, length, game-time offset)
    rounds: Vec<(i64, i64, i64, f64)>,
}

impl Clock {
    fn new(log: &NormalizedLog) -> Self {
        let mut rs: Vec<(i64, i64, i64)> = log
            .rounds
            .iter()
            .filter_map(|r| Some((r.round_num, r.start_time?, r.length_s?)))
            .collect();
        rs.sort_by_key(|r| r.1);
        let mut offset = 0.0;
        let rounds = rs
            .into_iter()
            .map(|(n, s, len)| {
                let row = (n, s, len, offset);
                offset += len as f64;
                row
            })
            .collect();
        Clock { rounds }
    }

    /// `(game seconds, round number)`, or `None` outside every round.
    fn game(&self, t: i64) -> Option<(f64, i64)> {
        self.rounds
            .iter()
            .find(|(_, s, len, _)| t >= *s && t <= s + len)
            .map(|(n, s, _, off)| (off + (t - s) as f64, *n))
    }

    fn duration(&self) -> f64 {
        self.rounds.last().map_or(0.0, |(_, _, len, off)| off + *len as f64)
    }
}

pub fn build(
    log: &NormalizedLog,
    raw: &RawLog,
    me: Option<SteamId>,
    capped: bool,
    jumps: Option<&Jumper>,
    windows: &[RoundWindow],
    segments: &[Segment],
) -> Analysis {
    let clock = Clock::new(log);
    let map_of = |t: i64| crate::kills::map_at(windows, t).map(str::to_string).or_else(|| log.map.clone());
    let jump = |t: i64| jumps.and_then(|j| j.at(t, JUMP_LEAD_S));
    let team_of: HashMap<u32, Team> = log.players.iter().map(|p| (p.id.account_id(), p.team)).collect();

    let players = log
        .players
        .iter()
        .map(|p| Player {
            account_id: p.id.account_id(),
            name: p.name.clone().unwrap_or_else(|| p.id.to_steamid3()),
            team: p.team,
            main_class: p.main_class(),
            is_me: me == Some(p.id),
        })
        .collect();

    let gs = GameState::build(raw);
    let fights = crate::fights::analyse(raw, &gs);

    // Kills logs.tf counts, placed in game time.
    let mut kills: Vec<KillView> = raw
        .kills
        .iter()
        .enumerate()
        .filter(|(_, k)| k.counts())
        .filter_map(|(i, k)| {
            let (t, round_num) = clock.game(k.at)?;
            let distance = match (k.killer_pos, k.victim_pos) {
                (Some(a), Some(b)) => Some(
                    (((a[0] - b[0]) as f64).powi(2) + ((a[1] - b[1]) as f64).powi(2) + ((a[2] - b[2]) as f64).powi(2))
                        .sqrt()
                        .round(),
                ),
                _ => None,
            };
            Some(KillView {
                t,
                round_num,
                killer: k.killer.account,
                victim: k.victim.account,
                assister: k.assister,
                killer_class: k.killer.class,
                victim_class: k.victim.class,
                weapon: k.weapon.clone(),
                custom: k.custom.clone(),
                killer_pos: k.killer_pos,
                victim_pos: k.victim_pos,
                distance,
                map: map_of(k.at),
                jump: jump(k.at),
                tags: fights.tags[i],
            })
        })
        .collect();
    kills.sort_by(|a, b| a.t.total_cmp(&b.t));
    let has_positions = kills.iter().any(|k| k.killer_pos.is_some());

    // Damage by the other player's class, and over time.
    let mut by_class: BTreeMap<(u32, TfClass, i64), (i64, i64)> = BTreeMap::new();
    let duration = clock.duration();
    let n_buckets = ((duration / BUCKET_S).ceil() as usize).max(1);
    let mut series: BTreeMap<u32, Vec<i64>> = BTreeMap::new();
    for d in &raw.damage {
        let amount = d.counted(capped);
        if amount == 0 || d.attacker.account == d.victim.account {
            continue;
        }
        let round = clock.game(d.at).map_or(0, |(_, r)| r);
        if let Some(c) = d.victim.class {
            by_class.entry((d.attacker.account, c, round)).or_default().0 += amount;
        }
        if let Some(c) = d.attacker.class {
            by_class.entry((d.victim.account, c, round)).or_default().1 += amount;
        }
        if let Some((t, _)) = clock.game(d.at) {
            let b = ((t / BUCKET_S) as usize).min(n_buckets - 1);
            series.entry(d.attacker.account).or_insert_with(|| vec![0; n_buckets])[b] += amount;
        }
    }
    let damage = by_class
        .into_iter()
        .map(|((account_id, other_class, round_num), (dealt, taken))| ClassDamage { account_id, other_class, round_num, dealt, taken })
        .collect();
    let damage_series =
        series.into_iter().map(|(account_id, buckets)| DamageSeries { account_id, buckets }).collect();

    // Play-by-play: logs.tf's ubers, drops and caps, the chat, and streaks.
    let mut events: Vec<PlayEvent> = Vec::new();
    for r in &log.rounds {
        let Some(start) = r.start_time else { continue };
        for e in &r.events {
            if !matches!(e.kind.as_str(), "charge" | "drop" | "pointcap") {
                continue;
            }
            let at = start + e.at_s;
            let Some((t, round_num)) = clock.game(at) else { continue };
            events.push(PlayEvent {
                t,
                round_num,
                kind: e.kind.clone(),
                team: e.team,
                player: e.player.map(|p| p.account_id()),
                text: e.medigun.clone().or(e.point.map(|p| p.to_string())),
                victims: Vec::new(),
                team_chat: false,
                jump: jump(at),
            });
        }
    }
    for c in &raw.chat {
        let Some((t, round_num)) = clock.game(c.at) else { continue };
        events.push(PlayEvent {
            t,
            round_num,
            kind: "chat".into(),
            team: c.account.and_then(|a| team_of.get(&a).copied()),
            player: c.account,
            text: Some(c.message.clone()),
            victims: Vec::new(),
            team_chat: c.team_chat,
            jump: None,
        });
    }
    events.extend(streaks(&kills, &team_of));
    events.sort_by(|a, b| a.t.total_cmp(&b.t));

    // Each map segment on game time, from its first round's start to its last
    // round's end.
    let span = |n: i64| clock.rounds.iter().find(|r| r.0 == n).map(|(_, _, len, off)| (*off, off + *len as f64));
    let order: Vec<i64> = clock.rounds.iter().map(|r| r.0).collect();
    let between = |first: i64, last: i64| -> Vec<i64> {
        match (order.iter().position(|&n| n == first), order.iter().position(|&n| n == last)) {
            (Some(a), Some(b)) if a <= b => order[a..=b].to_vec(),
            _ => vec![first],
        }
    };
    let map_segments: Vec<MapSegment> = if segments.is_empty() {
        vec![MapSegment {
            map: log.map.clone(),
            first_round: clock.rounds.first().map_or(1, |r| r.0),
            last_round: clock.rounds.last().map_or(1, |r| r.0),
            rounds: order.clone(),
            start_s: 0.0,
            end_s: clock.duration(),
            red_wins: 0,
            blue_wins: 0,
        }]
    } else {
        segments
            .iter()
            .map(|s| MapSegment {
                map: s.map.clone(),
                first_round: s.first_round,
                last_round: s.last_round,
                rounds: between(s.first_round, s.last_round),
                start_s: span(s.first_round).map_or(0.0, |x| x.0),
                end_s: span(s.last_round).map_or(clock.duration(), |x| x.1),
                red_wins: s.red_wins,
                blue_wins: s.blue_wins,
            })
            .collect()
    };

    let state = StateSeries::new(&gs, &clock, log);
    // First picks in game time. The raw log's round numbers run in file
    // order; the game clock knows each round by its start.
    let first_picks = fights
        .first_picks
        .iter()
        .filter_map(|f| {
            let k = raw.kills.iter().find(|k| k.round == f.round && k.killer.account == f.killer && k.victim.account == f.victim)?;
            let (t, round_num) = clock.game(k.at)?;
            Some(FirstPickView { t, round_num, after_s: f.after_s, killer: f.killer, victim: f.victim })
        })
        .collect();
    Analysis {
        state,
        fights: fights.players,
        first_picks,
        segments: map_segments,
        log_id: log.log_id,
        map: log.map.clone(),
        duration_s: duration,
        rounds: clock
            .rounds
            .iter()
            .map(|(n, _, len, off)| RoundSpan { round_num: *n, start_s: *off, end_s: off + *len as f64 })
            .collect(),
        players,
        kills,
        damage,
        damage_series,
        bucket_s: BUCKET_S,
        events,
        has_positions,
        damage_capped: capped,
    }
}

/// Runs of `STREAK_MIN` or more kills without dying, reported at the last kill.
fn streaks(kills: &[KillView], team_of: &HashMap<u32, Team>) -> Vec<PlayEvent> {
    let mut open: HashMap<u32, Vec<&KillView>> = HashMap::new();
    let mut out = Vec::new();
    let close = |who: u32, run: Vec<&KillView>, out: &mut Vec<PlayEvent>| {
        if run.len() >= STREAK_MIN {
            let last = run[run.len() - 1];
            out.push(PlayEvent {
                t: last.t,
                round_num: last.round_num,
                kind: "streak".into(),
                team: team_of.get(&who).copied(),
                player: Some(who),
                text: Some(run.len().to_string()),
                victims: run.iter().map(|k| k.victim).collect(),
                team_chat: false,
                jump: run[0].jump,
            });
        }
    };
    for k in kills {
        if let Some(run) = open.remove(&k.victim) {
            close(k.victim, run, &mut out);
        }
        open.entry(k.killer).or_default().push(k);
    }
    for (who, run) in open {
        close(who, run, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(t: f64, killer: u32, victim: u32) -> KillView {
        KillView {
            t,
            round_num: 1,
            killer,
            victim,
            assister: None,
            killer_class: None,
            victim_class: None,
            weapon: String::new(),
            custom: None,
            killer_pos: None,
            victim_pos: None,
            distance: None,
            map: None,
            jump: None,
            tags: None,
        }
    }

    #[test]
    fn a_death_ends_a_streak() {
        let ks = [kv(1.0, 1, 10), kv(2.0, 1, 11), kv(3.0, 2, 1), kv(4.0, 1, 12), kv(5.0, 1, 13), kv(6.0, 1, 14)];
        let s = streaks(&ks, &HashMap::new());
        assert_eq!(s.len(), 1, "two kills then a death is not a streak");
        assert_eq!(s[0].text.as_deref(), Some("3"));
        assert_eq!(s[0].victims, vec![12, 13, 14]);
        assert_eq!(s[0].t, 6.0);
    }
}
