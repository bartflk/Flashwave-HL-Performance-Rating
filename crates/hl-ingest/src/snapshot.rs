//! The league snapshot (Q43, Flashy): every ETF2L official's results, rosters
//! and league ratings, shipped inside the app, so the player lookup, the
//! medals and the MVPs work on a new install without downloading 5,000+
//! logs from logs.tf first.
//!
//! **What is in it:** ETF2L's seasons, matches, maps, rosters and teams; the
//! league sample's log list and who played each log; the league ratings for
//! the current model; and the pool's baselines and scale, so a new install
//! rates its own games on the league's scale. **What is not:** the logs
//! themselves (logs.tf's JSON and the raw server logs, ~650 MB), and nothing
//! of the owner's own: their matches, demos, settings and ETF2L profile stay
//! on their PC.
//!
//! **Made at each release** from the maintainer's database (`hl snapshot`,
//! docs/releasing.md), for that release's rating model. A snapshot for
//! another model is not imported: its ratings would be on the wrong scale.
//!
//! **Imported once per snapshot, at startup** ([`apply_built_in`]). Rows
//! already here win; the snapshot only fills gaps. Its officials are marked
//! `snapshot`, so the league download does not fetch them again. Its
//! baselines and scale replace this install's only while the install's own
//! pool is smaller than the snapshot's ([`pool`], `rating::rate_all`).

use crate::league_rating::rate_one;
use anyhow::{bail, Context, Result};
use hl_db::{Db, TableCopy};
use hl_rating::{Weights, MODEL_VERSION};
use std::io::{Read, Write};
use std::path::Path;

/// The snapshot this build ships (`hl snapshot` writes it).
const BUILT_IN: &[u8] = include_bytes!("../../../league/snapshot.sqlite3.gz");

/// In this database: which snapshot was imported, for which model, and the
/// size of the pool its scale was measured on.
const KEY_AT: &str = "league_snapshot_at";
const KEY_VERSION: &str = "league_snapshot_version";
const KEY_POOL: &str = "league_snapshot_pool";

/// In the snapshot file itself.
const META_AT: &str = "snapshot_made_at";
const META_VERSION: &str = "snapshot_model";
const META_POOL: &str = "snapshot_pool";
const META_LOGS: &str = "snapshot_logs";
const META_NEWEST: &str = "snapshot_newest";

/// What a snapshot says about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub made_at: i64,
    pub model: String,
    /// Performances the scale was measured over.
    pub pool: i64,
    /// Officials whose ratings it carries.
    pub logs: i64,
    /// When the newest of them was played.
    pub newest: i64,
}

/// Public tables that make up the league, newest rows from the file.
fn league_tables(keep_ours: bool) -> Vec<TableCopy<'static>> {
    let t = |table| TableCopy { table, filter: None, keep_ours };
    vec![
        t("etf2l_competition"),
        t("etf2l_team"),
        t("etf2l_season_match"),
        t("etf2l_season_map"),
        t("etf2l_season_player"),
        t("etf2l_player"),
        t("trends_career"),
        t("etf2l_transfer"),
        t("etf2l_transfer_fetch"),
        t("league_log"),
        t("league_log_player"),
        TableCopy { table: "league_rating", filter: Some(format!("model_version = '{MODEL_VERSION}'")), keep_ours },
        // Every player's numbers in the officials (season tables, profiles,
        // impact): 6.6 MB of the file, measured 21 October 2026. The league's
        // logs only, never the owner's own matches. Per-weapon rows stay out
        // (8 MB more for one panel): an install has them for the logs it holds.
        TableCopy { table: "log_stat", filter: Some("log_id IN (SELECT log_id FROM src.league_log)".into()), keep_ours },
    ]
}

