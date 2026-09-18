//! logs.tf JSON -> [`NormalizedLog`].
//!
//! Works on `serde_json::Value` rather than typed structs on purpose. The
//! account spans logs from 2014 to today: early logs key players by SteamID2,
//! capability flags vary per log, and fields come and go. A typed parse would
//! reject a whole 12-year-old log over one odd field; this extracts what is
//! there and treats anything absent as absent.

use hl_core::matchdata::*;
use hl_core::{SteamId, TfClass};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub fn normalize(log_id: i64, raw: &Value) -> anyhow::Result<NormalizedLog> {
    let info = raw.get("info").unwrap_or(&Value::Null);
    let players_obj = raw
        .get("players")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("log {log_id}: no players object"))?;

    let names = raw.get("names").and_then(Value::as_object);
    let classkills = raw.get("classkills").and_then(Value::as_object);
    let classdeaths = raw.get("classdeaths").and_then(Value::as_object);
    let classassists = raw.get("classkillassists").and_then(Value::as_object);

    let mut players = Vec::with_capacity(players_obj.len());
    for (key, p) in players_obj {
        // Unparseable ids (bots, malformed uploads) are skipped rather than
        // failing the whole log.
        let Ok(id) = SteamId::parse(key) else {
            tracing::debug!(log_id, key, "skipping player with unparseable id");
            continue;
        };
        let Some(team) = p.get("team").and_then(Value::as_str).and_then(Team::parse) else {
            continue; // spectators and unassigned players
        };

        players.push(PlayerLine {
            id,
            name: names
                .and_then(|n| n.get(key))
                .and_then(Value::as_str)
                .map(str::to_owned),
            team,
            stats: stats(p),
            classes: classes(p),
            vs: vs_lines(
                classkills.and_then(|m| m.get(key)),
                classdeaths.and_then(|m| m.get(key)),
                classassists.and_then(|m| m.get(key)),
            ),
        });
    }

    // Deterministic order makes normalized output diffable and testable.
    players.sort_by_key(|p| (p.team.as_str(), p.id));

    let teams = raw.get("teams");
    let score = |t: &str| teams.and_then(|x| x.get(t)).map(|x| int(x, "score")).unwrap_or(0);

    Ok(NormalizedLog {
        log_id,
        title: str_field(info, "title"),
        map: str_field(info, "map"),
        played_at: info.get("date").and_then(as_i64),
        duration_s: int(raw, "length"),
        red_score: score("Red"),
        blue_score: score("Blue"),
        flags: LogFlags {
            real_damage: flag(info, "hasRealDamage"),
            accuracy: flag(info, "hasAccuracy"),
            hs: flag(info, "hasHS"),
            hs_hit: flag(info, "hasHS_hit"),
            bs: flag(info, "hasBS"),
            cp: flag(info, "hasCP"),
            dt: flag(info, "hasDT"),
            airshots: flag(info, "hasAS"),
            hr: flag(info, "hasHR"),
        },
        players,
        rounds: rounds(raw),
    })
}

fn stats(p: &Value) -> PlayerStats {
    PlayerStats {
        kills: int(p, "kills"),
        deaths: int(p, "deaths"),
        assists: int(p, "assists"),
        suicides: int(p, "suicides"),
        dmg: int(p, "dmg"),
        dmg_real: int(p, "dmg_real"),
        dt: int(p, "dt"),
        dt_real: int(p, "dt_real"),
        hr: int(p, "hr"),
        heal: int(p, "heal"),
        ubers: int(p, "ubers"),
        drops: int(p, "drops"),
        headshots: int(p, "headshots"),
        headshots_hit: int(p, "headshots_hit"),
        backstabs: int(p, "backstabs"),
        medkits: int(p, "medkits"),
        medkits_hp: int(p, "medkits_hp"),
        sentries: int(p, "sentries"),
        cpc: int(p, "cpc"),
        ic: int(p, "ic"),
        lks: int(p, "lks"),
        airshots: int(p, "as"),
    }
}

/// Per-class lines, merging any class that appears twice (a player who swaps
/// off and back on) and dropping logs.tf's `undefined` bucket.
fn classes(p: &Value) -> Vec<ClassLine> {
    let mut by_class: BTreeMap<TfClass, ClassLine> = BTreeMap::new();
    for c in p.get("class_stats").and_then(Value::as_array).into_iter().flatten() {
        let Some(class) = c.get("type").and_then(Value::as_str).and_then(|t| TfClass::parse(t).ok())
        else {
            continue;
        };
        let line = by_class.entry(class).or_insert(ClassLine {
            class,
            time_s: 0,
            kills: 0,
            assists: 0,
            deaths: 0,
            dmg: 0,
        });
        line.time_s += int(c, "total_time");
        line.kills += int(c, "kills");
        line.assists += int(c, "assists");
        line.deaths += int(c, "deaths");
        line.dmg += int(c, "dmg");
    }
    by_class.into_values().collect()
}

