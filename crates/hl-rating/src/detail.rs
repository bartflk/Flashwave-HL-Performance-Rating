//! Everything the match page shows, built from one normalized log.
//!
//! Orientation: when the owner played, their team is always on the left, so
//! every matchup reads "us vs them". Otherwise Red is on the left.

use crate::impact::Impact;
use crate::model::{extract, rate, Baseline, Rating, MODEL_VERSION};
use crate::weights::Weights;
use hl_core::matchdata::{EventLine, LogFlags, NormalizedLog, PlayerLine, Team};
use hl_core::{SteamId, TfClass};
use serde::Serialize;
use std::collections::HashMap;

/// Time on a class below this is a spawn swap or a brief flex, not a
/// matchup. Ninety seconds matches the format classifier's threshold.
const MIN_MATCHUP_TIME_S: i64 = 90;

/// How many of the largest differences get called out as decisive.
const DECISIVE_COUNT: usize = 3;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDetail {
    pub log_id: i64,
    pub title: Option<String>,
    pub map: Option<String>,
    pub played_at: Option<i64>,
    pub duration_s: i64,
    pub red_score: i64,
    pub blue_score: i64,
    pub flags: LogFlags,
    /// The owner's team, when they played.
    pub my_team: Option<Team>,
    /// `W`, `L` or `T` from the owner's side; `None` when they did not play.
    pub result: Option<&'static str>,
    pub left_team: Team,
    pub matchups: Vec<Matchup>,
    pub players: Vec<PlayerRow>,
    pub rounds: Vec<RoundRow>,
    pub model_version: &'static str,
    /// False until baselines exist (first sync or rebuild not yet run).
    pub rated: bool,
    // Index context, filled in by the caller from `log_index`.
    pub format: Option<String>,
    pub league: Option<String>,
    pub etf2l_match_id: Option<i64>,
    pub demos_tf_id: Option<i64>,
    /// Set when a user `weights.toml` exists but could not be used.
    pub weights_warning: Option<String>,
    /// Demos linked to this match; filled in by the caller.
    pub demos: Vec<DemoView>,
}