/// The pool's baselines and scale for the current model.
fn scale_tables() -> Vec<TableCopy<'static>> {
    let filter = Some(format!("model_version = '{MODEL_VERSION}'"));
    vec![
        TableCopy { table: "baseline", filter: filter.clone(), keep_ours: false },
        TableCopy { table: "rating_scale", filter, keep_ours: false },
    ]
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Write the snapshot of `source` (the database at `source_path`, never the
/// live one: a copy) to `out`, gzipped. `source` must be rated for the
/// current model, league included.
pub async fn export(source: &Db, source_path: &Path, w: &Weights, out: &Path) -> Result<Meta> {
    let Some(pool) = source.rating_scale_size(MODEL_VERSION).await? else {
        bail!("this database has no {MODEL_VERSION} ratings yet: run `hl rate` on it first");
    };
    let plain = out.with_extension("");
    let _ = std::fs::remove_file(&plain);
    let snap = Db::connect(&plain).await?;
    let mut tables = league_tables(false);
    tables.extend(scale_tables());
    // The logs are not in the snapshot: their officials are marked so an
    // install does not take them for downloaded, nor fetch them again.
    snap.copy_in(
        source_path,
        &tables,
        &["UPDATE league_log SET json_source = 'snapshot', raw_state = 'snapshot' WHERE json_source IS NOT NULL".into()],
    )
    .await?;

    // The owner's own officials are rated as the owner's games, not in the
    // league table; here they are rated as league games too, so the lookup
    // of the owner (and of everyone in their matches) is complete.
    let baseline = crate::rating::load_baseline(source).await?;
    let scale = crate::rating::load_scale(source).await?.context("no rating scale")?;
    for log_id in source.league_logs_without_rating(MODEL_VERSION).await? {
        let (Some(json), zip) = source.league_log_files(log_id).await? else { continue };
        // Off the async thread, as the app rates league logs: parsing a raw
        // log needs more stack than a CLI's main thread has on Windows.
        let w2 = w.clone();
        let Ok(perfs) = tokio::task::spawn_blocking(move || rate_one(log_id, &json, zip.as_deref(), &w2)).await? else { continue };
        let rows: Vec<(i64, u32, &str, f64, f64, String)> = perfs
            .iter()
            .filter_map(|p| {
                let r = hl_rating::model::rate(p, &baseline, w)?.scaled(&scale);
                Some((log_id, p.account_id, p.class.as_str(), r.score, r.minutes, hl_rating::guide::group_scores_json(&r.parts)))
            })
            .collect();
        snap.put_league_ratings(MODEL_VERSION, &rows).await?;
    }

    let (logs, newest) = snap.snapshot_counts().await?;
    let meta = Meta { made_at: now(), model: MODEL_VERSION.to_string(), pool, logs, newest: newest.unwrap_or(0) };
    for (k, v) in [
        (META_AT, meta.made_at.to_string()),
        (META_VERSION, meta.model.clone()),
        (META_POOL, meta.pool.to_string()),
        (META_LOGS, meta.logs.to_string()),
        (META_NEWEST, meta.newest.to_string()),
    ] {
        snap.set_setting(k, &v).await?;
    }
    snap.vacuum().await?;
    snap.pool().close().await;

    let bytes = std::fs::read(&plain)?;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(&bytes)?;
    std::fs::write(out, gz.finish()?)?;
    std::fs::remove_file(&plain)?;
    Ok(meta)
}

/// The built-in snapshot, unpacked to a temporary file, with its meta.
async fn unpack_built_in() -> Result<(std::path::PathBuf, Meta)> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(BUILT_IN).read_to_end(&mut bytes).context("unpacking the league snapshot")?;
    // Its own name each time: two unpacks at once (the startup import and a
    // check) must not delete each other's file.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("hl-league-snapshot-{}-{n}.sqlite3", std::process::id()));
    std::fs::write(&path, bytes)?;
    let meta = read_meta(&path).await?;
    Ok((path, meta))
}

async fn read_meta(path: &Path) -> Result<Meta> {
    let snap = Db::connect(path).await?;
    let get = |k: &'static str| {
        let snap = snap.clone();
        async move { snap.get_setting(k).await.map(|v| v.unwrap_or_default()) }
    };
    let meta = Meta {
        made_at: get(META_AT).await?.parse().unwrap_or(0),
        model: get(META_VERSION).await?,
        pool: get(META_POOL).await?.parse().unwrap_or(0),
        logs: get(META_LOGS).await?.parse().unwrap_or(0),
        newest: get(META_NEWEST).await?.parse().unwrap_or(0),
    };
    snap.pool().close().await;
    Ok(meta)
}

