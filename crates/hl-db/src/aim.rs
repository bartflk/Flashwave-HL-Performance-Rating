//! Aim read from demos (PLAN §14): stored per kill, rebuilt by the aim pass.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;
use std::collections::HashSet;

/// One kill's aim, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimRow {
    pub demo_id: i64,
    pub tick: i64,
    pub at_raw: Option<i64>,
    pub victim: Option<u32>,
    pub error_deg: f64,
    pub before_deg: f64,
    pub flick_deg: f64,
    pub range_units: f64,
    pub height: f64,
    pub victim_seen: bool,
    pub headshot: bool,
}

/// Averages over the kills a demo could answer for.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimTotals {
    pub kills: i64,
    pub error_deg: f64,
    pub before_deg: f64,
    pub flick_deg: f64,
    pub range_units: f64,
    /// Kills where the crosshair was already within 3 degrees a second before.
    pub held_share: f64,
}

impl Db {
    /// Replace one log's aim rows and mark it read at `version`.
    pub async fn replace_aim(&self, log_id: i64, version: i64, rows: &[AimRow]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM demo_aim WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        for r in rows {
            sqlx::query(
                "INSERT OR REPLACE INTO demo_aim
                    (log_id, demo_id, tick, at_raw, victim, error_deg, before_deg, flick_deg,
                     range_units, height, victim_seen, headshot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )
            .bind(log_id)
            .bind(r.demo_id)
            .bind(r.tick)
            .bind(r.at_raw)
            .bind(r.victim.map(i64::from))
            .bind(r.error_deg)
            .bind(r.before_deg)
            .bind(r.flick_deg)
            .bind(r.range_units)
            .bind(r.height)
            .bind(i64::from(r.victim_seen))
            .bind(i64::from(r.headshot))
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("INSERT OR REPLACE INTO aim_log (log_id, version, kills) VALUES (?1, ?2, ?3)")
            .bind(log_id)
            .bind(version)
            .bind(rows.len() as i64)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Logs already read at this version of the pass.
    pub async fn aim_logs(&self, version: i64) -> Result<HashSet<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT log_id FROM aim_log WHERE version = ?1")
            .bind(version)
            .fetch_all(self.pool())
            .await?;
        Ok(ids.into_iter().collect())
    }

    /// Logs with a demo linked and a clock to place it: the aim pass's queue.
    pub async fn aim_queue(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT DISTINCT l.log_id
             FROM demo_link l
             JOIN demo d ON d.demo_id = l.demo_id
             JOIN log_clock c ON c.log_id = l.log_id
             WHERE d.start_utc IS NOT NULL AND d.tick_rate IS NOT NULL
             ORDER BY l.log_id DESC",
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn aim_for_log(&self, log_id: i64) -> Result<Vec<AimRow>> {
        let rows = sqlx::query(
            "SELECT demo_id, tick, at_raw, victim, error_deg, before_deg, flick_deg,
                    range_units, height, victim_seen, headshot
             FROM demo_aim WHERE log_id = ?1 ORDER BY tick",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(row).collect())
    }

    /// Averages over every stored kill, or over one log's.
    pub async fn aim_totals(&self, log_id: Option<i64>) -> Result<Option<AimTotals>> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS kills, AVG(error_deg) AS e, AVG(before_deg) AS b,
                    AVG(flick_deg) AS f, AVG(range_units) AS r,
                    AVG(CASE WHEN before_deg <= 3 THEN 1.0 ELSE 0.0 END) AS held
             FROM demo_aim
             WHERE victim_seen = 1 AND (?1 IS NULL OR log_id = ?1)",
        )
        .bind(log_id)
        .fetch_one(self.pool())
        .await?;
        let kills: i64 = row.get("kills");
        Ok((kills > 0).then(|| AimTotals {
            kills,
            error_deg: row.get("e"),
            before_deg: row.get("b"),
            flick_deg: row.get("f"),
            range_units: row.get("r"),
            held_share: row.get("held"),
        }))
    }
}

fn row(r: sqlx::sqlite::SqliteRow) -> AimRow {
    AimRow {
        demo_id: r.get("demo_id"),
        tick: r.get("tick"),
        at_raw: r.get("at_raw"),
        victim: r.get::<Option<i64>, _>("victim").map(|v| v as u32),
        error_deg: r.get("error_deg"),
        before_deg: r.get("before_deg"),
        flick_deg: r.get("flick_deg"),
        range_units: r.get("range_units"),
        height: r.get("height"),
        victim_seen: r.get::<i64, _>("victim_seen") != 0,
        headshot: r.get::<i64, _>("headshot") != 0,
    }
}
