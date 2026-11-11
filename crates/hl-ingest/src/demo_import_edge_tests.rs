//! Edge cases of importing a demo: its ids, its start time, the shift that
//! lines its kills up with a log, and the links people paste.

use super::*;

#[test]
fn an_imported_log_id_is_always_negative_and_ignores_case() {
    for name in ["", "a.dem", "match-20260823-1956-pl_upward_f12.dem", "名前☃.dem", &"x".repeat(5000)] {
        let id = log_id_for(name);
        assert!((-1_000_000_000..0).contains(&id), "{name:.20}: {id}");
        assert_eq!(id, log_id_for(&name.to_uppercase()), "{name:.20}");
    }
    assert_ne!(log_id_for("a.dem"), log_id_for("b.dem"));
}

#[test]
fn the_start_time_of_odd_names_falls_back_to_the_file() {
    // A bad date in the STV name falls through to the next rule, then to the file.
    assert_eq!(wall_start("match-20261340-2561-x.dem", 10_000, 600), 9_400);
    assert_eq!(wall_start("match-2026082-1956-x.dem", 10_000, 600), 9_400);
    assert_eq!(wall_start("match-20260823-195-x.dem", 10_000, 600), 9_400);
    assert_eq!(wall_start("match-x.dem", 10_000, 600), 9_400);
    // An impossible Demo Support stamp (month 13) likewise.
    assert_eq!(wall_start("x2026-13-17_21-00-24.dem", 10_000, 600), 9_400);
    // A leap day is a day.
    assert_eq!(wall_start("match-20240229-2359-x.dem", 0, 0), 1_709_251_140);
    // Upper-case prefix, and a name with more dashes than parts it needs.
    assert_eq!(wall_start("MATCH-20260823-1956-pl_upward-f12.dem", 0, 0), 1_787_514_960);
    // The file's own time less its length can go before 1970 for a bad mtime: no panic.
    assert_eq!(wall_start("x.dem", 0, 600), -600);
}

#[test]
fn the_best_shift_does_not_depend_on_hash_order() {
    // Two shifts with exactly the same evidence: the answer must be the same every time.
    let demo = [(10.0, 1, 2), (50.0, 3, 4)];
    let log = [(1010, 1, 2), (2050, 3, 4)];
    let first = best_shift(&demo, &log);
    for _ in 0..200 {
        assert_eq!(best_shift(&demo, &log), first);
    }
}

#[test]
fn a_stronger_shift_beats_a_weaker_one_and_neighbours_pool_their_votes() {
    // Three kills agree on 1000 (one of them a second off), one on 5000.
    let demo = [(10.0, 1, 2), (40.0, 3, 4), (95.0, 2, 1), (300.0, 7, 8)];
    let log = [(1010, 1, 2), (1041, 3, 4), (1095, 2, 1), (5300, 7, 8)];
    assert_eq!(best_shift(&demo, &log), Some((1000, 3)));
    // Nothing in common.
    assert_eq!(best_shift(&[(1.0, 1, 2)], &[(1, 9, 9)]), None);
    assert_eq!(best_shift(&[(1.0, 1, 2)], &[]), None);
}

#[test]
fn the_same_kill_pair_many_times_matches_each_log_kill_once() {
    // One player kills the same player five times, a minute apart.
    let demo: Vec<(f64, u32, u32)> = (0..5).map(|i| (10.0 + 60.0 * i as f64, 1, 2)).collect();
    let log: Vec<(i64, u32, u32)> = (0..5).map(|i| (1010 + 60 * i, 1, 2)).collect();
    let (shift, matched) = best_shift(&demo, &log).unwrap();
    assert_eq!(shift, 1000);
    assert_eq!(matched, 5);
}

#[test]
fn pasted_demos_tf_links() {
    assert_eq!(demostf_id("https://demos.tf/990239?x=1"), Some(990239));
    assert_eq!(demostf_id("https://demos.tf/990239#t=5"), Some(990239));
    assert_eq!(demostf_id("http://demos.tf/1"), Some(1));
    assert_eq!(demostf_id("0"), None);
    assert_eq!(demostf_id("-5"), None);
    assert_eq!(demostf_id(""), None);
    assert_eq!(demostf_id("https://demos.tf/"), None);
    assert_eq!(demostf_id("https://demos.tf"), None);
    assert_eq!(demostf_id("https://example.com/990239"), None);
    assert_eq!(demostf_id("99999999999999999999999"), None);
}

#[test]
fn map_fields_of_combined_logs_split_on_every_separator() {
    assert_eq!(map_names("a+b , c/d & e"), ["a", "b", "c", "d", "e"]);
    assert_eq!(map_names(" + , "), Vec::<String>::new());
    assert_eq!(map_names("  pl_upward_f12  "), ["pl_upward_f12"]);
}

#[test]
fn a_zipped_log_unzips_to_the_same_text() {
    for text in ["", "L 01/01/2026 - 00:00:00: x\n", "名前☃ \"quoted\"\r\nsecond"] {
        let zip = zip_text(text).unwrap();
        assert_eq!(crate::rawlog::unzip(&zip).unwrap(), text);
    }
}
