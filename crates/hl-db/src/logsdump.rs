//! The logs.tf dump's index (`hl logsdump`'s index-v2.sqlite3), read-only:
//! which Highlander logs a set of players were in around a time. Used to find
//! the logs of officials from before trends.tf linked logs to ETF2L matches
//! (March 2019). Only log ids and their players come from it; the logs
//! themselves are downloaded from drops.tf and logs.tf as any other.

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::collections::HashMap;
use std::path::Path;

/// One Highlander log in the dump.
#[derive(Debug, Clone)]
pub struct DumpLog {
    pub log_id: i64,
    pub map: String,
    pub played_at: i64,
    pub duration_s: i64,
    pub title: String,
    /// Logs.tf's player count.
    pub players: i64,
    /// Whether it is a second upload of a game already in the dump.
    pub duplicate: bool,
}

pub struct DumpIndex {
    pool: SqlitePool,
}

impl DumpIndex {
    /// Opened read-only: the index is never written from here.
    pub async fn open(path: &Path) -> Result<Self> {
        let opts = SqliteConnectOptions::new().filename(path).create_if_missing(false).read_only(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await
            .with_context(|| format!("opening the logs.tf dump index {}", path.display()))?;
        Ok(Self { pool })
    }

    /// The Highlander logs any of `accounts` played between `from` and `to`,
    /// each with those of `accounts` in it.
    pub async fn logs_with(&self, accounts: &[i64], from: i64, to: i64) -> Result<HashMap<i64, Vec<i64>>> {
        let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
        if accounts.is_empty() {
            return Ok(out);
        }
        let ids = serde_json::to_string(accounts)?;
        // rating_account (account_id, played_at) answers this per player.
        let rows = sqlx::query(
            "SELECT log_id, account_id FROM rating
             WHERE account_id IN (SELECT value FROM json_each(?1)) AND played_at BETWEEN ?2 AND ?3",
        )
        .bind(ids)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        for r in rows {
            out.entry(r.get(0)).or_default().push(r.get(1));
        }
        Ok(out)
    }

    /// The logs themselves.
    pub async fn logs(&self, ids: &[i64]) -> Result<Vec<DumpLog>> {
        let ids = serde_json::to_string(ids)?;
        let rows = sqlx::query(
            "SELECT l.log_id, COALESCE(l.map, ''), l.played_at, COALESCE(l.duration_s, 0), COALESCE(l.title, ''), COALESCE(l.players, 0),
                    EXISTS (SELECT 1 FROM duplicate d WHERE d.log_id = l.log_id)
             FROM log l WHERE l.log_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| DumpLog {
                log_id: r.get(0),
                map: r.get(1),
                played_at: r.get(2),
                duration_s: r.get(3),
                title: r.get(4),
                players: r.get(5),
                duplicate: r.get::<i64, _>(6) != 0,
            })
            .collect())
    }
}
