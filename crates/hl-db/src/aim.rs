//! Aim read from demos (PLAN §14): stored per kill, rebuilt by the aim pass.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;
use std::collections::HashSet;

/// Which matches an aim figure covers: one log, or every log matching a
/// class, kind and period. `None` everywhere means every match read.
#[derive(Debug, Clone, Default)]
pub struct AimFilter<'a> {
    /// The owner's account id: `class` is their main class in the match.
    pub me: u32,
    pub log_id: Option<i64>,
    /// The owner's main class in the match, e.g. `sniper`.
    pub class: Option<&'a str>,
    /// `official`, `scrim` or `pug`.
    pub kind: Option<&'a str>,
    /// Played between these, unix seconds.
    pub from: Option<i64>,
    pub to: Option<i64>,
}

/// The rows of `table` this filter covers, as a `WHERE` clause over `?1..?5`.
/// Every filter is a parameter, so nothing is ever pasted into SQL.
fn scope(table: &str) -> String {
    format!(
        "WHERE (?1 IS NULL OR {table}.log_id = ?1)
           AND (?2 IS NULL OR EXISTS (SELECT 1 FROM match_player mp
                                      WHERE mp.log_id = {table}.log_id AND mp.account_id = ?6
                                        AND mp.main_class = ?2))
           AND (?3 IS NULL OR EXISTS (SELECT 1 FROM match_context c
                                      WHERE c.log_id = {table}.log_id AND c.kind = ?3))
           AND (?4 IS NULL OR EXISTS (SELECT 1 FROM match m WHERE m.log_id = {table}.log_id AND m.played_at >= ?4))
           AND (?5 IS NULL OR EXISTS (SELECT 1 FROM match m WHERE m.log_id = {table}.log_id AND m.played_at <= ?5))"
    )
}

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
    /// The miss split in two, degrees: positive is right of the head, and
    /// above it. At the shot, and a second before.
    pub dx_deg: f64,
    pub dy_deg: f64,
    pub before_dx_deg: f64,
    pub before_dy_deg: f64,
    pub flick_deg: f64,
    pub range_units: f64,
    pub height: f64,
    pub victim_seen: bool,
    pub headshot: bool,
}

/// One death, as the demo saw it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeathRow {
    pub demo_id: i64,
    pub tick: i64,
    pub at_raw: Option<i64>,
    pub killer: Option<u32>,
    pub killer_range: Option<f64>,
    pub nearest_mate: Option<f64>,
    pub mates_near: i64,
    pub scoped: bool,
}

