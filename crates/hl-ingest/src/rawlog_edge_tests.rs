//! Edge cases of the raw server log parser: names, midnight, disconnects,
//! assists a second late, the clock shift.

use super::*;

fn kills_of(text: &str) -> Vec<Kill> {
    parse(text).kills
}

const KILL: &str = r#""a<4><[U:1:11]><Red>" killed "b<5><[U:1:12]><Blue>" with "scattergun" (attacker_position "1 2 3") (victim_position "4 5 6")"#;

#[test]
fn unicode_and_quoted_names_keep_their_accounts() {
    let log = r#"L 03/01/2026 - 20:00:00: World triggered "Round_Start"
L 03/01/2026 - 20:00:05: "Zoë <3 ☃<4><[U:1:11]><Red>" killed "名前 "the" <b><7><[U:1:22]><Blue>" with "scattergun" (attacker_position "1 2 3") (victim_position "4 5 6")
L 03/01/2026 - 20:00:05: "a>"<5><[U:1:33]><Red>" committed suicide with "world" (attacker_position "0 0 0")
"#;
    let p = parse(log);
    assert_eq!(p.kills.len(), 1);
    assert_eq!(p.kills[0].killer.account, 11);
    assert_eq!(p.kills[0].victim.account, 22);
    assert_eq!(p.kills[0].weapon, "scattergun");
    assert!(p.events.iter().any(|e| matches!(&e.kind, EventKind::Suicide(a) if a.account == 33)));
}

#[test]
fn a_team_kill_and_a_suicide_are_told_apart() {
    let log = r#"L 03/01/2026 - 20:00:00: World triggered "Round_Start"
L 03/01/2026 - 20:00:05: "a<4><[U:1:11]><Red>" killed "b<5><[U:1:12]><Red>" with "rocketlauncher" (attacker_position "1 2 3") (victim_position "4 5 6")
L 03/01/2026 - 20:00:06: "a<4><[U:1:11]><Red>" committed suicide with "world" (attacker_position "1 2 3")
"#;
    let p = parse(log);
    // The team kill is in the kill list with both teams Red; the suicide is
    // an event, not a kill.
    assert_eq!(p.kills.len(), 1);
    assert_eq!(p.kills[0].killer.team, p.kills[0].victim.team);
    assert_eq!(p.events.iter().filter(|e| matches!(e.kind, EventKind::Suicide(_))).count(), 1);
}

#[test]
fn a_log_spanning_midnight_keeps_its_order() {
    let log = format!(
        "L 03/01/2026 - 23:59:58: World triggered \"Round_Start\"\nL 03/01/2026 - 23:59:59: {KILL}\nL 03/02/2026 - 00:00:01: {KILL}\nL 03/02/2026 - 00:00:02: World triggered \"Round_Win\" (winner \"Red\")\n"
    );
    let k = kills_of(&log);
    assert_eq!(k[1].at - k[0].at, 2);
    assert!(k.iter().all(|x| x.live && x.round == 1));
}

#[test]
fn dates_across_a_leap_day_and_a_year_end() {
    let t = |s: &str| split_time(&format!("L {s}: x")).unwrap().0;
    assert_eq!(t("03/01/2024 - 00:00:00") - t("02/28/2024 - 00:00:00"), 2 * 86_400);
    assert_eq!(t("03/01/2026 - 00:00:00") - t("02/28/2026 - 00:00:00"), 86_400);
    assert_eq!(t("01/01/2027 - 00:00:00") - t("12/31/2026 - 23:59:59"), 1);
}

#[test]
fn a_disconnect_in_a_round_is_an_event_and_the_kill_still_counts() {
    let log = format!(
        "L 03/01/2026 - 20:00:00: World triggered \"Round_Start\"\nL 03/01/2026 - 20:00:04: \"b<5><[U:1:12]><Blue>\" disconnected (reason \"Disconnect by user.\")\nL 03/01/2026 - 20:00:05: {KILL}\nL 03/01/2026 - 20:00:06: \"b<9><[U:1:12]><Blue>\" connected, address \"1.2.3.4:5\"\n"
    );
    let p = parse(&log);
    assert_eq!(p.events.iter().filter(|e| matches!(e.kind, EventKind::Left(12))).count(), 2);
    assert_eq!(p.kills.len(), 1);
}

#[test]
fn a_kill_after_the_game_is_over_is_not_live() {
    let log = format!(
        "L 03/01/2026 - 20:00:00: World triggered \"Round_Start\"\nL 03/01/2026 - 20:00:04: World triggered \"Round_Win\" (winner \"Red\")\nL 03/01/2026 - 20:00:05: World triggered \"Game_Over\" (reason \"Reached Win Limit\")\nL 03/01/2026 - 20:00:06: {KILL}\n"
    );
    let k = kills_of(&log);
    assert!(!k[0].live);
    assert_eq!(k[0].round, 1);
}

#[test]
fn lines_that_are_not_log_lines_are_skipped_without_a_panic() {
    // Short lines, a multi-byte character where the clock should be, a bad
    // date, a CRLF ending, a stray byte-order mark.
    let log = "L 03/01/2026 - 2\nL 03/01/2026 - 20:00:0\u{e9}: x\nL ab/cd/efgh - 20:00:00: x\r\nL 03/01/2026 - 20:00:00: World triggered \"Round_Start\"\r\n\u{feff}L 03/01/2026 - 20:00:01: x\n";
    let p = parse(log);
    assert_eq!(p.rounds, 1);
    assert_eq!(parse("").rounds, 0);
}