/// What the built-in snapshot is, for `hl snapshot --check` and Settings.
pub async fn built_in_meta() -> Result<Meta> {
    let (path, meta) = unpack_built_in().await?;
    let _ = std::fs::remove_file(path);
    Ok(meta)
}

/// What [`apply_built_in`] did.
#[derive(Debug, Clone, Default)]
pub struct Applied {
    pub imported: bool,
    /// The snapshot's scale was taken: this install's own games must be
    /// rated again on it.
    pub rescore: bool,
}

/// Import the built-in snapshot, once per snapshot. Called at startup.
pub async fn apply_built_in(db: &Db) -> Result<Applied> {
    let (path, meta) = unpack_built_in().await?;
    let result = apply(db, &path, &meta).await;
    let _ = std::fs::remove_file(&path);
    result
}

async fn apply(db: &Db, path: &Path, meta: &Meta) -> Result<Applied> {
    if meta.model != MODEL_VERSION {
        // A snapshot from another model: its ratings mean nothing here. If
        // an older one was imported, its officials go back to "not
        // downloaded", so the league download may fetch them for real.
        if db.get_setting(KEY_VERSION).await?.is_some_and(|v| v != MODEL_VERSION) {
            db.release_snapshot_officials().await?;
        }
        tracing::warn!(snapshot = %meta.model, model = MODEL_VERSION, "league snapshot is for another model: not imported");
        return Ok(Applied::default());
    }
    if db.get_setting(KEY_AT).await?.as_deref() == Some(meta.made_at.to_string().as_str()) {
        return Ok(Applied::default());
    }

    let local_pool = db.rating_scale_size(MODEL_VERSION).await?.unwrap_or(0);
    let take_scale = local_pool < meta.pool;
    let mut tables = league_tables(true);
    if take_scale {
        // Replaced whole, not merged: a scale is one measurement.
        db.clear_baselines(MODEL_VERSION).await?;
        tables.extend(scale_tables());
    }
    db.copy_in(
        path,
        &tables,
        // Officials this install had listed but not downloaded are the
        // snapshot's now, so the download leaves them be.
        &["UPDATE main.league_log SET json_source = 'snapshot', raw_state = 'snapshot'
           WHERE json_source IS NULL AND log_id IN (SELECT log_id FROM src.league_log WHERE json_source = 'snapshot')"
            .into()],
    )
    .await?;
    db.set_setting(KEY_AT, &meta.made_at.to_string()).await?;
    db.set_setting(KEY_VERSION, &meta.model).await?;
    if take_scale {
        db.set_setting(KEY_POOL, &meta.pool.to_string()).await?;
    }
    tracing::info!(logs = meta.logs, take_scale, "league snapshot imported");
    Ok(Applied { imported: true, rescore: take_scale })
}

