//! Invariants of the rating model: what must hold for any input, not only
//! the numbers of one log. Edge cases first (no minutes, no deaths, a player
//! alone on a class, a pool where everyone is equal), then the direction of
//! every weighted stat, then the pools a rating is measured against.

use hl_core::matchdata::{ClassLine, LogFlags, NormalizedLog, PlayerLine, PlayerStats, Team, VsLine};
use hl_core::{SteamId, TfClass};
use hl_ingest::normalize::normalize;
use hl_rating::model::{extract, rate, Component, Performance, Scale, MIN_MAP_POOL, RATING_SPREAD};
use hl_rating::{Baseline, Weights};

fn w() -> Weights {
    Weights::default_weights()
}

fn line(class: TfClass, secs: i64, kills: i64, deaths: i64, dmg: i64) -> ClassLine {
    ClassLine { class, time_s: secs, kills, assists: 0, deaths, dmg, shots: 0, hits: 0 }
}

fn player(id: u32, classes: Vec<ClassLine>, stats: PlayerStats, vs: Vec<VsLine>) -> PlayerLine {
    PlayerLine { id: SteamId::from_account_id(id), name: None, team: Team::Red, stats, classes, vs }
}

fn flags() -> LogFlags {
    LogFlags { hs: true, bs: true, cp: true, ..Default::default() }
}

fn perf(class: TfClass, values: Vec<(Component, f64)>) -> Performance {
    Performance { account_id: 1, class, minutes: 20.0, values, map: None }
}

/// Every component a class's model weighs, with the pool 1..=n for each.
fn pool_of(class: TfClass, n: u32) -> Baseline {
    let perfs: Vec<Performance> = (1..=n)
        .map(|i| Performance {
            account_id: 1000 + i,
            class,
            minutes: 20.0,
            values: Component::ALL.iter().map(|c| (*c, f64::from(i))).collect(),
            map: None,
        })
        .collect();
    Baseline::build(&perfs, None)
}

// ---- finite for any input --------------------------------------------------

#[test]
fn a_player_with_no_class_time_is_not_rated() {
    let p = player(1, vec![line(TfClass::Soldier, 0, 5, 2, 900)], PlayerStats::default(), vec![]);
    assert!(extract(&p, &flags(), &w(), None, None).is_none(), "zero seconds on every class");
    let none = player(2, vec![], PlayerStats::default(), vec![]);
    assert!(extract(&none, &flags(), &w(), None, None).is_none(), "no class line at all");
}

#[test]
fn the_minimum_is_a_boundary_not_a_range() {
    let weights = w();
    let min = (weights.general.min_minutes * 60.0) as i64;
    let at = player(1, vec![line(TfClass::Soldier, min, 5, 2, 900)], PlayerStats::default(), vec![]);
    let under = player(2, vec![line(TfClass::Soldier, min - 1, 5, 2, 900)], PlayerStats::default(), vec![]);
    assert!(extract(&at, &flags(), &weights, None, None).is_some(), "exactly the minimum rates");
    assert!(extract(&under, &flags(), &weights, None, None).is_none(), "a second under does not");
}

#[test]
fn raw_values_are_finite_with_zero_deaths_zero_kills_and_huge_numbers() {
    let weights = w();
    for class in TfClass::ALL {
        for (kills, deaths, dmg) in [(0, 0, 0), (0, 9, 0), (30, 0, 12_000), (i64::from(i32::MAX), 0, i64::from(i32::MAX))] {
            let stats = PlayerStats { heal: i64::from(i32::MAX), ubers: 9, drops: 3, headshots: 7, backstabs: 4, cpc: 2, ..Default::default() };
            let p = player(1, vec![line(class, 900, kills, deaths, dmg)], stats, vec![]);
            let got = extract(&p, &flags(), &weights, None, None).expect("15 minutes is rateable");
            assert_eq!(got.class, class);
            for (c, v) in &got.values {
                assert!(v.is_finite(), "{} {} = {v} with kills {kills}, deaths {deaths}", class.as_str(), c.key());
            }
        }
    }
}

