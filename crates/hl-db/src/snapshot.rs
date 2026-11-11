//! Copying tables between this database and another file (the league
//! snapshot, PLAN Q43). Both ways go through `ATTACH` on one connection and
//! one transaction, and match columns by name, so a file made by a slightly
//! different schema still copies what the two have in common.

use crate::Db;
use anyhow::{Context, Result};
use sqlx::Row;
use std::path::Path;

/// One table to copy in from the attached file.
pub struct TableCopy<'a> {
    pub table: &'a str,
    /// A `WHERE` on the attached file's rows (`src.<table>`), if any.
    pub filter: Option<String>,
    /// `true`: rows already here win (`INSERT OR IGNORE`). `false`: the
    /// file's rows replace them (`INSERT OR REPLACE`).
    pub keep_ours: bool,
}

impl Db {
    /// Copy rows of `tables` in from the database file at `from`, then run
    /// `after` (which may read `src.*`), all in one transaction: a failure
    /// leaves this database as it was.
    pub async fn copy_in(&self, from: &Path, tables: &[TableCopy<'_>], after: &[String]) -> Result<()> {
        self.copy_in_with(from, &[], tables, after).await
    }

    /// [`copy_in`](Self::copy_in), with `before` run first inside the same
    /// transaction: what is cleared to be replaced whole is cleared only if
    /// the copy then succeeds (LG-5).
    pub async fn copy_in_with(&self, from: &Path, before: &[String], tables: &[TableCopy<'_>], after: &[String]) -> Result<()> {
        // ATTACH makes an empty database where the file is missing and then
        // copies nothing from it, which reads as success.
        if !from.is_file() {
            anyhow::bail!("{} is not a file", from.display());
        }
        let mut conn = self.pool().acquire().await?;
        sqlx::query("ATTACH DATABASE ?1 AS src")
            .bind(from.to_string_lossy().to_string())
            .execute(&mut *conn)
            .await
            .with_context(|| format!("opening {}", from.display()))?;
        let copied: Result<()> = async {
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let work: Result<()> = async {
                for sql in before {
                    sqlx::query(sql).execute(&mut *conn).await.with_context(|| sql.clone())?;
                }
                for t in tables {
                    let ours = columns(&mut conn, "main", t.table).await?;
                    let theirs = columns(&mut conn, "src", t.table).await?;
                    let shared: Vec<String> =
                        ours.iter().filter(|c| theirs.contains(c)).map(|c| format!("\"{c}\"")).collect();
                    if shared.is_empty() {
                        continue;
                    }
                    let cols = shared.join(", ");
                    let sql = format!(
                        "INSERT OR {} INTO main.\"{t}\" ({cols}) SELECT {cols} FROM src.\"{t}\"{w}",
                        if t.keep_ours { "IGNORE" } else { "REPLACE" },
                        t = t.table,
                        w = t.filter.as_deref().map(|f| format!(" WHERE {f}")).unwrap_or_default(),
                    );
                    sqlx::query(&sql).execute(&mut *conn).await.with_context(|| format!("copying {}", t.table))?;
                }
                for sql in after {
                    sqlx::query(sql).execute(&mut *conn).await.with_context(|| sql.clone())?;
                }
                Ok(())
            }
            .await;
            match work {
                Ok(()) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(())
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        .await;
        // Whatever happened, the file is let go. A DETACH that fails would
        // hand the pool a connection with `src` still attached (LG-13), so
        // that connection is dropped from the pool and closed instead.
        if let Err(e) = sqlx::query("DETACH DATABASE src").execute(&mut *conn).await {
            tracing::warn!(error = %e, "could not detach the copied file; closing the connection");
            drop(conn.detach());
            return copied.and(Err(e.into()));
        }
        copied
    }

    /// What a league snapshot holds: officials with a league rating, and
    /// when the newest of its officials was played.
    pub async fn snapshot_counts(&self) -> Result<(i64, Option<i64>)> {
        let logs = sqlx::query_scalar("SELECT COUNT(DISTINCT log_id) FROM league_rating").fetch_one(self.pool()).await?;
        let newest = sqlx::query_scalar("SELECT MAX(played_at) FROM league_log WHERE json_source = 'snapshot'")
            .fetch_one(self.pool())
            .await?;
        Ok((logs, newest))
    }

    /// A snapshot's officials back to "not downloaded", so the league
    /// download may fetch them for real.
    pub async fn release_snapshot_officials(&self) -> Result<()> {
        sqlx::query("UPDATE league_log SET json_source = NULL, raw_state = NULL WHERE json_source = 'snapshot'")
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Shrink the file after a large copy: what the snapshot ships.
    pub async fn vacuum(&self) -> Result<()> {
        sqlx::query("VACUUM").execute(self.pool()).await?;
        Ok(())
    }
}

/// A table's columns in `schema`; empty when the table is not there.
async fn columns(conn: &mut sqlx::SqliteConnection, schema: &str, table: &str) -> Result<Vec<String>> {
    let rows = sqlx::query(&format!("PRAGMA {schema}.table_info(\"{table}\")")).fetch_all(&mut *conn).await?;
    Ok(rows.into_iter().map(|r| r.get::<String, _>("name")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn what_is_cleared_first_comes_back_when_the_copy_fails() {
        let dir = std::env::temp_dir().join(format!("hl-copy-in-before-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let theirs = Db::connect(dir.join("theirs.sqlite3")).await.unwrap();
        theirs.pool().close().await;
        let ours = Db::connect(dir.join("ours.sqlite3")).await.unwrap();
        sqlx::query("INSERT INTO etf2l_team (team_id, name) VALUES (1, 'ours one')").execute(ours.pool()).await.unwrap();
        let clear = vec!["DELETE FROM main.etf2l_team".to_string()];
        let bad = ours.copy_in_with(&dir.join("theirs.sqlite3"), &clear, &[], &["SELECT * FROM no_such_table".into()]).await;
        assert!(bad.is_err());
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM etf2l_team").fetch_one(ours.pool()).await.unwrap();
        assert_eq!(n, 1, "the clearing was rolled back with the copy");
        ours.copy_in_with(&dir.join("theirs.sqlite3"), &clear, &[], &[]).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM etf2l_team").fetch_one(ours.pool()).await.unwrap();
        assert_eq!(n, 0, "and done when it works");
    }

    #[tokio::test]
    async fn copies_in_keeping_ours_or_replacing_and_rolls_back_on_failure() {
        let dir = std::env::temp_dir().join(format!("hl-copy-in-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let theirs = Db::connect(dir.join("theirs.sqlite3")).await.unwrap();
        for (id, name) in [(1, "theirs one"), (2, "theirs two")] {
            sqlx::query("INSERT INTO etf2l_team (team_id, name) VALUES (?1, ?2)").bind(id).bind(name).execute(theirs.pool()).await.unwrap();
        }
        theirs.pool().close().await;

        let ours = Db::connect(dir.join("ours.sqlite3")).await.unwrap();
        sqlx::query("INSERT INTO etf2l_team (team_id, name) VALUES (1, 'ours one')").execute(ours.pool()).await.unwrap();
        let copy = |keep_ours| vec![TableCopy { table: "etf2l_team", filter: None, keep_ours }];
        ours.copy_in(&dir.join("theirs.sqlite3"), &copy(true), &[]).await.unwrap();
        let names: Vec<String> = sqlx::query_scalar("SELECT name FROM etf2l_team ORDER BY team_id").fetch_all(ours.pool()).await.unwrap();
        assert_eq!(names, ["ours one", "theirs two"], "ours kept, theirs added");

        ours.copy_in(&dir.join("theirs.sqlite3"), &copy(false), &[]).await.unwrap();
        let first: String = sqlx::query_scalar("SELECT name FROM etf2l_team WHERE team_id = 1").fetch_one(ours.pool()).await.unwrap();
        assert_eq!(first, "theirs one", "replaced when asked");

        // A failing statement afterwards undoes the copy too.
        sqlx::query("DELETE FROM etf2l_team").execute(ours.pool()).await.unwrap();
        let bad = ours.copy_in(&dir.join("theirs.sqlite3"), &copy(true), &["SELECT * FROM no_such_table".into()]).await;
        assert!(bad.is_err());
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM etf2l_team").fetch_one(ours.pool()).await.unwrap();
        assert_eq!(n, 0, "rolled back");
    }
}
