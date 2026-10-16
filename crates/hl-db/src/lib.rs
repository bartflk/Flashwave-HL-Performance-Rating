//! SQLite access layer.
//!
//! Uses runtime-checked queries (`sqlx::query`) rather than the compile-time
//! macros on purpose: the macros require a live database at build time, which
//! would make a fresh clone fail to compile before it has ever been run.

mod aim;

/// The logs the owner's own rating covers: their rated Highlander logs, and
/// the per-map logs a rated combined log replaced. A league log among these
/// is left to the owner's rating, so no game is in the pool twice; any
/// other -- a final whose combined upload the app could not rate (S31 Low's
/// "FINAL", on a map name it does not know) -- is rated as the league's.
pub(crate) const OWNER_COVERED: &str = "SELECT i.log_id FROM log_index i JOIN log_raw x ON x.log_id = i.log_id
      WHERE i.superseded_by IS NULL AND COALESCE(i.format_override, i.format) = 'highlander'
    UNION
    SELECT s.log_id FROM log_index s WHERE s.superseded_by IN (
      SELECT i.log_id FROM log_index i JOIN log_raw x ON x.log_id = i.log_id
       WHERE i.superseded_by IS NULL AND COALESCE(i.format_override, i.format) = 'highlander')";
mod context;
mod demos;
mod fights;
mod catalogue;
mod league_sample;
mod leagues;
mod mass;
mod matches;
mod players;
mod ratings;
mod rawlog;
mod roundmap;
mod snapshot;
mod timeline;
mod transfers;