#[test]
fn headshot_share_needs_kills_and_a_log_that_counted_headshots() {
    let weights = w();
    let stats = PlayerStats { headshots: 3, ..Default::default() };
    // Only models that weigh it show it; the generic fallback does not, so
    // ask through a model that does.
    let with = weights.with_model(TfClass::Sniper, vec![(Component::HeadshotShare, 1.0)]);
    let at = |f: LogFlags, kills: i64| {
        let p = player(1, vec![line(TfClass::Sniper, 1200, kills, 1, 100)], stats.clone(), vec![]);
        extract(&p, &f, &with, None, None).unwrap().values.into_iter().next().map(|(_, v)| v)
    };
    assert_eq!(at(flags(), 6), Some(50.0));
    assert_eq!(at(flags(), 0), None, "no kills, no share: skipped, not 0 and not NaN");
    assert_eq!(at(LogFlags { hs: false, ..flags() }, 6), None, "a log that did not record headshots has none");
}

#[test]
fn a_log_without_points_does_not_score_caps() {
    let with = w().with_model(TfClass::Scout, vec![(Component::Caps, 1.0), (Component::Dpm, 1.0)]);
    let p = player(1, vec![line(TfClass::Scout, 1200, 9, 3, 4000)], PlayerStats { cpc: 4, ..Default::default() }, vec![]);
    let no_points = extract(&p, &LogFlags { cp: false, ..flags() }, &with, None, None).unwrap();
    assert!(no_points.values.iter().all(|(c, _)| *c != Component::Caps));
    let points = extract(&p, &flags(), &with, None, None).unwrap();
    assert!(points.values.iter().any(|(c, _)| *c == Component::Caps));
}

#[test]
fn deaths_are_read_under_the_name_the_maps_mode_gives_them() {
    let with = w().with_model(TfClass::Soldier, vec![(Component::Deaths, 1.0), (Component::DeathsAttackDefend, 1.0)]);
    let p = player(1, vec![line(TfClass::Soldier, 1200, 9, 6, 4000)], PlayerStats::default(), vec![]);
    let koth = extract(&p, &flags(), &with, None, None).unwrap();
    assert_eq!(koth.values.iter().map(|(c, _)| *c).collect::<Vec<_>>(), [Component::Deaths]);
    let pl = extract(&p, &LogFlags { attack_defend: true, ..flags() }, &with, None, None).unwrap();
    assert_eq!(pl.values.iter().map(|(c, _)| *c).collect::<Vec<_>>(), [Component::DeathsAttackDefend]);
    // The same count either way: 6 deaths in 20 minutes is 3 per 10.
    assert_eq!(koth.values[0].1, 3.0);
    assert_eq!(pl.values[0].1, 3.0);
}

#[test]
fn a_rating_is_finite_for_a_player_alone_on_a_class() {
    // The pool is one performance: the player's own, or somebody else's.
    let weights = w();
    for class in TfClass::ALL {
        let one = Baseline::build(&[perf(class, vec![(Component::Dpm, 100.0)])], None);
        let r = rate(&perf(class, vec![(Component::Dpm, 100.0)]), &one, &weights);
        if let Some(r) = r {
            assert!(r.score.is_finite() && (0.0..=100.0).contains(&r.score), "{} {}", class.as_str(), r.score);
        }
    }
}

#[test]
fn an_all_equal_pool_puts_everyone_in_the_middle_and_cannot_be_scaled() {
    let weights = w();
    let class = TfClass::Heavy;
    let equal: Vec<Performance> = (0..30).map(|i| Performance { account_id: i, ..perf(class, vec![(Component::Dpm, 250.0)]) }).collect();
    let b = Baseline::build(&equal, None);
    let r = rate(&perf(class, vec![(Component::Dpm, 250.0)]), &b, &weights).unwrap();
    assert_eq!(r.score, 50.0, "a tie with everyone is the middle");
    assert!(Scale::of(&[r.score; 30]).is_none(), "and a pool where everyone is equal has no spread to scale by");
}