/// Where to jump in a demo: open it with `playdemo`, then `demo_gototick`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Jump {
    pub demo_id: i64,
    pub tick: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoView {
    pub demo_id: i64,
    pub file_name: String,
    /// The argument to `playdemo`: relative to `tf`, no extension.
    pub playdemo_arg: String,
    /// `pov` (your own recording) or `stv` (SourceTV, all 18 players).
    pub kind: String,
    pub recorder: Option<String>,
    pub duration_s: f64,
    pub recorded_at: Option<i64>,
    pub size_bytes: i64,
    /// How the demo was matched to this log.
    pub method: String,
    /// Share of this match's rounds inside the demo.
    pub log_share: f64,
    pub markers: usize,
    /// True when tick positions are estimated rather than derived from exact
    /// file times (STV demos, placed from their upload time).
    pub approximate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Matchup {
    pub class: TfClass,
    pub left: Option<Side>,
    pub right: Option<Side>,
    /// Kills each side's player(s) on this class scored on the other side's
    /// player(s) on this class. `None` when the log cannot attribute them.
    pub head_to_head: Option<(i64, i64)>,
    /// left score - right score. Positive favours the left.
    pub diff: Option<f64>,
    /// `left`, `right` or `even`; `None` when a side is missing.
    pub winner: Option<&'static str>,
    /// One of the few largest gaps of the match.
    pub decisive: bool,
    pub involves_me: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Side {
    /// Whoever played the class longest.
    pub account_id: u32,
    pub name: String,
    /// Anyone else who played it for a meaningful stretch.
    pub subs: Vec<String>,
    pub time_s: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub dmg: i64,
    /// The starter's rating, with its score replaced by the time-weighted
    /// average across everyone rated on this class for the team. `None` when
    /// nobody on this class had it as their main class long enough to rate.
    pub rating: Option<Rating>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRow {
    pub account_id: u32,
    pub steamid64: String,
    pub name: String,
    pub team: Team,
    pub main_class: Option<TfClass>,
    /// Every class played, longest first.
    pub classes: Vec<(TfClass, i64)>,
    pub time_s: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub dmg: i64,
    pub dpm: i64,
    pub dt: i64,
    pub hr: i64,
    pub heal: i64,
    pub ubers: i64,
    pub drops: i64,
    pub headshots: i64,
    pub headshots_hit: i64,
    pub backstabs: i64,
    pub airshots: i64,
    pub cpc: i64,
    /// Health packs picked up (logs.tf's HP column).
    pub medkits: i64,
    /// Rating on the player's main class; `None` if too short to rate.
    pub rating: Option<Rating>,
    pub is_me: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundRow {
    pub round_num: i64,
    /// Seconds from the start of the first round, for laying rounds on one axis.
    pub start_offset_s: Option<i64>,
    pub length_s: Option<i64>,
    pub winner: Option<Team>,
    pub firstcap: Option<Team>,
    pub red_kills: Option<i64>,
    pub blue_kills: Option<i64>,
    pub red_dmg: Option<i64>,
    pub blue_dmg: Option<i64>,
    pub red_ubers: Option<i64>,
    pub blue_ubers: Option<i64>,
    pub events: Vec<EventRow>,
    /// A stopwatch half: each team wore the other's colour. Teams above are
    /// still the stable teams; this is only for saying "you played RED".
    pub colours_swapped: bool,
    /// The round's start in a linked demo.
    pub jump: Option<Jump>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRow {
    pub at_s: i64,
    pub kind: String,
    pub team: Option<Team>,
    pub player: Option<String>,
    pub killer: Option<String>,
    pub killer_is_me: bool,
    pub medigun: Option<String>,
    pub point: Option<i64>,
    /// Event payload where the kind has one (a killstreak's length).
    pub value: Option<String>,
    /// This moment in a linked demo, a few seconds early to show the lead-up.
    pub jump: Option<Jump>,
}

/// `impacts` holds each player's kills valued from the raw log, when there is
/// one; the stored ratings are built the same way, so the page agrees with them.
pub fn build(
    log: &NormalizedLog,
    me: Option<SteamId>,
    w: &Weights,
    baseline: &Baseline,
    impacts: &HashMap<u32, Impact>,
) -> MatchDetail {
    let my_team = me.and_then(|m| log.players.iter().find(|p| p.id == m)).map(|p| p.team);
    let left_team = my_team.unwrap_or(Team::Red);

    let result = my_team.map(|t| {
        let (mine, theirs) = match t {
            Team::Red => (log.red_score, log.blue_score),
            Team::Blue => (log.blue_score, log.red_score),
        };
        match mine.cmp(&theirs) {
            std::cmp::Ordering::Greater => "W",
            std::cmp::Ordering::Less => "L",
            std::cmp::Ordering::Equal => "T",
        }
    });

    let names: HashMap<SteamId, String> = log
        .players
        .iter()
        .map(|p| (p.id, display_name(p)))
        .collect();

    MatchDetail {
        log_id: log.log_id,
        title: log.title.clone(),
        map: log.map.clone(),
        played_at: log.played_at,
        duration_s: log.duration_s,
        red_score: log.red_score,
        blue_score: log.blue_score,
        flags: log.flags,
        my_team,
        result,
        left_team,
        matchups: matchups(log, left_team, me, w, baseline, impacts),
        players: players(log, me, w, baseline, impacts),
        rounds: rounds(log, &names, me),
        model_version: MODEL_VERSION,
        rated: !baseline.is_empty(),
        format: None,
        league: None,
        etf2l_match_id: None,
        demos_tf_id: None,
        weights_warning: None,
        demos: Vec::new(),
    }
}

fn matchups(
    log: &NormalizedLog,
    left: Team,
    me: Option<SteamId>,
    w: &Weights,
    baseline: &Baseline,
    impacts: &HashMap<u32, Impact>,
) -> Vec<Matchup> {
    let mut rows: Vec<Matchup> = TfClass::ALL
        .iter()
        .map(|&class| {
            let l = side(log, left, class, w, baseline, impacts);
            let r = side(log, left.other(), class, w, baseline, impacts);
            let score = |s: &Option<Side>| s.as_ref().and_then(|s| s.rating.as_ref()).map(|r| r.score);
            let diff = match (score(&l), score(&r)) {
                (Some(a), Some(b)) => Some(round2(a - b)),
                _ => None,
            };
            let winner = diff.map(|d| {
                if d.abs() < w.general.even_margin {
                    "even"
                } else if d > 0.0 {
                    "left"
                } else {
                    "right"
                }
            });
            let involves_me = me.is_some_and(|m| {
                log.players.iter().any(|p| {
                    p.id == m && p.classes.iter().any(|c| c.class == class && c.time_s >= MIN_MATCHUP_TIME_S)
                })
            });
            Matchup {
                class,
                head_to_head: head_to_head(log, left, class),
                left: l,
                right: r,
                diff,
                winner,
                decisive: false,
                involves_me,
            }
        })
        .collect();

    // Call out the largest real gaps. Even matchups never qualify.
    let mut order: Vec<usize> = (0..rows.len())
        .filter(|&i| matches!(rows[i].winner, Some("left" | "right")))
        .collect();
    order.sort_by(|&a, &b| {
        let da = rows[a].diff.unwrap_or(0.0).abs();
        let db = rows[b].diff.unwrap_or(0.0).abs();
        db.total_cmp(&da)
    });
    for i in order.into_iter().take(DECISIVE_COUNT) {
        rows[i].decisive = true;
    }
    rows
}

/// Everyone on `team` who played `class` long enough to count.
fn on_class(log: &NormalizedLog, team: Team, class: TfClass) -> Vec<(&PlayerLine, i64)> {
    let mut v: Vec<_> = log
        .players
        .iter()
        .filter(|p| p.team == team)
        .filter_map(|p| {
            let t = p.classes.iter().find(|c| c.class == class)?.time_s;
            (t >= MIN_MATCHUP_TIME_S).then_some((p, t))
        })
        .collect();
    v.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
    v
}

fn side(
    log: &NormalizedLog,
    team: Team,
    class: TfClass,
    w: &Weights,
    baseline: &Baseline,
    impacts: &HashMap<u32, Impact>,
) -> Option<Side> {
    let players = on_class(log, team, class);
    let (primary, _) = *players.first()?;

    let mut s = Side {
        account_id: primary.id.account_id(),
        name: display_name(primary),
        subs: players.iter().skip(1).map(|(p, _)| display_name(p)).collect(),
        time_s: 0,
        kills: 0,
        deaths: 0,
        assists: 0,
        dmg: 0,
        rating: None,
    };

    // Everyone rated on this class, weighted by their time on it.
    let mut rated: Vec<(Rating, i64)> = Vec::new();
    for (p, _) in &players {
        let line = p.classes.iter().find(|c| c.class == class).expect("filtered on class");
        s.time_s += line.time_s;
        s.kills += line.kills;
        s.deaths += line.deaths;
        s.assists += line.assists;
        s.dmg += line.dmg;
        if p.main_class() == Some(class) {
            if let Some(r) = extract(p, &log.flags, w, impacts.get(&p.id.account_id())).and_then(|perf| rate(&perf, baseline, w)) {
                rated.push((r, line.time_s));
            }
        }
    }

    let total_t: i64 = rated.iter().map(|(_, t)| t).sum();
    if total_t > 0 {
        let weighted = rated.iter().map(|(r, t)| r.score * *t as f64).sum::<f64>() / total_t as f64;
        // Show the starter's breakdown; rank on the team's time-weighted score.
        let mut r = rated.into_iter().next().expect("total_t > 0").0;
        r.score = round1(weighted);
        s.rating = Some(r);
    }
    Some(s)
}

/// Kills between the two players on this class. `classkills` only says what
/// class a victim was, so this attributes it through each player's main class;
/// if either side's starter mainly played something else, it is unknown.
fn head_to_head(log: &NormalizedLog, left: Team, class: TfClass) -> Option<(i64, i64)> {
    let count = |team: Team| -> Option<i64> {
        let players = on_class(log, team, class);
        if players.is_empty() || players.iter().any(|(p, _)| p.main_class() != Some(class)) {
            return None;
        }
        Some(
            players
                .iter()
                .flat_map(|(p, _)| p.vs.iter())
                .filter(|v| v.other_class == class)
                .map(|v| v.kills)
                .sum(),
        )
    };
    Some((count(left)?, count(left.other())?))
}

fn players(
    log: &NormalizedLog,
    me: Option<SteamId>,
    w: &Weights,
    baseline: &Baseline,
    impacts: &HashMap<u32, Impact>,
) -> Vec<PlayerRow> {
    let class_order = |c: Option<TfClass>| {
        c.and_then(|c| TfClass::ALL.iter().position(|x| *x == c)).unwrap_or(99)
    };

    let mut rows: Vec<PlayerRow> = log
        .players
        .iter()
        .map(|p| {
            let s = &p.stats;
            let time_s = p.total_time();
            let mut classes: Vec<(TfClass, i64)> =
                p.classes.iter().filter(|c| c.time_s > 0).map(|c| (c.class, c.time_s)).collect();
            classes.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
            let main = p.main_class();
            PlayerRow {
                account_id: p.id.account_id(),
                steamid64: p.id.to_steamid64(),
                name: display_name(p),
                team: p.team,
                main_class: main,
                classes,
                time_s,
                kills: s.kills,
                deaths: s.deaths,
                assists: s.assists,
                dmg: s.dmg,
                dpm: if time_s > 0 { s.dmg * 60 / time_s } else { 0 },
                dt: s.dt,
                hr: s.hr,
                heal: s.heal,
                ubers: s.ubers,
                drops: s.drops,
                headshots: s.headshots,
                headshots_hit: s.headshots_hit,
                backstabs: s.backstabs,
                airshots: s.airshots,
                cpc: s.cpc,
                medkits: s.medkits,
                rating: extract(p, &log.flags, w, impacts.get(&p.id.account_id())).and_then(|perf| rate(&perf, baseline, w)),
                is_me: me == Some(p.id),
            }
        })
        .collect();

    rows.sort_by_key(|r| (r.team.as_str(), class_order(r.main_class), std::cmp::Reverse(r.time_s)));
    rows
}

fn rounds(log: &NormalizedLog, names: &HashMap<SteamId, String>, me: Option<SteamId>) -> Vec<RoundRow> {
    let first_start = log.rounds.iter().filter_map(|r| r.start_time).min();
    let name = |id: Option<SteamId>| id.and_then(|i| names.get(&i).cloned());

    log.rounds
        .iter()
        .map(|r| RoundRow {
            round_num: r.round_num,
            start_offset_s: r.start_time.zip(first_start).map(|(s, f)| s - f),
            length_s: r.length_s,
            winner: r.winner,
            firstcap: r.firstcap,
            red_kills: r.red.kills,
            blue_kills: r.blue.kills,
            red_dmg: r.red.dmg,
            blue_dmg: r.blue.dmg,
            red_ubers: r.red.ubers,
            blue_ubers: r.blue.ubers,
            events: r
                .events
                .iter()
                .map(|e: &EventLine| EventRow {
                    at_s: e.at_s,
                    kind: e.kind.clone(),
                    team: e.team,
                    player: name(e.player),
                    killer: name(e.killer),
                    killer_is_me: me.is_some() && e.killer == me,
                    medigun: e.medigun.clone(),
                    point: e.point,
                    value: None,
                    jump: None,
                })
                .collect(),
            colours_swapped: r.colours_swapped,
            jump: None,
        })
        .collect()
}

fn display_name(p: &PlayerLine) -> String {
    p.name.clone().unwrap_or_else(|| p.id.to_steamid3())
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
