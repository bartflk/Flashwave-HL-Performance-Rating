//! Invariants of everything computed beside the rating: round impact, the win
//! table behind it, and the per-class rows `log_stat` keeps. Real logs where
//! they exist (the two raw server logs in the fixtures), built input where an
//! edge case needs one.

use hl_ingest::fights::analyse;
use hl_ingest::rawlog::{parse, unzip, RawLog};
use hl_ingest::round_impact::{impacts, tally, Tally, WinTable};
use hl_ingest::state::GameState;

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).expect(&path)
}

fn raw_log(id: i64) -> RawLog {
    parse(&unzip(&fixture(&format!("log_{id}.log.zip"))).unwrap())
}

// ---- the win table --------------------------------------------------------

#[test]
fn the_shipped_table_is_a_probability_that_grows_with_the_numbers() {
    let t = WinTable::shipped();
    for adv in 0..3 {
        for own in 0..=9 {
            for enemy in 0..=9 {
                let p = t.p(adv, own, enemy);
                assert!(p > 0.0 && p < 1.0, "[{adv}][{own}][{enemy}] = {p}");
            }
        }
    }
    // A side with one more alive, everything else the same, wins more often.
    // Not asserted for the corners: with three or fewer alive a side is a
    // round nearly decided, there is little data and the cells are noisy
    // (the measured adv 0 table has 2v1 below 1v1). The middle is the game.
    for adv in 0..3 {
        for own in 4..9 {
            for enemy in 4..9 {
                assert!(t.p(adv, own + 1, enemy) >= t.p(adv, own, enemy) - 0.02, "adv {adv}: {own}v{enemy} to {}v{enemy}", own + 1);
                assert!(t.p(adv, own, enemy) >= t.p(adv, own, enemy + 1) - 0.02, "adv {adv}: {own}v{enemy} to {own}v{}", enemy + 1);
            }
        }
    }
    // Asking past nine is asking about nine (a log with a sub or a double count).
    assert_eq!(t.p(1, 40, 3), t.p(1, 9, 3));
    assert_eq!(t.p(1, 3, 40), t.p(1, 3, 9));
}

#[test]
fn a_table_measured_from_nothing_is_the_prior_and_well_formed() {
    let t = WinTable::from_tally(&Tally::default());
    assert_eq!(t.kills, 0);
    assert_eq!((t.p.len(), t.p[0].len(), t.p[0][0].len()), (3, 10, 10));
    for adv in 0..3 {
        for own in 0..=9 {
            for enemy in 0..=9 {
                let p = t.p(adv, own, enemy);
                assert!(p.is_finite() && p > 0.0 && p < 1.0);
                // Mirrors agree, with nothing to tell them apart but the prior.
                assert!((p + t.p(2 - adv, enemy, own) - 1.0).abs() < 1e-9, "[{adv}][{own}][{enemy}]");
            }
        }
    }
    assert!((t.p(1, 5, 5) - 0.5).abs() < 1e-9, "even numbers, nobody's advantage: a coin flip");
}

#[test]
fn a_cell_seen_once_cannot_swing_the_table() {
    // One 7v6 kill that the killer's side lost.
    let mut once = Tally::default();
    once.cells[1][7][6] = (0, 1);
    let t = WinTable::from_tally(&once);
    let base = WinTable::from_tally(&Tally::default());
    assert!(t.p(1, 7, 6) < base.p(1, 7, 6), "it moves the cell the right way");
    assert!(base.p(1, 7, 6) - t.p(1, 7, 6) < 0.05, "but by a few hundredths, not to zero");
    // The mirror cell moves the other way, by the same.
    assert!((t.p(1, 6, 7) - base.p(1, 6, 7) - (base.p(1, 7, 6) - t.p(1, 7, 6))).abs() < 1e-9);
}

#[test]
fn counting_a_log_with_no_kills_adds_nothing() {
    let raw = parse("");
    let gs = GameState::build(&raw);
    let mut t = Tally::default();
    tally(&raw, &gs, &|_| true, &mut t);
    assert_eq!(t.kills, 0);
    assert!(impacts(&raw, &gs, &|_| true, false, WinTable::shipped()).is_empty());
}

// ---- impact on real logs ---------------------------------------------------

fn real_impacts(id: i64, attack_defend: bool) -> (RawLog, std::collections::HashMap<u32, hl_ingest::round_impact::PlayerImpact>) {
    let raw = raw_log(id);
    let gs = GameState::build(&raw);
    let f = analyse(&raw, &gs);
    let m = impacts(&raw, &gs, &|i| f.tags.get(i).is_some_and(Option::is_some), attack_defend, WinTable::shipped());
    (raw, m)
}