#[test]
fn nothing_in_the_pool_means_no_rating_not_zero() {
    let r = rate(&perf(TfClass::Sniper, vec![(Component::ImpactKills, 3.0)]), &Baseline::default(), &w());
    assert!(r.is_none());
    // A class with no pool, next to a class that has one.
    let b = pool_of(TfClass::Scout, 20);
    assert!(rate(&perf(TfClass::Sniper, vec![(Component::ImpactKills, 3.0)]), &b, &w()).is_none());
    // A performance with no values at all.
    assert!(rate(&perf(TfClass::Scout, vec![]), &b, &w()).is_none());
}

#[test]
fn components_the_model_does_not_weigh_do_not_make_a_rating() {
    // The Sniper model has no heal in it; a Sniper with only heal rates nothing.
    let b = pool_of(TfClass::Sniper, 20);
    assert!(rate(&perf(TfClass::Sniper, vec![(Component::Heal, 10.0)]), &b, &w()).is_none());
}

#[test]
fn a_value_that_is_not_a_number_never_becomes_a_score() {
    let weights = w();
    let b = pool_of(TfClass::Soldier, 50);
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let r = rate(&perf(TfClass::Soldier, vec![(Component::Dpm, bad), (Component::ImpactKills, 25.0)]), &b, &weights);
        let r = r.expect("the good component still rates");
        assert!(r.score.is_finite(), "{bad} gave {}", r.score);
        assert!(r.parts.iter().all(|p| p.raw.is_finite() && p.percentile.is_finite()), "{bad} must not reach the working");
    }
    // And a pool never holds one.
    let polluted = vec![perf(TfClass::Soldier, vec![(Component::Dpm, f64::NAN)]), perf(TfClass::Soldier, vec![(Component::Dpm, 5.0)])];
    let b = Baseline::build(&polluted, None);
    assert_eq!(b.percentile(TfClass::Soldier, Component::Dpm, None, 5.0), Some(0.5));
}

// ---- better is never lower -------------------------------------------------

/// With everything else equal, a better stat never rates lower, in the
/// direction the model says is better, for every component of every class.
#[test]
fn better_stats_never_rate_lower_for_every_weighted_component() {
    let weights = w();
    for class in TfClass::ALL {
        let b = pool_of(class, 100);
        for &(c, wt) in weights.model_for(class) {
            if wt <= 0.0 {
                continue;
            }
            let mut last: Option<f64> = None;
            // Worse to better.
            let sweep: Vec<f64> = if c.higher_is_better() { (0..=120).step_by(5).map(f64::from).collect() } else { (0..=120).rev().step_by(5).map(f64::from).collect() };
            for v in sweep {
                let r = rate(&perf(class, vec![(c, v)]), &b, &weights).unwrap_or_else(|| panic!("{} {} rates", class.as_str(), c.key()));
                assert!((0.0..=100.0).contains(&r.score), "{} {} {v}: {}", class.as_str(), c.key(), r.score);
                if let Some(l) = last {
                    assert!(r.score >= l, "{} {}: {v} rated {} after {l}", class.as_str(), c.key(), r.score);
                }
                last = Some(r.score);
            }
            let (worst, best) = if c.higher_is_better() { (-1.0, 1000.0) } else { (1000.0, -1.0) };
            let lo = rate(&perf(class, vec![(c, worst)]), &b, &weights).unwrap().score;
            let hi = rate(&perf(class, vec![(c, best)]), &b, &weights).unwrap().score;
            assert!(hi > lo, "{} {}: the best game {hi} must beat the worst {lo}", class.as_str(), c.key());
            assert_eq!((lo, hi), (0.0, 100.0), "{} {}: a single component spans the whole range", class.as_str(), c.key());
        }
    }
}

