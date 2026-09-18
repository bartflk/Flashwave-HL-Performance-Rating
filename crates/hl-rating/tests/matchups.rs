//! Matchups against a real log: the owner's 19-player Sniper game, where a
//! Heavy sub swapped in on the other team.

use hl_core::matchdata::Team;
use hl_core::{SteamId, TfClass};
use hl_ingest::normalize::normalize;
use hl_rating::{build_detail, Weights};

const ME: u32 = 139_131_191;

fn detail() -> hl_rating::MatchDetail {
    let path = format!(
        "{}/../hl-ingest/tests/fixtures/hl_sub_19p_4114301.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let log = normalize(4114301, &raw).unwrap();
    build_detail(&log, Some(SteamId::from_account_id(ME)), &Weights::default_weights())
}

#[test]
fn owners_team_is_on_the_left() {
    let d = detail();
    assert_eq!(d.left_team, d.my_team.unwrap());
}

#[test]
fn nine_matchups_in_lineup_order() {
    let d = detail();
    let classes: Vec<_> = d.matchups.iter().map(|m| m.class).collect();
    assert_eq!(classes, TfClass::ALL.to_vec());
}

/// The raw log says: my classkills.sniper = 4, my classdeaths.sniper = 1.
/// So the sniper head-to-head, from my side, is 4-1.
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
    assert!(!decisive.is_empty() && decisive.len() <= 3);
    assert!(decisive.iter().all(|m| m.winner != Some("even")));
}

#[test]
fn box_score_has_everyone_grouped_by_team() {
    let d = detail();
    assert_eq!(d.players.len(), 19);
    assert_eq!(d.players.iter().filter(|p| p.is_me).count(), 1);
    // Blue sorts before Red, and the team boundary is crossed exactly once.
    let teams: Vec<Team> = d.players.iter().map(|p| p.team).collect();
    assert_eq!(teams.windows(2).filter(|w| w[0] != w[1]).count(), 1);
}

#[test]
fn values_show_their_working() {
    let d = detail();
    let me = d.players.iter().find(|p| p.is_me).unwrap();
    let v = me.value.as_ref().unwrap();
    // The score is exactly the sum of its displayed terms (to rounding).
    let sum = v.impact_kills + v.impact_assists - v.death_cost + v.medic_term;
    assert!((v.score - sum).abs() < 0.05, "score {} vs terms {}", v.score, sum);
    assert!(!v.approximate, "sniper is my main class here");
}