pub use context::{
    ContextCounts, ContextGameRow, ContextRow, Etf2lMatchRow, MatchContext, MatchSides, MateRow, OfficialInfo,
    OfficialRow, OwnGameRow, SideTeam,
};
pub use aim::{AimFilter, AimRow, AimTotals, DeathRow, LifeIndexRow, LifeTotals, PathRow};
pub use fights::{ClassGame, FightFilter, FightRow, FightTotals, FightsWrite, SeasonOfficial, FIGHT_COLUMNS};
pub use demos::{ClockInput, ClockRow, DemoRow, DemoStats, DownloadedDemo, LinkedDemo};
pub use rawlog::{ChatRow, KillRow, RawlogStats, StoredKill};
pub use roundmap::{
    PartRow, ResolverLog, RoundMapRow, RoundMapStats, RoundRow, RoundWindow, Segment, SegmentRow,
};
pub use ratings::{HistoryDbRow, RatingRow, VsTotals};
pub use timeline::{TimelineRow, TimelineTotals};
pub use leagues::{Competition, CompetitionRow, H2hRow, ResultRow, SeasonMap, SeasonMatch, SeasonMatchRow, TeamHit, TeamRef};
pub use catalogue::{CatMatch, Etf2lPlayer, RatedGame, RosterRow};
pub use transfers::Transfer;
pub use mass::{MassBatch, MassDemo};
pub use league_sample::{Candidate, LeagueLogRow, TierProgress};
pub use snapshot::TableCopy;
pub use players::{PlayerClass, PlayerHit, PlayerSummary};
pub use matches::{
    FailedLog, IndexInfo, IndexStats, LogsTfIndexRow, MatchFilter, MatchPage, MatchSummary, PartSummary, MyLine,
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
/// Rows per multi-row INSERT. SQLite allows 32,766 bound values per
/// statement; the widest table written this way has 20 columns, so 500 rows
/// stays well inside it while cutting round trips by that factor.
pub(crate) const BATCH_ROWS: usize = 500;

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

    /// Write a consistent copy of the whole database to `path`, which must
    /// not exist. Safe while the app is using the original: SQLite reads it
    /// inside a transaction and the result is an ordinary database file.
    pub async fn vacuum_into(&self, path: &Path) -> Result<()> {
        // The path goes in as a bound value, never pasted into the statement.
        sqlx::query("VACUUM INTO ?1")
            .bind(path.to_string_lossy().as_ref())
            .execute(&self.pool)
            .await
            .with_context(|| format!("copying the database to `{}`", path.display()))?;
        Ok(())
    }

    /// How many logs a database file holds, without opening it properly.
    ///
    /// Read-only and without migrations, because the caller is asking about a
    /// *backup*: migrating one as a side effect of looking at it would rewrite
    /// the very file being kept as it was. A file that is not a database, or
    /// is too old to have `log_index`, counts as empty rather than an error —
    /// the question is only ever "is there anything in here".
    pub async fn peek_matches(path: &Path) -> i64 {
        let opts = SqliteConnectOptions::new().filename(path).create_if_missing(false).read_only(true);
        let Ok(mut conn) = <sqlx::sqlite::SqliteConnection as sqlx::Connection>::connect_with(&opts).await
        else {
            return 0;
        };
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM log_index")
            .fetch_one(&mut conn)
            .await
            .unwrap_or(0);
        let _ = sqlx::Connection::close(conn).await;
        n
    }

    /// Let go of the file. Only startup work needs this: everything else keeps
    /// the pool for as long as the app runs.
    pub async fn close(&self) {
        self.pool.close().await;
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

/// Migrations already shipped must never change, not even a comment: the
/// database keeps each one's checksum and refuses to open when a file it has
/// applied differs -- every installed copy would fail to start after the
/// update. (A comment rename in 0026 did exactly that to the dev database.)
/// A new migration is a new file, added to the end of this list.
#[cfg(test)]
mod frozen_migrations {
    const SHIPPED: &[(&str, u64)] = &[
        ("0001_init.sql", 0xd211cab677938a73),
        ("0002_matches.sql", 0xaa48f1a4ae68df57),
        ("0003_round_colours.sql", 0x96569b3c03bd54f8),
        ("0004_ratings.sql", 0xe4f0d6c25901cdc4),
        ("0005_demos.sql", 0x84a6b035324ade7a),
        ("0006_context.sql", 0xfbca8c1bb46dab22),
        ("0007_rawlog.sql", 0x914b4e0bea20a11c),
        ("0008_round_maps.sql", 0xed4cc9023cec9582),
        ("0009_fights.sql", 0x5a5763ad08ff1d4f),
        ("0010_death_context.sql", 0xe71636ac3689a789),
        ("0011_fight_kast.sql", 0xc48f9ac4e95fc7c0),
        ("0012_kill_situation.sql", 0xff6ddbf406060350),
        ("0013_demo_aim.sql", 0x22e619691f063175),
        ("0014_demo_death.sql", 0x5cf60f3c21242af4),
        ("0015_aim_offsets.sql", 0xb0f4fab2b08cba1f),
        ("0016_death_angle.sql", 0xe206130cc02b46b1),
        ("0017_aim_path.sql", 0xfa01dd7f0917445c),
        ("0018_demo_path.sql", 0x054417dd800de8d8),
        ("0019_path_players.sql", 0x919bcfb8c91959cd),
        ("0020_path_caps.sql", 0xa5805e6f5c0524ea),
        ("0021_official_guess.sql", 0xea134882562f52dd),
        ("0022_rating_scale.sql", 0x7777c8ec7d351f04),
        ("0023_baseline_per_map.sql", 0xd6762d394c13905d),
        ("0024_aim_per_player.sql", 0x12945ef387c8d83e),
        ("0025_demo_deleted.sql", 0x7f8db42d678cb7c5),
        ("0026_cap_costs.sql", 0xb07dfd12d17d7a52),
        ("0027_kill_credit.sql", 0x795e229217b08f1d),
        ("0028_demo_timeline.sql", 0xac25a93f8002f17d),
        ("0029_cap_spawn_delay.sql", 0x35e56160a46ca344),
        ("0030_etf2l_seasons.sql", 0x96e012b7662cc3a6),
        ("0031_log_stand_in.sql", 0xc1cb597e502e6f16),
        ("0032_map_hints.sql", 0xf42b55ff5516d7dd),
        ("0033_league_sample.sql", 0x60586fab4de9abc6),
        ("0034_player_catalogue.sql", 0xd034c4bcfbebec48),
        ("0035_league_rating.sql", 0xd109c7e71d4b431f),
        ("0036_trends_career.sql", 0xc71d916bad5afbbd),
        ("0037_etf2l_transfers.sql", 0x0aea2736b8e226a0),
        ("0038_kind_override.sql", 0xc57e09ed6dc2a9e5),
        ("0039_mass_import.sql", 0xa59f6ce07de16df6),
    ];

    fn fnv1a(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
    }

    #[test]
    fn no_shipped_migration_has_changed() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        files.sort();
        let names: Vec<&str> = SHIPPED.iter().map(|(n, _)| *n).collect();
        assert_eq!(files, names, "a migration was added or removed: add a new one to the end of SHIPPED, never rename");
        for (name, hash) in SHIPPED {
            // Line endings are normalised: a checkout with CRLF is the same file.
            let text = std::fs::read(dir.join(name)).unwrap();
            let lf: Vec<u8> = text.into_iter().filter(|b| *b != b'\r').collect();
            assert_eq!(fnv1a(&lf), *hash, "{name} has been edited after it shipped");
        }
    }
}
