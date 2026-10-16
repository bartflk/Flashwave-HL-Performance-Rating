//! One ETF2L official, for the ETF2L tab's match page (Q74, Flashy; PLAN
//! §27): its score and maps, who played with the class and rating they had
//! in it, the two teams' earlier meetings, and its logs.
//!
//! Everything comes from the database: the season's results and match pages
//! (Q29), the league sample and the owner's matches for the ratings. A match
//! not played yet has its header and head-to-head only; the page shows each
//! team's lineup from its own page instead.

use anyhow::Result;
use hl_db::{Db, MatchLog, MatchMap, ResultRow};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchPlayer {
    pub account_id: u32,
    pub name: String,
    pub team_id: Option<i64>,
    /// The class they played most in this match's rated logs.
    pub class: Option<String>,
    /// Their average rating on that class in this match (1.00 is a
    /// typical game).
    pub rating: Option<f64>,
}

/// An earlier official between the two teams, from `clan1`'s side.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub match_id: i64,
    pub time: Option<i64>,
    pub season: i64,
    pub division: Option<String>,
    pub round: Option<String>,
    pub score_a: Option<i64>,
    pub score_b: Option<i64>,
    pub default_win: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeagueMatch {
    #[serde(flatten)]
    pub head: ResultRow,
    pub maps: Vec<MatchMap>,
    pub players: Vec<MatchPlayer>,
    pub meetings: Vec<Meeting>,
    pub logs: Vec<MatchLog>,
    pub url: String,
}

/// The class played most often in a set of games, and the average rating
/// on it. Ties go to the higher-rated class's first appearance.
fn class_and_rating(games: &[(String, f64)]) -> (Option<String>, Option<f64>) {
    let mut by: Vec<(String, Vec<f64>)> = Vec::new();
    for (class, score) in games {
        match by.iter_mut().find(|(c, _)| c == class) {
            Some((_, v)) => v.push(*score),
            None => by.push((class.clone(), vec![*score])),
        }
    }
    let best = by.into_iter().max_by_key(|(_, v)| v.len());
    match best {
        Some((class, v)) => {
            let avg = v.iter().sum::<f64>() / v.len() as f64;
            (Some(class), Some(avg))
        }
        None => (None, None),
    }
}

pub async fn league_match(db: &Db, match_id: i64) -> Result<Option<LeagueMatch>> {
    let Some(head) = db.league_match(match_id).await? else { return Ok(None) };
    let maps = db.league_match_maps(match_id).await?;
    let logs = db.league_match_logs(match_id).await?;
    let names: HashMap<u32, String> = db.etf2l_names().await?;
    let mut players = Vec::new();
    for p in db.league_match_players(match_id).await? {
        let games: Vec<(String, f64)> = db
            .rated_games(hl_rating::MODEL_VERSION, Some(p.account_id))
            .await?
            .into_iter()
            .filter(|g| g.etf2l_match_id == Some(match_id))
            .map(|g| (g.class, g.score))
            .collect();
        let (class, rating) = class_and_rating(&games);
        let name = p.name.filter(|n| !n.trim().is_empty()).or_else(|| names.get(&p.account_id).cloned()).unwrap_or_else(|| p.account_id.to_string());
        players.push(MatchPlayer { account_id: p.account_id, name, team_id: p.team_id, class, rating });
    }
    let meetings = db
        .head_to_head(head.clan1.id, head.clan2.id)
        .await?
        .into_iter()
        .filter(|m| m.match_id != match_id)
        .map(|m| Meeting {
            match_id: m.match_id,
            time: m.time,
            season: m.season,
            division: m.division,
            round: m.round,
            score_a: m.score_a,
            score_b: m.score_b,
            default_win: m.default_win,
        })
        .collect();
    Ok(Some(LeagueMatch { url: format!("https://etf2l.org/matches/{match_id}/"), head, maps, players, meetings, logs }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_class_is_the_one_played_most_and_rated_on_it_alone() {
        let g = vec![("sniper".to_string(), 60.0), ("spy".to_string(), 90.0), ("sniper".to_string(), 40.0)];
        assert_eq!(class_and_rating(&g), (Some("sniper".to_string()), Some(50.0)));
        assert_eq!(class_and_rating(&[]), (None, None));
    }

    #[tokio::test]
    async fn a_match_reads_with_its_maps_players_and_earlier_meetings() {
        let db = Db::connect_in_memory().await.unwrap();
        for sql in [
            "INSERT INTO etf2l_competition (competition_id, season, season_name, division, stage, name, fetched_at) VALUES (1, 36, 'Autumn 2026', 'High', 'regular', 'HL 36', 0)",
            "INSERT INTO etf2l_team (team_id, name) VALUES (1, 'DD14'), (2, 'TWICE')",
            "INSERT INTO etf2l_season_match (match_id, competition_id, time, clan1_id, clan2_id, r1, r2) VALUES (10, 1, 100, 1, 2, 4, 2), (11, 1, 300, 2, 1, 6, 0)",
            "INSERT INTO etf2l_season_map (match_id, match_order, map, clan1, clan2) VALUES (11, 2, 'koth_product_final', 3, 0), (11, 1, 'pl_upward_f12', 3, 0)",
            "INSERT INTO etf2l_season_player (match_id, account_id, team_id, name) VALUES (11, 7, 2, 'seven'), (11, 8, 1, '')",
        ] {
            sqlx::query(sql).execute(db.pool()).await.unwrap();
        }
        let m = league_match(&db, 11).await.unwrap().unwrap();
        assert_eq!((m.head.clan1.name.as_str(), m.head.r1, m.head.r2), ("TWICE", Some(6), Some(0)));
        assert_eq!(m.maps.iter().map(|x| x.map.as_str()).collect::<Vec<_>>(), vec!["pl_upward_f12", "koth_product_final"], "in the order played");
        assert_eq!(m.players.len(), 2);
        assert_eq!(m.players.iter().find(|p| p.account_id == 8).unwrap().name, "8", "no name at all: the account");
        assert_eq!(m.meetings.iter().map(|x| x.match_id).collect::<Vec<_>>(), vec![10], "the match itself is not an earlier meeting");
        assert_eq!((m.meetings[0].score_a, m.meetings[0].score_b), (Some(2), Some(4)), "from TWICE's side");
        assert_eq!(m.url, "https://etf2l.org/matches/11/");
        assert!(league_match(&db, 99).await.unwrap().is_none());
    }
}