#[test]
fn every_component_that_reads_fewer_is_better_is_flipped_in_the_working() {
    let weights = w();
    let b = pool_of(TfClass::Soldier, 100);
    // Deaths: 1 is the best in a pool of 1..=100.
    let r = rate(&perf(TfClass::Soldier, vec![(Component::Deaths, 1.0)]), &b, &weights).unwrap();
    let part = &r.parts[0];
    assert_eq!(part.raw, 1.0);
    assert!(part.percentile > 99.0, "the fewest deaths is the top percentile, got {}", part.percentile);
    assert!(!Component::Deaths.higher_is_better());
    assert!(Component::Dpm.higher_is_better());
}

#[test]
fn a_rating_is_the_weighted_average_of_its_parts_and_the_weights_add_to_one() {
    let weights = w();
    let class = TfClass::Medic;
    let b = pool_of(class, 100);
    let values: Vec<(Component, f64)> = weights.model_for(class).iter().map(|(c, _)| (*c, 37.0)).collect();
    let r = rate(&perf(class, values), &b, &weights).unwrap();
    let total: f64 = r.parts.iter().map(|p| p.weight).sum();
    // Each weight is rounded to two decimals on its own, so up to half a
    // hundredth off per part.
    assert!((total - 1.0).abs() <= 0.005 * r.parts.len() as f64, "weights add to {total}");
    let sum: f64 = r.parts.iter().map(|p| p.percentile * p.weight).sum();
    // The score is worked from unrounded percentiles and weights (RA-7), only
    // what is shown is rounded: the working agrees to within that rounding.
    assert!((sum - r.score).abs() < 1.0, "{sum} vs {}", r.score);
    assert!(r.parts.iter().all(|p| (0.0..=100.0).contains(&p.percentile)));
}

#[test]
fn a_missing_component_hands_its_weight_to_the_others_in_proportion() {
    let weights = w().with_model(TfClass::Scout, vec![(Component::Dpm, 3.0), (Component::ImpactKills, 1.0), (Component::Opening, 6.0)]);
    let b = pool_of(TfClass::Scout, 100);
    let r = rate(&perf(TfClass::Scout, vec![(Component::Dpm, 50.0), (Component::ImpactKills, 50.0)]), &b, &weights).unwrap();
    let share = |c: Component| r.parts.iter().find(|p| p.component == c).unwrap().weight;
    assert_eq!(share(Component::Dpm), 0.75);
    assert_eq!(share(Component::ImpactKills), 0.25);
}

#[test]
fn a_zero_weight_component_is_not_in_the_working() {
    let weights = w().with_model(TfClass::Scout, vec![(Component::Dpm, 1.0), (Component::ImpactKills, 0.0)]);
    let b = pool_of(TfClass::Scout, 100);
    let r = rate(&perf(TfClass::Scout, vec![(Component::Dpm, 50.0), (Component::ImpactKills, 50.0)]), &b, &weights).unwrap();
    assert_eq!(r.parts.len(), 1);
}

// ---- the scale ------------------------------------------------------------

#[test]
fn a_better_score_never_scales_lower_and_the_floor_holds() {
    let s = Scale::of(&(0..=100).map(f64::from).collect::<Vec<_>>()).unwrap();
    let mut last = -1.0;
    for score in (0..=100).map(f64::from) {
        let r = s.rating(score);
        assert!(r >= last && r >= 0.0 && r.is_finite());
        last = r;
    }
    assert_eq!(s.rating(f64::NAN), 0.0, "not a number is not a good game");
    assert!(s.per_percentile() > 0.0);
    assert!((s.per_percentile() * s.sd - RATING_SPREAD).abs() < 1e-12);
}

#[test]
fn a_scale_is_measured_from_the_pool_it_is_given() {
    let s = Scale::of(&[40.0, 60.0]).unwrap();
    assert_eq!((s.mean, s.sd), (50.0, 10.0));
    assert!(Scale::of(&[1.0, f64::NAN]).is_none(), "a pool with a NaN in it cannot be measured");
}

// ---- the pools ------------------------------------------------------------

