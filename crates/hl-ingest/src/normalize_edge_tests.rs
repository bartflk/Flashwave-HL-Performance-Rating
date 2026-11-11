//! Edge cases of reading a logs.tf JSON: missing parts, odd types, ids in
//! several forms, a class that was played for no time, stopwatch colour swaps.

use super::*;
use serde_json::json;

fn minimal(players: Value) -> Value {
    json!({ "info": { "map": "pl_upward_f12", "date": 1_700_000_000 }, "length": 1800, "players": players, "teams": { "Red": { "score": 2 }, "Blue": { "score": 1 } } })
}

fn norm(v: &Value) -> NormalizedLog {
    normalize(1, v).unwrap()
}

#[test]
fn a_log_without_players_is_an_error_but_an_empty_one_is_a_log() {
    assert!(normalize(1, &json!({})).is_err());
    assert!(normalize(1, &json!({ "players": [] })).is_err(), "players must be an object");
    assert!(normalize(1, &json!(null)).is_err());
    let n = norm(&json!({ "players": {} }));
    assert!(n.players.is_empty() && n.rounds.is_empty());
    assert_eq!((n.red_score, n.blue_score, n.duration_s), (0, 0, 0));
    assert_eq!((n.title, n.map, n.played_at), (None, None, None));
}

#[test]
fn ids_in_any_form_and_unusable_players_are_dealt_with_per_player() {
    let n = norm(&minimal(json!({
        "[U:1:2006]": { "team": "Red" },
        "STEAM_0:1:1": { "team": "Blue" },
        "76561197960265733": { "team": "Blue" },
        "BOT": { "team": "Red" },
        "": { "team": "Red" },
        "[U:1:9]": { "team": "Spectator" },
        "[U:1:10]": {},
        "[U:1:11]": { "team": 5 }
    })));
    let got: Vec<(Team, u32)> = n.players.iter().map(|p| (p.team, p.id.account_id())).collect();
    assert_eq!(got, vec![(Team::Blue, 3), (Team::Blue, 5), (Team::Red, 2006)]);
}

#[test]
fn the_same_account_under_two_keys_is_one_player() {
    // STEAM_0:1:1 and [U:1:3] are the same account; a database row per
    // (log, account) cannot hold both.
    let n = norm(&minimal(json!({
        "[U:1:3]": { "team": "Blue", "kills": 5 },
        "STEAM_0:1:1": { "team": "Blue", "kills": 7 }
    })));
    assert_eq!(n.players.len(), 1);
}

#[test]
fn numbers_may_be_floats_strings_bools_or_missing() {
    let n = norm(&json!({
        "length": "1800.4",
        "players": { "[U:1:1]": { "team": "Red", "kills": 5.6, "deaths": "3", "assists": true, "dmg": null, "heal": [1], "dt": 1e30, "hr": -4 } }
    }));
    let s = &n.players[0].stats;
    assert_eq!((s.kills, s.deaths, s.assists, s.dmg, s.heal, s.hr), (6, 3, 1, 0, 0, -4));
    assert_eq!(s.dt, i64::MAX, "an absurd float saturates instead of wrapping");
    assert_eq!(n.duration_s, 1800);
}

#[test]
fn empty_titles_and_maps_are_absent_and_the_map_decides_attack_defend() {
    let mut v = minimal(json!({}));
    v["info"]["title"] = json!("");
    assert_eq!(norm(&v).title, None);
    assert!(norm(&v).flags.attack_defend);
    v["info"]["map"] = json!("koth_product_final");
    assert!(!norm(&v).flags.attack_defend);
    v["info"]["map"] = json!("");
    assert_eq!(norm(&v).map, None);
    assert!(!norm(&v).flags.attack_defend);
    v["info"]["hasRealDamage"] = json!(true);
    v["info"]["hasHS"] = json!("yes");
    let f = norm(&v).flags;
    assert!(f.real_damage && !f.hs, "only a real boolean sets a flag");
}