/// A kill moves a round's chances from one side to the other, so what the
/// killer and assister gain the victim loses, and the whole lobby sums to 0.
#[test]
fn impact_over_a_whole_log_sums_to_zero() {
    for id in [4121291, 513611] {
        let (_, m) = real_impacts(id, false);
        assert!(m.len() >= 12, "{id}: {} players", m.len());
        let total: f64 = m.values().map(|p| p.impact).sum();
        assert!(total.abs() < 1e-6, "{id}: impact sums to {total}");
        assert!(m.values().all(|p| p.impact.is_finite()));
        // The best of a real lobby is above zero and the worst below it.
        assert!(m.values().any(|p| p.impact > 0.5) && m.values().any(|p| p.impact < -0.5));
    }
}

#[test]
fn on_attack_defend_the_two_sides_make_up_the_whole() {
    for id in [4121291, 513611] {
        let (_, m) = real_impacts(id, true);
        for (a, p) in &m {
            assert!((p.impact_attack + p.impact_defence - p.impact).abs() < 1e-9, "{id} {a}: {} + {} vs {}", p.impact_attack, p.impact_defence, p.impact);
            assert!(p.secs_attack >= 0 && p.secs_defence >= 0);
        }
        let kills: u32 = m.values().map(|p| p.kills_attack + p.kills_defence).sum();
        let deaths: u32 = m.values().map(|p| p.deaths_attack + p.deaths_defence).sum();
        assert_eq!(kills, deaths, "{id}: every counted kill is a death on the other side");
        // And not on a symmetric map: the split stays empty.
        let (_, sym) = real_impacts(id, false);
        assert!(sym.values().all(|p| p.impact_attack == 0.0 && p.impact_defence == 0.0 && p.kills_attack == 0 && p.secs_attack == 0));
    }
}

#[test]
fn bursts_never_outnumber_the_kills_that_make_them() {
    for id in [4121291, 513611] {
        let (raw, m) = real_impacts(id, false);
        let counted = raw.kills.iter().filter(|k| k.counts()).count() as u32;
        let (m2, m3): (u32, u32) = m.values().fold((0, 0), |(a, b), p| (a + p.multi2, b + p.multi3));
        assert!(2 * m2 + 3 * m3 <= counted, "{id}: {m2} doubles and {m3} triples out of {counted} kills");
        assert!(m3 + m2 > 0, "a real match has a double kill in it");
    }
}

#[test]
fn a_kill_that_is_not_counted_gives_nobody_any_impact() {
    let raw = raw_log(4121291);
    let gs = GameState::build(&raw);
    let m = impacts(&raw, &gs, &|_| false, false, WinTable::shipped());
    assert!(m.is_empty(), "no counted kill, no impact, and nobody is invented with 0");
}

// ---- log_stat rows --------------------------------------------------------

fn json(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap()
}

#[test]
fn the_rows_of_a_log_without_a_server_log_have_no_server_numbers() {
    for (name, id) in [("hl_sub_19p_4114301.json", 4114301), ("hl_2023_3445078.json", 3445078), ("hl_combined_6rounds_4109131.json", 4109131)] {
        let (stats, weapons) = hl_ingest::logstat::derive(id, &json(name), None).unwrap();
        assert!(!stats.is_empty(), "{name}");
        let _ = weapons;
        for s in &stats {
            assert_eq!(s.log_id, id);
            assert!(s.time_s > 0 && s.kills >= 0 && s.deaths >= 0 && s.dmg >= 0, "{name}: {s:?}");
            assert!(s.impact.is_none() && s.opening_kills.is_none() && s.sentry_kills.is_none(), "no server log, so these are missing, not zero");
        }
        // Exactly one main class per player, and it is the one played longest.
        let mut by: std::collections::HashMap<i64, Vec<&hl_db::LogStat>> = Default::default();
        for s in &stats {
            by.entry(s.account_id).or_default().push(s);
        }
        for (a, rows) in by {
            assert_eq!(rows.iter().filter(|r| r.main).count(), 1, "{name}: {a} has one main class");
            let longest = rows.iter().map(|r| r.time_s).max().unwrap();
            assert_eq!(rows.iter().find(|r| r.main).unwrap().time_s, longest, "{name}: {a}");
            let mut classes: Vec<&str> = rows.iter().map(|r| r.class.as_str()).collect();
            classes.sort();
            classes.dedup();
            assert_eq!(classes.len(), rows.len(), "{name}: {a} has a row per class, not two for one");
        }
    }
}

#[test]
fn a_log_that_is_not_json_is_an_error_not_a_panic() {
    assert!(hl_ingest::logstat::derive(1, "", None).is_err());
    assert!(hl_ingest::logstat::derive(1, "{\"players\": 3}", None).is_err() || hl_ingest::logstat::derive(1, "{\"players\": 3}", None).unwrap().0.is_empty());
    assert!(hl_ingest::logstat::derive(1, "[]", None).is_err() || hl_ingest::logstat::derive(1, "[]", None).unwrap().0.is_empty());
}