fn on_map(class: TfClass, map: &str, id: u32, dpm: f64) -> Performance {
    Performance { account_id: id, class, minutes: 20.0, values: vec![(Component::Dpm, dpm)], map: Some(map.to_string()) }
}

#[test]
fn a_map_with_too_few_games_falls_back_to_every_map() {
    let class = TfClass::Sniper;
    let mut perfs: Vec<Performance> = (0..MIN_MAP_POOL as u32 - 1).map(|i| on_map(class, "pl_upward", i, 100.0 + f64::from(i))).collect();
    // The general pool also has a lot of low games on other maps.
    perfs.extend((0..300).map(|i| on_map(class, "koth_product", 1000 + i, f64::from(i % 50))));
    let b = Baseline::build(&perfs, None);
    assert_eq!(b.pool_for(class, Some("pl_upward")).0, None, "99 games is not a pool");
    let general = b.percentile(class, Component::Dpm, None, 120.0).unwrap();
    assert_eq!(b.percentile(class, Component::Dpm, Some("pl_upward"), 120.0), Some(general), "so it reads the general pool");
    assert_eq!(b.percentile(class, Component::Dpm, Some("never_played"), 120.0), Some(general), "and so does a map nobody has played");
}

#[test]
fn a_map_with_enough_games_is_measured_against_itself() {
    let class = TfClass::Sniper;
    let mut perfs: Vec<Performance> = (0..MIN_MAP_POOL as u32).map(|i| on_map(class, "pl_upward", i, 100.0 + f64::from(i))).collect();
    perfs.extend((0..300).map(|i| on_map(class, "koth_product", 1000 + i, f64::from(i % 50))));
    let b = Baseline::build(&perfs, None);
    let (map, n) = b.pool_for(class, Some("pl_upward"));
    assert_eq!((map, n), (Some("pl_upward"), MIN_MAP_POOL));
    // 150 is above the whole of Product but in the middle of Upward.
    let on_upward = b.percentile(class, Component::Dpm, Some("pl_upward"), 150.0).unwrap();
    let anywhere = b.percentile(class, Component::Dpm, None, 150.0).unwrap();
    assert!((0.45..0.55).contains(&on_upward), "{on_upward}");
    assert!(anywhere > on_upward);
    // And every game is in the general pool too.
    assert_eq!(b.pool_size(class), MIN_MAP_POOL + 300);
}

#[test]
fn stored_pools_come_back_as_they_went_in() {
    let class = TfClass::Demoman;
    let mut perfs: Vec<Performance> = (0..MIN_MAP_POOL as u32 + 5).map(|i| on_map(class, "pl_vigil", i, f64::from(i * 7 % 101))).collect();
    perfs.extend((0..40).map(|i| on_map(TfClass::Pyro, "pl_vigil", 5000 + i, f64::from(i))));
    let b = Baseline::build(&perfs, None);
    let stored: Vec<(TfClass, Component, Option<String>, Vec<f64>)> = b.parts().map(|(cl, c, m, v)| (cl, c, m.map(str::to_string), v.to_vec())).collect();
    let back = Baseline::from_parts(stored);
    for class in [TfClass::Demoman, TfClass::Pyro, TfClass::Medic] {
        for map in [None, Some("pl_vigil"), Some("elsewhere")] {
            for v in [-5.0, 0.0, 33.0, 100.0, 400.0] {
                assert_eq!(b.percentile(class, Component::Dpm, map, v), back.percentile(class, Component::Dpm, map, v), "{class:?} {map:?} {v}");
            }
            assert_eq!(b.pool_for(class, map).1, back.pool_for(class, map).1);
        }
    }
    assert!(!back.is_empty());
}

