//! The 6v6 league sample's storage (the 6v6 plan, P5; table `sixes_league_log`, 0050).

use crate::{Db, Result};

/// One ETF2L 6v6 log as trends.tf lists it.
pub struct SixesLeagueLogRow<'a> {
    pub log_id: i64,
    pub etf2l_match_id: i64,
    pub map: &'a str,
    pub played_at: i64,
    pub duration_s: Option<i64>,
    pub title: Option<&'a str>,
}

/// Where the 6v6 sample stands.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SixesSampleCounts {
    pub listed: i64,
    pub downloaded: i64,
    /// Given up on: no source had it.
    pub missing: i64,
    pub oldest: Option<i64>,
    pub newest: Option<i64>,
}

impl Db {
    /// Add listed logs; one held already keeps what it has.
    pub async fn upsert_sixes_league_logs(&self, rows: &[SixesLeagueLogRow<'_>]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        for r in rows {
            sqlx::query(
                "INSERT INTO sixes_league_log (log_id, etf2l_match_id, map, played_at, duration_s, title)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(log_id) DO NOTHING",
            )
            .bind(r.log_id)
            .bind(r.etf2l_match_id)
            .bind(r.map)
            .bind(r.played_at)
            .bind(r.duration_s)
            .bind(r.title)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The next logs to download, newest first: played maps (five minutes
    /// or more), not tried `max_attempts` times.
    pub async fn sixes_league_todo(&self, limit: i64, max_attempts: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT log_id FROM sixes_league_log
              WHERE json_source IS NULL AND json_attempts < ?2 AND COALESCE(duration_s, 0) >= 300
              ORDER BY played_at DESC, log_id DESC LIMIT ?1",
        )
        .bind(limit)
        .bind(max_attempts)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn put_sixes_league_json(&self, log_id: i64, json: &str, source: &str) -> Result<()> {
        sqlx::query("UPDATE sixes_league_log SET json = ?2, json_source = ?3 WHERE log_id = ?1")
            .bind(log_id)
            .bind(json)
            .bind(source)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn sixes_league_failed(&self, log_id: i64) -> Result<()> {
        sqlx::query("UPDATE sixes_league_log SET json_attempts = json_attempts + 1 WHERE log_id = ?1").bind(log_id).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn sixes_league_counts(&self, max_attempts: i64) -> Result<SixesSampleCounts> {
        let r: (i64, i64, i64, Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT COUNT(*), COUNT(json_source), COALESCE(SUM(json_source IS NULL AND json_attempts >= ?1), 0), MIN(played_at), MAX(played_at)
               FROM sixes_league_log WHERE COALESCE(duration_s, 0) >= 300",
        )
        .bind(max_attempts)
        .fetch_one(self.pool())
        .await?;
        Ok(SixesSampleCounts { listed: r.0, downloaded: r.1, missing: r.2, oldest: r.3, newest: r.4 })
    }

    /// Downloaded logs whose raw log is not tried yet, newest first.
    pub async fn sixes_league_raw_todo(&self, limit: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT log_id FROM sixes_league_log WHERE json_source IS NOT NULL AND raw_state IS NULL
              ORDER BY played_at DESC, log_id DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// A raw log, or that there is none to have.
    pub async fn put_sixes_league_raw(&self, log_id: i64, zip: Option<&[u8]>) -> Result<()> {
        sqlx::query("UPDATE sixes_league_log SET raw = ?2, raw_state = ?3 WHERE log_id = ?1")
            .bind(log_id)
            .bind(zip)
            .bind(if zip.is_some() { "ok" } else { "missing" })
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Every stored sample raw log, zipped, by log.
    pub async fn sixes_league_raws(&self) -> Result<Vec<(i64, Vec<u8>)>> {
        Ok(sqlx::query_as("SELECT log_id, raw FROM sixes_league_log WHERE raw IS NOT NULL").fetch_all(self.pool()).await?)
    }

    /// The oldest and newest listed, to list only what is new next time.
    pub async fn sixes_league_span(&self) -> Result<(i64, Option<i64>, Option<i64>)> {
        Ok(sqlx::query_as("SELECT COUNT(*), MIN(played_at), MAX(played_at) FROM sixes_league_log").fetch_one(self.pool()).await?)
    }

    /// Every downloaded sample log's JSON, for the rating's pools; the
    /// owner's own logs left out (they are in already).
    pub async fn sixes_league_jsons(&self) -> Result<Vec<(i64, String)>> {
        Ok(sqlx::query_as(
            "SELECT l.log_id, l.json FROM sixes_league_log l
              WHERE l.json IS NOT NULL AND l.log_id NOT IN (SELECT log_id FROM log_raw)",
        )
        .fetch_all(self.pool())
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_sample_lists_downloads_newest_first_and_gives_up_on_a_lost_log() {
        let db = Db::connect_in_memory().await.unwrap();
        let row = |log_id: i64, played_at: i64, duration_s: i64| SixesLeagueLogRow { log_id, etf2l_match_id: 9, map: "cp_process_f12", played_at, duration_s: Some(duration_s), title: None };
        db.upsert_sixes_league_logs(&[row(1, 100, 1500), row(2, 300, 1500), row(3, 200, 60)]).await.unwrap();
        assert_eq!(db.sixes_league_todo(10, 3).await.unwrap(), vec![2, 1], "newest first; a restart under five minutes is not a map");
        db.put_sixes_league_json(2, "{}", "drops.tf").await.unwrap();
        for _ in 0..3 {
            db.sixes_league_failed(1).await.unwrap();
        }
        assert!(db.sixes_league_todo(10, 3).await.unwrap().is_empty());
        let c = db.sixes_league_counts(3).await.unwrap();
        assert_eq!((c.listed, c.downloaded, c.missing), (2, 1, 1));
        assert_eq!(db.sixes_league_jsons().await.unwrap(), vec![(2, "{}".to_string())]);
        // Its raw log next, once its JSON is in.
        assert_eq!(db.sixes_league_raw_todo(10).await.unwrap(), vec![2]);
        db.put_sixes_league_raw(2, Some(b"zip")).await.unwrap();
        assert!(db.sixes_league_raw_todo(10).await.unwrap().is_empty());
        assert_eq!(db.sixes_league_raws().await.unwrap(), vec![(2, b"zip".to_vec())]);
    }
}
