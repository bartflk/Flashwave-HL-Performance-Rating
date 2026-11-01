//! RGL's decided matches, kept as its API answered them (migration 0044):
//! a match with a winner does not change, so a finished season's tables are
//! read from here instead of from RGL again. What they mean is worked out in
//! `hl_ingest::rgl`. Its other answers worth keeping are kept by path
//! (migration 0045): a finished season's, a finished team's, and matches
//! still to be played, with when they were read.

use crate::Db;
use anyhow::Result;
use sqlx::Row;
use std::collections::HashMap;

impl Db {
    /// The stored answers of these matches, by id; the rest are not kept.
    pub async fn rgl_matches(&self, ids: &[i64]) -> Result<HashMap<i64, String>> {
        let mut out = HashMap::new();
        for chunk in ids.chunks(500) {
            let sql = format!("SELECT match_id, body FROM rgl_match WHERE match_id IN ({})", vec!["?"; chunk.len()].join(","));
            let mut q = sqlx::query(&sql);
            for id in chunk {
                q = q.bind(id);
            }
            for r in q.fetch_all(self.pool()).await? {
                out.insert(r.get("match_id"), r.get("body"));
            }
        }
        Ok(out)
    }

    /// Keep a decided match's answer.
    pub async fn store_rgl_match(&self, match_id: i64, body: &str) -> Result<()> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        sqlx::query("INSERT OR REPLACE INTO rgl_match (match_id, body, stored_at) VALUES (?1, ?2, ?3)")
            .bind(match_id)
            .bind(body)
            .bind(now)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Kept answers of these paths, by path, with when each was kept (unix s).
    pub async fn rgl_answers(&self, paths: &[String]) -> Result<HashMap<String, (String, i64)>> {
        let mut out = HashMap::new();
        for chunk in paths.chunks(500) {
            let sql = format!("SELECT path, body, stored_at FROM rgl_answer WHERE path IN ({})", vec!["?"; chunk.len()].join(","));
            let mut q = sqlx::query(&sql);
            for p in chunk {
                q = q.bind(p);
            }
            for r in q.fetch_all(self.pool()).await? {
                out.insert(r.get("path"), (r.get("body"), r.get("stored_at")));
            }
        }
        Ok(out)
    }

    /// Keep an answer under its path, replacing an older copy.
    pub async fn store_rgl_answer(&self, path: &str, body: &str) -> Result<()> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        sqlx::query("INSERT OR REPLACE INTO rgl_answer (path, body, stored_at) VALUES (?1, ?2, ?3)")
            .bind(path)
            .bind(body)
            .bind(now)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Forget a kept answer: a match kept as still to be played, once decided.
    pub async fn drop_rgl_answer(&self, path: &str) -> Result<()> {
        sqlx::query("DELETE FROM rgl_answer WHERE path = ?1").bind(path).execute(self.pool()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn a_stored_match_is_read_back_and_others_are_not() {
        let db = crate::Db::connect_in_memory().await.unwrap();
        db.store_rgl_match(35150, r#"{"matchId":35150,"winner":15183}"#).await.unwrap();
        db.store_rgl_match(35150, r#"{"matchId":35150,"winner":15199}"#).await.unwrap();
        let got = db.rgl_matches(&[35150, 35151]).await.unwrap();
        assert_eq!(got.len(), 1);
        assert!(got[&35150].contains("15199"));
    }

    #[tokio::test]
    async fn an_answer_is_kept_by_path_and_dropped() {
        let db = crate::Db::connect_in_memory().await.unwrap();
        db.store_rgl_answer("/seasons/193", r#"{"name":"HL Season 24"}"#).await.unwrap();
        db.store_rgl_answer("/matches/35600", "{}").await.unwrap();
        db.drop_rgl_answer("/matches/35600").await.unwrap();
        let got = db.rgl_answers(&["/seasons/193".into(), "/matches/35600".into()]).await.unwrap();
        assert_eq!(got.len(), 1);
        assert!(got["/seasons/193"].0.contains("Season 24") && got["/seasons/193"].1 > 0);
    }
}
