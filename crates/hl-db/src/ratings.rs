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
        rows: &[(String, String, Option<String>, Vec<f64>)],
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM baseline WHERE model_version = ?1")
            .bind(version)
            .execute(&mut *tx)
            .await?;
        for (class, component, map, values) in rows {
            sqlx::query(
                "INSERT INTO baseline (model_version, class, component, map, n, sorted_values)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(version)
            .bind(class)
            .bind(component)
            .bind(map.as_deref().unwrap_or(""))
            .bind(values.len() as i64)
            .bind(serde_json::to_string(values)?)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn load_baselines(
        &self,
        version: &str,
    ) -> Result<Vec<(String, String, Option<String>, Vec<f64>)>> {
        let rows = sqlx::query(
            "SELECT class, component, map, sorted_values FROM baseline WHERE model_version = ?1",
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
                    // '' is the pool of every map, stored that way because a
                    // primary key column cannot be NULL.
                    Some(r.get::<String, _>("map")).filter(|m| !m.is_empty()),
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

    /// Store the pool's middle and spread, which turn a weighted percentile
    /// into a rating around 1.00.
    pub async fn replace_rating_scale(&self, version: &str, mean: f64, sd: f64, n: usize) -> Result<()> {
        sqlx::query(
            "INSERT INTO rating_scale (model_version, mean, sd, n, made_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(model_version) DO UPDATE SET
                mean = excluded.mean, sd = excluded.sd, n = excluded.n, made_at = excluded.made_at",
        )
        .bind(version)
        .bind(mean)
        .bind(sd)
        .bind(n as i64)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The stored `(mean, sd)`, or `None` before the first full rating pass.
    pub async fn rating_scale(&self, version: &str) -> Result<Option<(f64, f64)>> {
        Ok(sqlx::query_as("SELECT mean, sd FROM rating_scale WHERE model_version = ?1")
            .bind(version)
            .fetch_optional(self.pool())
            .await?)
    }

    /// Replace one log's ratings, leaving every other log alone.
    ///
    /// For rating a log the moment it arrives, mid sync: `replace_ratings`
    /// clears the whole model version, which would empty the match list every
    /// time a single match landed.
    pub async fn put_ratings_for_log(
        &self,
        version: &str,
        log_id: i64,
        rows: &[RatingRow<'_>],
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM rating WHERE model_version = ?1 AND log_id = ?2")
            .bind(version)
            .bind(log_id)
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
    /// For every player of every kept, decided (not tied) match: did their
    /// team win, and when was it played. Keyed by `(log_id, account_id)`.
    pub async fn decided_results(&self) -> Result<std::collections::HashMap<(i64, u32), (bool, Option<i64>)>> {
        let rows = sqlx::query(
            "SELECT p.log_id, p.account_id, p.team, m.red_score, m.blue_score, m.played_at
             FROM match_player p
             JOIN match m     ON m.log_id = p.log_id
             JOIN log_index i ON i.log_id = p.log_id
             WHERE i.superseded_by IS NULL AND m.red_score IS NOT NULL AND m.blue_score IS NOT NULL
               AND m.red_score != m.blue_score",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let (red, blue): (i64, i64) = (r.get("red_score"), r.get("blue_score"));
                let team: String = r.get("team");
                let key = (r.get::<i64, _>("log_id"), r.get::<i64, _>("account_id") as u32);
                (key, ((red > blue) == (team == "Red"), r.get("played_at")))
            })
            .collect())
    }

    /// Ratings stored for a model version: zero after the model changes.
    pub async fn rating_count(&self, version: &str) -> Result<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM rating WHERE model_version = ?1").bind(version).fetch_one(self.pool()).await?)
    }

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

    /// For each of the owner's games on a class, the opposite number and how
    /// they rate over their *other* games (Q9).
    ///
    /// Highlander puts exactly one of each class a side, so "the opponent" is
    /// well defined without any guessing: the player of the same class on the
    /// other team. Their own game here is left out of their average, or every
    /// player's strength would include the match being judged by it.
    ///
    /// Returns `(log_id, opponent account, their average elsewhere, how many
    /// games that average is over)`.
    pub async fn class_opponents(
        &self,
        account_id: u32,
        class: &str,
        version: &str,
    ) -> Result<Vec<(i64, u32, f64, i64)>> {
        let rows = sqlx::query(
            "WITH mine AS (
                 SELECT r.log_id, p.team
                 FROM rating r
                 JOIN match_player p ON p.log_id = r.log_id AND p.account_id = r.account_id
                 WHERE r.account_id = ?1 AND r.class = ?2 AND r.model_version = ?3
             ),
             opp AS (
                 SELECT m.log_id, r.account_id AS opp_id
                 FROM mine m
                 JOIN match_player p ON p.log_id = m.log_id AND p.team <> m.team
                 JOIN rating r ON r.log_id = m.log_id AND r.account_id = p.account_id
                                AND r.class = ?2 AND r.model_version = ?3
             )
             SELECT o.log_id, o.opp_id,
                    (SELECT AVG(x.score) FROM rating x
                      WHERE x.account_id = o.opp_id AND x.class = ?2
                        AND x.model_version = ?3 AND x.log_id <> o.log_id) AS strength,
                    (SELECT COUNT(*) FROM rating x
                      WHERE x.account_id = o.opp_id AND x.class = ?2
                        AND x.model_version = ?3 AND x.log_id <> o.log_id) AS games
             FROM opp o",
        )
        .bind(account_id)
        .bind(class)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A baseline is a sorted pool of floats, and a rating is a value's place
    /// in it. One bit of drift turns a tie into a near miss and moves a
    /// percentile by half a rank, so stored and in-memory pools must be the
    /// same numbers, not nearly the same.
    ///
    /// This holds only because `serde_json` is built with `float_roundtrip`:
    /// its default parser is not correctly rounded. The value below is a real
    /// one from a medic-picks pool, and it is off by one bit without it.
    #[tokio::test]
    async fn a_stored_baseline_comes_back_bit_for_bit() {
        let db = Db::connect_in_memory().await.unwrap();
        let values = vec![0.0, 0.996_677_740_863_787_5, 1.0 / 3.0, f64::MAX];
        db.replace_baselines("v-test", &[("sniper".into(), "medic_picks".into(), None, values.clone())])
            .await
            .unwrap();

        let back = db.load_baselines("v-test").await.unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].3, values, "stored as text, read back as the same floats");
        assert_eq!(back[0].2, None, "the general pool, not a map's");
    }
}