/// The pool the snapshot's scale was measured on, while it is the scale in
/// use: `rate_all` keeps it rather than measure a smaller pool of its own.
pub async fn pool(db: &Db) -> Result<Option<i64>> {
    if db.get_setting(KEY_VERSION).await?.as_deref() != Some(MODEL_VERSION) {
        return Ok(None);
    }
    Ok(db.get_setting(KEY_POOL).await?.and_then(|v| v.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_built_in_snapshot_reads_and_names_its_model() {
        let meta = built_in_meta().await.expect("the built-in snapshot unpacks");
        assert!(meta.made_at > 0 && meta.pool > 0 && meta.logs > 0, "{meta:?}");
        assert!(meta.model.starts_with('v'), "{meta:?}");
    }

    #[tokio::test]
    async fn a_new_install_gets_the_league_and_its_scale_from_the_built_in_snapshot() {
        // A file, as an install is.
        let dir = std::env::temp_dir().join(format!("hl-snapshot-new-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::connect(dir.join("hl.sqlite3")).await.unwrap();
        let meta = built_in_meta().await.unwrap();
        if meta.model != MODEL_VERSION {
            // A model change waits on a new snapshot (`hl snapshot --check`
            // stops the release); nothing to import meanwhile.
            assert!(!apply_built_in(&db).await.unwrap().imported);
            return;
        }
        let a = apply_built_in(&db).await.unwrap();
        assert!(a.imported && a.rescore);
        assert_eq!(db.rating_scale_size(MODEL_VERSION).await.unwrap(), Some(meta.pool));
        assert!(!crate::rating::load_baseline(&db).await.unwrap().is_empty(), "the league's baselines");
        let rated = db.rated_games(MODEL_VERSION, None).await.unwrap();
        assert!(rated.len() > 50_000, "every official's players, rated: {}", rated.len());
        assert!(db.league_rateable().await.unwrap().is_empty(), "nothing to rate: the logs are not here");
    }

    #[tokio::test]
    async fn an_import_fills_gaps_marks_officials_and_takes_the_scale_once() {
        let dir = std::env::temp_dir().join(format!("hl-snapshot-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A tiny snapshot: one official, one rating, a scale over 500.
        let snap_path = dir.join("snap.sqlite3");
        let snap = Db::connect(&snap_path).await.unwrap();
        for sql in [
            "INSERT INTO league_log (log_id, etf2l_match_id, map, played_at, json_source, raw_state) VALUES (10, 1, 'pl_vigil_rc10', 1000, 'snapshot', 'snapshot')".to_string(),
            "INSERT INTO league_log (log_id, etf2l_match_id, map, played_at) VALUES (11, 1, 'pl_vigil_rc10', 2000)".to_string(),
            format!("INSERT INTO league_rating (model_version, log_id, account_id, class, score, minutes, groups) VALUES ('{MODEL_VERSION}', 10, 7, 'sniper', 1.2, 30, '{{}}')"),
            format!("INSERT INTO rating_scale (model_version, mean, sd, n, made_at) VALUES ('{MODEL_VERSION}', 50, 18, 500, 'now')"),
            "INSERT INTO etf2l_team (team_id, name) VALUES (5, 'from the snapshot')".to_string(),
        ] {
            sqlx::query(&sql).execute(snap.pool()).await.unwrap();
        }
        snap.pool().close().await;
        let meta = Meta { made_at: 42, model: MODEL_VERSION.into(), pool: 500, logs: 1, newest: 1000 };

        let db = Db::connect(dir.join("mine.sqlite3")).await.unwrap();
        // This install already knows team 5 by another name, and listed log
        // 10 without downloading it.
        sqlx::query("INSERT INTO etf2l_team (team_id, name) VALUES (5, 'mine')").execute(db.pool()).await.unwrap();
        sqlx::query("INSERT INTO league_log (log_id, etf2l_match_id, map, played_at, picked) VALUES (10, 1, 'pl_vigil_rc10', 1000, 1)").execute(db.pool()).await.unwrap();
        assert_eq!(db.league_json_todo(10, true, 3).await.unwrap(), [10], "before: the download would fetch it");

        let a = apply(&db, &snap_path, &meta).await.unwrap();
        assert!(a.imported && a.rescore, "a new install takes the scale");
        let team: String = sqlx::query_scalar("SELECT name FROM etf2l_team WHERE team_id = 5").fetch_one(db.pool()).await.unwrap();
        assert_eq!(team, "mine", "rows already here win");
        let marks: Vec<(i64, Option<String>)> =
            sqlx::query_as("SELECT log_id, json_source FROM league_log ORDER BY log_id").fetch_all(db.pool()).await.unwrap();
        assert_eq!(marks, [(10, Some("snapshot".into())), (11, None)], "the downloaded one is the snapshot's, the other still to fetch");
        assert!(db.league_json_todo(10, true, 3).await.unwrap().iter().all(|id| *id != 10), "the download skips it");
        assert_eq!(pool(&db).await.unwrap(), Some(500));

        // Once per snapshot.
        assert!(!apply(&db, &snap_path, &meta).await.unwrap().imported);
        // A bigger pool of its own keeps its own scale.
        sqlx::query("UPDATE rating_scale SET n = 9000").execute(db.pool()).await.unwrap();
        let newer = Meta { made_at: 43, ..meta.clone() };
        assert!(!apply(&db, &snap_path, &newer).await.unwrap().rescore);

        // A snapshot for another model is not imported.
        let other = Meta { made_at: 44, model: "v0".into(), ..meta };
        assert!(!apply(&db, &snap_path, &other).await.unwrap().imported);
    }
}