#[test]
fn a_class_played_for_no_time_is_kept_and_repeats_are_added() {
    let n = norm(&minimal(json!({
        "[U:1:1]": { "team": "Red", "class_stats": [
            { "type": "soldier", "total_time": 0, "kills": 0, "assists": 0, "deaths": 0, "dmg": 0, "weapon": {} },
            { "type": "medic", "total_time": 600, "kills": 1, "assists": 5, "deaths": 2, "dmg": 10,
              "weapon": { "medigun": { "shots": 0, "hits": 0 }, "syringegun_medic": { "shots": 100, "hits": 25 } } },
            { "type": "medic", "total_time": 300, "kills": 2, "assists": 1, "deaths": 0, "dmg": 5 },
            { "type": "undefined", "total_time": 99 },
            { "type": "heavyweapons", "total_time": 5 },
            { "total_time": 7 },
            { "type": "scout" }
        ] }
    })));
    let c = &n.players[0].classes;
    let by = |k: TfClass| c.iter().find(|x| x.class == k);
    assert_eq!(by(TfClass::Soldier).unwrap().time_s, 0);
    let medic = by(TfClass::Medic).unwrap();
    assert_eq!((medic.time_s, medic.kills, medic.assists, medic.deaths, medic.dmg, medic.shots, medic.hits), (900, 3, 6, 2, 15, 100, 25));
    assert_eq!(by(TfClass::Heavy).unwrap().time_s, 5);
    assert_eq!(by(TfClass::Scout).unwrap().time_s, 0);
    assert_eq!(c.len(), 4, "undefined and untyped buckets are dropped");
}

#[test]
fn versus_lines_subtract_kills_from_logstf_kills_plus_assists() {
    let v = json!({
        "players": { "[U:1:1]": { "team": "Red" } },
        "classkills": { "[U:1:1]": { "scout": 3, "medic": 1 } },
        "classdeaths": { "[U:1:1]": { "scout": 2, "sniper": 4 } },
        "classkillassists": { "[U:1:1]": { "scout": 5, "medic": 0, "spy": 2, "bogus": 9 } }
    });
    let n = norm(&v);
    let vs = &n.players[0].vs;
    let get = |k: TfClass| vs.iter().find(|x| x.other_class == k).unwrap();
    assert_eq!((get(TfClass::Scout).kills, get(TfClass::Scout).deaths, get(TfClass::Scout).assists), (3, 2, 2));
    // More kills than kills-plus-assists: never a negative assist.
    assert_eq!(get(TfClass::Medic).assists, 0);
    assert_eq!((get(TfClass::Sniper).kills, get(TfClass::Sniper).deaths), (0, 4));
    assert_eq!(get(TfClass::Spy).assists, 2);
    assert_eq!(vs.len(), 4, "an unknown class name is skipped");
}

#[test]
fn players_come_out_sorted_by_team_then_id_whatever_the_file_order() {
    let a = norm(&minimal(json!({ "[U:1:9]": { "team": "Red" }, "[U:1:2]": { "team": "Red" }, "[U:1:5]": { "team": "Blue" } })));
    let b = norm(&minimal(json!({ "[U:1:5]": { "team": "Blue" }, "[U:1:2]": { "team": "Red" }, "[U:1:9]": { "team": "Red" } })));
    let order = |n: &NormalizedLog| n.players.iter().map(|p| (p.team, p.id.account_id())).collect::<Vec<_>>();
    assert_eq!(order(&a), order(&b));
    assert_eq!(order(&a), vec![(Team::Blue, 5), (Team::Red, 2), (Team::Red, 9)]);
}

fn round(start: i64, len: i64, winner: &str, teams: &[(&str, &str)], events: Value) -> Value {
    let players: serde_json::Map<String, Value> = teams.iter().map(|(id, t)| (id.to_string(), json!({ "team": t }))).collect();
    json!({ "start_time": start, "length": len, "winner": winner, "firstcap": winner, "players": players, "events": events,
            "team": { "Red": { "kills": 4, "dmg": 100, "ubers": 1 }, "Blue": { "kills": 9, "dmg": 900 } } })
}

#[test]
fn a_stopwatch_half_with_swapped_colours_is_written_in_the_stable_teams() {
    let mut v = minimal(json!({ "[U:1:1]": { "team": "Red" }, "[U:1:2]": { "team": "Red" }, "[U:1:3]": { "team": "Blue" } }));
    v["rounds"] = json!([
        round(1000, 100, "Red", &[("[U:1:1]", "Red"), ("[U:1:2]", "Red"), ("[U:1:3]", "Blue")], json!([])),
        // Second half: the colours swap; one player is mislabelled and must not flip the round back.
        round(2000, 100, "Blue", &[("[U:1:1]", "Blue"), ("[U:1:2]", "Blue"), ("[U:1:3]", "Blue")], json!([{ "type": "pointcap", "time": 150, "team": "Blue", "point": 1 }])),
    ]);
    let n = norm(&v);
    assert!(!n.rounds[0].colours_swapped && n.rounds[1].colours_swapped);
    assert_eq!(n.rounds[0].winner, Some(Team::Red));
    // "Blue" won the swapped round: that is the stable Red team.
    assert_eq!(n.rounds[1].winner, Some(Team::Red));
    assert_eq!(n.rounds[1].firstcap, Some(Team::Red));
    // Numbers written under "Blue" belong to the stable Red team.
    assert_eq!(n.rounds[1].red.kills, Some(9));
    assert_eq!(n.rounds[1].blue.kills, Some(4));
    assert_eq!(n.rounds[0].red.ubers, Some(1));
    assert_eq!(n.rounds[1].events[0].team, Some(Team::Red));
    assert_eq!((n.rounds[0].round_num, n.rounds[1].round_num), (1, 2));
}

