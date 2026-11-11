//! The stored timeline: every kind of change and sample round-trips, odd
//! shapes (empty, no samples, restarts) decode, and damaged blobs are errors.

use hl_demos::timeline::*;

fn person(i: u32) -> Person {
    Person { steamid: format!("[U:1:{i}]"), name: format!("player \"{i}\" \u{540d}\u{524d}\n\ttab") }
}

fn every_field(bits: [u8; 20]) -> Vec<Field> {
    vec![
        Field::Health(0),
        Field::Health(u16::MAX),
        Field::MaxHealth(300),
        Field::Class(9),
        Field::Team(3),
        Field::Alive(true),
        Field::Alive(false),
        Field::Here(true),
        Field::Here(false),
        Field::Conds(bits),
        Field::Charge(100),
        Field::Medigun(2),
        Field::HealTarget(None),
        Field::HealTarget(Some(0)),
        Field::HealTarget(Some(u16::MAX)),
        Field::Cloak(37),
        Field::DisguiseClass(8),
        Field::DisguiseTeam(2),
        Field::Ping(u16::MAX),
        Field::Ping(0),
    ]
}

fn tl(people: usize) -> Timeline {
    Timeline {
        version: TIMELINE_VERSION,
        tick_rate: 66.67,
        stride: 1,
        seams: vec![(0, 0)],
        people: (0..people as u32).map(person).collect(),
        tracks: vec![Track::default(); people],
        ..Timeline::default()
    }
}

fn back(t: &Timeline) -> Timeline {
    Timeline::decode(&t.encode().unwrap()).unwrap()
}

#[test]
fn an_empty_timeline_round_trips() {
    let t = tl(0);
    let b = back(&t);
    assert!(b.people.is_empty() && b.tracks.is_empty() && b.events.is_empty() && b.objects.is_empty());
    assert_eq!(b.end(), 0);
    assert_eq!(b.tick_rate, 66.67);
    assert_eq!(b.version, TIMELINE_VERSION);
}

#[test]
fn people_without_any_record_round_trip() {
    let b = back(&tl(3));
    assert_eq!(b.people.len(), 3);
    assert!(b.tracks.iter().all(|t| t.samples.is_empty() && t.changes.is_empty()));
    assert_eq!(b.people[1].name, person(1).name, "quotes, newlines and tabs in a name survive");
}

#[test]
fn every_kind_of_change_round_trips_in_order_with_equal_times() {
    let mut bits = [0u8; 20];
    bits[0] = 0xff;
    bits[19] = 0x81;
    let mut t = tl(2);
    for (i, f) in every_field(bits).into_iter().enumerate() {
        // Several at one tick, then a gap, then a far one.
        let at = match i % 3 {
            0 => 0,
            1 => 7,
            _ => 1_000_000 + i as u32,
        };
        let floor = t.tracks[1].changes.last().map_or(0, |c| c.t);
        t.tracks[1].changes.push(Change { t: at.max(floor), field: f });
    }
    let b = back(&t);
    assert_eq!(b.tracks[1].changes, t.tracks[1].changes);
    assert!(b.tracks[0].changes.is_empty());
}

#[test]
fn samples_round_trip_across_the_wrap_of_the_yaw_and_the_extremes_of_the_map() {
    let mut t = tl(1);
    let yaws = [0.0f32, 90.0, 179.99, 180.0, -179.99, -180.0, -0.001, 359.9, -359.9, 720.5, 0.003];
    let pitches = [0.0f32, 89.0, -89.0, 89.99, -89.99, 0.005, 12.345];
    for i in 0..60u32 {
        t.tracks[0].samples.push(Sample {
            t: i * 3,
            pos: [(i as f32 * 977.0) - 15_000.0, 32_000.0 - i as f32 * 1200.5, if i % 2 == 0 { -8192.0 } else { 16_384.0 }],
            yaw: yaws[i as usize % yaws.len()],
            pitch: pitches[i as usize % pitches.len()],
        });
    }
    let b = back(&t);
    assert_eq!(b.tracks[0].samples.len(), 60);
    for (a, e) in b.tracks[0].samples.iter().zip(&t.tracks[0].samples) {
        assert_eq!(a.t, e.t);
        for k in 0..3 {
            assert_eq!(a.pos[k], e.pos[k].round(), "positions are kept to the unit");
        }
        // Angles compared as directions: 180 and -180 are the same way.
        let d = (a.yaw - e.yaw).rem_euclid(360.0);
        let d = d.min(360.0 - d);
        assert!(d < 0.01, "yaw {} came back as {}", e.yaw, a.yaw);
        assert!((a.pitch - e.pitch).abs() < 0.006, "pitch {} came back as {}", e.pitch, a.pitch);
        assert!(a.yaw > -180.01 && a.yaw <= 180.0, "yaw comes back in -180..180, got {}", a.yaw);
    }
}