#[test]
fn percentiles_are_in_range_and_ties_count_half() {
    let class = TfClass::Scout;
    let perfs: Vec<Performance> = [1.0, 2.0, 2.0, 2.0, 3.0].iter().enumerate().map(|(i, v)| Performance { account_id: i as u32, ..perf(class, vec![(Component::Dpm, *v)]) }).collect();
    let b = Baseline::build(&perfs, None);
    for v in [f64::NEG_INFINITY, -1.0, 1.0, 2.0, 2.5, 3.0, 99.0, f64::INFINITY] {
        let p = b.percentile(class, Component::Dpm, None, v).unwrap();
        assert!((0.0..=1.0).contains(&p), "{v}: {p}");
    }
    assert_eq!(b.percentile(class, Component::Dpm, None, 2.0), Some((1.0 + 1.5) / 5.0));
    // Never decreases with the value.
    let mut last = 0.0;
    for v in (0..40).map(|i| f64::from(i) / 10.0) {
        let p = b.percentile(class, Component::Dpm, None, v).unwrap();
        assert!(p >= last);
        last = p;
    }
}

#[test]
fn the_owner_is_out_of_their_own_pool_in_every_component_and_map() {
    let class = TfClass::Spy;
    let mut perfs: Vec<Performance> = (0..MIN_MAP_POOL as u32).map(|i| on_map(class, "cp_steel", i + 1, f64::from(i))).collect();
    perfs.push(on_map(class, "cp_steel", 0, 9999.0));
    let b = Baseline::build(&perfs, Some(0));
    assert_eq!(b.pool_size(class), MIN_MAP_POOL);
    assert_eq!(b.percentile(class, Component::Dpm, Some("cp_steel"), 9999.0), Some(1.0), "the owner's own big game is not in the pool");
}

// ---- weights --------------------------------------------------------------

#[test]
fn the_default_weights_are_what_the_code_claims() {
    let weights = w();
    for class in TfClass::ALL {
        let model = weights.model_for(class);
        assert!(!model.is_empty());
        let mut seen = std::collections::HashSet::new();
        for (c, x) in model {
            assert!(x.is_finite() && *x >= 0.0, "{} {} = {x}", class.as_str(), c.key());
            assert!(seen.insert(*c), "{} lists {} twice", class.as_str(), c.key());
        }
        assert!(model.iter().map(|(_, x)| x).sum::<f64>() > 0.0);
        // In the stable order the rest of the app relies on.
        let order: Vec<usize> = model.iter().map(|(c, _)| Component::ALL.iter().position(|x| x == c).unwrap()).collect();
        assert!(order.windows(2).all(|p| p[0] < p[1]), "{} is out of order", class.as_str());
    }
    assert!(weights.general.min_minutes > 0.0 && weights.general.assist_share >= 0.0 && weights.general.even_margin >= 0.0);
}

#[test]
fn every_component_key_round_trips_and_is_unique() {
    let mut keys = std::collections::HashSet::new();
    for c in Component::ALL {
        assert_eq!(Component::parse(c.key()), Some(c));
        assert!(keys.insert(c.key()), "{} twice", c.key());
        assert!(!c.label().is_empty() && !c.unit().is_empty());
    }
    assert_eq!(Component::parse("nonsense"), None);
    assert_eq!(Component::parse(""), None);
}

#[test]
fn weights_that_are_not_numbers_are_refused_not_rated_by() {
    // A model weight of infinity makes every renormalised share NaN.
    let line = hl_rating::weights::DEFAULT_TOML
        .lines()
        .skip_while(|l| !l.starts_with("[model.sniper]"))
        .find(|l| l.contains('=') && !l.trim_start().starts_with('#'))
        .expect("the Sniper model has a weight")
        .to_string();
    let name = line.split('=').next().unwrap().trim().to_string();
    let inf = hl_rating::weights::DEFAULT_TOML.replacen(&line, &format!("{name} = inf"), 1);
    assert!(Weights::parse(&inf).is_err(), "an infinite weight must not parse");
    let nan = hl_rating::weights::DEFAULT_TOML.replacen(&line, &format!("{name} = nan"), 1);
    assert!(Weights::parse(&nan).is_err(), "a weight that is not a number must not parse");
    let scout = hl_rating::weights::DEFAULT_TOML.lines().find(|l| l.starts_with("scout ")).expect("a Scout victim value");
    let victim = hl_rating::weights::DEFAULT_TOML.replacen(scout, "scout = inf", 1);
    assert!(Weights::parse(&victim).is_err(), "nor a class value");
    let minutes = hl_rating::weights::DEFAULT_TOML.replacen("min_minutes = 5.0", "min_minutes = -5.0", 1);
    assert_ne!(minutes, hl_rating::weights::DEFAULT_TOML);
    assert!(Weights::parse(&minutes).is_err(), "nor a negative minimum");
}

