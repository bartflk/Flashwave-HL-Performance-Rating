//! Every player's numbers in every log (`log_stat`, `log_weapon`): the
//! season rankings page, the profile panels and the season awards.

use crate::{Db, Result};
use serde::Serialize;

/// One player, one class, one log. See `0041_log_stat.sql`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LogStat {
    pub log_id: i64,
    pub account_id: i64,
    pub class: String,
    pub main: bool,
    pub time_s: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub dmg: i64,
    pub dt: i64,
    pub heal: i64,
    pub ubers: i64,
    pub drops: i64,
    pub airshots: i64,
    pub headshots: i64,
    pub backstabs: i64,
    pub caps: i64,
    pub medkits: i64,
    pub medkits_hp: i64,
    pub longest_streak: i64,
    pub adv_lost: i64,
    pub build_s: Option<f64>,
    pub use_s: Option<f64>,
    pub opening_kills: Option<i64>,
    pub opening_deaths: Option<i64>,
    pub first_picks: Option<i64>,
    pub trade_kills: Option<i64>,
    pub traded_deaths: Option<i64>,
    pub cleanup_kills: Option<i64>,
    pub forces: Option<i64>,
    pub fights_present: Option<i64>,
    pub fights_kast: Option<i64>,
    pub impact: Option<f64>,
    pub impact_attack: Option<f64>,
    pub impact_defence: Option<f64>,
    pub kills_attack: Option<i64>,
    pub deaths_attack: Option<i64>,
    pub kills_defence: Option<i64>,
    pub deaths_defence: Option<i64>,
    pub secs_attack: Option<i64>,
    pub secs_defence: Option<i64>,
    pub down_kills: Option<i64>,
    pub multi2: Option<i64>,
    pub multi3: Option<i64>,
    /// Sentries and all buildings destroyed on this class, from the server
    /// log; `None` without one.
    pub sentry_kills: Option<i64>,
    pub building_kills: Option<i64>,
}

/// Kills and damage with one weapon, one class, one log.
#[derive(Debug, Clone, Default, PartialEq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LogWeapon {
    pub log_id: i64,
    pub account_id: i64,
    pub class: String,
    pub weapon: String,
    pub kills: i64,
    pub dmg: i64,
    pub shots: i64,
    pub hits: i64,
}

/// `log_stat`'s columns, in the struct's order.
pub const LOG_STAT_COLUMNS: &[&str] = &[
    "log_id", "account_id", "class", "main", "time_s", "kills", "deaths", "assists", "dmg", "dt", "heal", "ubers", "drops",
    "airshots", "headshots", "backstabs", "caps", "medkits", "medkits_hp", "longest_streak", "adv_lost", "build_s", "use_s",
    "opening_kills", "opening_deaths", "first_picks", "trade_kills", "traded_deaths", "cleanup_kills", "forces",
    "fights_present", "fights_kast", "impact", "impact_attack", "impact_defence", "kills_attack", "deaths_attack",
    "kills_defence", "deaths_defence", "secs_attack", "secs_defence", "down_kills", "multi2", "multi3",
    "sentry_kills", "building_kills",
];