#[test]
fn an_assist_one_second_late_is_attached_but_two_is_not() {
    let assist = r#""c<6><[U:1:13]><Red>" triggered "kill assist" against "b<5><[U:1:12]><Blue>" (assister_position "0 0 0")"#;
    let log = format!(
        "L 03/01/2026 - 20:00:00: World triggered \"Round_Start\"\nL 03/01/2026 - 20:00:05: {KILL}\nL 03/01/2026 - 20:00:06: {assist}\nL 03/01/2026 - 20:00:20: {KILL}\nL 03/01/2026 - 20:00:22: {assist}\n"
    );
    let k = kills_of(&log);
    assert_eq!(k[0].assister, Some(13));
    assert_eq!(k[1].assister, None);
}

#[test]
fn the_latest_role_is_the_class_even_for_no_time() {
    let log = r#"L 03/01/2026 - 20:00:00: "a<4><[U:1:11]><Red>" spawned as "Soldier"
L 03/01/2026 - 20:00:00: "a<4><[U:1:11]><Red>" changed role to "medic"
L 03/01/2026 - 20:00:00: "b<5><[U:1:12]><Blue>" spawned as "Engineer"
L 03/01/2026 - 20:00:01: World triggered "Round_Start"
L 03/01/2026 - 20:00:05: "b<5><[U:1:12]><Blue>" killed "a<4><[U:1:11]><Red>" with "shotgun_primary" (attacker_position "1 2 3") (victim_position "4 5 6")
"#;
    let k = kills_of(log);
    assert_eq!(k[0].victim.class, Some(TfClass::Medic));
    assert_eq!(k[0].killer.class, Some(TfClass::Engineer));
}

#[test]
fn positions_round_and_survive_junk() {
    assert_eq!(position("1.5 -2.5 3.49"), Some([2, -3, 3]));
    assert_eq!(position("1 2"), None);
    assert_eq!(position("a b c"), None);
    assert_eq!(position(""), None);
}

#[test]
fn the_hit_cap_switches_on_the_documented_date() {
    let who = Actor { account: 1, team: None, class: None };
    let d = Damage { at: 0, live: true, attacker: who, victim: who, amount: 1986 };
    assert_eq!(d.counted(true), 450);
    assert_eq!(d.counted(false), 1986);
    assert_eq!(Damage { live: false, ..d }.counted(true), 0);
    assert!(hits_capped(None));
    assert!(hits_capped(Some(HIT_CAP_SINCE)));
    assert!(!hits_capped(Some(HIT_CAP_SINCE - 1)));
}

#[test]
fn frame_offset_edges() {
    assert_eq!(frame_offset(&[], &[1, 2]), None);
    assert_eq!(frame_offset(&[1, 2], &[]), None);
    // A log two hours ahead of the server's clock shifts forward.
    assert_eq!(frame_offset(&[1000], &[1000 + 7200]), Some(7200));
    // The majority hour wins over a single stray match.
    let raw = [10_000, 20_000, 30_000, 40_000];
    let logstf = [10_000 - 3600, 20_000 - 3600, 30_000 - 3600, 40_000 - 7200];
    assert_eq!(frame_offset(&raw, &logstf), Some(-3600));
}

#[test]
fn shifting_moves_every_time() {
    let mut p = parse(&format!(
        "L 03/01/2026 - 20:00:01: World triggered \"Round_Start\"\nL 03/01/2026 - 20:00:05: {KILL}\nL 03/01/2026 - 20:00:06: \"a<4><[U:1:11]><Red>\" triggered \"shot_fired\" (weapon \"scattergun\")\nL 03/01/2026 - 20:00:07: \"a<4><[U:1:11]><Red>\" say \"hi\"\nL 03/01/2026 - 20:00:08: \"a<4><[U:1:11]><Red>\" triggered \"damage\" against \"b<5><[U:1:12]><Blue>\" (damage \"50\")\n"
    ));
    let before = (p.kills[0].at, p.round_starts[0], p.chat[0].at, p.damage[0].at, p.shots[0].0, p.events[0].at);
    p.shift(-3600);
    let after = (p.kills[0].at, p.round_starts[0], p.chat[0].at, p.damage[0].at, p.shots[0].0, p.events[0].at);
    assert_eq!(after, (before.0 - 3600, before.1 - 3600, before.2 - 3600, before.3 - 3600, before.4 - 3600, before.5 - 3600));
}

#[test]
fn a_kill_before_any_round_belongs_to_round_zero_and_a_pause_changes_nothing() {
    let log = format!(
        "L 03/01/2026 - 19:59:00: {KILL}\nL 03/01/2026 - 20:00:00: World triggered \"Round_Start\"\nL 03/01/2026 - 20:00:10: World triggered \"Game_Paused\"\nL 03/01/2026 - 20:09:10: World triggered \"Game_Unpaused\"\nL 03/01/2026 - 20:09:11: {KILL}\n"
    );
    let k = kills_of(&log);
    assert_eq!((k[0].round, k[0].live), (0, false));
    assert_eq!((k[1].round, k[1].live), (1, true));
}