fn vs_lines(kills: Option<&Value>, deaths: Option<&Value>, assists: Option<&Value>) -> Vec<VsLine> {
    let mut by_class: BTreeMap<TfClass, VsLine> = BTreeMap::new();
    let mut add = |src: Option<&Value>, apply: fn(&mut VsLine, i64)| {
        for (class, n) in src.and_then(Value::as_object).into_iter().flatten() {
            let Ok(class) = TfClass::parse(class) else { continue };
            let n = as_i64(n).unwrap_or(0);
            let line = by_class.entry(class).or_insert(VsLine {
                other_class: class,
                kills: 0,
                deaths: 0,
                assists: 0,
            });
            apply(line, n);
        }
    };
    add(kills, |l, n| l.kills += n);
    add(deaths, |l, n| l.deaths += n);
    add(assists, |l, n| l.assists += n);
    by_class.into_values().collect()
}

fn rounds(raw: &Value) -> Vec<RoundLine> {
    let Some(rounds) = raw.get("rounds").and_then(Value::as_array) else {
        return Vec::new();
    };
    let first_start = rounds.iter().filter_map(|r| r.get("start_time").and_then(as_i64)).min();
    let overall = overall_teams(raw);

    rounds
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let base = round_base(r, first_start);
            let swapped = colours_swapped(r, &overall);
            // A colour as written in this round -> the stable team wearing it.
            let stable = |t: Team| if swapped { t.other() } else { t };
            let colour_stats = |t: &str| {
                let x = r.get("team").and_then(|x| x.get(t));
                RoundTeam {
                    kills: x.and_then(|x| x.get("kills")).and_then(as_i64),
                    dmg: x.and_then(|x| x.get("dmg")).and_then(as_i64),
                    ubers: x.and_then(|x| x.get("ubers")).and_then(as_i64),
                }
            };
            // The stable Red team's numbers sit under "Blue" in a swapped round.
            let (red, blue) = if swapped {
                (colour_stats("Blue"), colour_stats("Red"))
            } else {
                (colour_stats("Red"), colour_stats("Blue"))
            };
            let colour = |k: &str| r.get(k).and_then(Value::as_str).and_then(Team::parse);

            RoundLine {
                round_num: i as i64 + 1,
                start_time: r.get("start_time").and_then(as_i64),
                length_s: r.get("length").and_then(as_i64),
                winner: colour("winner").map(stable),
                firstcap: colour("firstcap").map(stable),
                red,
                blue,
                events: r
                    .get("events")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|e| event(e, base))
                    .map(|mut e| {
                        e.team = e.team.map(stable);
                        e
                    })
                    .collect(),
                colours_swapped: swapped,
            }
        })
        .collect()
}

/// Each player's team for the log as a whole: their stable team identity.
fn overall_teams(raw: &Value) -> HashMap<&str, Team> {
    raw.get("players")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(id, p)| Some((id.as_str(), p.get("team")?.as_str().and_then(Team::parse)?)))
        .collect()
}

/// Did the teams wear each other's colours this round? Stopwatch swaps sides
/// between halves, and the log writes each round in that round's colours.
/// Decided by majority over `round.players[id].team`, so one mislabelled
/// player cannot flip a round. Logs without per-round teams are never swapped.
fn colours_swapped(r: &Value, overall: &HashMap<&str, Team>) -> bool {
    let (mut same, mut flipped) = (0, 0);
    for (id, p) in r.get("players").and_then(Value::as_object).into_iter().flatten() {
        let Some(round_team) = p.get("team").and_then(Value::as_str).and_then(Team::parse) else {
            continue;
        };
        match overall.get(id.as_str()) {
            Some(&t) if t == round_team => same += 1,
            Some(_) => flipped += 1,
            None => {}
        }
    }
    flipped > same
}

/// Where this round starts, in the log's own clock.
///
/// logs.tf stamps events in seconds since the log began, not since the round
/// began. The round's `round_win` event lands exactly at `start + length`, so
/// that pins the start without trusting anything else. Failing that, fall back
/// to wall-clock start times, which assumes the log began with round 1.
fn round_base(r: &Value, first_start: Option<i64>) -> i64 {
    let length = r.get("length").and_then(as_i64);
    let win_at = r
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|e| e.get("type").and_then(Value::as_str) == Some("round_win"))
        .and_then(|e| e.get("time").and_then(as_i64));

    match (win_at, length) {
        (Some(t), Some(len)) if t >= len => t - len,
        _ => r
            .get("start_time")
            .and_then(as_i64)
            .zip(first_start)
            .map(|(s, f)| (s - f).max(0))
            .unwrap_or(0),
    }
}

fn event(e: &Value, base: i64) -> Option<EventLine> {
    let id = |k: &str| e.get(k).and_then(Value::as_str).and_then(|s| SteamId::parse(s).ok());
    Some(EventLine {
        // Seconds into the round; see `round_base`.
        at_s: (e.get("time").and_then(as_i64)? - base).max(0),
        kind: e.get("type").and_then(Value::as_str)?.to_owned(),
        team: e.get("team").and_then(Value::as_str).and_then(Team::parse),
        player: id("steamid"),
        killer: id("killer"),
        medigun: str_field(e, "medigun"),
        point: e.get("point").and_then(as_i64),
    })
}

// ---- lenient field access ----------------------------------------------------

/// Numbers in logs.tf JSON are sometimes floats and occasionally strings.
fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64)),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

fn int(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(as_i64).unwrap_or(0)
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
