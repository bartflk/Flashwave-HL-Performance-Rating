//! The owner's 6v6 profile (Flashy: "please bring them up to standard"):
//! the same reads as the Highlander profile's, over `sixes_rating` by role
//! instead of `rating` by class. Only the owner's own stored logs: the 6v6
//! league sample's logs are in `sixes_rating` too, but have no `match` row.

use crate::context::{OwnGameRow, RESULT_SQL};
use crate::{Db, HistoryDbRow, Result};
use sqlx::Row;

impl Db {
    /// The roles (and off-classes) the owner has rated 6v6 games in, most played first.
    pub async fn sixes_rated_roles(&self, account_id: u32, version: &str) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT r.role, COUNT(*) AS n
             FROM sixes_rating r
             JOIN match m     ON m.log_id = r.log_id
             JOIN log_index i ON i.log_id = r.log_id
             WHERE r.account_id = ?1 AND r.model_version = ?2 AND i.superseded_by IS NULL
             GROUP BY r.role ORDER BY n DESC, r.role",
        )
        .bind(account_id as i64)
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get("role"), r.get("n"))).collect())
    }

    /// [`Db::rating_history`] for 6v6: every own game in `role`, its
    /// rating on the app's scale as the score.
    pub async fn sixes_rating_history(&self, account_id: u32, role: &str, version: &str) -> Result<Vec<HistoryDbRow>> {
        let rows = sqlx::query(
            "SELECT r.log_id, m.played_at, m.map, m.title, i.league, p.team, c.kind,
                    m.red_score, m.blue_score, r.rating AS score, r.minutes, r.parts
             FROM sixes_rating r
             JOIN match m        ON m.log_id = r.log_id
             JOIN log_index i    ON i.log_id = r.log_id
             JOIN match_player p ON p.log_id = r.log_id AND p.account_id = r.account_id
             LEFT JOIN match_context c ON c.log_id = r.log_id
             WHERE r.account_id = ?1 AND r.role = ?2 AND r.model_version = ?3
               AND i.superseded_by IS NULL",
        )
        .bind(account_id as i64)
        .bind(role)
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| HistoryDbRow {
                log_id: r.get("log_id"),
                played_at: r.get("played_at"),
                map: r.get("map"),
                title: r.get("title"),
                league: r.get("league"),
                team: r.get("team"),
                kind: r.get("kind"),
                red_score: r.get("red_score"),
                blue_score: r.get("blue_score"),
                score: r.get("score"),
                minutes: r.get("minutes"),
                parts_json: r.get("parts"),
            })
            .collect())
    }

    /// [`Db::class_opponents`] for 6v6: per own game in `role`, the other
    /// side's player in the same role, their average rating in that role
    /// over their other rated games (league sample included) and how many.
    pub async fn sixes_role_opponents(&self, account_id: u32, role: &str, version: &str) -> Result<Vec<(i64, u32, f64, i64)>> {
        let rows = sqlx::query(
            "WITH mine AS (
                 SELECT r.log_id, p.team
                 FROM sixes_rating r
                 JOIN match_player p ON p.log_id = r.log_id AND p.account_id = r.account_id
                 WHERE r.account_id = ?1 AND r.role = ?2 AND r.model_version = ?3
             ),
             opp AS (
                 SELECT m.log_id, r.account_id AS opp_id
                 FROM mine m
                 JOIN match_player p ON p.log_id = m.log_id AND p.team <> m.team
                 JOIN sixes_rating r ON r.log_id = m.log_id AND r.account_id = p.account_id
                                    AND r.role = ?2 AND r.model_version = ?3
             )
             SELECT o.log_id, o.opp_id,
                    (SELECT AVG(x.rating) FROM sixes_rating x
                      WHERE x.account_id = o.opp_id AND x.role = ?2
                        AND x.model_version = ?3 AND x.log_id <> o.log_id) AS strength,
                    (SELECT COUNT(*) FROM sixes_rating x
                      WHERE x.account_id = o.opp_id AND x.role = ?2
                        AND x.model_version = ?3 AND x.log_id <> o.log_id) AS games
             FROM opp o",
        )
        .bind(account_id as i64)
        .bind(role)
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let strength: Option<f64> = r.get("strength");
                Some((r.get("log_id"), r.get::<i64, _>("opp_id") as u32, strength?, r.get("games")))
            })
            .collect())
    }

    /// [`Db::own_games`] for 6v6: the owner's classified 6v6 games, each
    /// with their main-role rating (off-class stints left out).
    pub async fn own_sixes_games(&self, me: u32, version: &str) -> Result<Vec<OwnGameRow>> {
        let rows = sqlx::query(&format!(
            "SELECT c.log_id, COALESCE(m.played_at, 0) AS played_at, c.kind, c.team_id, c.team_name,
                    {RESULT_SQL} AS result,
                    (SELECT MAX(r.rating) FROM sixes_rating r
                      WHERE r.log_id = c.log_id AND r.account_id = ?1 AND r.model_version = ?2
                        AND r.role NOT LIKE 'off:%') AS score
             FROM match_context c
             JOIN match m        ON m.log_id = c.log_id
             JOIN match_player p ON p.log_id = c.log_id AND p.account_id = ?1
             WHERE {}
             ORDER BY m.played_at, c.log_id",
            crate::sixes_only("c.log_id")
        ))
        .bind(me as i64)
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| OwnGameRow {
                log_id: r.get("log_id"),
                played_at: r.get("played_at"),
                kind: r.get("kind"),
                team_id: r.get("team_id"),
                team_name: r.get("team_name"),
                result: r.get("result"),
                score: r.get("score"),
            })
            .collect())
    }

    /// The roles any player has rated 6v6 games in, the owner's logs and the
    /// league sample's together, most played first (ETF2L › 6v6 › Players).
    pub async fn sixes_player_roles(&self, account_id: u32, version: &str) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT r.role, COUNT(DISTINCT r.log_id) AS n
             FROM sixes_rating r
             WHERE r.account_id = ?1 AND r.model_version = ?2
               AND (r.log_id IN (SELECT log_id FROM match) OR r.log_id IN (SELECT log_id FROM sixes_league_log WHERE json IS NOT NULL))
             GROUP BY r.role ORDER BY n DESC, r.role",
        )
        .bind(account_id as i64)
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get("role"), r.get("n"))).collect())
    }

    /// [`Db::sixes_rating_history`] for the league sample's logs (those not
    /// also the owner's): the side and the score read from the log's JSON,
    /// every one an official.
    pub async fn sixes_league_history(&self, account_id: u32, role: &str, version: &str) -> Result<Vec<HistoryDbRow>> {
        let rows = sqlx::query(
            "SELECT r.log_id, l.played_at, l.map, l.title,
                    -- Older logs key players by STEAM_0:X:Y, not [U:1:N] (audit M2).
                    COALESCE(json_extract(l.json, '$.players.\"[U:1:' || ?1 || ']\".team'),
                             json_extract(l.json, '$.players.\"STEAM_0:' || (?1 % 2) || ':' || (?1 / 2) || '\".team'), '') AS team,
                    json_extract(l.json, '$.teams.Red.score') AS red_score,
                    json_extract(l.json, '$.teams.Blue.score') AS blue_score,
                    r.rating AS score, r.minutes, r.parts
             FROM sixes_rating r
             JOIN sixes_league_log l ON l.log_id = r.log_id
             WHERE r.account_id = ?1 AND r.role = ?2 AND r.model_version = ?3
               AND l.json IS NOT NULL AND r.log_id NOT IN (SELECT log_id FROM match)",
        )
        .bind(account_id as i64)
        .bind(role)
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| HistoryDbRow {
                log_id: r.get("log_id"),
                played_at: r.get("played_at"),
                map: r.get("map"),
                title: r.get("title"),
                league: Some("etf2l".to_string()),
                team: r.get("team"),
                kind: Some("official".to_string()),
                red_score: r.get("red_score"),
                blue_score: r.get("blue_score"),
                score: r.get("score"),
                minutes: r.get("minutes"),
                parts_json: r.get("parts"),
            })
            .collect())
    }

    /// The logs.tf JSON of these 6v6 logs, the owner's and the league
    /// sample's: a player's headline numbers are read from it.
    pub async fn sixes_jsons_of(&self, logs: &[i64]) -> Result<Vec<(i64, String)>> {
        let ids = serde_json::to_string(logs)?;
        Ok(sqlx::query_as(
            "SELECT log_id, json FROM log_raw WHERE log_id IN (SELECT value FROM json_each(?1))
             UNION ALL
             SELECT log_id, json FROM sixes_league_log WHERE json IS NOT NULL AND log_id IN (SELECT value FROM json_each(?1))
               AND log_id NOT IN (SELECT log_id FROM log_raw)",
        )
        .bind(ids)
        .fetch_all(self.pool())
        .await?)
    }

    /// Every 6v6-rated player's main-role games: `(account, role, games,
    /// average)`, for the 6v6 player search.
    pub async fn sixes_rated_players(&self) -> Result<Vec<(u32, String, i64, f64)>> {
        let rows: Vec<(i64, String, i64, f64)> = sqlx::query_as(
            "SELECT account_id, role, COUNT(*), AVG(rating) FROM sixes_rating WHERE role NOT LIKE 'off:%' GROUP BY account_id, role",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(a, r, g, v)| (a as u32, r, g, v)).collect())
    }

    /// Everyone on an ETF2L 6v6 lineup the app holds.
    pub async fn sixes_roster_accounts(&self) -> Result<Vec<u32>> {
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT p.account_id FROM etf2l_season_player p
             JOIN etf2l_season_match m ON m.match_id = p.match_id
             JOIN etf2l_competition c ON c.competition_id = m.competition_id
             WHERE c.format = 'sixes'",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|a| a as u32).collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::Db;

    #[tokio::test]
    async fn a_league_game_s_side_and_score_come_from_its_json() {
        let db = Db::connect_in_memory().await.unwrap();
        let json = r#"{"teams":{"Red":{"score":4},"Blue":{"score":2}},"players":{"[U:1:42]":{"team":"Red"}}}"#;
        sqlx::query("INSERT INTO sixes_league_log (log_id, etf2l_match_id, map, played_at, json) VALUES (9, 1, 'cp_process_f12', 100, ?1)")
            .bind(json)
            .execute(db.pool())
            .await
            .unwrap();
        db.replace_sixes_ratings("s0", &[(9, 42, "pocket".to_string(), 1.2, 60.0, 20.0, "[]".to_string())]).await.unwrap();
        let rows = db.sixes_league_history(42, "pocket", "s0").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].team.as_str(), rows[0].red_score, rows[0].blue_score, rows[0].kind.as_deref()), ("Red", Some(4), Some(2), Some("official")));
        assert_eq!(db.sixes_player_roles(42, "s0").await.unwrap(), vec![("pocket".to_string(), 1)]);
        assert_eq!(db.sixes_jsons_of(&[9]).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_older_log_s_steam_0_key_still_gives_the_side() {
        let db = Db::connect_in_memory().await.unwrap();
        // Account 43 is STEAM_0:1:21.
        let json = r#"{"teams":{"Red":{"score":1},"Blue":{"score":3}},"players":{"STEAM_0:1:21":{"team":"Blue"}}}"#;
        sqlx::query("INSERT INTO sixes_league_log (log_id, etf2l_match_id, map, played_at, json) VALUES (9, 1, 'cp_badlands', 100, ?1)")
            .bind(json)
            .execute(db.pool())
            .await
            .unwrap();
        db.replace_sixes_ratings("s1", &[(9, 43, "medic".to_string(), 1.0, 50.0, 20.0, "[]".to_string())]).await.unwrap();
        let rows = db.sixes_league_history(43, "medic", "s1").await.unwrap();
        assert_eq!(rows[0].team, "Blue");
    }
}
