//! Matchups against real logs: the owner's 19-player Sniper game, where a
//! Heavy sub swapped in on the other team, rated against a pool built from
//! every real fixture log.

use hl_core::matchdata::{NormalizedLog, Team};
use hl_core::{SteamId, TfClass};
use hl_ingest::normalize::normalize;
use hl_rating::model::extract;
use hl_rating::{build_detail, Baseline, MatchDetail, Weights};

const ME: u32 = 139_131_191;

fn load(name: &str, id: i64) -> NormalizedLog {
    let path = format!("{}/../hl-ingest/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    normalize(id, &raw).unwrap()
}

/// A pool from all four Highlander fixtures: small, but real performances.
fn baseline(w: &Weights) -> Baseline {
    let logs = [
        load("hl_2014_steamid2_295610.json", 295610),
        load("hl_2023_3445078.json", 3445078),
        load("hl_sub_19p_4114301.json", 4114301),
        load("hl_combined_6rounds_4109131.json", 4109131),
    ];
    let perfs: Vec<_> = logs
        .iter()
        .flat_map(|l| l.players.iter().filter_map(|p| extract(p, &l.flags, w, None, None)))
        .collect();
    Baseline::build(&perfs, Some(ME))
}

fn detail() -> MatchDetail {
    let w = Weights::default_weights();
    let log = load("hl_sub_19p_4114301.json", 4114301);
    build_detail(&log, Some(SteamId::from_account_id(ME)), &w, &baseline(&w), &Default::default(), &Default::default())
}

#[test]
fn owners_team_is_on_the_left() {
    let d = detail();
    assert_eq!(d.left_team, d.my_team.unwrap());
    assert!(d.rated);
}

#[test]
fn nine_matchups_in_lineup_order() {
    let d = detail();
    let classes: Vec<_> = d.matchups.iter().map(|m| m.class).collect();
    assert_eq!(classes, TfClass::ALL.to_vec());
}

/// The raw log says: my classkills.sniper = 4, my classdeaths.sniper = 1.
#[test]
fn sniper_duel_reads_four_one() {
    let d = detail();
    let sniper = d.matchups.iter().find(|m| m.class == TfClass::Sniper).unwrap();
    assert!(sniper.involves_me);
    assert_eq!(sniper.left.as_ref().unwrap().account_id, ME);
    assert_eq!(sniper.head_to_head, Some((4, 1)));
}

/// The team with ten players had a Heavy swap mid-match.
#[test]
fn a_sub_is_reported_not_dropped() {
    let d = detail();
    let heavy = d.matchups.iter().find(|m| m.class == TfClass::Heavy).unwrap();
    let with_sub = [heavy.left.as_ref(), heavy.right.as_ref()]
        .into_iter()
        .flatten()
        .find(|s| !s.subs.is_empty());
    assert!(with_sub.is_some(), "the sub Heavy should be listed");
}

#[test]
fn at_most_three_decisive_and_never_even() {
    let d = detail();
    let decisive: Vec<_> = d.matchups.iter().filter(|m| m.decisive).collect();
    assert!(decisive.len() <= 3);
    assert!(decisive.iter().all(|m| m.winner != Some("even")));
}

#[test]
fn box_score_has_everyone_grouped_by_team() {
    let d = detail();
    assert_eq!(d.players.len(), 19);
    assert_eq!(d.players.iter().filter(|p| p.is_me).count(), 1);
    let teams: Vec<Team> = d.players.iter().map(|p| p.team).collect();
    assert_eq!(teams.windows(2).filter(|w| w[0] != w[1]).count(), 1);
}

/// The rating is the weighted average of its displayed percentiles, put on
/// the 1.00 scale, and it uses the Sniper model: the duel is in it, caps are
/// not. The parts stay percentiles — they are the working, not the answer.
#[test]
fn ratings_show_their_working() {
    let d = detail();
    let me = d.players.iter().find(|p| p.is_me).unwrap();
    let r = me.rating.as_ref().expect("a 15-minute Sniper game is rateable");
    let sum: f64 = r.parts.iter().map(|p| p.percentile * p.weight).sum();
    let expected = hl_rating::model::Scale::default().rating(sum);
    assert!((r.score - expected).abs() < 0.01, "score {} vs parts {sum} -> {expected}", r.score);
    assert!((0.0..=3.0).contains(&r.score), "a rating, not a percentile: {}", r.score);
    let keys: Vec<_> = r.parts.iter().map(|p| p.component.key()).collect();
    assert!(keys.contains(&"duel"));
    assert!(!keys.contains(&"caps"));
}

/// Without a baseline nothing is rated, and the page says so rather than
/// inventing numbers.
#[test]
fn without_a_baseline_nothing_is_rated() {
    let w = Weights::default_weights();
    let log = load("hl_sub_19p_4114301.json", 4114301);
    let d = build_detail(&log, Some(SteamId::from_account_id(ME)), &w, &Baseline::default(), &Default::default(), &Default::default());
    assert!(!d.rated);
    assert!(d.players.iter().all(|p| p.rating.is_none()));
    assert!(d.matchups.iter().all(|m| m.winner.is_none()));
}
