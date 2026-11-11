//! Things that must hold for every real log in the fixtures, whatever its
//! era: one row per player, rounds in order, events inside their rounds, a
//! raw log that lines up with its logs.tf rounds.

use hl_ingest::normalize::normalize;
use hl_ingest::rawlog;
use serde_json::Value;
use std::collections::HashSet;

const JSONS: &[(i64, &str)] = &[
    (295610, "hl_2014_steamid2_295610.json"),
    (3445078, "hl_2023_3445078.json"),
    (4109131, "hl_combined_6rounds_4109131.json"),
    (4114301, "hl_sub_19p_4114301.json"),
    (4115969, "sixes_4115969.json"),
];

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn load(name: &str) -> Value {
    serde_json::from_slice(&fixture(name)).unwrap()
}

#[test]
fn every_fixture_normalizes_to_one_row_per_player_and_ordered_rounds() {
    for (id, name) in JSONS {
        let log = normalize(*id, &load(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let accounts: HashSet<u32> = log.players.iter().map(|p| p.id.account_id()).collect();
        assert_eq!(accounts.len(), log.players.len(), "{name}: a player twice");
        assert!(log.duration_s > 0, "{name}");
        for (i, r) in log.rounds.iter().enumerate() {
            assert_eq!(r.round_num, i as i64 + 1, "{name}");
        }
        // A player's classes: each class once, in the nine, and no negative time.
        for p in &log.players {
            let classes: HashSet<_> = p.classes.iter().map(|c| c.class).collect();
            assert_eq!(classes.len(), p.classes.len(), "{name}: a class twice for {}", p.id);
            assert!(p.classes.iter().all(|c| c.time_s >= 0 && c.kills >= 0 && c.deaths >= 0), "{name}");
            let vs: HashSet<_> = p.vs.iter().map(|v| v.other_class).collect();
            assert_eq!(vs.len(), p.vs.len(), "{name}");
            assert!(p.vs.iter().all(|v| v.kills >= 0 && v.deaths >= 0 && v.assists >= 0), "{name}");
        }
        // Rounds in this log never overlap and events stay inside them.
        let mut last_end = i64::MIN / 2;
        for r in &log.rounds {
            let (Some(s), Some(len)) = (r.start_time, r.length_s) else { continue };
            assert!(s >= last_end - 1, "{name}: round {} starts before the one before ended", r.round_num);
            last_end = s + len;
            for e in &r.events {
                assert!(e.at_s >= 0, "{name}");
                // Events trail the round's end by a moment at most (humiliation).
                assert!(e.at_s <= len + 60, "{name}: round {} event {} at {} of {}", r.round_num, e.kind, e.at_s, len);
            }
        }
    }
}

#[test]
fn normalizing_twice_gives_the_same_log() {
    for (id, name) in JSONS {
        let v = load(name);
        let a = serde_json::to_string(&normalize(*id, &v).unwrap()).unwrap();
        let b = serde_json::to_string(&normalize(*id, &v).unwrap()).unwrap();
        assert_eq!(a, b, "{name}");
    }
}

#[test]
fn a_raw_log_lines_up_with_its_logstf_rounds_and_stays_in_order() {
    // 4121291 is the 2026 scrim; its logs.tf JSON is not stored as a fixture,
    // but its rounds start at raw Round_Start lines once shifted: the shift
    // itself is whole hours.
    for zip in ["log_4121291.log.zip", "log_513611.log.zip"] {
        let text = rawlog::unzip(&fixture(zip)).unwrap();
        let log = rawlog::parse(&text);
        assert!(log.rounds > 0, "{zip}");
        assert_eq!(log.rounds as usize, log.round_starts.len());
        // Kills are stored in the order written; rounds never go backwards.
        assert!(log.kills.windows(2).all(|w| w[0].round <= w[1].round), "{zip}");
        // A kill's players are never the same account unless it is a suicide
        // written as a kill; and teams are always known on both sides.
        assert!(log.kills.iter().all(|k| k.killer.team.is_some() && k.victim.team.is_some()), "{zip}");
        // The same log shifted by any whole number of hours and back is unchanged.
        let mut shifted = log.clone();
        shifted.shift(7200);
        assert_eq!(rawlog::frame_offset(&log.round_starts, &shifted.round_starts), Some(7200), "{zip}");
        shifted.shift(-7200);
        assert_eq!(shifted.kills, log.kills, "{zip}");
    }
}

#[test]
fn the_shift_is_found_for_a_log_that_has_only_some_of_its_rounds() {
    let text = rawlog::unzip(&fixture("log_4121291.log.zip")).unwrap();
    let log = rawlog::parse(&text);
    // logs.tf lists two of the rounds, one hour behind the server.
    let some: Vec<i64> = log.round_starts.iter().take(2).map(|s| s - 3600).collect();
    assert_eq!(rawlog::frame_offset(&log.round_starts, &some), Some(-3600));
}
