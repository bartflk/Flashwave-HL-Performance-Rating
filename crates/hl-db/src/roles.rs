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

#[cfg(test)]
mod tests {
    use crate::Db;

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
