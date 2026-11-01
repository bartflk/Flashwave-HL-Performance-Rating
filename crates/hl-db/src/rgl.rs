//! RGL's decided matches, kept as its API answered them (migration 0044):
//! a match with a winner does not change, so a finished season's tables are
//! read from here instead of from RGL again. What they mean is worked out in
//! `hl_ingest::rgl`.

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
}
