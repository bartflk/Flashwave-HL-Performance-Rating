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
}
