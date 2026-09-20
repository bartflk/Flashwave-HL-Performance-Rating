//! Demo files, their links to logs, and log clocks.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;

pub struct DemoRow<'a> {
    pub path: &'a str,
    pub file_name: &'a str,
    pub playdemo_arg: &'a str,
    pub kind: &'a str,
    pub size_bytes: i64,
    pub mtime: i64,
    pub map: &'a str,
    pub server: &'a str,
    pub recorder: &'a str,
    pub playback_s: f64,
    pub ticks: i64,
    pub tick_rate: Option<f64>,
    pub start_utc: Option<f64>,
    pub filename_time: Option<&'a str>,
    pub demos_tf_id: Option<i64>,
    /// `(tick, name, value)` from the sidecar.
    pub events: Vec<(i64, &'a str, Option<&'a str>)>,
}

/// What `log_clock` needs for one kept log with rounds.
pub struct ClockInput {
    pub log_id: i64,
    pub map: Option<String>,
    pub raw_start: i64,
    pub raw_end: i64,
    pub played_at: Option<i64>,
    /// Per-round parts, when this is a combined log.
    pub parts: Vec<i64>,
}

pub struct ClockRow {
    pub log_id: i64,
    pub raw_start: i64,
    pub raw_end: i64,
    pub anchor_utc: i64,
    pub anchor_kind: &'static str,
    pub offset_s: i64,
    pub upload_delay_s: i64,
}

/// A demo linked to a log, with what the match page needs to jump into it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedDemo {
    pub demo_id: i64,
    /// Where the file is on this machine. Not sent to the UI.
    #[serde(skip)]
    pub path: String,
    pub file_name: String,
    pub playdemo_arg: String,
    pub kind: String,
    pub recorder: Option<String>,
    pub size_bytes: i64,
    pub playback_s: f64,
    pub ticks: i64,
    pub tick_rate: Option<f64>,
    pub start_utc: Option<f64>,
    pub filename_time: Option<String>,
    pub method: String,
    pub log_share: f64,
    /// `(tick, name, value)` markers from the sidecar.
    #[serde(skip)]
    pub events: Vec<(i64, String, Option<String>)>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoStats {
    pub demos: i64,
    pub linked: i64,
    pub stv: i64,
    pub markers: i64,
    pub matches_with_demo: i64,
}