#[test]
fn a_round_with_no_players_listed_is_never_swapped_and_a_tie_is_not_a_swap() {
    let mut v = minimal(json!({ "[U:1:1]": { "team": "Red" }, "[U:1:2]": { "team": "Blue" } }));
    v["rounds"] = json!([
        round(1000, 100, "Red", &[], json!([])),
        round(2000, 100, "Red", &[("[U:1:1]", "Blue"), ("[U:1:2]", "Blue")], json!([])),
        round(3000, 100, "Red", &[("[U:1:99]", "Blue")], json!([])),
    ]);
    let n = norm(&v);
    assert!(n.rounds.iter().all(|r| !r.colours_swapped));
}

#[test]
fn event_times_are_seconds_into_the_round() {
    let mut v = minimal(json!({ "[U:1:1]": { "team": "Red" } }));
    // The log's clock runs from its start: round 2 begins at 400 s, and its
    // round_win lands at start + length.
    v["rounds"] = json!([
        round(1000, 100, "Red", &[], json!([{ "type": "round_win", "time": 100, "team": "Red" }])),
        round(1400, 120, "Red", &[], json!([
            { "type": "pointcap", "time": 450, "team": "Red", "point": 2 },
            { "type": "kill", "time": 399, "killer": "[U:1:1]", "steamid": "STEAM_0:1:1" },
            { "type": "round_win", "time": 520, "team": "Red" },
            { "type": "charge", "steamid": "[U:1:1]", "medigun": "kritzkrieg" },
            { "type": "x", "time": "7" }
        ])),
    ]);
    let n = norm(&v);
    let ev = &n.rounds[1].events;
    assert_eq!(ev.iter().map(|e| e.at_s).collect::<Vec<_>>(), vec![50, 0, 120, 0]);
    assert_eq!(ev.len(), 4, "an event without a time is dropped");
    assert_eq!(ev[0].point, Some(2));
    assert_eq!(ev[1].killer.map(|k| k.account_id()), Some(1));
    assert_eq!(ev[1].player.map(|k| k.account_id()), Some(3), "a SteamID2 in an event reads like any other");
}

#[test]
fn a_round_without_its_win_event_falls_back_to_start_times() {
    let mut v = minimal(json!({ "[U:1:1]": { "team": "Red" } }));
    v["rounds"] = json!([
        round(5000, 100, "Red", &[], json!([{ "type": "pointcap", "time": 30, "team": "Red" }])),
        round(5400, 100, "Red", &[], json!([{ "type": "pointcap", "time": 430, "team": "Red" }])),
    ]);
    let n = norm(&v);
    assert_eq!(n.rounds[0].events[0].at_s, 30);
    assert_eq!(n.rounds[1].events[0].at_s, 30);
    // Rounds listed out of order still anchor on the earliest start.
    v["rounds"].as_array_mut().unwrap().reverse();
    let n = norm(&v);
    assert_eq!(n.rounds[1].events[0].at_s, 30);
    // An event before its round's start is clamped to 0, never negative.
    v["rounds"] = json!([round(5000, 100, "Red", &[], json!([{ "type": "pointcap", "time": -40 }]))]);
    assert_eq!(norm(&v).rounds[0].events[0].at_s, 0);
}

#[test]
fn rounds_that_are_not_a_list_or_have_no_fields_still_read() {
    let mut v = minimal(json!({}));
    v["rounds"] = json!("nope");
    assert!(norm(&v).rounds.is_empty());
    v["rounds"] = json!([{}, {"length": "x"}]);
    let n = norm(&v);
    assert_eq!(n.rounds.len(), 2);
    assert_eq!((n.rounds[0].start_time, n.rounds[0].length_s, n.rounds[0].winner), (None, None, None));
    assert!(n.rounds[0].events.is_empty());
}

#[test]
fn steam_ids_serialize_as_strings_in_every_form_they_are_read_from() {
    let id = SteamId::from_account_id(2006);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"76561197960267734\"");
    assert_eq!(serde_json::from_str::<SteamId>(&json).unwrap(), id);
    assert_eq!(serde_json::from_str::<SteamId>("\"[U:1:2006]\"").unwrap(), id);
    assert!(serde_json::from_str::<SteamId>("\"nope\"").is_err());
    assert!(serde_json::from_str::<SteamId>("76561197960267734").is_err(), "a bare JSON number is not a string");
}
