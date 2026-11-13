//! The pure parts of turning raw kills into rows and values.

use super::*;

fn stored(killer: u32, victim: u32, live: bool, custom: Option<&str>, assister: Option<u32>) -> StoredKill {
    StoredKill {
        at_raw: 0,
        live,
        killer,
        killer_class: Some("scout".into()),
        killer_team: Some("Red".into()),
        victim,
        victim_team: Some("Blue".into()),
        victim_class: Some("heavyweapons".into()),
        weapon: "scattergun".into(),
        custom: custom.map(str::to_string),
        assister,
    }
}

#[test]
fn a_feign_death_is_no_kill_but_its_assist_counts() {
    let c = ctx(&stored(1, 2, true, Some("feign_death"), Some(3)), None);
    assert!(!c.counts);
    assert!(c.assist_counts);
    assert_eq!(c.victim_class, Some(TfClass::Heavy), "logs.tf's heavyweapons reads as Heavy");
    assert_eq!(c.victim_team, Some(Team::Blue));
    // Outside a round neither counts.
    let c = ctx(&stored(1, 2, false, None, Some(3)), Some((1, -1)));
    assert!(!c.counts && !c.assist_counts);
    assert_eq!(c.situation, Some((1, -1)));
    // No assister, no assist.
    assert!(!ctx(&stored(1, 2, true, None, None), None).assist_counts);
    // A class or team the parser does not know is unknown, not a panic.
    let mut odd = stored(1, 2, true, None, None);
    odd.victim_class = Some("civilian".into());
    odd.victim_team = Some("Spectator".into());
    let c = ctx(&odd, None);
    assert_eq!((c.victim_class, c.victim_team), (None, None));
}

#[test]
fn the_map_of_a_moment_is_the_round_holding_it() {
    let w = |start, length, map: Option<&str>| RoundWindow { start, length, map: map.map(str::to_string) };
    let windows = [w(100, 50, Some("pl_upward_f12")), w(150, 50, Some("koth_product_final")), w(300, 10, None)];
    assert_eq!(map_at(&windows, 99), None);
    assert_eq!(map_at(&windows, 100), Some("pl_upward_f12"));
    assert_eq!(map_at(&windows, 149), Some("pl_upward_f12"));
    // The shared second belongs to the earlier round.
    assert_eq!(map_at(&windows, 150), Some("pl_upward_f12"));
    assert_eq!(map_at(&windows, 151), Some("koth_product_final"));
    assert_eq!(map_at(&windows, 200), Some("koth_product_final"));
    assert_eq!(map_at(&windows, 201), None);
    // A round whose map is not known yet has none.
    assert_eq!(map_at(&windows, 305), None);
    assert_eq!(map_at(&[], 1), None);
}

#[test]
fn no_kills_no_impacts() {
    let w = Weights::default_weights();
    assert!(impacts_for(&[], None, &[], None, &w).is_empty());
}

#[test]
fn round_and_fight_numbers_make_an_entry_for_a_player_with_no_kills() {
    let mut m: HashMap<u32, Impact> = HashMap::new();
    attach_round(&mut m, &HashMap::from([(7, 1.5)]));
    assert_eq!(m[&7].round, Some(1.5));
    attach_fights(&mut m, &[(7, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]), (8, [0; 14])]);
    let f = m[&7].fights.unwrap();
    assert_eq!((f.opening_kills, f.opening_deaths, f.kills, f.caps_spawn_delay), (1, 2, 3, 14));
    assert!(m[&8].fights.is_some() && m[&8].round.is_none());
    // Nothing at all adds nothing.
    attach_round(&mut m, &HashMap::new());
    attach_fights(&mut m, &[]);
    assert_eq!(m.len(), 2);
}

#[test]
fn stored_kills_keep_the_order_and_the_classes_as_text() {
    let log = rawlog::parse(
        r#"L 03/01/2026 - 20:00:00: "a<4><[U:1:11]><Red>" spawned as "Scout"
L 03/01/2026 - 20:00:00: "b<5><[U:1:12]><Blue>" spawned as "Heavy"
L 03/01/2026 - 20:00:01: World triggered "Round_Start"
L 03/01/2026 - 20:00:05: "a<4><[U:1:11]><Red>" killed "b<5><[U:1:12]><Blue>" with "scattergun" (attacker_position "1 2 3") (victim_position "4 5 6")
L 03/01/2026 - 20:00:06: "b<5><[U:1:12]><Blue>" killed "a<4><[U:1:11]><Red>" with "minigun" (customkill "feign_death") (attacker_position "1 2 3") (victim_position "4 5 6")
"#,
    );
    let k = stored_kills(&log);
    assert_eq!(k.len(), 2);
    assert_eq!((k[0].killer, k[0].victim), (11, 12));
    assert_eq!(k[0].killer_class.as_deref(), Some("scout"));
    assert_eq!(k[0].victim_class.as_deref(), Some("heavy"));
    assert_eq!(k[1].custom.as_deref(), Some("feign_death"));
    // The same kills go through the row builder without losing a field.
    let (rows, chat) = rows(&log);
    assert_eq!(rows.len(), 2);
    assert!(chat.is_empty());
    assert_eq!((rows[0].killer_team, rows[0].victim_team), (Some("Red"), Some("Blue")));
    assert_eq!(rows[0].round_num, 1);
    assert!(rows[0].live);
}