impl Db {
    /// Insert or refresh one demo and its markers; returns its id. Ids are
    /// stable across rescans because the path is the key.
    pub async fn upsert_demo(&self, d: &DemoRow<'_>) -> Result<i64> {
        let mut tx = self.pool().begin().await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO demo (path, file_name, playdemo_arg, kind, size_bytes, mtime, map, server,
                recorder, playback_s, ticks, tick_rate, start_utc, filename_time, demos_tf_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(path) DO UPDATE SET
                file_name = excluded.file_name, playdemo_arg = excluded.playdemo_arg,
                kind = excluded.kind, size_bytes = excluded.size_bytes, mtime = excluded.mtime,
                map = excluded.map, server = excluded.server, recorder = excluded.recorder,
                playback_s = excluded.playback_s, ticks = excluded.ticks,
                tick_rate = excluded.tick_rate,
                start_utc = COALESCE(excluded.start_utc, demo.start_utc),
                filename_time = excluded.filename_time,
                demos_tf_id = COALESCE(excluded.demos_tf_id, demo.demos_tf_id),
                indexed_at = datetime('now')
             RETURNING demo_id",
        )
        .bind(d.path)
        .bind(d.file_name)
        .bind(d.playdemo_arg)
        .bind(d.kind)
        .bind(d.size_bytes)
        .bind(d.mtime)
        .bind(d.map)
        .bind(d.server)
        .bind(d.recorder)
        .bind(d.playback_s)
        .bind(d.ticks)
        .bind(d.tick_rate)
        .bind(d.start_utc)
        .bind(d.filename_time)
        .bind(d.demos_tf_id)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM demo_event WHERE demo_id = ?1").bind(id).execute(&mut *tx).await?;
        for (seq, (tick, name, value)) in d.events.iter().enumerate() {
            sqlx::query("INSERT INTO demo_event (demo_id, seq, tick, name, value) VALUES (?1, ?2, ?3, ?4, ?5)")
                .bind(id)
                .bind(seq as i64)
                .bind(tick)
                .bind(name)
                .bind(value)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(id)
    }

    /// Forget demos whose files are gone. Returns how many were removed.
    pub async fn prune_demos(&self, keep_paths: &[String]) -> Result<u64> {
        let existing: Vec<(i64, String)> = sqlx::query("SELECT demo_id, path FROM demo")
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(|r| (r.get("demo_id"), r.get("path")))
            .collect();
        let keep: std::collections::HashSet<&str> = keep_paths.iter().map(String::as_str).collect();
        let mut removed = 0;
        for (id, path) in existing {
            if !keep.contains(path.as_str()) {
                for t in ["demo_link", "demo_event", "demo"] {
                    sqlx::query(&format!("DELETE FROM {t} WHERE demo_id = ?1")).bind(id).execute(self.pool()).await?;
                }
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Every kept log that has rounds, with what its clock needs.
    pub async fn clock_inputs(&self) -> Result<Vec<ClockInput>> {
        let rows = sqlx::query(
            "SELECT m.log_id, m.map, m.played_at, i.duplicate_of,
                    MIN(r.start_time) AS raw_start, MAX(r.start_time + r.length_s) AS raw_end
             FROM match m
             JOIN match_round r ON r.log_id = m.log_id
             JOIN log_index i   ON i.log_id = m.log_id
             WHERE i.superseded_by IS NULL
             GROUP BY m.log_id",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let dup: Option<String> = r.get("duplicate_of");
                Some(ClockInput {
                    log_id: r.get("log_id"),
                    map: r.get("map"),
                    raw_start: r.get::<Option<i64>, _>("raw_start")?,
                    raw_end: r.get::<Option<i64>, _>("raw_end")?,
                    played_at: r.get("played_at"),
                    parts: dup.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                })
            })
            .collect())
    }

    /// Upload times of per-round part logs, as trends.tf reported them.
    pub async fn part_upload_times(&self, parts: &[i64]) -> Result<Vec<i64>> {
        if parts.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; parts.len()].join(",");
        let sql = format!(
            "SELECT played_at FROM log_index WHERE log_id IN ({placeholders}) AND played_at IS NOT NULL"
        );
        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        for p in parts {
            q = q.bind(p);
        }
        Ok(q.fetch_all(self.pool()).await?)
    }

    pub async fn replace_log_clocks(&self, rows: &[ClockRow]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM log_clock").execute(&mut *tx).await?;
        for c in rows {
            sqlx::query(
                "INSERT INTO log_clock (log_id, raw_start, raw_end, anchor_utc, anchor_kind, offset_s, upload_delay_s)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(c.log_id)
            .bind(c.raw_start)
            .bind(c.raw_end)
            .bind(c.anchor_utc)
            .bind(c.anchor_kind)
            .bind(c.offset_s)
            .bind(c.upload_delay_s)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn log_clock_offset(&self, log_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar("SELECT offset_s FROM log_clock WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?)
    }

    /// `(demo_id, map, start_utc, playback_s)` for every demo with a usable span.
    pub async fn demo_spans(&self) -> Result<Vec<(i64, String, f64, f64)>> {
        let rows = sqlx::query(
            "SELECT demo_id, map, start_utc, playback_s FROM demo
             WHERE start_utc IS NOT NULL AND playback_s > 0 AND map IS NOT NULL",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("demo_id"), r.get("map"), r.get("start_utc"), r.get("playback_s")))
            .collect())
    }

    /// Replace every time-matched link. Links made from a demos.tf id are kept:
    /// they are exact by construction and not derived from the clock.
    pub async fn replace_demo_links(&self, links: &[(i64, i64, &str, f64)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM demo_link WHERE method <> 'demos.tf'").execute(&mut *tx).await?;
        for (demo_id, log_id, method, share) in links {
            sqlx::query(
                "INSERT INTO demo_link (demo_id, log_id, method, log_share) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(demo_id, log_id) DO NOTHING",
            )
            .bind(demo_id)
            .bind(log_id)
            .bind(method)
            .bind(share)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn add_demo_link(&self, demo_id: i64, log_id: i64, method: &str, share: f64) -> Result<()> {
        sqlx::query(
            "INSERT INTO demo_link (demo_id, log_id, method, log_share) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(demo_id, log_id) DO UPDATE SET method = excluded.method, log_share = excluded.log_share",
        )
        .bind(demo_id)
        .bind(log_id)
        .bind(method)
        .bind(share)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn demos_for_log(&self, log_id: i64) -> Result<Vec<LinkedDemo>> {
        let rows = sqlx::query(
            "SELECT d.demo_id, d.path, d.file_name, d.playdemo_arg, d.kind, d.recorder, d.size_bytes,
                    d.playback_s, d.ticks, d.tick_rate, d.start_utc, d.filename_time,
                    l.method, l.log_share
             FROM demo_link l JOIN demo d ON d.demo_id = l.demo_id
             WHERE l.log_id = ?1
             ORDER BY d.start_utc",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let demo_id: i64 = r.get("demo_id");
            let events = sqlx::query("SELECT tick, name, value FROM demo_event WHERE demo_id = ?1 ORDER BY seq")
                .bind(demo_id)
                .fetch_all(self.pool())
                .await?
                .into_iter()
                .map(|e| (e.get("tick"), e.get("name"), e.get("value")))
                .collect();
            out.push(LinkedDemo {
                demo_id,
                path: r.get("path"),
                file_name: r.get("file_name"),
                playdemo_arg: r.get("playdemo_arg"),
                kind: r.get("kind"),
                recorder: r.get("recorder"),
                size_bytes: r.get("size_bytes"),
                playback_s: r.get("playback_s"),
                ticks: r.get("ticks"),
                tick_rate: r.get("tick_rate"),
                start_utc: r.get("start_utc"),
                filename_time: r.get("filename_time"),
                method: r.get("method"),
                log_share: r.get("log_share"),
                events,
            });
        }
        Ok(out)
    }

    pub async fn demo_stats(&self) -> Result<DemoStats> {
        let q = |sql: &'static str| async move {
            sqlx::query_scalar::<_, i64>(sql).fetch_one(self.pool()).await
        };
        Ok(DemoStats {
            demos: q("SELECT COUNT(*) FROM demo").await?,
            linked: q("SELECT COUNT(DISTINCT demo_id) FROM demo_link").await?,
            stv: q("SELECT COUNT(*) FROM demo WHERE kind = 'stv'").await?,
            markers: q("SELECT COUNT(*) FROM demo_event").await?,
            matches_with_demo: q("SELECT COUNT(DISTINCT log_id) FROM demo_link").await?,
        })
    }
}
