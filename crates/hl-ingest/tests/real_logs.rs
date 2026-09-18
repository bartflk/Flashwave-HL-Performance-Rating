//! Normalization and classification against real logs.tf logs from the
//! account, one per era and edge case. Synthetic JSON would only test what I
//! already expected; these test what logs.tf actually sends.

use hl_core::matchdata::{Format, Team};
use hl_core::{SteamId, TfClass};
use hl_db::Db;
use hl_ingest::classify::classify;
use hl_ingest::normalize::normalize;
use serde_json::Value;

const ME: u32 = 139_131_191;

fn load(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect(&path)).expect("fixture is JSON")
}

/// 2014: players are keyed by SteamID2 (`STEAM_0:1:...`), not SteamID3, and
/// airshots were not recorded yet.
#[test]
fn old_log_keyed_by_steamid2_parses() {
    let log = normalize(295610, &load("hl_2014_steamid2_295610.json")).unwrap();
    assert_eq!(log.players.len(), 18, "every SteamID2 key must parse");
    assert!(!log.flags.airshots, "2014 logs predate airshot tracking");
    assert!(log.flags.hs);
    assert_eq!(classify(&log), Format::Highlander);
}

/// A 19-player Highlander log: a sub swapped in, so one team lists ten
/// players. Headcount says "not Highlander"; class coverage says it is.
#[test]
fn highlander_with_a_mid_match_sub_is_still_highlander() {
    let log = normalize(4114301, &load("hl_sub_19p_4114301.json")).unwrap();
    assert_eq!(log.players.len(), 19);
    let blue = log.players.iter().filter(|p| p.team == Team::Blue).count();
    let red = log.players.iter().filter(|p| p.team == Team::Red).count();
    assert_eq!((blue.max(red), blue.min(red)), (10, 9));
    assert_eq!(classify(&log), Format::Highlander);
}

/// The owner's line in a real Sniper game, checked against the numbers read
/// by hand from the raw JSON.
#[test]
fn owners_sniper_line_matches_the_raw_log() {
    let log = normalize(4114301, &load("hl_sub_19p_4114301.json")).unwrap();
    let me = log
        .players
        .iter()
        .find(|p| p.id == SteamId::from_account_id(ME))
        .expect("owner is in this log");

    assert_eq!(me.main_class(), Some(TfClass::Sniper));
    assert_eq!(me.total_time(), 910);
    assert_eq!((me.stats.kills, me.stats.deaths), (7, 5));
    assert_eq!(me.stats.dmg, 2617);
    assert_eq!((me.stats.headshots, me.stats.headshots_hit), (6, 7));

    // classkills {"pyro":3,"sniper":4}; classdeaths {"spy":1,"heavyweapons":2,
    // "sniper":1,"engineer":1}. Heavy arrives as logs.tf's "heavyweapons".
    let vs = |c: TfClass| me.vs.iter().find(|v| v.other_class == c);
    let sniper = vs(TfClass::Sniper).unwrap();
    assert_eq!((sniper.kills, sniper.deaths), (4, 1), "the sniper duel: 4-1");
    assert_eq!(vs(TfClass::Pyro).unwrap().kills, 3);
    assert_eq!(vs(TfClass::Heavy).unwrap().deaths, 2);
    assert!(vs(TfClass::Medic).is_none(), "no medic picks this game");
}

/// Round events carry the one timestamped kill logs.tf records: medic deaths,
/// with the killer attached.
#[test]
fn medic_deaths_carry_their_killer() {
    let log = normalize(3445078, &load("hl_2023_3445078.json")).unwrap();
    assert_eq!(log.rounds.len(), 5);
    let medic_deaths: Vec<_> = log
        .rounds
        .iter()
        .flat_map(|r| &r.events)
        .filter(|e| e.kind == "medic_death")
        .collect();
    assert!(!medic_deaths.is_empty());
    assert!(medic_deaths.iter().all(|e| e.player.is_some() && e.killer.is_some()));
    assert_eq!(classify(&log), Format::Highlander);
}

/// logs.tf stamps round events in seconds since the *log* started, not the
/// round. In a single-round log the two coincide, which hid this. In this
/// six-round official every event after round 1 was offset by the round's
/// start. Normalized `at_s` must be seconds into the round.
#[test]
fn round_events_are_relative_to_their_round() {
    let log = normalize(4109131, &load("hl_combined_6rounds_4109131.json")).unwrap();
    assert_eq!(log.rounds.len(), 6);
    for r in &log.rounds {
        let len = r.length_s.expect("every round has a length");
        for e in &r.events {
            assert!(
                (0..=len).contains(&e.at_s),
                "round {}: {} at {}s is outside 0..={len}s",
                r.round_num, e.kind, e.at_s
            );
        }
        // The round ends with its win, exactly at the round's length.
        if let Some(win) = r.events.iter().find(|e| e.kind == "round_win") {
            assert_eq!(win.at_s, len, "round {}", r.round_num);
        }
    }
}

/// Stopwatch swaps team colours between halves. The log records each round in
/// that round's colours, so a raw `winner: "Blue"` can mean either team.
/// Normalized rounds must speak in the stable teams (a player's overall
/// team), with the swap recorded separately.
///
/// Worked out by hand from the raw log, via each round's per-player colours:
/// the owner's team (overall Blue) won R1, R2 and R6 and lost R3, R4 and R5.
#[test]
fn stopwatch_colour_swaps_map_back_to_stable_teams() {
    let log = normalize(4109131, &load("hl_combined_6rounds_4109131.json")).unwrap();
    let me = log.players.iter().find(|p| p.id == SteamId::from_account_id(ME)).unwrap();
    assert_eq!(me.team, Team::Blue);

    let outcomes: Vec<bool> = log.rounds.iter().map(|r| r.winner == Some(me.team)).collect();
    assert_eq!(outcomes, [true, true, false, false, false, true]);

    let swapped: Vec<bool> = log.rounds.iter().map(|r| r.colours_swapped).collect();
    assert_eq!(swapped, [false, true, true, false, true, false]);

    // Every event names the medic's stable team, whatever colour they wore.
    let yanki = log.players.iter().find(|p| p.name.as_deref() == Some("Yanki")).unwrap();
    for e in log.rounds.iter().flat_map(|r| &r.events) {
        if e.player == Some(yanki.id) {
            assert_eq!(e.team, Some(yanki.team), "{} at {}s", e.kind, e.at_s);
        }
    }
}

#[test]
fn sixes_is_classified_as_sixes() {
    let log = normalize(4115969, &load("sixes_4115969.json")).unwrap();
    assert_eq!(classify(&log), Format::Sixes);
}

/// Writing the same log twice must leave exactly the same rows — that is what
/// makes `reprocess` safe to run at any time.
#[tokio::test]
async fn writing_a_match_is_idempotent() {
    let db = Db::connect_in_memory().await.unwrap();
    let log = normalize(4114301, &load("hl_sub_19p_4114301.json")).unwrap();

    let counts = |db: &Db| {
        let pool = db.pool().clone();
        async move {
            let mut out = Vec::new();
            for t in ["match", "match_player", "match_player_class", "match_class_vs", "match_round", "match_event"] {
                let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {t}"))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
                out.push((t, n));
            }
            out
        }
    };

    db.write_match(&log).await.unwrap();
    let first = counts(&db).await;
    db.write_match(&log).await.unwrap();
    let second = counts(&db).await;

    assert_eq!(first, second);
    assert_eq!(first[1], ("match_player", 19));
}
