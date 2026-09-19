//! Raw server logs, and the kills and chat derived from them.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;
use std::collections::HashMap;

/// One kill line, flattened for storage.
pub struct KillRow<'a> {
    pub at_raw: i64,
    pub round_num: i64,
    pub live: bool,
    pub killer: u32,
    pub killer_team: Option<&'a str>,
    pub killer_class: Option<&'a str>,
    pub victim: u32,
    pub victim_team: Option<&'a str>,
    pub victim_class: Option<&'a str>,
    pub weapon: &'a str,
    pub custom: Option<&'a str>,
    pub assister: Option<u32>,
    pub killer_pos: Option<[i32; 3]>,
    pub victim_pos: Option<[i32; 3]>,
}

pub struct ChatRow<'a> {
    pub at_raw: i64,
    pub account: Option<u32>,
    pub team_chat: bool,
    pub message: &'a str,
}

/// A stored kill, as read back.
#[derive(Debug, Clone)]
pub struct StoredKill {
    pub at_raw: i64,
    pub live: bool,
    pub killer: u32,
    pub killer_class: Option<String>,
    pub victim: u32,
    pub victim_team: Option<String>,
    pub victim_class: Option<String>,
    pub weapon: String,
    pub custom: Option<String>,
    pub assister: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawlogStats {
    /// Kept Highlander logs with a raw log stored.
    pub stored: i64,
    /// Kept Highlander logs still to fetch.
    pub pending: i64,
    /// logs.tf has no raw file for these.
    pub missing: i64,
    pub bytes: i64,
    pub kills: i64,
}

/// Kept Highlander logs: the ones worth a raw log.
const WANTED: &str = "SELECT i.log_id FROM log_index i JOIN log_raw r ON r.log_id = i.log_id
     WHERE i.superseded_by IS NULL AND COALESCE(i.format_override, i.format) = 'highlander'";

impl Db {
    pub async fn store_rawlog(&self, log_id: i64, zip: &[u8]) -> Result<()> {
        sqlx::query(
            "INSERT INTO rawlog (log_id, fetched_at, size_bytes, zip) VALUES (?1, unixepoch(), ?2, ?3)
             ON CONFLICT (log_id) DO UPDATE SET fetched_at = excluded.fetched_at,
                size_bytes = excluded.size_bytes, zip = excluded.zip",
        )
        .bind(log_id)
        .bind(zip.len() as i64)
        .bind(zip)
        .execute(self.pool())
        .await?;
        sqlx::query("DELETE FROM rawlog_missing WHERE log_id = ?1").bind(log_id).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn mark_rawlog_missing(&self, log_id: i64, reason: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO rawlog_missing (log_id, checked_at, reason) VALUES (?1, unixepoch(), ?2)
             ON CONFLICT (log_id) DO UPDATE SET checked_at = excluded.checked_at, reason = excluded.reason",
        )
        .bind(log_id)
        .bind(reason)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn rawlog(&self, log_id: i64) -> Result<Option<Vec<u8>>> {
        Ok(sqlx::query_scalar("SELECT zip FROM rawlog WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub async fn rawlog_ids(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT log_id FROM rawlog ORDER BY log_id").fetch_all(self.pool()).await?)
    }

    /// Kept Highlander logs with no raw log yet, newest first. Logs marked
    /// missing are retried after a week, in case logs.tf was having a bad day.
    pub async fn rawlog_queue(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(&format!(
            "SELECT log_id FROM ({WANTED})
             WHERE log_id NOT IN (SELECT log_id FROM rawlog)
               AND log_id NOT IN (SELECT log_id FROM rawlog_missing
                                  WHERE checked_at > unixepoch() - 7 * 86400)
             ORDER BY log_id DESC"
        ))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn replace_kill_events(&self, log_id: i64, kills: &[KillRow<'_>], chat: &[ChatRow<'_>]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM kill_event WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM chat_event WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        for (seq, k) in kills.iter().enumerate() {
            let kp = k.killer_pos;
            let vp = k.victim_pos;
            sqlx::query(
                "INSERT INTO kill_event (log_id, seq, at_raw, round_num, live, killer, killer_team, killer_class,
                    victim, victim_team, victim_class, weapon, custom, assister, kx, ky, kz, vx, vy, vz)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            )
            .bind(log_id)
            .bind(seq as i64)
            .bind(k.at_raw)
            .bind(k.round_num)
            .bind(k.live)
            .bind(k.killer as i64)
            .bind(k.killer_team)
            .bind(k.killer_class)
            .bind(k.victim as i64)
            .bind(k.victim_team)
            .bind(k.victim_class)
            .bind(k.weapon)
            .bind(k.custom)
            .bind(k.assister.map(i64::from))
            .bind(kp.map(|p| p[0]))
            .bind(kp.map(|p| p[1]))
            .bind(kp.map(|p| p[2]))
            .bind(vp.map(|p| p[0]))
            .bind(vp.map(|p| p[1]))
            .bind(vp.map(|p| p[2]))
            .execute(&mut *tx)
            .await?;
        }
        for (seq, c) in chat.iter().enumerate() {
            sqlx::query(
                "INSERT INTO chat_event (log_id, seq, at_raw, account, team_chat, message)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(log_id)
            .bind(seq as i64)
            .bind(c.at_raw)
            .bind(c.account.map(i64::from))
            .bind(c.team_chat)
            .bind(c.message)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Every stored kill, grouped by log. One query: the rating pass reads all
    /// of them at once.
    pub async fn all_kills(&self) -> Result<HashMap<i64, Vec<StoredKill>>> {
        let rows = sqlx::query(
            "SELECT log_id, at_raw, live, killer, killer_class, victim, victim_team, victim_class,
                    weapon, custom, assister
             FROM kill_event ORDER BY log_id, seq",
        )
        .fetch_all(self.pool())
        .await?;
        let mut out: HashMap<i64, Vec<StoredKill>> = HashMap::new();
        for r in rows {
            out.entry(r.get("log_id")).or_default().push(stored_kill(&r));
        }
        Ok(out)
    }

    pub async fn kills_for_log(&self, log_id: i64) -> Result<Vec<StoredKill>> {
        let rows = sqlx::query(
            "SELECT at_raw, live, killer, killer_class, victim, victim_team, victim_class, weapon, custom, assister
             FROM kill_event WHERE log_id = ?1 ORDER BY seq",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.iter().map(stored_kill).collect())
    }

    /// Logs whose derived kills disagree with logs.tf's per-player totals:
    /// `(logs compared, [(log_id, "account got/want, ...")])`. The parser
    /// reproduces logs.tf exactly, so any row here is a parser bug or a log
    /// logs.tf summarised differently.
    pub async fn check_kills_against_logstf(&self) -> Result<(i64, Vec<(i64, String)>)> {
        let compared: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT log_id) FROM kill_event")
            .fetch_one(self.pool())
            .await?;
        let rows = sqlx::query(
            "SELECT p.log_id, p.account_id, p.kills AS want, COALESCE(k.n, 0) AS got
             FROM match_player p
             JOIN (SELECT DISTINCT log_id FROM kill_event) l ON l.log_id = p.log_id
             LEFT JOIN (SELECT log_id, killer, COUNT(*) AS n FROM kill_event
                        WHERE live = 1 AND COALESCE(custom, '') != 'feign_death'
                        GROUP BY log_id, killer) k
               ON k.log_id = p.log_id AND k.killer = p.account_id
             WHERE p.kills != COALESCE(k.n, 0)
             ORDER BY p.log_id DESC",
        )
        .fetch_all(self.pool())
        .await?;
        let mut by_log: Vec<(i64, String)> = Vec::new();
        for r in rows {
            let log_id: i64 = r.get("log_id");
            let bit = format!("{} {}/{}", r.get::<i64, _>("account_id"), r.get::<i64, _>("got"), r.get::<i64, _>("want"));
            match by_log.last_mut() {
                Some((l, s)) if *l == log_id => {
                    s.push_str(", ");
                    s.push_str(&bit);
                }
                _ => by_log.push((log_id, bit)),
            }
        }
        Ok((compared, by_log))
    }

    /// Every map a round of a kept Highlander log was played on, including
    /// the maps inside combined logs.
    pub async fn kept_maps(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(&format!(
            "SELECT DISTINCT rm.map FROM round_map rm WHERE rm.map IS NOT NULL AND rm.log_id IN ({WANTED})"
        ))
        .fetch_all(self.pool())
        .await?)
    }

    /// Every counted kill on these maps with both players' positions:
    /// `(log_id, killer, victim, kx, ky, vx, vy)`. A kill's map is its round's,
    /// so combined logs contribute each map's kills to that map.
    #[allow(clippy::type_complexity)]
    pub async fn kill_positions(&self, maps: &[String]) -> Result<Vec<(i64, u32, u32, i32, i32, i32, i32)>> {
        if maps.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; maps.len()].join(", ");
        let sql = format!(
            "SELECT k.log_id, k.killer, k.victim, k.kx, k.ky, k.vx, k.vy
             FROM kill_event k
             JOIN match_round r ON r.log_id = k.log_id
                AND k.at_raw BETWEEN r.start_time AND r.start_time + r.length_s
             JOIN round_map rm ON rm.log_id = r.log_id AND rm.round_num = r.round_num
             WHERE rm.map IN ({marks}) AND k.live = 1 AND COALESCE(k.custom, '') != 'feign_death'
               AND k.kx IS NOT NULL AND k.vx IS NOT NULL
               AND k.log_id IN ({WANTED})"
        );
        let mut q = sqlx::query(&sql);
        for m in maps {
            q = q.bind(m);
        }
        Ok(q.fetch_all(self.pool())
            .await?
            .into_iter()
            .map(|r| {
                (
                    r.get("log_id"),
                    r.get::<i64, _>("killer") as u32,
                    r.get::<i64, _>("victim") as u32,
                    r.get::<i64, _>("kx") as i32,
                    r.get::<i64, _>("ky") as i32,
                    r.get::<i64, _>("vx") as i32,
                    r.get::<i64, _>("vy") as i32,
                )
            })
            .collect())
    }

    pub async fn rawlog_stats(&self) -> Result<RawlogStats> {
        let r = sqlx::query(&format!(
            "SELECT
                (SELECT COUNT(*) FROM ({WANTED}) w JOIN rawlog x ON x.log_id = w.log_id) AS stored,
                (SELECT COUNT(*) FROM ({WANTED}) w WHERE w.log_id NOT IN (SELECT log_id FROM rawlog)
                    AND w.log_id NOT IN (SELECT log_id FROM rawlog_missing)) AS pending,
                (SELECT COUNT(*) FROM rawlog_missing) AS missing,
                (SELECT COALESCE(SUM(size_bytes), 0) FROM rawlog) AS bytes,
                (SELECT COUNT(*) FROM kill_event) AS kills"
        ))
        .fetch_one(self.pool())
        .await?;
        Ok(RawlogStats {
            stored: r.get("stored"),
            pending: r.get("pending"),
            missing: r.get("missing"),
            bytes: r.get("bytes"),
            kills: r.get("kills"),
        })
    }
}

fn stored_kill(r: &sqlx::sqlite::SqliteRow) -> StoredKill {
    StoredKill {
        at_raw: r.get("at_raw"),
        live: r.get::<i64, _>("live") != 0,
        killer: r.get::<i64, _>("killer") as u32,
        killer_class: r.get("killer_class"),
        victim: r.get::<i64, _>("victim") as u32,
        victim_team: r.get("victim_team"),
        victim_class: r.get("victim_class"),
        weapon: r.get("weapon"),
        custom: r.get("custom"),
        assister: r.get::<Option<i64>, _>("assister").map(|a| a as u32),
    }
}
