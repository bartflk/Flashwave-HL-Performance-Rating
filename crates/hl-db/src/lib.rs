//! SQLite access layer.
//!
//! Uses runtime-checked queries (`sqlx::query`) rather than the compile-time
//! macros on purpose: the macros require a live database at build time, which
//! would make a fresh clone fail to compile before it has ever been run.

mod aim;
mod context;
mod demos;
mod fights;
mod matches;
mod ratings;
mod rawlog;
mod roundmap;

pub use context::{
    ContextCounts, ContextGameRow, ContextRow, Etf2lMatchRow, MatchContext, MateRow, OfficialInfo,
    OfficialRow, OwnGameRow,
};
pub use aim::{AimRow, AimTotals};
pub use fights::{ClassGame, FightFilter, FightRow, FightTotals, SeasonOfficial, FIGHT_COLUMNS};
pub use demos::{ClockInput, ClockRow, DemoRow, DemoStats, LinkedDemo};
pub use rawlog::{ChatRow, KillRow, RawlogStats, StoredKill};
pub use roundmap::{
    PartRow, ResolverLog, RoundMapRow, RoundMapStats, RoundRow, RoundWindow, Segment, SegmentRow,
};
pub use ratings::{HistoryDbRow, RatingRow, VsTotals};
pub use matches::{
    IndexInfo, IndexStats, LogsTfIndexRow, MatchFilter, MatchPage, MatchSummary, PartSummary, MyLine,
    TrendsIndexRow,
};

use anyhow::{Context, Result};
use hl_core::config::{keys, AppConfig};
use hl_core::SteamId;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use std::path::Path;


/// Embedded at compile time, applied at startup. Adding a migration means
/// dropping a file in `migrations/` and rebuilding.
///
/// **Never edit a migration once it has been applied anywhere** — not even a
/// comment. sqlx stores each migration's checksum and refuses to open a
/// database whose applied migrations no longer match. Changes go in a new file.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Open (creating if needed) the database at `path` and bring it up to the
    /// latest schema.
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating data directory `{}`", parent.display()))?;
        }

        // `filename` rather than a `sqlite://` URL: Windows paths contain a
        // drive colon and backslashes, which URL parsing mangles.
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await
            .with_context(|| format!("opening database `{}`", path.display()))?;

        MIGRATOR
            .run(&pool)
            .await
            .context("applying database migrations")?;

        tracing::info!(path = %path.display(), "database ready");
        Ok(Db { pool })
    }

    /// An in-memory database with migrations applied. For tests.
    pub async fn connect_in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .context("opening in-memory database")?;
        MIGRATOR.run(&pool).await.context("applying migrations")?;
        Ok(Db { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // ---- config -----------------------------------------------------------

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM app_config WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .with_context(|| format!("reading setting `{key}`"))?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO app_config (key, value, updated_at)
             VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value,
                                            updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .with_context(|| format!("writing setting `{key}`"))?;
        Ok(())
    }

    pub async fn clear_setting(&self, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM app_config WHERE key = ?1")
            .bind(key)
            .execute(&self.pool)
            .await
            .with_context(|| format!("clearing setting `{key}`"))?;
        Ok(())
    }

    pub async fn get_config(&self) -> Result<AppConfig> {
        let steamid = match self.get_setting(keys::STEAMID).await? {
            // A stored value that no longer parses is treated as unset rather
            // than as a hard failure: the user can just set it again.
            Some(raw) => SteamId::parse(&raw).ok(),
            None => None,
        };
        Ok(AppConfig {
            steamid,
            tf_path: self.get_setting(keys::TF_PATH).await?,
        })
    }

    // ---- players ----------------------------------------------------------

    /// Insert or update a player, returning their account id.
    pub async fn upsert_player(&self, id: SteamId, display_name: Option<&str>) -> Result<u32> {
        sqlx::query(
            "INSERT INTO player (account_id, steamid64, steamid3, display_name, updated_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(account_id) DO UPDATE SET
                 display_name = COALESCE(excluded.display_name, player.display_name),
                 updated_at   = excluded.updated_at",
        )
        .bind(id.account_id() as i64)
        .bind(id.to_steamid64())
        .bind(id.to_steamid3())
        .bind(display_name)
        .execute(&self.pool)
        .await
        .with_context(|| format!("upserting player {id}"))?;
        Ok(id.account_id())
    }

    /// Mark `id` as the owner of this install, clearing any previous owner.
    ///
    /// Done in one transaction because `player_single_me` would otherwise
    /// reject the new owner while the old one still holds the flag.
    pub async fn set_me(&self, id: SteamId) -> Result<()> {
        self.upsert_player(id, None).await?;

        let mut tx = self.pool.begin().await.context("starting transaction")?;
        sqlx::query("UPDATE player SET is_me = 0 WHERE is_me = 1")
            .execute(&mut *tx)
            .await
            .context("clearing previous owner")?;
        sqlx::query("UPDATE player SET is_me = 1 WHERE account_id = ?1")
            .bind(id.account_id() as i64)
            .execute(&mut *tx)
            .await
            .context("setting owner")?;
        tx.commit().await.context("committing owner change")?;

        self.set_setting(keys::STEAMID, &id.to_steamid64()).await?;
        Ok(())
    }

    pub async fn get_me(&self) -> Result<Option<SteamId>> {
        let row = sqlx::query("SELECT account_id FROM player WHERE is_me = 1")
            .fetch_optional(&self.pool)
            .await
            .context("reading owner")?;
        Ok(row.map(|r| SteamId::from_account_id(r.get::<i64, _>("account_id") as u32)))
    }

    // ---- sync state -------------------------------------------------------

    pub async fn record_sync(
        &self,
        source: &str,
        cursor: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO sync_state (source, cursor, last_run_at, last_error)
             VALUES (?1, ?2, datetime('now'), ?3)
             ON CONFLICT(source) DO UPDATE SET
                 cursor      = COALESCE(excluded.cursor, sync_state.cursor),
                 last_run_at = excluded.last_run_at,
                 last_error  = excluded.last_error",
        )
        .bind(source)
        .bind(cursor)
        .bind(error)
        .execute(&self.pool)
        .await
        .with_context(|| format!("recording sync state for `{source}`"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_apply_to_a_fresh_database() {
        let db = Db::connect_in_memory().await.unwrap();
        assert!(db.get_config().await.unwrap().steamid.is_none());
    }

    #[tokio::test]
    async fn settings_round_trip_and_overwrite() {
        let db = Db::connect_in_memory().await.unwrap();
        db.set_setting("k", "one").await.unwrap();
        db.set_setting("k", "two").await.unwrap();
        assert_eq!(db.get_setting("k").await.unwrap().as_deref(), Some("two"));
        db.clear_setting("k").await.unwrap();
        assert_eq!(db.get_setting("k").await.unwrap(), None);
    }

    #[tokio::test]
    async fn changing_owner_leaves_exactly_one() {
        let db = Db::connect_in_memory().await.unwrap();
        let a = SteamId::from_account_id(1);
        let b = SteamId::from_account_id(2);

        db.set_me(a).await.unwrap();
        assert_eq!(db.get_me().await.unwrap(), Some(a));

        db.set_me(b).await.unwrap();
        assert_eq!(db.get_me().await.unwrap(), Some(b));

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM player WHERE is_me = 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn config_survives_an_unparseable_stored_steamid() {
        let db = Db::connect_in_memory().await.unwrap();
        db.set_setting(keys::STEAMID, "garbage").await.unwrap();
        assert!(db.get_config().await.unwrap().steamid.is_none());
    }
}
