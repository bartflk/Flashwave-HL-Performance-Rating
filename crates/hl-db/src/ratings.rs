//! Rating baselines and rated performances.

use crate::Db;
use anyhow::{Context, Result};
use sqlx::Row;

/// One stored rating, as the model serialized it.
pub struct RatingRow<'a> {
    pub log_id: i64,
    pub account_id: u32,
    pub class: &'a str,
    pub score: f64,
    pub minutes: f64,
    pub parts_json: String,
}

/// One rated game in a player's history, with the match context the profile shows.
pub struct HistoryDbRow {
    pub log_id: i64,
    pub played_at: Option<i64>,
    pub map: Option<String>,
    pub title: Option<String>,
    pub league: Option<String>,
    pub team: String,
    /// `official`, `scrim` or `pug`; `None` before the context pass has run.
    pub kind: Option<String>,
    pub red_score: Option<i64>,
    pub blue_score: Option<i64>,
    pub score: f64,
    pub minutes: f64,
    pub parts_json: String,
}

/// Summed class-vs-class record over a set of games.
#[derive(Debug, Default)]
pub struct VsTotals {
    pub kills: i64,
    pub deaths: i64,
}

impl Db {
    /// Replace one model version's baselines in a single transaction.
    pub async fn replace_baselines(
        &self,
        version: &str,
        rows: &[(String, String, Vec<f64>)],
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM baseline WHERE model_version = ?1")
            .bind(version)
            .execute(&mut *tx)
            .await?;
        for (class, component, values) in rows {
            sqlx::query(
                "INSERT INTO baseline (model_version, class, component, n, sorted_values)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(version)
            .bind(class)
            .bind(component)
            .bind(values.len() as i64)
            .bind(serde_json::to_string(values)?)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn load_baselines(&self, version: &str) -> Result<Vec<(String, String, Vec<f64>)>> {
        let rows = sqlx::query(
            "SELECT class, component, sorted_values FROM baseline WHERE model_version = ?1",
        )
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|r| {
                let json: String = r.get("sorted_values");
                Ok((
                    r.get("class"),
                    r.get("component"),
                    serde_json::from_str(&json).context("stored baseline is not a JSON array")?,
                ))
            })
            .collect()
    }

    /// Replace every rating of one model version in a single transaction.
    pub async fn replace_ratings(&self, version: &str, rows: &[RatingRow<'_>]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM rating WHERE model_version = ?1")
            .bind(version)
            .execute(&mut *tx)
            .await?;
        for r in rows {
            sqlx::query(
                "INSERT INTO rating (log_id, account_id, model_version, class, score, minutes, parts)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(r.log_id)
            .bind(r.account_id as i64)
            .bind(version)
            .bind(r.class)
            .bind(r.score)
            .bind(r.minutes)
            .bind(&r.parts_json)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Kept Highlander logs with a stored raw log: the set that gets rated.
    pub async fn rateable_log_ids(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT r.log_id FROM log_raw r
             JOIN log_index i ON i.log_id = r.log_id
             WHERE i.superseded_by IS NULL
               AND COALESCE(i.format_override, i.format) = 'highlander'
             ORDER BY r.log_id",
        )
        .fetch_all(self.pool())
        .await?)
    }

    /// How many rated games a player has, per class, most first.
    pub async fn rated_classes(&self, account_id: u32, version: &str) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT class, COUNT(*) AS n FROM rating
             WHERE account_id = ?1 AND model_version = ?2
             GROUP BY class ORDER BY n DESC",
        )
        .bind(account_id as i64)
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get("class"), r.get("n"))).collect())
    }

    pub async fn rating_history(
        &self,
        account_id: u32,
        class: &str,
        version: &str,
    ) -> Result<Vec<HistoryDbRow>> {
        let rows = sqlx::query(
            "SELECT r.log_id, m.played_at, m.map, m.title, i.league, p.team, c.kind,
                    m.red_score, m.blue_score, r.score, r.minutes, r.parts
             FROM rating r
             JOIN match m        ON m.log_id = r.log_id
             JOIN log_index i    ON i.log_id = r.log_id
             JOIN match_player p ON p.log_id = r.log_id AND p.account_id = r.account_id
             LEFT JOIN match_context c ON c.log_id = r.log_id
             WHERE r.account_id = ?1 AND r.class = ?2 AND r.model_version = ?3
               AND i.superseded_by IS NULL",
        )
        .bind(account_id as i64)
        .bind(class)
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

    /// Kills on, and deaths to, `other_class`, summed over the rated games a
    /// player spent on `class`. For Sniper vs Sniper this is the career duel.
    pub async fn vs_totals(
        &self,
        account_id: u32,
        class: &str,
        other_class: &str,
        version: &str,
    ) -> Result<VsTotals> {
        let row = sqlx::query(
            "SELECT COALESCE(SUM(v.kills), 0) AS kills, COALESCE(SUM(v.deaths), 0) AS deaths
             FROM rating r
             JOIN match_class_vs v ON v.log_id = r.log_id AND v.account_id = r.account_id
             WHERE r.account_id = ?1 AND r.class = ?2 AND r.model_version = ?4
               AND v.other_class = ?3",
        )
        .bind(account_id as i64)
        .bind(class)
        .bind(other_class)
        .bind(version)
        .fetch_one(self.pool())
        .await?;
        Ok(VsTotals { kills: row.get("kills"), deaths: row.get("deaths") })
    }
}