#[test]
fn samples_at_the_same_tick_and_at_tick_zero_survive() {
    let mut t = tl(1);
    let s = |tick: u32| Sample { t: tick, pos: [1.0, 2.0, 3.0], yaw: 1.0, pitch: 1.0 };
    t.tracks[0].samples = vec![s(0), s(0), s(5), s(5), s(u32::MAX / 2)];
    let b = back(&t);
    assert_eq!(b.tracks[0].samples.iter().map(|x| x.t).collect::<Vec<_>>(), vec![0, 0, 5, 5, u32::MAX / 2]);
}

#[test]
fn events_objects_seams_and_entities_round_trip() {
    let mut t = tl(2);
    t.seams = vec![(0, 100), (500, 3), (900, 0)];
    t.user_ids = vec![(3, 0), (65_535, 1)];
    t.entities = vec![(2, 0), (2, 1), (1023, 1)];
    t.events = vec![
        (0, GameEvent::TeamPlaySetupFinished(tf_demo_parser::demo::gameevent_gen::TeamPlaySetupFinishedEvent {})),
        (70_000, GameEvent::TeamPlaySetupFinished(tf_demo_parser::demo::gameevent_gen::TeamPlaySetupFinishedEvent {})),
    ];
    let b = back(&t);
    assert_eq!(b.seams, t.seams);
    assert_eq!(b.user_ids, t.user_ids);
    assert_eq!(b.entities, t.entities);
    assert_eq!(b.events.iter().map(|e| e.0).collect::<Vec<_>>(), vec![0, 70_000]);
    assert_eq!(b.slot_of_user(65_535), Some(1));
    assert_eq!(b.slots_of_entity(2).collect::<Vec<_>>(), vec![0, 1]);
    assert_eq!(b.slot_of("[U:1:1]"), Some(1));
    assert_eq!(b.slot_of("nobody"), None);
}

#[test]
fn a_timeline_stored_before_entities_existed_still_reads() {
    // Version 1 heads had no `entities`, and no Ping changes at all.
    let mut t = tl(1);
    t.version = 1;
    t.tracks[0].changes = vec![Change { t: 1, field: Field::Health(100) }];
    let mut stored = t.encode().unwrap();
    assert!(!stored.head.contains("entities"), "an empty list is not written: {}", stored.head);
    stored.head = stored.head.replace(",\"entities\":[]", "");
    let b = Timeline::decode(&stored).unwrap();
    assert_eq!(b.version, 1);
    assert!(b.entities.is_empty());
    assert_eq!(b.now(0, 5).ping, 0, "no ping recorded reads as 0");
}

#[test]
fn damaged_blobs_are_errors_not_panics() {
    let mut t = tl(2);
    t.tracks[0].samples = (0..20).map(|i| Sample { t: i, pos: [i as f32; 3], yaw: 1.0, pitch: 2.0 }).collect();
    t.tracks[0].changes = vec![Change { t: 1, field: Field::Health(100) }, Change { t: 2, field: Field::Class(1) }];
    let good = t.encode().unwrap();
    assert!(Timeline::decode(&good).is_ok());

    let mut s = good.clone();
    s.head = "{".into();
    assert!(Timeline::decode(&s).is_err(), "bad head");
    let mut s = good.clone();
    s.samples = vec![1, 2, 3];
    assert!(Timeline::decode(&s).is_err(), "not deflate");
    let mut s = good.clone();
    s.changes.truncate(s.changes.len() / 2);
    assert!(Timeline::decode(&s).is_err(), "truncated changes");
    let mut s = good.clone();
    s.samples.truncate(s.samples.len() / 2);
    assert!(Timeline::decode(&s).is_err(), "truncated samples");
    // Tracks for a different number of people than the head names.
    let other = tl(3).encode().unwrap();
    let mut s = good.clone();
    s.samples = other.samples;
    assert!(Timeline::decode(&s).is_err(), "people and tracks disagree");
    let mut s = good;
    s.objects = Vec::new();
    assert!(Timeline::decode(&s).is_err(), "empty is not even a deflate stream");
}

