//! 6v6 roles per log (the 6v6 plan, P3; table `player_role`, 0048).

use crate::{Db, Result};
use sqlx::Row;

/// One stored role: `role` as `hl_core::Role::key` writes it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRole {
    pub account_id: u32,
    pub role: String,
    pub method: String,
    pub confidence: Option<f64>,
    pub seconds: Option<i64>,
    pub by_hand: bool,
}

impl Db {
    /// Replace a log's worked-out roles; a role set by hand stays, and
    /// a worked-out one for the same player and class is not added beside it.
    pub async fn put_roles(&self, log_id: i64, rows: &[(u32, String, &str, Option<f64>, i64)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM player_role WHERE log_id = ?1 AND by_hand = 0").bind(log_id).execute(&mut *tx).await?;
        for (account, role, method, confidence, seconds) in rows {
            sqlx::query(
                "INSERT INTO player_role (log_id, account_id, role, method, confidence, seconds)
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6
                  WHERE NOT EXISTS (SELECT 1 FROM player_role h WHERE h.log_id = ?1 AND h.account_id = ?2 AND h.by_hand = 1
                                      AND (h.role = ?3 OR (h.role NOT LIKE 'off:%' AND ?3 NOT LIKE 'off:%')))",
            )
            .bind(log_id)
            .bind(i64::from(*account))
            .bind(role)
            .bind(*method)
            .bind(*confidence)
            .bind(*seconds)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// One log's roles.
    pub async fn roles_of_log(&self, log_id: i64) -> Result<Vec<StoredRole>> {
        let rows = sqlx::query("SELECT account_id, role, method, confidence, seconds, by_hand FROM player_role WHERE log_id = ?1 ORDER BY account_id, role")
            .bind(log_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| StoredRole {
                account_id: r.get::<i64, _>("account_id") as u32,
                role: r.get("role"),
                method: r.get("method"),
                confidence: r.get("confidence"),
                seconds: r.get("seconds"),
                by_hand: r.get::<i64, _>("by_hand") != 0,
            })
            .collect())
    }

    /// Stored 6v6 logs with no roles worked out yet, newest first, with
    /// their logs.tf JSON.
    pub async fn sixes_logs_without_roles(&self, limit: i64) -> Result<Vec<(i64, String)>> {
        Ok(sqlx::query_as(
            "SELECT r.log_id, r.json FROM log_raw r JOIN log_index i ON i.log_id = r.log_id
              WHERE COALESCE(i.format_override, i.format) = 'sixes' AND i.superseded_by IS NULL
                AND NOT EXISTS (SELECT 1 FROM player_role p WHERE p.log_id = r.log_id AND p.by_hand = 0)
              ORDER BY i.played_at DESC, r.log_id DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }
}

/// One stored 6v6 preview rating (the 6v6 plan, P6).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SixesRatingRow {
    pub log_id: i64,
    pub account_id: u32,
    pub role: String,
    pub rating: f64,
    pub score: f64,
    pub minutes: f64,
    /// JSON: `[component, percentile, share]`.
    pub parts: String,
}

impl Db {
    /// Every stored 6v6 log's logs.tf JSON, for the rating pass.
    pub async fn sixes_log_jsons(&self) -> Result<Vec<(i64, String)>> {
        Ok(sqlx::query_as(
            "SELECT r.log_id, r.json FROM log_raw r JOIN log_index i ON i.log_id = r.log_id
              WHERE COALESCE(i.format_override, i.format) = 'sixes' AND i.superseded_by IS NULL ORDER BY r.log_id",
        )
        .fetch_all(self.pool())
        .await?)
    }

    /// Replace a model version's 6v6 ratings: `(log, account, role, rating, score, minutes, parts)`.
    pub async fn replace_sixes_ratings(&self, version: &str, rows: &[(i64, u32, String, f64, f64, f64, String)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM sixes_rating WHERE model_version = ?1").bind(version).execute(&mut *tx).await?;
        for (log, account, role, rating, score, minutes, parts) in rows {
            sqlx::query(
                "INSERT INTO sixes_rating (log_id, account_id, role, model_version, rating, score, minutes, parts) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(log)
            .bind(i64::from(*account))
            .bind(role)
            .bind(version)
            .bind(rating)
            .bind(score)
            .bind(minutes)
            .bind(parts)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The best players in one 6v6 role or off-class by their average
    /// rating, with at least `min_games` rated: `(account, games, average,
    /// best)`, best first (the 6v6 plan, P7: the Players board).
    pub async fn sixes_board(&self, version: &str, role: &str, min_games: i64, limit: i64) -> Result<Vec<(u32, i64, f64, f64)>> {
        let rows: Vec<(i64, i64, f64, f64)> = sqlx::query_as(
            "SELECT account_id, COUNT(*), AVG(rating), MAX(rating) FROM sixes_rating
              WHERE model_version = ?1 AND role = ?2
              GROUP BY account_id HAVING COUNT(*) >= ?3
              ORDER BY AVG(rating) DESC, COUNT(*) DESC, account_id LIMIT ?4",
        )
        .bind(version)
        .bind(role)
        .bind(min_games)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(a, n, avg, best)| (a as u32, n, avg, best)).collect())
    }

    /// Each player's main-role 6v6 ratings in one 6v6 season, by division:
    /// `(division, account, role, games, average)`, the owner's logs and the
    /// league sample's, placed by the ETF2L match each belongs to.
    pub async fn sixes_season_ratings(&self, version: &str, season: i64) -> Result<Vec<(String, u32, String, i64, f64)>> {
        let rows: Vec<(String, i64, String, i64, f64)> = sqlx::query_as(
            "SELECT m.division, r.account_id, r.role, COUNT(*), AVG(r.rating)
               FROM sixes_rating r
               JOIN (SELECT log_id, etf2l_match_id FROM sixes_league_log
                     UNION SELECT log_id, etf2l_match_id FROM log_index WHERE etf2l_match_id IS NOT NULL) l ON l.log_id = r.log_id
               JOIN etf2l_season_match m ON m.match_id = l.etf2l_match_id
               JOIN etf2l_competition c ON c.competition_id = m.competition_id
              WHERE c.format = 'sixes' AND c.season = ?2 AND r.model_version = ?1 AND r.role NOT LIKE 'off:%'
                AND m.division IS NOT NULL AND m.division != ''
              GROUP BY m.division, r.account_id, r.role",
        )
        .bind(version)
        .bind(season)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(d, a, role, n, avg)| (d, a as u32, role, n, avg)).collect())
    }

    /// One log's 6v6 ratings, or one player's, of a model version.
    pub async fn sixes_ratings(&self, version: &str, log_id: Option<i64>, account: Option<u32>) -> Result<Vec<SixesRatingRow>> {
        let rows = sqlx::query(
            "SELECT log_id, account_id, role, rating, score, minutes, parts FROM sixes_rating
              WHERE model_version = ?1 AND (?2 IS NULL OR log_id = ?2) AND (?3 IS NULL OR account_id = ?3)
              ORDER BY log_id, account_id, role",
        )
        .bind(version)
        .bind(log_id)
        .bind(account.map(i64::from))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SixesRatingRow {
                log_id: r.get("log_id"),
                account_id: r.get::<i64, _>("account_id") as u32,
                role: r.get("role"),
                rating: r.get("rating"),
                score: r.get("score"),
                minutes: r.get("minutes"),
                parts: r.get("parts"),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::Db;

    #[tokio::test]
    async fn the_board_ranks_a_role_by_average_rating_with_enough_games() {
        let db = Db::connect_in_memory().await.unwrap();
        let row = |log: i64, acc: u32, role: &str, r: f64| (log, acc, role.to_string(), r, 50.0, 20.0, "[]".to_string());
        db.replace_sixes_ratings("s0", &[row(1, 7, "pocket", 1.2), row(2, 7, "pocket", 1.0), row(1, 8, "pocket", 1.5), row(2, 9, "pocket", 0.9), row(3, 9, "pocket", 0.9), row(1, 7, "roamer", 2.0)])
            .await
            .unwrap();
        let b = db.sixes_board("s0", "pocket", 2, 10).await.unwrap();
        assert_eq!(b.iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(), vec![(7, 2), (9, 2)], "8 has one game; the roamer game is another role");
        assert!((b[0].2 - 1.1).abs() < 1e-9 && (b[0].3 - 1.2).abs() < 1e-9);
    }

    #[tokio::test]
    async fn a_role_set_by_hand_outlasts_the_pass() {
        let db = Db::connect_in_memory().await.unwrap();
        db.put_roles(1, &[(7, "pocket".into(), "healspread", Some(0.3), 900), (8, "roamer".into(), "healspread", Some(0.3), 900)]).await.unwrap();
        sqlx::query("UPDATE player_role SET role = 'roamer', method = 'hand', by_hand = 1 WHERE account_id = 7").execute(db.pool()).await.unwrap();
        // The pass runs again and still thinks 7 is the pocket: the hand's stays, alone.
        db.put_roles(1, &[(7, "pocket".into(), "healspread", Some(0.3), 900), (7, "off:heavy".into(), "class", None, 60), (8, "roamer".into(), "healspread", Some(0.3), 900)]).await.unwrap();
        let r: Vec<(u32, String, bool)> = db.roles_of_log(1).await.unwrap().into_iter().map(|x| (x.account_id, x.role, x.by_hand)).collect();
        assert_eq!(r, vec![(7, "off:heavy".into(), false), (7, "roamer".into(), true), (8, "roamer".into(), false)]);
    }
}