/// How the living time was spent, over one match or all of them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeTotals {
    /// Share of living time spent scoped in.
    pub scoped_share: f64,
    pub minutes: f64,
    pub deaths: i64,
    /// Averages over deaths where the demo carried a teammate.
    pub nearest_mate: Option<f64>,
    /// Share of deaths with nobody within the near distance, and with the
    /// player scoped at the time.
    pub alone_share: f64,
    pub scoped_share_deaths: f64,
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
    /// Where the crosshair usually sat at the shot: positive is right of the
    /// head, and above it. A steady bias is a sensitivity or habit, not luck.
    pub bias_x: f64,
    pub bias_y: f64,
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
                     range_units, height, victim_seen, headshot, dx_deg, dy_deg, before_dx_deg, before_dy_deg)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
            .bind(r.dx_deg)
            .bind(r.dy_deg)
            .bind(r.before_dx_deg)
            .bind(r.before_dy_deg)
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

    /// Replace one log's deaths and living time.
    pub async fn replace_deaths(&self, log_id: i64, rows: &[DeathRow], life: &[(i64, i64, i64)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM demo_death WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM demo_life WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        for r in rows {
            sqlx::query(
                "INSERT OR REPLACE INTO demo_death
                    (log_id, demo_id, tick, at_raw, killer, killer_range, nearest_mate, mates_near, scoped)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(log_id)
            .bind(r.demo_id)
            .bind(r.tick)
            .bind(r.at_raw)
            .bind(r.killer.map(i64::from))
            .bind(r.killer_range)
            .bind(r.nearest_mate)
            .bind(r.mates_near)
            .bind(i64::from(r.scoped))
            .execute(&mut *tx)
            .await?;
        }
        for &(demo_id, alive, scoped) in life {
            sqlx::query(
                "INSERT OR REPLACE INTO demo_life (log_id, demo_id, alive_ticks, scoped_ticks)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(log_id)
            .bind(demo_id)
            .bind(alive)
            .bind(scoped)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn deaths_for_log(&self, log_id: i64) -> Result<Vec<DeathRow>> {
        let rows = sqlx::query(
            "SELECT demo_id, tick, at_raw, killer, killer_range, nearest_mate, mates_near, scoped
             FROM demo_death WHERE log_id = ?1 ORDER BY tick",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| DeathRow {
                demo_id: r.get("demo_id"),
                tick: r.get("tick"),
                at_raw: r.get("at_raw"),
                killer: r.get::<Option<i64>, _>("killer").map(|v| v as u32),
                killer_range: r.get("killer_range"),
                nearest_mate: r.get("nearest_mate"),
                mates_near: r.get("mates_near"),
                scoped: r.get::<i64, _>("scoped") != 0,
            })
            .collect())
    }

    /// How the living time was spent, over the matches a filter covers.
    /// Ticks are turned into minutes at 66.67 a second, TF2's server rate.
    pub async fn life_totals(&self, f: &AimFilter<'_>) -> Result<Option<LifeTotals>> {
        let life = sqlx::query(&format!(
            "SELECT SUM(alive_ticks) AS alive, SUM(scoped_ticks) AS scoped
             FROM demo_life
             {}",
            scope("demo_life")
        ))
        .bind(f.log_id)
        .bind(f.class)
        .bind(f.kind)
        .bind(f.from)
        .bind(f.to)
        .bind(f.me)
        .fetch_one(self.pool())
        .await?;
        let alive: Option<i64> = life.get("alive");
        let Some(alive) = alive.filter(|a| *a > 0) else { return Ok(None) };
        let scoped: i64 = life.get::<Option<i64>, _>("scoped").unwrap_or(0);

        let d = sqlx::query(&format!(
            "SELECT COUNT(*) AS deaths, AVG(nearest_mate) AS mate,
                    AVG(CASE WHEN mates_near = 0 THEN 1.0 ELSE 0.0 END) AS alone,
                    AVG(CASE WHEN scoped = 1 THEN 1.0 ELSE 0.0 END) AS scoped
             FROM demo_death
             {}",
            scope("demo_death")
        ))
        .bind(f.log_id)
        .bind(f.class)
        .bind(f.kind)
        .bind(f.from)
        .bind(f.to)
        .bind(f.me)
        .fetch_one(self.pool())
        .await?;
        let deaths: i64 = d.get("deaths");
        Ok(Some(LifeTotals {
            scoped_share: scoped as f64 / alive as f64,
            minutes: alive as f64 / 66.67 / 60.0,
            deaths,
            nearest_mate: d.get("mate"),
            alone_share: d.get::<Option<f64>, _>("alone").unwrap_or(0.0),
            scoped_share_deaths: d.get::<Option<f64>, _>("scoped").unwrap_or(0.0),
        }))
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
                    range_units, height, victim_seen, headshot, dx_deg, dy_deg, before_dx_deg, before_dy_deg
             FROM demo_aim WHERE log_id = ?1 ORDER BY tick",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(row).collect())
    }

    /// Averages over the kills a filter covers.
    pub async fn aim_totals(&self, f: &AimFilter<'_>) -> Result<Option<AimTotals>> {
        let row = sqlx::query(&format!(
            "SELECT COUNT(*) AS kills, AVG(error_deg) AS e, AVG(before_deg) AS b,
                    AVG(flick_deg) AS f, AVG(range_units) AS r,
                    AVG(CASE WHEN before_deg <= 3 THEN 1.0 ELSE 0.0 END) AS held,
                    AVG(dx_deg) AS bx, AVG(dy_deg) AS by
             FROM demo_aim
             {} AND demo_aim.victim_seen = 1",
            scope("demo_aim")
        ))
        .bind(f.log_id)
        .bind(f.class)
        .bind(f.kind)
        .bind(f.from)
        .bind(f.to)
        .bind(f.me)
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
            bias_x: row.get::<Option<f64>, _>("bx").unwrap_or(0.0),
            bias_y: row.get::<Option<f64>, _>("by").unwrap_or(0.0),
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
        dx_deg: r.get("dx_deg"),
        dy_deg: r.get("dy_deg"),
        before_dx_deg: r.get("before_dx_deg"),
        before_dy_deg: r.get("before_dy_deg"),
        flick_deg: r.get("flick_deg"),
        range_units: r.get("range_units"),
        height: r.get("height"),
        victim_seen: r.get::<i64, _>("victim_seen") != 0,
        headshot: r.get::<i64, _>("headshot") != 0,
    }
}
