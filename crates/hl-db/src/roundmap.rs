//! Parts of combined logs, and the map of every round.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;
use std::collections::HashMap;

/// What the resolver needs about one kept Highlander log.
pub struct ResolverLog {
    pub log_id: i64,
    pub map_field: Option<String>,
    pub title: Option<String>,
    /// Parts trends.tf lists, as stored JSON.
    pub duplicate_of: Option<String>,
    pub clock_offset: Option<i64>,
    /// ETF2L's map list as stored JSON, for officials.
    pub etf2l_maps: Option<String>,
}

pub struct RoundRow {
    pub round_num: i64,
    pub start: i64,
    pub length: i64,
    /// Stable team.
    pub winner: Option<String>,
}

/// A part as trends.tf indexes it.
pub struct PartRow {
    pub log_id: i64,
    pub map: Option<String>,
    pub uploaded: Option<i64>,
    pub duration: Option<i64>,
    pub duplicate_of: Option<String>,
}

pub struct RoundMapRow {
    pub round_num: i64,
    pub map: Option<String>,
    pub source: Option<&'static str>,
}

pub struct SegmentRow {
    pub map: Option<String>,
    pub first_round: i64,
    pub last_round: i64,
    pub rounds: i64,
    pub red_wins: i64,
    pub blue_wins: i64,
}

/// One map of a match, as the match list and page show it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub map: Option<String>,
    pub first_round: i64,
    pub last_round: i64,
    pub rounds: i64,
    pub red_wins: i64,
    pub blue_wins: i64,
}

