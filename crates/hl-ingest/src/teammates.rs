//! Who the owner plays with, and how they do together.
//!
//! "Your rating with them" is the owner's average rating in games shared
//! with that teammate, against their average in the other games of the same
//! scope. It is a correlation, not an effect: a teammate from a strong era
//! lifts the number whatever they did.

use anyhow::Result;
use hl_core::SteamId;
use hl_db::{Db, MateRow, OwnGameRow};
use hl_rating::MODEL_VERSION;
use serde::Serialize;
use std::collections::HashMap;

/// Teammates with fewer shared games than this are left off the list.
pub const MIN_GAMES: usize = 5;
/// "Current" means a shared game within this long of the owner's latest game.
const CURRENT_S: i64 = 60 * 24 * 3600;
/// A team's core is its most frequent players.
const CORE_SIZE: usize = 9;
/// With/without comparisons need at least this many rated games on each side.
/// Below ten, one great game swings the average by several points.
const MIN_RATED: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Officials and scrims: games with a real team.
    Team,
    All,
}

impl Scope {
    fn admits(self, kind: &str) -> bool {
        self == Scope::All || kind != "pug"
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Teammates {
    pub games: usize,
    pub teams: Vec<TeamEra>,
    pub teammates: Vec<Teammate>,
    pub min_games: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Teammate {
    pub account_id: u32,
    pub steamid64: String,
    pub name: String,
    pub games: usize,
    pub officials: usize,
    pub wins: usize,
    pub losses: usize,
    pub first_played: i64,
    pub last_played: i64,
    /// The class they played most in shared games.
    pub main_class: Option<String>,
    /// Played together recently.
    pub current: bool,
    /// ETF2L teams you shared, most games first.
    pub teams: Vec<String>,
    /// The owner's average rating in shared games, and the difference from
    /// their average in the other games. `None` with too few rated games.
    pub my_avg_with: Option<f64>,
    pub my_avg_delta: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamEra {
    pub team_id: i64,
    pub name: String,
    pub first_played: i64,
    pub last_played: i64,
    pub games: usize,
    pub officials: usize,
    pub wins: usize,
    pub losses: usize,
    pub my_avg: Option<f64>,
    pub core: Vec<CoreMate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreMate {
    pub account_id: u32,
    pub name: String,
    pub main_class: Option<String>,
    pub games: usize,
}

pub async fn load(db: &Db, me: SteamId, scope: Scope) -> Result<Teammates> {
    let games = db.own_games(me.account_id(), MODEL_VERSION).await?;
    let mates = db.mate_rows(me.account_id()).await?;
    Ok(summarize(&games, &mates, scope))
}

pub fn summarize(games: &[OwnGameRow], mates: &[MateRow], scope: Scope) -> Teammates {
    let games: HashMap<i64, &OwnGameRow> =
        games.iter().filter(|g| scope.admits(&g.kind)).map(|g| (g.log_id, g)).collect();
    let latest = games.values().map(|g| g.played_at).max().unwrap_or(0);

    // Per teammate: the games they shared, and their line in each.
    let mut by_mate: HashMap<u32, Vec<(&OwnGameRow, &MateRow)>> = HashMap::new();
    for m in mates {
        if let Some(g) = games.get(&m.log_id) {
            by_mate.entry(m.account_id).or_default().push((g, m));
        }
    }

    let scored: Vec<f64> = games.values().filter_map(|g| g.score).collect();
    let total_score: f64 = scored.iter().sum();

    let mut teammates: Vec<Teammate> = by_mate
        .iter()
        .filter(|(_, shared)| shared.len() >= MIN_GAMES)
        .map(|(&account_id, shared)| {
            let mut shared = shared.clone();
            shared.sort_by_key(|(g, _)| (g.played_at, g.log_id));
            let with: Vec<f64> = shared.iter().filter_map(|(g, _)| g.score).collect();
            let without_n = scored.len() - with.len();
            let (avg_with, delta) = if with.len() >= MIN_RATED && without_n >= MIN_RATED {
                let a = mean(&with);
                let b = (total_score - with.iter().sum::<f64>()) / without_n as f64;
                (Some(round1(a)), Some(round1(a - b)))
            } else {
                (None, None)
            };
            let first = shared.first().map(|(g, _)| g.played_at).unwrap_or(0);
            let last = shared.last().map(|(g, _)| g.played_at).unwrap_or(0);
            Teammate {
                account_id,
                steamid64: SteamId::from_account_id(account_id).to_steamid64(),
                name: latest_name(shared.iter().map(|(_, m)| *m)),
                games: shared.len(),
                officials: shared.iter().filter(|(g, _)| g.kind == "official").count(),
                wins: shared.iter().filter(|(g, _)| g.result == "W").count(),
                losses: shared.iter().filter(|(g, _)| g.result == "L").count(),
                first_played: first,
                last_played: last,
                main_class: most_common(shared.iter().filter_map(|(_, m)| m.main_class.clone())),
                current: latest - last <= CURRENT_S,
                teams: by_count(shared.iter().filter_map(|(g, _)| g.team_name.clone())),
                my_avg_with: avg_with,
                my_avg_delta: delta,
            }
        })
        .collect();
    teammates.sort_by(|a, b| b.games.cmp(&a.games).then(b.last_played.cmp(&a.last_played)));

    // Teams: games grouped by the owner's ETF2L team.
    let mut by_team: HashMap<i64, Vec<&OwnGameRow>> = HashMap::new();
    for g in games.values() {
        if let Some(t) = g.team_id {
            by_team.entry(t).or_default().push(g);
        }
    }
    let mut teams: Vec<TeamEra> = by_team
        .into_iter()
        .map(|(team_id, mut gs)| {
            gs.sort_by_key(|g| (g.played_at, g.log_id));
            let ids: std::collections::HashSet<i64> = gs.iter().map(|g| g.log_id).collect();
            let mut counts: HashMap<u32, Vec<&MateRow>> = HashMap::new();
            for m in mates.iter().filter(|m| ids.contains(&m.log_id)) {
                counts.entry(m.account_id).or_default().push(m);
            }
            let mut core: Vec<CoreMate> = counts
                .into_iter()
                .map(|(account_id, rows)| CoreMate {
                    account_id,
                    name: latest_name(rows.iter().copied()),
                    main_class: most_common(rows.iter().filter_map(|m| m.main_class.clone())),
                    games: rows.len(),
                })
                .collect();
            core.sort_by(|a, b| b.games.cmp(&a.games).then(a.name.cmp(&b.name)));
            core.truncate(CORE_SIZE);
            let scores: Vec<f64> = gs.iter().filter_map(|g| g.score).collect();
            TeamEra {
                team_id,
                // The name from the team's most recent game: teams get renamed.
                name: gs.iter().rev().find_map(|g| g.team_name.clone()).unwrap_or_default(),
                first_played: gs.first().map(|g| g.played_at).unwrap_or(0),
                last_played: gs.last().map(|g| g.played_at).unwrap_or(0),
                games: gs.len(),
                officials: gs.iter().filter(|g| g.kind == "official").count(),
                wins: gs.iter().filter(|g| g.result == "W").count(),
                losses: gs.iter().filter(|g| g.result == "L").count(),
                my_avg: (!scores.is_empty()).then(|| round1(mean(&scores))),
                core,
            }
        })
        .collect();
    teams.sort_by_key(|t| std::cmp::Reverse(t.last_played));

    Teammates { games: games.len(), teams, teammates, min_games: MIN_GAMES }
}

/// The name from the most recent log: players change names, the latest is
/// the one you know them by. `mates` must be in any order; log ids grow with time.
fn latest_name<'a>(mates: impl Iterator<Item = &'a MateRow>) -> String {
    mates
        .filter(|m| m.name.as_deref().is_some_and(|n| !n.trim().is_empty()))
        .max_by_key(|m| m.log_id)
        .and_then(|m| m.name.clone())
        .unwrap_or_else(|| "unknown".to_string())
}

fn most_common(items: impl Iterator<Item = String>) -> Option<String> {
    by_count(items).into_iter().next()
}

/// Distinct items, most frequent first; ties alphabetical so output is stable.
fn by_count(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for i in items {
        *counts.entry(i).or_default() += 1;
    }
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().map(|(s, _)| s).collect()
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: i64, kind: &str, team: Option<(i64, &str)>, result: &str, score: f64) -> OwnGameRow {
        OwnGameRow {
            log_id: id,
            played_at: 1_000_000 + id * 86_400,
            kind: kind.to_string(),
            team_id: team.map(|t| t.0),
            team_name: team.map(|t| t.1.to_string()),
            result: result.to_string(),
            score: Some(score),
        }
    }

    fn mate(log: i64, who: u32, name: &str, class: &str) -> MateRow {
        MateRow { log_id: log, account_id: who, name: Some(name.into()), main_class: Some(class.into()), time_s: 1800 }
    }

    #[test]
    fn counts_rates_and_compares_with_the_other_games() {
        // Twenty team games; teammate 7 plays the first twelve, where the owner scores 70.
        let games: Vec<_> = (1..=20)
            .map(|i| game(i, "scrim", Some((5, "SBQRRA")), if i <= 12 { "W" } else { "L" }, if i <= 12 { 70.0 } else { 40.0 }))
            .collect();
        let mates: Vec<_> = (1..=12).map(|i| mate(i, 7, if i == 12 { "new name" } else { "old" }, "medic")).collect();
        let t = summarize(&games, &mates, Scope::Team);
        let m = &t.teammates[0];
        assert_eq!((m.games, m.wins, m.losses), (12, 12, 0));
        assert_eq!(m.name, "new name");
        assert_eq!(m.main_class.as_deref(), Some("medic"));
        assert_eq!(m.teams, vec!["SBQRRA".to_string()]);
        assert_eq!(m.my_avg_with, None, "only eight games without them: too few to compare");

        let more: Vec<_> = (21..=22).map(|i| game(i, "scrim", None, "L", 40.0)).collect();
        let games2: Vec<_> = games.into_iter().chain(more).collect();
        let m = summarize(&games2, &mates, Scope::Team).teammates[0].clone();
        assert_eq!(m.my_avg_with, Some(70.0));
        assert_eq!(m.my_avg_delta, Some(30.0));
    }

    #[test]
    fn pugs_are_out_of_team_scope() {
        let games: Vec<_> = (1..=6).map(|i| game(i, "pug", None, "W", 50.0)).collect();
        let mates: Vec<_> = (1..=6).map(|i| mate(i, 7, "p", "scout")).collect();
        assert!(summarize(&games, &mates, Scope::Team).teammates.is_empty());
        assert_eq!(summarize(&games, &mates, Scope::All).teammates.len(), 1);
    }

    #[test]
    fn teams_group_games_and_take_the_latest_name() {
        let games = vec![
            game(1, "official", Some((5, "Old Name")), "W", 60.0),
            game(2, "scrim", Some((5, "New Name")), "L", 40.0),
            game(3, "scrim", Some((9, "Other")), "W", 50.0),
        ];
        let t = summarize(&games, &[mate(1, 7, "a", "medic"), mate(2, 7, "a", "medic")], Scope::Team);
        assert_eq!(t.teams.len(), 2);
        let five = t.teams.iter().find(|x| x.team_id == 5).unwrap();
        assert_eq!(five.name, "New Name");
        assert_eq!((five.games, five.officials, five.wins, five.losses), (2, 1, 1, 1));
        assert_eq!(five.my_avg, Some(50.0));
        assert_eq!(five.core[0].games, 2);
        assert_eq!(t.teams[0].team_id, 9, "most recent team first");
    }
}
