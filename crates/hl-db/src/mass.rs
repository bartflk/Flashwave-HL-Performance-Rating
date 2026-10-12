//! Mass imports (Q69, Emiel): a run that fetched one player's demos on one
//! map from demos.tf, and what became of each demo in it.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;

/// One run, as the manage window lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MassBatch {
    pub batch_id: i64,
    pub account_id: i64,
    pub map: String,
    pub class: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    /// `None` until the owner chose; then whether the files are kept.
    pub kept: Option<bool>,
    pub demos: Vec<MassDemo>,
}

/// One demo of a run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MassDemo {
    pub demos_tf_id: i64,
    pub demo_id: Option<i64>,
    pub log_id: Option<i64>,
    /// `queued`, `done`, `failed` or `skipped`.
    pub status: String,
    pub error: Option<String>,
    /// The file's size while it is on disk; `None` once deleted.
    pub size_bytes: Option<i64>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Db {
    /// Start a run with its demos queued; its id.
    pub async fn create_mass_batch(&self, account_id: u32, map: &str, class: Option<&str>, demos_tf_ids: &[i64]) -> Result<i64> {
        let mut tx = self.pool().begin().await?;
        let id: i64 = sqlx::query_scalar("INSERT INTO mass_import (account_id, map, class, created_at) VALUES (?1, ?2, ?3, ?4) RETURNING batch_id")
            .bind(i64::from(account_id))
            .bind(map)
            .bind(class)
            .bind(now())
            .fetch_one(&mut *tx)
            .await?;
        for d in demos_tf_ids {
            sqlx::query("INSERT OR IGNORE INTO mass_import_demo (batch_id, demos_tf_id, status) VALUES (?1, ?2, 'queued')")
                .bind(id)
                .bind(d)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(id)
    }

    /// Record what happened to one demo of a run.
    pub async fn set_mass_demo(
        &self,
        batch_id: i64,
        demos_tf_id: i64,
        status: &str,
        demo_id: Option<i64>,
        log_id: Option<i64>,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE mass_import_demo SET status = ?3, demo_id = COALESCE(?4, demo_id), log_id = COALESCE(?5, log_id), error = ?6
             WHERE batch_id = ?1 AND demos_tf_id = ?2",
        )
        .bind(batch_id)
        .bind(demos_tf_id)
        .bind(status)
        .bind(demo_id)
        .bind(log_id)
        .bind(error)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn finish_mass_batch(&self, batch_id: i64) -> Result<()> {
        sqlx::query("UPDATE mass_import SET finished_at = ?2 WHERE batch_id = ?1").bind(batch_id).bind(now()).execute(self.pool()).await?;
        Ok(())
    }

    /// The owner's answer to "keep the files?".
    pub async fn set_mass_kept(&self, batch_id: i64, kept: bool) -> Result<()> {
        sqlx::query("UPDATE mass_import SET kept = ?2 WHERE batch_id = ?1").bind(batch_id).bind(kept).execute(self.pool()).await?;
        Ok(())
    }

    /// Every run, newest first, with its demos and their files' sizes.
    pub async fn mass_batches(&self) -> Result<Vec<MassBatch>> {
        let batches = sqlx::query("SELECT batch_id, account_id, map, class, created_at, finished_at, kept FROM mass_import ORDER BY batch_id DESC")
            .fetch_all(self.pool())
            .await?;
        let demos = sqlx::query(
            "SELECT m.batch_id, m.demos_tf_id, m.demo_id, m.log_id, m.status, m.error,
                    CASE WHEN d.deleted_at IS NULL THEN d.size_bytes END AS size_bytes
             FROM mass_import_demo m LEFT JOIN demo d ON d.demo_id = m.demo_id
             ORDER BY m.batch_id, m.demos_tf_id DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(batches
            .into_iter()
            .map(|b| {
                let batch_id: i64 = b.get("batch_id");
                MassBatch {
                    batch_id,
                    account_id: b.get("account_id"),
                    map: b.get("map"),
                    class: b.get("class"),
                    created_at: b.get("created_at"),
                    finished_at: b.get("finished_at"),
                    kept: b.get::<Option<i64>, _>("kept").map(|k| k != 0),
                    demos: demos
                        .iter()
                        .filter(|d| d.get::<i64, _>("batch_id") == batch_id)
                        .map(|d| MassDemo {
                            demos_tf_id: d.get("demos_tf_id"),
                            demo_id: d.get("demo_id"),
                            log_id: d.get("log_id"),
                            status: d.get("status"),
                            error: d.get("error"),
                            size_bytes: d.get("size_bytes"),
                        })
                        .collect(),
                }
            })
            .collect())
    }

    /// Demos whose run the owner chose to keep: never swept away after
    /// reading, whatever the cleanup setting says.
    pub async fn kept_mass_demo_ids(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT m.demo_id FROM mass_import_demo m JOIN mass_import b ON b.batch_id = m.batch_id
             WHERE b.kept = 1 AND m.demo_id IS NOT NULL",
        )
        .fetch_all(self.pool())
        .await?)
    }

    /// Forget a run (its demos and routes stay; only the record goes).
    pub async fn forget_mass_batch(&self, batch_id: i64) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM mass_import_demo WHERE batch_id = ?1").bind(batch_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM mass_import WHERE batch_id = ?1").bind(batch_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// A demos.tf demo already on this machine: its demo id and whether its
    /// file is still there.
    pub async fn demo_by_demostf_id(&self, demos_tf_id: i64) -> Result<Option<(i64, bool)>> {
        Ok(sqlx::query("SELECT demo_id, deleted_at FROM demo WHERE demos_tf_id = ?1 ORDER BY demo_id DESC LIMIT 1")
            .bind(demos_tf_id)
            .fetch_optional(self.pool())
            .await?
            .map(|r| (r.get("demo_id"), r.get::<Option<i64>, _>("deleted_at").is_none())))
    }

    /// Logs on this map's base played within `window` seconds of `at`, as
    /// `(log_id, played_at, map)`: where a demo's match may already be stored.
    pub async fn logs_near(&self, at: i64, window: i64) -> Result<Vec<(i64, i64, Option<String>)>> {
        Ok(sqlx::query_as(
            "SELECT i.log_id, i.played_at, i.map FROM log_index i JOIN match m ON m.log_id = i.log_id
             WHERE i.played_at BETWEEN ?1 - ?2 AND ?1 + ?2",
        )
        .bind(at)
        .bind(window)
        .fetch_all(self.pool())
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use crate::Db;

    #[tokio::test]
    async fn a_run_records_its_demos_and_the_choice_to_keep_them() {
        let db = Db::connect_in_memory().await.unwrap();
        let id = db.create_mass_batch(42, "product", Some("spy"), &[990239, 990254]).await.unwrap();
        db.set_mass_demo(id, 990239, "done", Some(7), Some(3426174), None).await.unwrap();
        db.set_mass_demo(id, 990254, "failed", None, None, Some("not this match")).await.unwrap();
        db.finish_mass_batch(id).await.unwrap();
        let all = db.mass_batches().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].kept, None, "not asked yet");
        let done = all[0].demos.iter().find(|d| d.demos_tf_id == 990239).unwrap();
        assert_eq!((done.status.as_str(), done.demo_id, done.log_id), ("done", Some(7), Some(3426174)));
        assert!(db.kept_mass_demo_ids().await.unwrap().is_empty());
        db.set_mass_kept(id, true).await.unwrap();
        assert_eq!(db.kept_mass_demo_ids().await.unwrap(), vec![7]);
        db.forget_mass_batch(id).await.unwrap();
        assert!(db.mass_batches().await.unwrap().is_empty());
    }
}