// ---- real logs ------------------------------------------------------------

fn load(name: &str, id: i64) -> NormalizedLog {
    let path = format!("{}/../hl-ingest/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    normalize(id, &raw).unwrap()
}

fn fixtures() -> Vec<NormalizedLog> {
    vec![
        load("hl_2014_steamid2_295610.json", 295610),
        load("hl_2023_3445078.json", 3445078),
        load("hl_sub_19p_4114301.json", 4114301),
        load("hl_combined_6rounds_4109131.json", 4109131),
    ]
}

#[test]
fn every_player_of_every_real_log_rates_finite_and_in_range() {
    let weights = w();
    let logs = fixtures();
    let perfs: Vec<Performance> = logs.iter().flat_map(|l| l.players.iter().filter_map(|p| extract(p, &l.flags, &weights, None, None))).collect();
    assert!(perfs.len() > 40, "{} rateable performances", perfs.len());
    let b = Baseline::build(&perfs, None);
    let mut scores = Vec::new();
    for p in &perfs {
        assert!(p.minutes >= weights.general.min_minutes);
        assert!(p.values.iter().all(|(_, v)| v.is_finite()), "{:?}", p.values);
        let Some(r) = rate(p, &b, &weights) else { continue };
        assert!((0.0..=100.0).contains(&r.score), "{}", r.score);
        assert!(r.parts.iter().all(|x| (0.0..=100.0).contains(&x.percentile) && x.weight > 0.0 && x.weight <= 1.0));
        scores.push(r.score);
    }
    let scale = Scale::of(&scores).expect("real games differ from each other");
    assert!((20.0..80.0).contains(&scale.mean) && scale.sd > 1.0, "{scale:?}");
    let rated: Vec<f64> = scores.iter().map(|s| scale.rating(*s)).collect();
    assert!(rated.iter().all(|r| r.is_finite() && *r >= 0.0));
    let mean = rated.iter().sum::<f64>() / rated.len() as f64;
    assert!((mean - 1.0).abs() < 0.05, "the pool's own average game rates about 1.00, got {mean}");
}

#[test]
fn the_medics_numbers_come_from_heals_ubers_and_drops_per_the_time_played() {
    let weights = w();
    let logs = fixtures();
    let mut seen = 0;
    for l in &logs {
        for p in l.players.iter().filter(|p| p.main_class() == Some(TfClass::Medic)) {
            let Some(line) = p.classes.iter().find(|c| c.class == TfClass::Medic) else { continue };
            let minutes = line.time_s as f64 / 60.0;
            if minutes < weights.general.min_minutes {
                continue;
            }
            let all = Weights::default_weights().with_model(
                TfClass::Medic,
                vec![(Component::Heal, 1.0), (Component::Ubers, 1.0), (Component::Drops, 1.0), (Component::Deaths, 1.0), (Component::Dpm, 1.0)],
            );
            let got = extract(p, &l.flags, &all, None, None).unwrap();
            let get = |c: Component| got.values.iter().find(|(x, _)| *x == c).map(|(_, v)| *v);
            assert!((get(Component::Heal).unwrap() - p.stats.heal as f64 / minutes).abs() < 1e-9, "heal per minute");
            assert!((get(Component::Ubers).unwrap() - p.stats.ubers as f64 * 10.0 / minutes).abs() < 1e-9, "ubers per 10 minutes");
            assert!((get(Component::Drops).unwrap() - p.stats.drops as f64 * 10.0 / minutes).abs() < 1e-9);
            assert!(get(Component::Heal).unwrap() > 0.0, "a Medic heals");
            seen += 1;
        }
    }
    assert!(seen >= 4, "the fixtures hold {seen} Medics");
}

#[test]
fn a_class_switcher_is_rated_on_the_class_they_played_longest_only() {
    let weights = w().with_model(TfClass::Soldier, vec![(Component::Dpm, 1.0), (Component::Deaths, 1.0)]);
    let p = player(
        1,
        vec![line(TfClass::Scout, 600, 20, 1, 9000), line(TfClass::Soldier, 1200, 8, 6, 3600), line(TfClass::Medic, 60, 0, 0, 0)],
        PlayerStats::default(),
        vec![],
    );
    let got = extract(&p, &flags(), &weights, None, None).unwrap();
    assert_eq!(got.class, TfClass::Soldier);
    assert_eq!(got.minutes, 20.0, "the time on that class, not the whole log");
    let get = |c: Component| got.values.iter().find(|(x, _)| *x == c).unwrap().1;
    assert_eq!(get(Component::Dpm), 180.0, "the Soldier's damage over the Soldier's minutes, not the Scout's");
    assert_eq!(get(Component::Deaths), 3.0);
}

#[test]
fn equal_time_on_two_classes_picks_the_same_one_every_time() {
    let p = player(1, vec![line(TfClass::Scout, 900, 1, 1, 1), line(TfClass::Soldier, 900, 1, 1, 1)], PlayerStats::default(), vec![]);
    let first = extract(&p, &flags(), &w(), None, None).unwrap().class;
    for _ in 0..5 {
        assert_eq!(extract(&p, &flags(), &w(), None, None).unwrap().class, first);
    }
}

#[test]
fn without_a_raw_log_the_components_that_need_one_are_missing_not_zero() {
    let weights = w();
    let needs_raw = [
        Component::Opening,
        Component::Untraded,
        Component::UntradedDeaths,
        Component::FlankDeaths,
        Component::StationaryDeaths,
        Component::FightKast,
        Component::FightKastEngaged,
        Component::FightSwing,
        Component::FightSwingShared,
        Component::RoundImpact,
        Component::CapsContested,
        Component::CapsMatesDead,
        Component::CapsSpawnDelay,
    ];
    for class in TfClass::ALL {
        let model = needs_raw.iter().map(|c| (*c, 1.0)).collect();
        let w2 = weights.with_model(class, model);
        let p = player(1, vec![line(class, 1200, 5, 5, 1000)], PlayerStats::default(), vec![]);
        let got = extract(&p, &flags(), &w2, None, None).unwrap();
        assert!(got.values.is_empty(), "{}: {:?}", class.as_str(), got.values);
    }
}

#[test]
fn impact_falls_back_to_classkills_at_the_general_values() {
    let weights = w().with_model(TfClass::Soldier, vec![(Component::ImpactKills, 1.0), (Component::ImpactAssists, 1.0)]);
    let vs = vec![
        VsLine { other_class: TfClass::Medic, kills: 2, deaths: 0, assists: 1 },
        VsLine { other_class: TfClass::Scout, kills: 3, deaths: 1, assists: 0 },
    ];
    let p = player(1, vec![line(TfClass::Soldier, 1200, 5, 1, 1000)], PlayerStats::default(), vs);
    let got = extract(&p, &flags(), &weights, None, None).unwrap();
    let kills = 2.0 * weights.victim(TfClass::Medic) + 3.0 * weights.victim(TfClass::Scout);
    let assists = weights.victim(TfClass::Medic) * weights.general.assist_share;
    let get = |c: Component| got.values.iter().find(|(x, _)| *x == c).unwrap().1;
    assert!((get(Component::ImpactKills) - kills * 10.0 / 20.0).abs() < 1e-9);
    assert!((get(Component::ImpactAssists) - assists * 10.0 / 20.0).abs() < 1e-9);
}