/// A round's map and its window in logs.tf's round-time frame.
#[derive(Debug, Clone)]
pub struct RoundWindow {
    pub start: i64,
    pub length: i64,
    pub map: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundMapStats {
    pub rounds: i64,
    pub unresolved: i64,
    /// Rounds per source: `log`, `part`, `geometry`, ...
    pub by_source: Vec<(String, i64)>,
    pub multi_map_logs: i64,
    pub parts_stored: i64,
}

const KEPT: &str = "SELECT i.log_id FROM log_index i JOIN match m ON m.log_id = i.log_id
     WHERE i.superseded_by IS NULL AND COALESCE(i.format_override, i.format) = 'highlander'";

impl Db {
    pub async fn resolver_logs(&self) -> Result<Vec<ResolverLog>> {
        let rows = sqlx::query(&format!(
            "SELECT m.log_id, m.map, m.title, i.duplicate_of, lc.offset_s, e.maps AS etf2l_maps
             FROM match m
             JOIN log_index i ON i.log_id = m.log_id
             LEFT JOIN log_clock lc ON lc.log_id = m.log_id
             LEFT JOIN match_context c ON c.log_id = m.log_id
             LEFT JOIN etf2l_match e ON e.match_id = c.etf2l_match_id
             WHERE m.log_id IN ({KEPT})"
        ))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ResolverLog {
                log_id: r.get("log_id"),
                map_field: r.get("map"),
                title: r.get("title"),
                duplicate_of: r.get("duplicate_of"),
                clock_offset: r.get("offset_s"),
                etf2l_maps: r.get("etf2l_maps"),
            })
            .collect())
    }

    pub async fn all_rounds(&self) -> Result<HashMap<i64, Vec<RoundRow>>> {
        let rows = sqlx::query(
            "SELECT log_id, round_num, start_time, length_s, winner FROM match_round
             WHERE start_time IS NOT NULL AND length_s IS NOT NULL ORDER BY log_id, start_time",
        )
        .fetch_all(self.pool())
        .await?;
        let mut out: HashMap<i64, Vec<RoundRow>> = HashMap::new();
        for r in rows {
            out.entry(r.get("log_id")).or_default().push(RoundRow {
                round_num: r.get("round_num"),
                start: r.get("start_time"),
                length: r.get("length_s"),
                winner: r.get("winner"),
            });
        }
        Ok(out)
    }

    /// Every counted kill's time and both positions, per log.
    #[allow(clippy::type_complexity)]
    pub async fn kill_points(&self) -> Result<HashMap<i64, Vec<(i64, i32, i32, i32, i32)>>> {
        let rows = sqlx::query(
            "SELECT log_id, at_raw, kx, ky, vx, vy FROM kill_event
             WHERE live = 1 AND COALESCE(custom, '') != 'feign_death' AND kx IS NOT NULL AND vx IS NOT NULL",
        )
        .fetch_all(self.pool())
        .await?;
        let mut out: HashMap<i64, Vec<(i64, i32, i32, i32, i32)>> = HashMap::new();
        for r in rows {
            out.entry(r.get("log_id")).or_default().push((
                r.get("at_raw"),
                r.get::<i64, _>("kx") as i32,
                r.get::<i64, _>("ky") as i32,
                r.get::<i64, _>("vx") as i32,
                r.get::<i64, _>("vy") as i32,
            ));
        }
        Ok(out)
    }

    pub async fn part_rows(&self, ids: &[i64]) -> Result<Vec<PartRow>> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(r) = sqlx::query("SELECT log_id, map, played_at, duration_s, duplicate_of FROM log_index WHERE log_id = ?1")
                .bind(id)
                .fetch_optional(self.pool())
                .await?
            {
                out.push(PartRow {
                    log_id: r.get("log_id"),
                    map: r.get("map"),
                    uploaded: r.get("played_at"),
                    duration: r.get("duration_s"),
                    duplicate_of: r.get("duplicate_of"),
                });
            }
        }
        Ok(out)
    }

    pub async fn store_part_raw(&self, log_id: i64, json: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO part_raw (log_id, fetched_at, json) VALUES (?1, unixepoch(), ?2)
             ON CONFLICT (log_id) DO UPDATE SET fetched_at = excluded.fetched_at, json = excluded.json",
        )
        .bind(log_id)
        .bind(json)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn part_raw_ids(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT log_id FROM part_raw").fetch_all(self.pool()).await?)
    }

    pub async fn part_raw(&self, log_id: i64) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT json FROM part_raw WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub async fn replace_round_maps(&self, logs: &[(i64, Vec<RoundMapRow>, Vec<SegmentRow>)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM round_map").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM log_segment").execute(&mut *tx).await?;
        for (log_id, rounds, segments) in logs {
            for r in rounds {
                sqlx::query("INSERT INTO round_map (log_id, round_num, map, source) VALUES (?1, ?2, ?3, ?4)")
                    .bind(log_id)
                    .bind(r.round_num)
                    .bind(&r.map)
                    .bind(r.source)
                    .execute(&mut *tx)
                    .await?;
            }
            for (seq, s) in segments.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO log_segment (log_id, seq, map, first_round, last_round, rounds, red_wins, blue_wins)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .bind(log_id)
                .bind(seq as i64)
                .bind(&s.map)
                .bind(s.first_round)
                .bind(s.last_round)
                .bind(s.rounds)
                .bind(s.red_wins)
                .bind(s.blue_wins)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn segments(&self, log_id: i64) -> Result<Vec<Segment>> {
        let rows = sqlx::query(
            "SELECT map, first_round, last_round, rounds, red_wins, blue_wins FROM log_segment
             WHERE log_id = ?1 ORDER BY seq",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Segment {
                map: r.get("map"),
                first_round: r.get("first_round"),
                last_round: r.get("last_round"),
                rounds: r.get("rounds"),
                red_wins: r.get("red_wins"),
                blue_wins: r.get("blue_wins"),
            })
            .collect())
    }

    /// Round windows with their maps, for one log.
    pub async fn round_windows(&self, log_id: i64) -> Result<Vec<RoundWindow>> {
        Ok(self.all_round_windows_where(Some(log_id)).await?.remove(&log_id).unwrap_or_default())
    }

    /// Each round's number and span on the log's own clock: what a moment
    /// needs to be placed in a round.
    pub async fn round_spans(&self, log_id: i64) -> Result<Vec<(i64, i64, i64)>> {
        let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
            "SELECT round_num, start_time, length_s FROM match_round
             WHERE log_id = ?1 AND start_time IS NOT NULL ORDER BY round_num",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Round windows with their maps, for every log.
    pub async fn all_round_windows(&self) -> Result<HashMap<i64, Vec<RoundWindow>>> {
        self.all_round_windows_where(None).await
    }

    async fn all_round_windows_where(&self, log_id: Option<i64>) -> Result<HashMap<i64, Vec<RoundWindow>>> {
        let rows = sqlx::query(
            "SELECT r.log_id, r.start_time, r.length_s, rm.map
             FROM match_round r
             JOIN round_map rm ON rm.log_id = r.log_id AND rm.round_num = r.round_num
             WHERE r.start_time IS NOT NULL AND (?1 IS NULL OR r.log_id = ?1)",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        let mut out: HashMap<i64, Vec<RoundWindow>> = HashMap::new();
        for r in rows {
            out.entry(r.get("log_id")).or_default().push(RoundWindow {
                start: r.get("start_time"),
                length: r.get::<Option<i64>, _>("length_s").unwrap_or(0),
                map: r.get("map"),
            });
        }
        Ok(out)
    }

    pub async fn round_map_stats(&self) -> Result<RoundMapStats> {
        let by = sqlx::query("SELECT COALESCE(source, 'unresolved') AS s, COUNT(*) AS n FROM round_map GROUP BY s ORDER BY n DESC")
            .fetch_all(self.pool())
            .await?;
        let by_source: Vec<(String, i64)> = by.into_iter().map(|r| (r.get("s"), r.get("n"))).collect();
        let rounds = by_source.iter().map(|(_, n)| n).sum();
        let unresolved = by_source.iter().find(|(s, _)| s == "unresolved").map_or(0, |(_, n)| *n);
        let multi_map_logs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM (SELECT log_id FROM log_segment GROUP BY log_id HAVING COUNT(DISTINCT map) > 1)",
        )
        .fetch_one(self.pool())
        .await?;
        let parts_stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM part_raw").fetch_one(self.pool()).await?;
        Ok(RoundMapStats { rounds, unresolved, by_source, multi_map_logs, parts_stored })
    }
}
