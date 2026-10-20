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

/// One demo the app downloaded, for the cleanup panel (PLAN Q23).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedDemo {
    pub demo_id: i64,
    pub file_name: String,
    pub size_bytes: i64,
    pub map: Option<String>,
    pub start_utc: Option<f64>,
    /// Matches this demo is linked to, and whether every one of them has
    /// been read at the current pass version.
    pub logs: i64,
    pub read: bool,
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
    /// The file has been deleted to save space (PLAN Q23), but the row and
    /// everything derived from it remain. Only a demo the app downloaded
    /// reaches this state, so it can always be fetched again.
    pub deleted: bool,
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
    /// Demos kept as timelines (Q3), what they take up, and how many of
    /// them no longer have their file.
    pub timelines: i64,
    pub timeline_bytes: i64,
    pub timelines_file_gone: i64,
}

impl Db {
    /// Kept Highlander logs with a map and a time but no demos.tf id:
    /// `(log_id, played_at, map)`. What `demostf::index` goes looking for.
    ///
    /// A combined log's parts too (Q52): each part is one recording, uploaded
    /// seconds after its own log, where the combined log is stamped once and
    /// can hold one id. The combined log's STV is its parts' demos.
    pub async fn logs_without_demo_id(&self) -> Result<Vec<(i64, i64, Vec<String>)>> {
        // Each log's maps as its rounds resolved them, falling back to the
        // map field. A combined log's own field is free text -- "upward +
        // steel" -- and matches nothing.
        let rows: Vec<(i64, i64, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT i.log_id, i.played_at, i.map,
                    (SELECT group_concat(DISTINCT rm.map) FROM round_map rm WHERE rm.log_id = i.log_id)
             FROM log_index i
             WHERE i.demos_tf_id IS NULL
               AND i.played_at IS NOT NULL
               AND ( COALESCE(i.format_override, i.format) = 'highlander'
                  OR (COALESCE(i.format_override, i.format) IS NULL AND COALESCE(i.player_count, 0) >= 16) )
             ORDER BY i.played_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(log_id, at, map, resolved)| {
                let maps: Vec<String> = match resolved {
                    Some(r) if !r.is_empty() => r.split(',').map(str::to_string).collect(),
                    _ => map.filter(|m| !m.is_empty()).into_iter().collect(),
                };
                // A log with no map at all still goes: demos.tf can find its
                // demo by time alone, and the demo names the map (Q30).
                (log_id, at, maps)
            })
            .collect())
    }

    /// Kept logs since `since` that look like several logs in one -- a
    /// combined title, maps joined in the map field, or longer than one map
    /// runs -- with no part known yet: what the sync looks for parts of.
    pub async fn combined_without_parts(&self, since: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT m.log_id FROM match m JOIN log_index i ON i.log_id = m.log_id
              WHERE i.superseded_by IS NULL AND m.played_at >= ?1
                AND EXISTS (SELECT 1 FROM log_raw r WHERE r.log_id = m.log_id)
                AND NOT EXISTS (SELECT 1 FROM log_index p WHERE p.superseded_by = m.log_id)
                AND (lower(COALESCE(m.title, '')) LIKE '%combined%' OR m.map LIKE '%/%' OR m.map LIKE '%+%' OR m.duration_s >= 2700)
              ORDER BY m.played_at DESC",
        )
        .bind(since)
        .fetch_all(self.pool())
        .await?)
    }

    /// A log and the parts folded into it (a combined log's), for a lookup
    /// of just that match.
    pub async fn log_and_parts(&self, log_id: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT log_id FROM log_index WHERE log_id = ?1 OR superseded_by = ?1")
            .bind(log_id)
            .fetch_all(self.pool())
            .await?)
    }

    /// The demos.tf ids of a log's STV: its own, then its parts' in the
    /// order they were played, each once.
    pub async fn stv_ids(&self, log_id: i64) -> Result<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT demos_tf_id FROM (
                 SELECT demos_tf_id, 0 AS k, 0 AS at, log_id FROM log_index WHERE log_id = ?1
                 UNION ALL
                 SELECT demos_tf_id, 1, COALESCE(played_at, 0), log_id FROM log_index WHERE superseded_by = ?1
             ) WHERE demos_tf_id IS NOT NULL ORDER BY k, at, log_id",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        let mut seen = std::collections::HashSet::new();
        Ok(ids.into_iter().filter(|i| seen.insert(*i)).collect())
    }

    /// Record the demos.tf demo each log was matched to, and the map demos.tf
    /// lists for it. Only fills gaps: a log trends.tf already linked keeps
    /// the id it was given.
    pub async fn set_demos_tf_ids(&self, pairs: &[(i64, i64, Option<String>)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        for (log_id, demo_id, map) in pairs {
            sqlx::query(
                "UPDATE log_index SET demos_tf_id = ?2, demos_tf_map = ?3 WHERE log_id = ?1 AND demos_tf_id IS NULL",
            )
            .bind(log_id)
            .bind(demo_id)
            .bind(map)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

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
                -- A file being indexed is a file on disk: a demo that was
                -- deleted and has been downloaded again is no longer gone.
                deleted_at = NULL,
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
    /// Forget demos that are no longer on disk -- except the ones the app
    /// downloaded, which are marked gone instead (PLAN Q23).
    ///
    /// The difference matters because `demos_tf_id` is the only way back. A
    /// demo we fetched and then deleted, by hand or by the cleanup, can be
    /// fetched again from that id; dropping the row throws the id away and
    /// the match silently reverts to having no demo. Everything derived
    /// from it -- aim, deaths, routes -- stays either way: it was derived
    /// once and the file has no further say in it.
    ///
    /// A demo with a stored timeline (Q3) is marked gone too, whoever put it
    /// there: the timeline is everything the file held, and dropping the row
    /// would cut it off from the matches it belongs to.
    pub async fn prune_demos(&self, keep_paths: &[String]) -> Result<u64> {
        self.prune_demos_except(keep_paths, &[]).await
    }

    /// [`prune_demos`](Self::prune_demos), leaving alone every demo under
    /// one of `offline`: a folder on a drive that is not plugged in right
    /// now is not a folder whose demos were deleted.
    pub async fn prune_demos_except(&self, keep_paths: &[String], offline: &[String]) -> Result<u64> {
        let existing: Vec<(i64, String, Option<i64>)> =
            sqlx::query(
                "SELECT d.demo_id, d.path,
                        COALESCE(d.demos_tf_id, (SELECT -1 FROM demo_timeline t WHERE t.demo_id = d.demo_id)) AS demos_tf_id
                 FROM demo d WHERE d.deleted_at IS NULL",
            )
                .fetch_all(self.pool())
                .await?
                .into_iter()
                .map(|r| (r.get("demo_id"), r.get("path"), r.get("demos_tf_id")))
                .collect();
        let keep: std::collections::HashSet<&str> = keep_paths.iter().map(String::as_str).collect();
        let mut removed = 0;
        for (id, path, demos_tf_id) in existing {
            if keep.contains(path.as_str()) || offline.iter().any(|dir| path.starts_with(dir.as_str())) {
                continue;
            }
            // Fetchable again, or kept as a timeline: marked, not dropped.
            if demos_tf_id.is_some() {
                sqlx::query("UPDATE demo SET deleted_at = unixepoch() WHERE demo_id = ?1")
                    .bind(id)
                    .execute(self.pool())
                    .await?;
                continue;
            }
            for t in ["demo_link", "demo_event", "demo"] {
                sqlx::query(&format!("DELETE FROM {t} WHERE demo_id = ?1")).bind(id).execute(self.pool()).await?;
            }
            removed += 1;
        }
        Ok(removed)
    }

    /// The demos the app downloaded and still holds on disk, newest first.
    ///
    /// Only these may be deleted to reclaim space (PLAN Q23): the app put
    /// them there and `demos_tf_id` can fetch them again. A POV demo is the
    /// player's own recording and is never listed here, and neither is an
    /// STV they had before the app existed -- no id, no way back.
    ///
    /// `read` says every log this demo is linked to has been read at the
    /// current pass version, which is what "finished with it" means.
    ///
    /// Also, since Q3, its timeline kept at `timeline_version`: the file may
    /// only go once everything it holds has been recorded.
    pub async fn downloaded_demos(&self, aim_version: i64, timeline_version: i64) -> Result<Vec<DownloadedDemo>> {
        let rows = sqlx::query(
            "SELECT d.demo_id, d.file_name, d.size_bytes, d.map, d.start_utc,
                    (SELECT COUNT(*) FROM demo_link l WHERE l.demo_id = d.demo_id) AS logs,
                    CASE WHEN EXISTS (SELECT 1 FROM demo_timeline t
                                       WHERE t.demo_id = d.demo_id AND t.version >= ?2)
                         THEN (SELECT COUNT(*) FROM demo_link l
                                WHERE l.demo_id = d.demo_id
                                  AND EXISTS (SELECT 1 FROM aim_log a
                                               WHERE a.log_id = l.log_id AND a.version = ?1))
                         ELSE 0 END AS logs_read
             FROM demo d
             WHERE d.demos_tf_id IS NOT NULL AND d.deleted_at IS NULL
             ORDER BY d.start_utc DESC",
        )
        .bind(aim_version)
        .bind(timeline_version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let logs: i64 = r.get("logs");
                let logs_read: i64 = r.get("logs_read");
                DownloadedDemo {
                    demo_id: r.get("demo_id"),
                    file_name: r.get("file_name"),
                    size_bytes: r.get("size_bytes"),
                    map: r.get("map"),
                    start_utc: r.get("start_utc"),
                    logs,
                    // A demo linked to nothing has been read for nothing, so
                    // it is not "finished with" -- it is unused.
                    read: logs > 0 && logs_read == logs,
                }
            })
            .collect())
    }

    /// Where a demo's file is, and whether it is already marked gone.
    pub async fn demo_path(&self, demo_id: i64) -> Result<Option<(String, bool)>> {
        Ok(sqlx::query("SELECT path, deleted_at FROM demo WHERE demo_id = ?1")
            .bind(demo_id)
            .fetch_optional(self.pool())
            .await?
            .map(|r| (r.get("path"), r.get::<Option<i64>, _>("deleted_at").is_some())))
    }

    /// Mark one demo's file as deleted. The row stays: it holds the
    /// demos.tf id this demo can be fetched back from.
    pub async fn mark_demo_deleted(&self, demo_id: i64) -> Result<()> {
        sqlx::query("UPDATE demo SET deleted_at = unixepoch() WHERE demo_id = ?1")
            .bind(demo_id)
            .execute(self.pool())
            .await?;
        Ok(())
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

    /// A log's upload time and the end of its last round, on its own clock:
    /// enough to work out its clock's offset where it has no server log.
    pub async fn log_upload_and_end(&self, log_id: i64) -> Result<Option<(i64, i64)>> {
        let row: Option<(Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT (SELECT played_at FROM log_index WHERE log_id = ?1),
                    (SELECT MAX(start_time + length_s) FROM match_round WHERE log_id = ?1)",
        )
        .bind(log_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.and_then(|(a, b)| Some((a?, b?))))
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
        // demos.tf's links, imported demos' (Q18) and demos dropped on a
        // match page are not the folder scan's to redo: none is found by
        // placing logs on the clock.
        sqlx::query("DELETE FROM demo_link WHERE method NOT IN ('demos.tf', 'import', 'manual')").execute(&mut *tx).await?;
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

    /// Note which demos.tf demo a file is (Q53: one added by its link), so
    /// it can be fetched back after its file is deleted.
    pub async fn set_demo_demos_tf_id(&self, demo_id: i64, demos_tf_id: i64) -> Result<()> {
        sqlx::query("UPDATE demo SET demos_tf_id = ?2 WHERE demo_id = ?1").bind(demo_id).bind(demos_tf_id).execute(self.pool()).await?;
        Ok(())
    }

    /// Set when a demo started, where the folder scan could not tell (an
    /// STV's file time is when it was downloaded). A rescan keeps it.
    pub async fn set_demo_start(&self, demo_id: i64, start_utc: f64) -> Result<()> {
        sqlx::query("UPDATE demo SET start_utc = ?2 WHERE demo_id = ?1").bind(demo_id).bind(start_utc).execute(self.pool()).await?;
        Ok(())
    }

    /// Logs a demo is linked to, `(log_id, share of the log it covers)`.
    pub async fn links_of_demo(&self, demo_id: i64) -> Result<Vec<(i64, f64)>> {
        Ok(sqlx::query_as("SELECT log_id, log_share FROM demo_link WHERE demo_id = ?1").bind(demo_id).fetch_all(self.pool()).await?)
    }

    /// Demo rows with this file name, `(demo_id, path)`: the folder scan
    /// writes paths with mixed separators, so callers compare them on disk.
    pub async fn demos_named(&self, file_name: &str) -> Result<Vec<(i64, String)>> {
        Ok(sqlx::query_as("SELECT demo_id, path FROM demo WHERE file_name = ?1").bind(file_name).fetch_all(self.pool()).await?)
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
                    d.playback_s, d.ticks, d.tick_rate, d.start_utc, d.filename_time, d.deleted_at,
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
                deleted: r.get::<Option<i64>, _>("deleted_at").is_some(),
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
            timelines: q("SELECT COUNT(*) FROM demo_timeline").await?,
            timeline_bytes: q("SELECT COALESCE(SUM(stored_bytes), 0) FROM demo_timeline").await?,
            timelines_file_gone: q(
                "SELECT COUNT(*) FROM demo_timeline t JOIN demo d ON d.demo_id = t.demo_id WHERE d.deleted_at IS NOT NULL",
            )
            .await?,
        })
    }
}