#[test]
fn ticks_and_t_with_a_restart_in_the_demo() {
    let mut t = tl(0);
    // Stretch 0: t 0..500 is demo ticks 100..600. Stretch 1: t 500.. is ticks 3..
    t.seams = vec![(0, 100), (500, 3)];
    assert_eq!(t.tick_of(0), 100);
    assert_eq!(t.tick_of(499), 599);
    assert_eq!(t.tick_of(500), 3);
    assert_eq!(t.tick_of(510), 13);
    // Demo tick 3 only exists in the second stretch, 100 only in the first.
    assert_eq!(t.t_of(3), Some(500));
    assert_eq!(t.t_of(100), Some(0));
    // Tick 50 is before the first stretch's first tick and inside the second's range? No: 50 >= 3.
    assert_eq!(t.t_of(50), Some(547));
    // Tick 2 is in neither stretch.
    assert_eq!(t.t_of(2), None);
    // Both stretches hold tick 300? first: t=200; second: t=797. The first wins, unless asked.
    assert_eq!(t.t_of(300), Some(200));
    assert_eq!(t.t_in(1, 300), Some(797));
    // The first stretch ends where the second begins: tick 600 is not in it.
    assert_eq!(t.t_in(0, 600), None);
    assert_eq!(t.t_in(5, 3), None);
    // Without seams, a timeline reads its own clock as the demo's.
    let bare = Timeline::default();
    assert_eq!(bare.tick_of(77), 77);
    assert_eq!(bare.t_of(77), None);
}

#[test]
fn nearest_sample_prefers_the_closest_and_respects_the_window() {
    let mut t = tl(1);
    let s = |tick: u32, x: f32| Sample { t: tick, pos: [x, 0.0, 0.0], yaw: 0.0, pitch: 0.0 };
    t.tracks[0].samples = vec![s(10, 1.0), s(20, 2.0), s(40, 4.0)];
    assert_eq!(t.sample_near(0, 10, 0).unwrap().pos[0], 1.0);
    assert_eq!(t.sample_near(0, 14, 5).unwrap().pos[0], 1.0);
    assert_eq!(t.sample_near(0, 16, 5).unwrap().pos[0], 2.0);
    assert!(t.sample_near(0, 30, 5).is_none());
    assert!(t.sample_near(0, 0, 9).is_none());
    assert_eq!(t.sample_near(0, 0, 10).unwrap().t, 10);
    assert_eq!(t.sample_near(0, 1000, 960).unwrap().t, 40);
    assert!(t.sample_near(5, 10, 100).is_none(), "no such slot");
}

#[test]
fn stretches_apply_every_change_of_a_moment_before_the_stretch_begins() {
    let mut t = tl(1);
    t.tracks[0].changes = vec![
        Change { t: 10, field: Field::Alive(true) },
        Change { t: 10, field: Field::Here(true) },
        Change { t: 10, field: Field::Health(125) },
        Change { t: 40, field: Field::Alive(false) },
        Change { t: 40, field: Field::Health(0) },
        Change { t: 60, field: Field::Alive(true) },
    ];
    // Something else recorded later extends the last stretch.
    t.tracks[0].samples = vec![Sample { t: 100, pos: [0.0; 3], yaw: 0.0, pitch: 0.0 }];
    assert_eq!(t.end(), 100);
    let s = t.stretches(0);
    assert_eq!(s.len(), 3);
    assert_eq!((s[0].0, s[0].1), (10, 40));
    assert!(s[0].2.live() && s[0].2.health == 125);
    assert!(!s[1].2.live() && s[1].2.health == 0);
    assert_eq!((s[2].0, s[2].1), (60, 100));
    assert_eq!(t.ticks_where(0, |_| true), 30 + 40, "only while alive and carried");
    // Alive again at 60 with the health the last change gave: 0 until a new one arrives.
    assert_eq!(t.ticks_where(0, |n| n.health == 0), 40);
    // A slot with nothing, or no such slot, has no stretches and no time.
    assert!(t.stretches(7).is_empty());
    assert_eq!(t.ticks_where(7, |_| true), 0);
    assert_eq!(t.now(0, 9), Now::default());
    assert_eq!(t.now(0, 10).health, 125);
    assert_eq!(t.now(7, 10), Now::default());
}

#[test]
fn seconds_never_divide_by_a_missing_rate() {
    let mut t = tl(0);
    t.tick_rate = 0.0;
    assert_eq!(t.seconds(66), 66.0);
    t.tick_rate = 66.0;
    assert_eq!(t.seconds(66), 1.0);
}
