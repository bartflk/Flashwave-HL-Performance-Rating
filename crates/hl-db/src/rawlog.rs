//! Raw server logs, and the kills and chat derived from them.

use crate::{Db, BATCH_ROWS};
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
    /// The killer's colour at that moment, to tell a team kill from a kill.
    pub killer_team: Option<String>,
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

    /// A raw log and the kills and chat read from it, in one transaction. The
    /// raw log is what takes a log out of the raw-log queue, so it must never
    /// land without its kills: a sync cancelled between two separate writes
    /// left a log whose kills were never derived and never asked for again
    /// (audit SY-1). `aligned` is whether the kills could be put on logs.tf's
    /// clock (see `frame_offset`).
    pub async fn store_rawlog_with_kills(&self, log_id: i64, zip: &[u8], kills: &[KillRow<'_>], chat: &[ChatRow<'_>], aligned: bool) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO rawlog (log_id, fetched_at, size_bytes, zip) VALUES (?1, unixepoch(), ?2, ?3)
             ON CONFLICT (log_id) DO UPDATE SET fetched_at = excluded.fetched_at,
                size_bytes = excluded.size_bytes, zip = excluded.zip",
        )
        .bind(log_id)
        .bind(zip.len() as i64)
        .bind(zip)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM rawlog_missing WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        write_kill_events(&mut tx, log_id, kills, chat).await?;
        set_unaligned(&mut tx, log_id, !aligned).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Note whether a log's kills sit on logs.tf's clock, for a log written
    /// without [`store_rawlog_with_kills`](Self::store_rawlog_with_kills).
    pub async fn set_rawlog_aligned(&self, log_id: i64, aligned: bool) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        set_unaligned(&mut tx, log_id, !aligned).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Whether this log's kills could not be put on logs.tf's clock: their
    /// times are the server's own, hours off, and must not be joined to a
    /// demo's clock.
    pub async fn rawlog_unaligned(&self, log_id: i64) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT 1 FROM rawlog_unaligned WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?
            .is_some())
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
               -- A zip that would not open is more likely a cut-off download
               -- than a bad file: asked again within the hour, not the week
               -- (audit SY-11).
               AND log_id NOT IN (SELECT log_id FROM rawlog_missing
                                  WHERE checked_at > unixepoch() - CASE reason WHEN 'unreadable zip' THEN 3600 ELSE 7 * 86400 END)
             ORDER BY log_id DESC"
        ))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn replace_kill_events(&self, log_id: i64, kills: &[KillRow<'_>], chat: &[ChatRow<'_>]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        write_kill_events(&mut tx, log_id, kills, chat).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Several logs' kills and chat replaced in one transaction.
    ///
    /// For a rebuild, which rewrites every log at once. A commit per log cost
    /// more than the writing did: on 738 logs, 13 s one transaction each
    /// against 3.9 s for all of them in one (measured in SQLite directly, so
    /// none of it is this code). Callers pass a group at a time, so a failure
    /// loses one group's work and a rerun redoes it.
    pub async fn replace_kill_events_many(&self, logs: &[(i64, Vec<KillRow<'_>>, Vec<ChatRow<'_>>)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        for (log_id, kills, chat) in logs {
            write_kill_events(&mut tx, *log_id, kills, chat).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Every stored kill, grouped by log. One query: the rating pass reads all
    /// of them at once.
    pub async fn all_kills(&self) -> Result<HashMap<i64, Vec<StoredKill>>> {
        let rows = sqlx::query(
            "SELECT log_id, at_raw, live, killer, killer_class, killer_team, victim, victim_team, victim_class,
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
            "SELECT at_raw, live, killer, killer_class, killer_team, victim, victim_team, victim_class, weapon, custom, assister
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

    /// One player's kills and deaths on these maps, with where they were:
    /// `(log_id, got_the_kill, their team, their class, x, y)` (Q28).
    #[allow(clippy::type_complexity)]
    pub async fn player_kill_positions(&self, account: u32, maps: &[String]) -> Result<Vec<(i64, bool, String, String, i32, i32)>> {
        if maps.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; maps.len()].join(", ");
        let sql = format!(
            "SELECT k.log_id, k.killer = ?1 AS got,
                    CASE WHEN k.killer = ?1 THEN k.killer_team ELSE k.victim_team END AS team,
                    CASE WHEN k.killer = ?1 THEN k.killer_class ELSE k.victim_class END AS class,
                    CASE WHEN k.killer = ?1 THEN k.kx ELSE k.vx END AS x,
                    CASE WHEN k.killer = ?1 THEN k.ky ELSE k.vy END AS y
             FROM kill_event k
             JOIN match_round r ON r.log_id = k.log_id
                AND k.at_raw BETWEEN r.start_time AND r.start_time + r.length_s
             JOIN round_map rm ON rm.log_id = r.log_id AND rm.round_num = r.round_num
             WHERE rm.map IN ({marks}) AND k.live = 1 AND COALESCE(k.custom, '') != 'feign_death'
               AND (k.killer = ?1 OR k.victim = ?1) AND k.killer != k.victim
               AND k.kx IS NOT NULL AND k.vx IS NOT NULL
               AND k.log_id IN ({WANTED})"
        );
        let mut q = sqlx::query(&sql).bind(i64::from(account));
        for m in maps {
            q = q.bind(m);
        }
        Ok(q.fetch_all(self.pool())
            .await?
            .into_iter()
            .map(|r| {
                (
                    r.get("log_id"),
                    r.get::<i64, _>("got") != 0,
                    r.get::<Option<String>, _>("team").unwrap_or_default(),
                    r.get::<Option<String>, _>("class").unwrap_or_default(),
                    r.get::<i64, _>("x") as i32,
                    r.get::<i64, _>("y") as i32,
                )
            })
            .collect())
    }

    /// Logs with a parsed STV timeline that this player is in (Q28).
    pub async fn player_stv_logs(&self, account: u32) -> Result<Vec<(i64, String)>> {
        Ok(sqlx::query_as(
            "SELECT DISTINCT l.log_id, COALESCE(m.map, '') FROM demo_link l
             JOIN demo d ON d.demo_id = l.demo_id AND d.kind = 'stv'
             JOIN demo_timeline t ON t.demo_id = d.demo_id
             JOIN match m ON m.log_id = l.log_id
             JOIN match_player p ON p.log_id = l.log_id AND p.account_id = ?1",
        )
        .bind(i64::from(account))
        .fetch_all(self.pool())
        .await?)
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
        killer_team: r.get("killer_team"),
        victim: r.get::<i64, _>("victim") as u32,
        victim_team: r.get("victim_team"),
        victim_class: r.get("victim_class"),
        weapon: r.get("weapon"),
        custom: r.get("custom"),
        assister: r.get::<Option<i64>, _>("assister").map(|a| a as u32),
    }
}

/// Replace one log's kills and chat inside an open transaction.
async fn set_unaligned(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, log_id: i64, unaligned: bool) -> Result<()> {
    if unaligned {
        sqlx::query("INSERT OR IGNORE INTO rawlog_unaligned (log_id) VALUES (?1)").bind(log_id).execute(&mut **tx).await?;
    } else {
        sqlx::query("DELETE FROM rawlog_unaligned WHERE log_id = ?1").bind(log_id).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn write_kill_events(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    log_id: i64,
    kills: &[KillRow<'_>],
    chat: &[ChatRow<'_>],
) -> Result<()> {
    sqlx::query("DELETE FROM kill_event WHERE log_id = ?1").bind(log_id).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM chat_event WHERE log_id = ?1").bind(log_id).execute(&mut **tx).await?;
    // Many rows per statement rather than one: each statement is a round
    // trip to SQLite's worker thread, and a rebuild writes 225,000 kills.
    // A row per statement spent most of the rebuild waiting on those hops.
    let rows: Vec<(usize, &KillRow)> = kills.iter().enumerate().collect();
    for batch in rows.chunks(BATCH_ROWS) {
        let mut q = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "INSERT INTO kill_event (log_id, seq, at_raw, round_num, live, killer, killer_team, killer_class,
                victim, victim_team, victim_class, weapon, custom, assister, kx, ky, kz, vx, vy, vz) ",
        );
        q.push_values(batch, |mut b, &(seq, k)| {
            let (kp, vp) = (k.killer_pos, k.victim_pos);
            b.push_bind(log_id)
                .push_bind(seq as i64)
                .push_bind(k.at_raw)
                .push_bind(k.round_num)
                .push_bind(k.live)
                .push_bind(k.killer as i64)
                .push_bind(k.killer_team)
                .push_bind(k.killer_class)
                .push_bind(k.victim as i64)
                .push_bind(k.victim_team)
                .push_bind(k.victim_class)
                .push_bind(k.weapon)
                .push_bind(k.custom)
                .push_bind(k.assister.map(i64::from))
                .push_bind(kp.map(|p| p[0]))
                .push_bind(kp.map(|p| p[1]))
                .push_bind(kp.map(|p| p[2]))
                .push_bind(vp.map(|p| p[0]))
                .push_bind(vp.map(|p| p[1]))
                .push_bind(vp.map(|p| p[2]));
        });
        q.build().execute(&mut **tx).await?;
    }
    let rows: Vec<(usize, &ChatRow)> = chat.iter().enumerate().collect();
    for batch in rows.chunks(BATCH_ROWS) {
        let mut q = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "INSERT INTO chat_event (log_id, seq, at_raw, account, team_chat, message) ",
        );
        q.push_values(batch, |mut b, &(seq, c)| {
            b.push_bind(log_id)
                .push_bind(seq as i64)
                .push_bind(c.at_raw)
                .push_bind(c.account.map(i64::from))
                .push_bind(c.team_chat)
                .push_bind(c.message);
        });
        q.build().execute(&mut **tx).await?;
    }
    Ok(())
}

impl Db {
    /// Mark a log as stored from a stand-in source rather than logs.tf.
    pub async fn mark_stand_in(&self, log_id: i64, source: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO log_stand_in (log_id, source) VALUES (?1, ?2)
             ON CONFLICT (log_id) DO UPDATE SET source = excluded.source, stored_at = unixepoch()",
        )
        .bind(log_id)
        .bind(source)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Where a stand-in log came from; `None` for a log from logs.tf.
    pub async fn stand_in(&self, log_id: i64) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT source FROM log_stand_in WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?)
    }

    /// The real JSON is in: the stand-in's row goes, and so does its made-up
    /// raw log, which puts the log back in the raw-log queue for the real one.
    /// Its kills stay until that arrives and replaces them.
    pub async fn clear_stand_in(&self, log_id: i64) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        let was = sqlx::query("DELETE FROM log_stand_in WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        if was.rows_affected() > 0 {
            sqlx::query("DELETE FROM rawlog WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM rawlog_missing WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