impl Db {
    /// One log's rows, replacing whatever it had.
    pub async fn put_log_stats(&self, log_id: i64, stats: &[LogStat], weapons: &[LogWeapon]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM log_stat WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM log_weapon WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        let sql = format!(
            "INSERT OR REPLACE INTO log_stat ({}) VALUES ({})",
            LOG_STAT_COLUMNS.join(", "),
            (1..=LOG_STAT_COLUMNS.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ")
        );
        for s in stats {
            sqlx::query(&sql)
                .bind(s.log_id)
                .bind(s.account_id)
                .bind(&s.class)
                .bind(s.main)
                .bind(s.time_s)
                .bind(s.kills)
                .bind(s.deaths)
                .bind(s.assists)
                .bind(s.dmg)
                .bind(s.dt)
                .bind(s.heal)
                .bind(s.ubers)
                .bind(s.drops)
                .bind(s.airshots)
                .bind(s.headshots)
                .bind(s.backstabs)
                .bind(s.caps)
                .bind(s.medkits)
                .bind(s.medkits_hp)
                .bind(s.longest_streak)
                .bind(s.adv_lost)
                .bind(s.build_s)
                .bind(s.use_s)
                .bind(s.opening_kills)
                .bind(s.opening_deaths)
                .bind(s.first_picks)
                .bind(s.trade_kills)
                .bind(s.traded_deaths)
                .bind(s.cleanup_kills)
                .bind(s.forces)
                .bind(s.fights_present)
                .bind(s.fights_kast)
                .bind(s.impact)
                .bind(s.impact_attack)
                .bind(s.impact_defence)
                .bind(s.kills_attack)
                .bind(s.deaths_attack)
                .bind(s.kills_defence)
                .bind(s.deaths_defence)
                .bind(s.secs_attack)
                .bind(s.secs_defence)
                .bind(s.down_kills)
                .bind(s.multi2)
                .bind(s.multi3)
                .bind(s.sentry_kills)
                .bind(s.building_kills)
                .execute(&mut *tx)
                .await?;
        }
        for w in weapons {
            sqlx::query(
                "INSERT OR REPLACE INTO log_weapon (log_id, account_id, class, weapon, kills, dmg, shots, hits)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(w.log_id)
            .bind(w.account_id)
            .bind(&w.class)
            .bind(&w.weapon)
            .bind(w.kills)
            .bind(w.dmg)
            .bind(w.shots)
            .bind(w.hits)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Logs whose rows were written before buildings were counted and that
    /// have a server log to count them from: read again for them alone.
    pub async fn logs_without_building_kills(&self, limit: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT DISTINCT s.log_id FROM log_stat s
             WHERE s.sentry_kills IS NULL
               AND (EXISTS (SELECT 1 FROM rawlog r WHERE r.log_id = s.log_id)
                    OR EXISTS (SELECT 1 FROM league_rawlog r WHERE r.log_id = s.log_id))
             ORDER BY s.log_id DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// One log's building kills, `(account, class, sentries, buildings)`;
    /// every other row of the log gets none.
    pub async fn put_building_kills(&self, log_id: i64, rows: &[(i64, String, i64, i64)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE log_stat SET sentry_kills = 0, building_kills = 0 WHERE log_id = ?1")
            .bind(log_id)
            .execute(&mut *tx)
            .await?;
        for (account, class, sentries, buildings) in rows {
            sqlx::query("UPDATE log_stat SET sentry_kills = ?3, building_kills = ?4 WHERE log_id = ?1 AND account_id = ?2 AND class = ?5")
                .bind(log_id)
                .bind(account)
                .bind(sentries)
                .bind(buildings)
                .bind(class)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// One log's sentry kills by account, every class together: the
    /// scoreboard's column.
    pub async fn sentry_kills_of_log(&self, log_id: i64) -> Result<Vec<(i64, i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT account_id, SUM(sentry_kills), SUM(building_kills) FROM log_stat
             WHERE log_id = ?1 AND sentry_kills IS NOT NULL GROUP BY account_id",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?)
    }

    /// Forgets every row, to read them all again.
    pub async fn clear_log_stats(&self) -> Result<()> {
        sqlx::query("DELETE FROM log_stat").execute(self.pool()).await?;
        sqlx::query("DELETE FROM log_weapon").execute(self.pool()).await?;
        Ok(())
    }

    /// Every row, or one account's.
    pub async fn log_stats(&self, account: Option<u32>) -> Result<Vec<LogStat>> {
        Ok(sqlx::query_as::<_, LogStat>(&format!(
            "SELECT {} FROM log_stat WHERE ?1 IS NULL OR account_id = ?1",
            LOG_STAT_COLUMNS.join(", ")
        ))
        .bind(account.map(i64::from))
        .fetch_all(self.pool())
        .await?)
    }

    /// Each player's round impact per log, from the main-class rows:
    /// `log -> account -> impact`. One log, or every log.
    pub async fn round_impacts(&self, log: Option<i64>) -> Result<std::collections::HashMap<i64, std::collections::HashMap<u32, f64>>> {
        let rows: Vec<(i64, i64, f64)> = sqlx::query_as(
            "SELECT log_id, account_id, impact FROM log_stat WHERE main = 1 AND impact IS NOT NULL AND (?1 IS NULL OR log_id = ?1)",
        )
        .bind(log)
        .fetch_all(self.pool())
        .await?;
        let mut out: std::collections::HashMap<i64, std::collections::HashMap<u32, f64>> = std::collections::HashMap::new();
        for (l, a, i) in rows {
            out.entry(l).or_default().insert(a as u32, i);
        }
        Ok(out)
    }

    /// Every main-class game of 5 minutes or more: `(class, damage, seconds,
    /// weight, map)`, an official weighing 1, a scrim 0.5 and a pug 0.25
    /// (Flashy: "take the DPM from scrims and pugs less serious"). The map
    /// is the log's own, or none for a combined log or one over several maps.
    pub async fn dpm_sample(&self) -> Result<Vec<(String, i64, i64, f64, Option<String>)>> {
        Ok(sqlx::query_as(
            "SELECT s.class, s.dmg, s.time_s, CASE WHEN EXISTS (SELECT 1 FROM league_log l WHERE l.log_id = s.log_id) THEN 1.0
                  ELSE COALESCE((SELECT CASE c.kind WHEN 'official' THEN 1.0 WHEN 'scrim' THEN 0.5 ELSE 0.25 END
                                 FROM match_context c WHERE c.log_id = s.log_id), 0.5) END,
                    (SELECT COALESCE((SELECT l.map FROM league_log l WHERE l.log_id = s.log_id),
                              (SELECT m.map FROM match m WHERE m.log_id = s.log_id))
              WHERE NOT EXISTS (SELECT 1 FROM log_index pi WHERE pi.superseded_by = s.log_id)
                AND (SELECT COUNT(DISTINCT g.map) FROM log_segment g WHERE g.log_id = s.log_id) <= 1)
             FROM log_stat s WHERE s.main AND s.time_s >= 300",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// One player's main-class games of 5 minutes or more, for placing them
    /// on the stat scales: `(log, class, damage, seconds, shots, hits, map)`,
    /// shots and hits where the log recorded 30 shots or more.
    pub async fn own_stat_lines(&self, account: u32) -> Result<Vec<(i64, String, i64, i64, Option<i64>, Option<i64>, Option<String>)>> {
        Ok(sqlx::query_as(
            "SELECT s.log_id, s.class, s.dmg, s.time_s, a.shots, a.hits,
                    (SELECT COALESCE((SELECT l.map FROM league_log l WHERE l.log_id = s.log_id),
                              (SELECT m.map FROM match m WHERE m.log_id = s.log_id))
              WHERE NOT EXISTS (SELECT 1 FROM log_index pi WHERE pi.superseded_by = s.log_id)
                AND (SELECT COUNT(DISTINCT g.map) FROM log_segment g WHERE g.log_id = s.log_id) <= 1)
             FROM log_stat s
             LEFT JOIN (SELECT w.log_id, w.class, SUM(w.shots) AS shots, SUM(w.hits) AS hits FROM log_weapon w
                        WHERE w.account_id = ?1 GROUP BY w.log_id, w.class HAVING SUM(w.shots) >= 30) a
               ON a.log_id = s.log_id AND a.class = s.class
             WHERE s.account_id = ?1 AND s.main AND s.time_s >= 300",
        )
        .bind(i64::from(account))
        .fetch_all(&self.pool)
        .await?)
    }

    /// The same games' accuracy where the log recorded it, 30 shots or
    /// more: `(class, shots, hits, weight, map)`.
    pub async fn accuracy_sample(&self) -> Result<Vec<(String, i64, i64, f64, Option<String>)>> {
        Ok(sqlx::query_as(
            "SELECT s.class, SUM(w.shots), SUM(w.hits), CASE WHEN EXISTS (SELECT 1 FROM league_log l WHERE l.log_id = s.log_id) THEN 1.0
                  ELSE COALESCE((SELECT CASE c.kind WHEN 'official' THEN 1.0 WHEN 'scrim' THEN 0.5 ELSE 0.25 END
                                 FROM match_context c WHERE c.log_id = s.log_id), 0.5) END,
                    (SELECT COALESCE((SELECT l.map FROM league_log l WHERE l.log_id = s.log_id),
                              (SELECT m.map FROM match m WHERE m.log_id = s.log_id))
              WHERE NOT EXISTS (SELECT 1 FROM log_index pi WHERE pi.superseded_by = s.log_id)
                AND (SELECT COUNT(DISTINCT g.map) FROM log_segment g WHERE g.log_id = s.log_id) <= 1)
             FROM log_stat s JOIN log_weapon w ON w.log_id = s.log_id AND w.account_id = s.account_id AND w.class = s.class
             WHERE s.main AND s.time_s >= 300
             GROUP BY s.log_id, s.account_id, s.class HAVING SUM(w.shots) >= 30",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Every log's kills, damage and seconds by player and class:
    /// `(log, account, class, kills, dmg, secs)`.
    pub async fn log_output(&self) -> Result<Vec<(i64, i64, String, i64, i64, i64)>> {
        Ok(sqlx::query_as("SELECT log_id, account_id, class, kills, dmg, time_s FROM log_stat").fetch_all(&self.pool).await?)
    }

    /// The rows of the given logs.
    pub async fn log_stats_for_logs(&self, logs: &[i64]) -> Result<Vec<LogStat>> {
        let ids = serde_json::to_string(logs)?;
        Ok(sqlx::query_as::<_, LogStat>(&format!(
            "SELECT {} FROM log_stat WHERE log_id IN (SELECT value FROM json_each(?1))",
            LOG_STAT_COLUMNS.join(", ")
        ))
        .bind(ids)
        .fetch_all(self.pool())
        .await?)
    }

    /// Every row on one class: the population a player's numbers on it are
    /// placed against.
    pub async fn log_stats_of_class(&self, class: &str) -> Result<Vec<LogStat>> {
        Ok(sqlx::query_as::<_, LogStat>(&format!("SELECT {} FROM log_stat WHERE class = ?1", LOG_STAT_COLUMNS.join(", ")))
            .bind(class)
            .fetch_all(self.pool())
            .await?)
    }

    /// One log's rows: a match page's table.
    pub async fn log_stats_of(&self, log_id: i64) -> Result<Vec<LogStat>> {
        Ok(sqlx::query_as::<_, LogStat>(&format!("SELECT {} FROM log_stat WHERE log_id = ?1", LOG_STAT_COLUMNS.join(", ")))
            .bind(log_id)
            .fetch_all(self.pool())
            .await?)
    }

    /// One account's weapon rows.
    pub async fn log_weapons(&self, account: u32) -> Result<Vec<LogWeapon>> {
        Ok(sqlx::query_as::<_, LogWeapon>(
            "SELECT log_id, account_id, class, weapon, kills, dmg, shots, hits FROM log_weapon WHERE account_id = ?1",
        )
        .bind(i64::from(account))
        .fetch_all(self.pool())
        .await?)
    }

    /// Logs with their files stored but no `log_stat` rows yet, the
    /// league sample's and the owner's, newest first.
    pub async fn logs_without_stats(&self, limit: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT log_id FROM (
                 SELECT l.log_id, l.played_at FROM league_log l
                 WHERE EXISTS (SELECT 1 FROM league_log_json j WHERE j.log_id = l.log_id)
                 UNION
                 SELECT r.log_id, COALESCE(m.played_at, 0) FROM log_raw r LEFT JOIN match m ON m.log_id = r.log_id
             ) x
             WHERE NOT EXISTS (SELECT 1 FROM log_stat s WHERE s.log_id = x.log_id)
             ORDER BY played_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }
}
