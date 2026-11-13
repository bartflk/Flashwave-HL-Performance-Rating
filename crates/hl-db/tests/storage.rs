//! Storage edge cases: upgrades from every shipped schema, settings, the
//! index's upserts, the fetch queue's order and back-off, and re-writing a
//! match. Everything runs on temporary databases.

use hl_core::matchdata::{ClassLine, LogFlags, NormalizedLog, PlayerLine, PlayerStats, RoundLine, RoundTeam, Team};
use hl_core::{SteamId, TfClass};
use hl_db::{Db, LogsTfIndexRow, MatchFilter, TrendsIndexRow, MAX_FETCH_TRIES};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::path::PathBuf;

static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hl-db-storage-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn scalar(db: &Db, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(db.pool()).await.unwrap()
}

// ---- migrations ------------------------------------------------------------

/// A database as an install of an older version left it: only the first `n`
/// migrations applied, with rows in what existed then.
async fn database_at(path: &std::path::Path, n: usize) {
    let opts = SqliteConnectOptions::new().filename(path).create_if_missing(true).foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
    let partial = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(MIGRATIONS.migrations[..n].to_vec()),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    partial.run(&pool).await.unwrap();
    let mut sql = vec![
        "INSERT INTO app_config (key, value) VALUES ('name', 'Zoë ☃ 日本')".to_string(),
        "INSERT INTO player (account_id, steamid64, steamid3, display_name, is_me) VALUES (7, '76561197960265735', '[U:1:7]', 'Zoë', 1)".to_string(),
    ];
    if n >= 2 {
        sql.push("INSERT INTO log_index (log_id, title, map, played_at, player_count) VALUES (1, 'one', 'pl_upward_f12', 100, 18), (2, 'two', NULL, NULL, NULL)".into());
        sql.push("INSERT INTO log_raw (log_id, json) VALUES (1, '{}')".into());
        sql.push(if n >= 40 {
            "INSERT INTO log_fetch_error (log_id, attempts, error, gone) VALUES (3, 2, 'timed out', 0), (4, 1, 'https://logs.tf/api/v1/log/4: HTTP 404 Not Found', 1)".into()
        } else {
            "INSERT INTO log_fetch_error (log_id, attempts, error) VALUES (3, 2, 'timed out'), (4, 1, 'https://logs.tf/api/v1/log/4: HTTP 404 Not Found')".into()
        });
    }
    if n >= 4 {
        sql.push(if n >= 23 {
            "INSERT INTO baseline (model_version, class, component, map, n, sorted_values) VALUES ('v1', 'sniper', 'dpm', 'pl_upward', 3, '[1,2,3]')".into()
        } else {
            "INSERT INTO baseline (model_version, class, component, n, sorted_values) VALUES ('v1', 'sniper', 'dpm', 3, '[1,2,3]')".into()
        });
    }
    for s in sql {
        sqlx::query(&s).execute(&pool).await.unwrap_or_else(|e| panic!("at {n}: {s}: {e}"));
    }
    pool.close().await;
}

#[tokio::test]
async fn a_database_from_every_shipped_version_upgrades_without_losing_rows() {
    let dir = temp("upgrade");
    let total = MIGRATIONS.migrations.len();
    assert!(total >= 45);
    for n in 1..=total {
        let path = dir.join(format!("at-{n}.sqlite3"));
        database_at(&path, n).await;
        let db = Db::connect(&path).await.unwrap_or_else(|e| panic!("upgrading from migration {n}: {e:#}"));

        assert_eq!(db.get_setting("name").await.unwrap().as_deref(), Some("Zoë ☃ 日本"), "from {n}");
        assert_eq!(db.get_me().await.unwrap(), Some(SteamId::from_account_id(7)), "from {n}");
        if n >= 2 {
            assert_eq!(scalar(&db, "SELECT COUNT(*) FROM log_index").await, 2, "from {n}");
            assert_eq!(db.raw_log(1).await.unwrap().as_deref(), Some("{}"), "from {n}");
            // 0040 files the deleted ones: a 404 is gone, a timeout is not.
            let failed = db.failed_logs().await.unwrap();
            assert_eq!(failed.len(), 2, "from {n}");
            let gone: Vec<(i64, bool)> = failed.iter().map(|f| (f.log_id, f.gone)).collect();
            assert!(gone.contains(&(4, true)), "from {n}: {gone:?}");
            assert!(gone.contains(&(3, false)), "from {n}: {gone:?}");
        }
        if n >= 4 {
            // 0023 moved every baseline into the pool of every map ('').
            let b = db.load_baselines("v1").await.unwrap();
            assert_eq!(b.len(), 1, "from {n}");
            assert_eq!(b[0].3, vec![1.0, 2.0, 3.0], "from {n}");
            assert_eq!(b[0].2.as_deref(), if n >= 23 { Some("pl_upward") } else { None }, "from {n}");
        }
        let check: String = sqlx::query_scalar("PRAGMA integrity_check").fetch_one(db.pool()).await.unwrap();
        assert_eq!(check, "ok", "from {n}");
        let broken = sqlx::query("PRAGMA foreign_key_check").fetch_all(db.pool()).await.unwrap();
        assert!(broken.is_empty(), "from {n}: {} broken foreign keys", broken.len());
        db.close().await;
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn opening_an_upgraded_database_again_changes_nothing() {
    let dir = temp("reopen");
    let path = dir.join("hl.sqlite3");
    database_at(&path, 20).await;
    let db = Db::connect(&path).await.unwrap();
    db.set_setting("k", "v").await.unwrap();
    db.close().await;
    let db = Db::connect(&path).await.unwrap();
    assert_eq!(db.get_setting("k").await.unwrap().as_deref(), Some("v"));
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM _sqlx_migrations").await, MIGRATIONS.migrations.len() as i64);
    db.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- settings and players --------------------------------------------------

#[tokio::test]
async fn settings_round_trip_unicode_empty_and_long_values() {
    let db = Db::connect_in_memory().await.unwrap();
    for (k, v) in [
        ("naïve ключ 日本", "Ünïcödé ☃ 😀 \u{200d} \"quoted\" 'single' ; DROP TABLE app_config; --"),
        ("empty", ""),
        ("multi", "line one\r\nline two\n\ttabbed"),
    ] {
        db.set_setting(k, v).await.unwrap();
        assert_eq!(db.get_setting(k).await.unwrap().as_deref(), Some(v), "{k}");
    }
    let long = "x".repeat(2_000_000);
    db.set_setting("long", &long).await.unwrap();
    assert_eq!(db.get_setting("long").await.unwrap().map(|s| s.len()), Some(2_000_000));
    // Keys are case sensitive, and clearing one touches no other.
    db.set_setting("Key", "a").await.unwrap();
    db.set_setting("key", "b").await.unwrap();
    db.clear_setting("Key").await.unwrap();
    assert_eq!(db.get_setting("Key").await.unwrap(), None);
    assert_eq!(db.get_setting("key").await.unwrap().as_deref(), Some("b"));
    // Clearing what is not there is fine.
    db.clear_setting("never set").await.unwrap();
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM app_config WHERE key = 'empty'").await, 1);
}

#[tokio::test]
async fn the_switches_default_off_and_flip() {
    let db = Db::connect_in_memory().await.unwrap();
    assert!(!db.all_history().await.unwrap());
    assert!(!db.auto_delete_demos().await.unwrap());
    db.set_all_history(true).await.unwrap();
    db.set_auto_delete_demos(true).await.unwrap();
    assert!(db.all_history().await.unwrap() && db.auto_delete_demos().await.unwrap());
    db.set_all_history(false).await.unwrap();
    assert!(!db.all_history().await.unwrap());
}

#[tokio::test]
async fn a_player_upsert_keeps_the_name_it_has_when_given_none() {
    let db = Db::connect_in_memory().await.unwrap();
    let id = SteamId::from_account_id(42);
    db.upsert_player(id, Some("Zoë")).await.unwrap();
    db.upsert_player(id, None).await.unwrap();
    let name: Option<String> = sqlx::query_scalar("SELECT display_name FROM player WHERE account_id = 42").fetch_one(db.pool()).await.unwrap();
    assert_eq!(name.as_deref(), Some("Zoë"));
    db.upsert_player(id, Some("Zoë 2")).await.unwrap();
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM player").await, 1, "an upsert upserts");
    // The same owner twice is still one owner.
    db.set_me(id).await.unwrap();
    db.set_me(id).await.unwrap();
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM player WHERE is_me = 1").await, 1);
    assert_eq!(db.get_me().await.unwrap(), Some(id));
}

#[tokio::test]
async fn sync_state_keeps_its_cursor_through_a_failed_run_and_forgets_the_error() {
    let db = Db::connect_in_memory().await.unwrap();
    assert!(!db.source_synced("trends").await.unwrap());
    assert_eq!(db.trends_cursor().await.unwrap(), None);
    db.record_sync("trends", Some("1700000000"), None).await.unwrap();
    assert!(db.source_synced("trends").await.unwrap());
    assert_eq!(db.trends_cursor().await.unwrap(), Some(1_700_000_000));
    // A failed run keeps the cursor and is not "synced".
    db.record_sync("trends", None, Some("HTTP 503")).await.unwrap();
    assert_eq!(db.trends_cursor().await.unwrap(), Some(1_700_000_000));
    assert!(!db.source_synced("trends").await.unwrap());
    // The next good run clears the error.
    db.record_sync("trends", None, None).await.unwrap();
    assert!(db.source_synced("trends").await.unwrap());
    // A cursor that is not a number reads as none.
    db.record_sync("trends", Some("garbage"), None).await.unwrap();
    assert_eq!(db.trends_cursor().await.unwrap(), None);
}

// ---- the index -------------------------------------------------------------

fn trends(log_id: i64, title: Option<&'static str>, map: Option<&'static str>, format: Option<&'static str>) -> TrendsIndexRow<'static> {
    TrendsIndexRow {
        log_id,
        title,
        map,
        played_at: Some(1_700_000_000),
        duration_s: Some(1800),
        format,
        league: None,
        etf2l_match_id: None,
        demos_tf_id: None,
        duplicate_of: None,
        raw_json: "{}",
    }
}

fn logstf(log_id: i64, played_at: Option<i64>, count: Option<i64>) -> LogsTfIndexRow<'static> {
    LogsTfIndexRow { log_id, title: Some("a game"), map: Some("pl_upward_f12"), played_at, player_count: count, raw_json: "{}" }
}

#[tokio::test]
async fn re_indexing_a_log_updates_it_and_never_blanks_what_it_knew() {
    let db = Db::connect_in_memory().await.unwrap();
    let a = trends(1, Some("Real title"), Some("pl_upward_f12"), Some("highlander"));
    db.upsert_trends_rows(&[a, trends(1, Some("Real title"), Some("pl_upward_f12"), Some("highlander"))]).await.unwrap();
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM log_index").await, 1, "the same log twice is one row");

    // A later row with an empty title or map must not erase a real one.
    db.upsert_trends_rows(&[trends(1, Some(""), Some(""), Some("highlander"))]).await.unwrap();
    let (title, map): (String, String) = sqlx::query_as("SELECT title, map FROM log_index WHERE log_id = 1").fetch_one(db.pool()).await.unwrap();
    assert_eq!((title.as_str(), map.as_str()), ("Real title", "pl_upward_f12"));
    // Nor a missing one.
    db.upsert_trends_rows(&[trends(1, None, None, None)]).await.unwrap();
    let (title, fmt): (String, String) = sqlx::query_as("SELECT title, format FROM log_index WHERE log_id = 1").fetch_one(db.pool()).await.unwrap();
    assert_eq!((title.as_str(), fmt.as_str()), ("Real title", "highlander"));

    // An empty batch is nothing.
    db.upsert_trends_rows(&[]).await.unwrap();
    db.upsert_logstf_rows(&[]).await.unwrap();
}

#[tokio::test]
async fn logs_tf_only_fills_gaps_and_keeps_a_known_player_count() {
    let db = Db::connect_in_memory().await.unwrap();
    db.upsert_logstf_rows(&[logstf(1, Some(500), Some(18))]).await.unwrap();
    // A later search row without a count does not forget it.
    db.upsert_logstf_rows(&[logstf(1, None, None)]).await.unwrap();
    let (played, count): (Option<i64>, Option<i64>) = sqlx::query_as("SELECT played_at, player_count FROM log_index WHERE log_id = 1").fetch_one(db.pool()).await.unwrap();
    assert_eq!((played, count), (Some(500), Some(18)));
    // trends.tf's time wins over logs.tf's.
    db.upsert_logstf_rows(&[logstf(1, Some(999), Some(20))]).await.unwrap();
    assert_eq!(db.log_played_at(1).await.unwrap(), Some(500));
    assert_eq!(db.log_played_at(404).await.unwrap(), None, "a log nobody indexed has no time");
    assert!(db.is_indexed(1).await.unwrap() && !db.is_indexed(404).await.unwrap());
}

#[tokio::test]
async fn supersessions_are_recomputed_from_scratch() {
    let db = Db::connect_in_memory().await.unwrap();
    for id in 1..=3 {
        db.upsert_logstf_rows(&[logstf(id, Some(id), Some(18))]).await.unwrap();
    }
    db.apply_supersessions(&HashMap::from([(1, 3), (2, 3)])).await.unwrap();
    assert_eq!(db.superseded_by(1).await.unwrap(), Some(3));
    assert_eq!(db.superseded_by(3).await.unwrap(), None);
    assert_eq!(db.superseded_by(99).await.unwrap(), None, "an unknown log is nobody's part");
    // The next pass says only 2 is a part: 1 is free again.
    db.apply_supersessions(&HashMap::from([(2, 3)])).await.unwrap();
    assert_eq!(db.superseded_by(1).await.unwrap(), None);
    db.apply_supersessions(&HashMap::new()).await.unwrap();
    assert_eq!(db.superseded_by(2).await.unwrap(), None);
    let parts = db.parts_of(3).await.unwrap();
    assert!(parts.is_empty());
}

// ---- the fetch queue -------------------------------------------------------

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

async fn index(db: &Db, id: i64, played_at: Option<i64>) {
    db.upsert_logstf_rows(&[LogsTfIndexRow { log_id: id, title: None, map: None, played_at, player_count: Some(18), raw_json: "{}" }]).await.unwrap();
}

#[tokio::test]
async fn the_queue_is_newest_first_and_breaks_ties_the_same_way_every_time() {
    let db = Db::connect_in_memory().await.unwrap();
    let t = now() - 3600;
    // Three at the very same second, one older, one undated.
    for (id, at) in [(5, Some(t)), (9, Some(t)), (7, Some(t)), (8, Some(t - 10)), (6, None)] {
        index(&db, id, at).await;
    }
    for _ in 0..3 {
        assert_eq!(db.fetch_queue(3).await.unwrap(), vec![9, 7, 5, 8, 6]);
    }
}

#[tokio::test]
async fn a_failing_log_waits_exactly_a_week_after_its_third_try() {
    let db = Db::connect_in_memory().await.unwrap();
    for id in 1..=4 {
        index(&db, id, Some(now() - 86_400)).await;
        for _ in 0..3 {
            db.record_fetch_error(id, "timed out", false).await.unwrap();
        }
    }
    // Waiting since: just over a week, just under, and a clock set back so
    // the last try lies in the future.
    for (id, when) in [(1, "-7 days"), (2, "-6 days', '-23 hours"), (3, "+1 day")] {
        sqlx::query(&format!("UPDATE log_fetch_error SET last_attempt_at = datetime('now', '{when}') WHERE log_id = {id}")).execute(db.pool()).await.unwrap();
    }
    sqlx::query("UPDATE log_fetch_error SET attempts = ?1, last_attempt_at = datetime('now', '-9 days') WHERE log_id = 4")
        .bind(MAX_FETCH_TRIES)
        .execute(db.pool())
        .await
        .unwrap();
    let queue = db.fetch_queue(3).await.unwrap();
    assert_eq!(queue, vec![1], "exactly a week is due; a day short, a future try and out-of-tries are not");

    // The sixth failure is the last: it is then given up and the stats say so.
    for _ in 0..(MAX_FETCH_TRIES - 3) {
        db.record_fetch_error(1, "timed out", false).await.unwrap();
    }
    assert!(db.failed_logs().await.unwrap().iter().find(|f| f.log_id == 1).unwrap().given_up);
}

#[tokio::test]
async fn storing_a_log_ends_its_failures_and_a_stored_log_is_not_queued() {
    let db = Db::connect_in_memory().await.unwrap();
    index(&db, 1, Some(now() - 100)).await;
    db.record_fetch_error(1, "nope", false).await.unwrap();
    db.store_raw_log(1, "{\"a\":1}").await.unwrap();
    db.store_raw_log(1, "{\"a\":2}").await.unwrap(); // a second store replaces
    assert_eq!(db.raw_log(1).await.unwrap().as_deref(), Some("{\"a\":2}"));
    assert_eq!(db.raw_log_ids().await.unwrap(), vec![1]);
    assert!(db.failed_logs().await.unwrap().is_empty());
    // Stored but never written as a match (a cancelled sync): fetched again (SY-1).
    assert_eq!(db.fetch_queue(3).await.unwrap(), vec![1]);
    sqlx::query("UPDATE log_index SET format = 'highlander' WHERE log_id = 1").execute(db.pool()).await.unwrap();
    db.write_match(&log(1, vec![], 0)).await.unwrap();
    assert!(db.fetch_queue(3).await.unwrap().is_empty());
}

#[tokio::test]
async fn deleted_logs_are_rescued_newest_first_with_a_stable_order() {
    let db = Db::connect_in_memory().await.unwrap();
    let t = now() - 500;
    for id in [10, 11, 12, 13] {
        index(&db, id, Some(t)).await;
        db.record_fetch_error(id, "HTTP 404", true).await.unwrap();
    }
    index(&db, 14, Some(t + 100)).await;
    db.record_fetch_error(14, "HTTP 404", true).await.unwrap();
    assert_eq!(db.gone_rescue_todo(3).await.unwrap(), vec![14, 13, 12]);
    db.mark_rescue_tried(&[14, 13, 12]).await.unwrap();
    assert_eq!(db.gone_rescue_todo(10).await.unwrap(), vec![11, 10]);
    db.mark_rescue_tried(&[]).await.unwrap();
    assert_eq!(db.gone_rescue_todo(0).await.unwrap(), Vec::<i64>::new());
}

// ---- writing a match -------------------------------------------------------

fn player(account: u32, team: Team, class: TfClass, kills: i64) -> PlayerLine {
    PlayerLine {
        id: SteamId::from_account_id(account),
        name: Some(format!("Pläyer ☃ {account}")),
        team,
        stats: PlayerStats { kills, deaths: 1, dmg: 1000, ..Default::default() },
        classes: vec![ClassLine { class, time_s: 1500, kills, assists: 0, deaths: 1, dmg: 1000, shots: 0, hits: 0 }],
        vs: vec![],
    }
}

fn log(id: i64, players: Vec<PlayerLine>, rounds: usize) -> NormalizedLog {
    NormalizedLog {
        log_id: id,
        title: Some("a game".into()),
        map: Some("pl_upward_f12".into()),
        played_at: Some(1_700_000_000 + id),
        duration_s: 1800,
        red_score: 3,
        blue_score: 1,
        flags: LogFlags::default(),
        players,
        rounds: (1..=rounds as i64)
            .map(|n| RoundLine {
                round_num: n,
                start_time: Some(n * 100),
                length_s: Some(90),
                winner: Some(Team::Red),
                firstcap: None,
                red: RoundTeam::default(),
                blue: RoundTeam::default(),
                events: vec![],
                colours_swapped: false,
            })
            .collect(),
    }
}

async fn indexed_highlander(db: &Db, id: i64) {
    index(db, id, Some(1_700_000_000 + id)).await;
    sqlx::query("UPDATE log_index SET format = 'highlander' WHERE log_id = ?1").bind(id).execute(db.pool()).await.unwrap();
}

#[tokio::test]
async fn writing_a_match_again_replaces_it_and_touches_no_other() {
    let db = Db::connect_in_memory().await.unwrap();
    for id in [1, 2] {
        indexed_highlander(&db, id).await;
    }
    db.write_match(&log(1, vec![player(10, Team::Red, TfClass::Sniper, 5), player(11, Team::Blue, TfClass::Medic, 0)], 3)).await.unwrap();
    db.write_match(&log(2, vec![player(10, Team::Blue, TfClass::Spy, 9)], 2)).await.unwrap();

    // Match 1 again: one player left, the other gone, one round fewer.
    db.write_match(&log(1, vec![player(10, Team::Red, TfClass::Sniper, 7)], 2)).await.unwrap();
    db.write_match(&log(1, vec![player(10, Team::Red, TfClass::Sniper, 7)], 2)).await.unwrap();

    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM match").await, 2);
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM match_player WHERE log_id = 1").await, 1, "the player who left the log is gone");
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM match_player_class WHERE log_id = 1").await, 1);
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM match_round WHERE log_id = 1").await, 2);
    assert_eq!(scalar(&db, "SELECT kills FROM match_player WHERE log_id = 1").await, 7);
    // Match 2 is as it was.
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM match_player WHERE log_id = 2").await, 1);
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM match_round WHERE log_id = 2").await, 2);
    assert_eq!(scalar(&db, "SELECT kills FROM match_player WHERE log_id = 2").await, 9);
}

#[tokio::test]
async fn a_match_with_no_players_or_rounds_is_stored_and_listed() {
    let db = Db::connect_in_memory().await.unwrap();
    indexed_highlander(&db, 1).await;
    db.write_match(&log(1, vec![], 0)).await.unwrap();
    let page = db.list_matches(Some(10), &MatchFilter { limit: 10, ..Default::default() }).await.unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items.len(), 1);
    assert!(page.items[0].me.is_none(), "the owner is not in it");
}

#[tokio::test]
async fn the_match_list_pages_without_gaps_or_repeats_and_ties_keep_their_order() {
    let db = Db::connect_in_memory().await.unwrap();
    // Six matches, three of them at the very same time.
    for id in 1..=6 {
        indexed_highlander(&db, id).await;
        let mut l = log(id, vec![player(10, Team::Red, TfClass::Soldier, 5)], 1);
        l.played_at = Some(if id <= 3 { 1_700_000_000 } else { 1_700_000_000 + id });
        db.write_match(&l).await.unwrap();
    }
    let filter = |limit, offset, sort: Option<&str>| MatchFilter { limit, offset, sort: sort.map(str::to_string), model_version: "v".into(), ..Default::default() };
    for sort in [None, Some("kills"), Some("dpm")] {
        let mut seen = Vec::new();
        for offset in (0..6).step_by(2) {
            let page = db.list_matches(Some(10), &filter(2, offset, sort)).await.unwrap();
            assert_eq!(page.total, 6);
            seen.extend(page.items.iter().map(|m| m.log_id));
        }
        let all: Vec<i64> = db.list_matches(Some(10), &filter(100, 0, sort)).await.unwrap().items.iter().map(|m| m.log_id).collect();
        assert_eq!(seen, all, "pages join up under {sort:?}");
        assert_eq!(all, vec![6, 5, 4, 3, 2, 1], "newest first, then the larger id, under {sort:?}");
    }
    // Past the end is empty, not an error.
    assert!(db.list_matches(Some(10), &filter(10, 60, None)).await.unwrap().items.is_empty());
}

#[tokio::test]
async fn the_classes_a_player_filters_by_are_listed_in_a_fixed_order() {
    let db = Db::connect_in_memory().await.unwrap();
    for (id, class) in [(1, TfClass::Spy), (2, TfClass::Sniper), (3, TfClass::Medic), (4, TfClass::Sniper), (5, TfClass::Spy)] {
        indexed_highlander(&db, id).await;
        db.write_match(&log(id, vec![player(10, Team::Red, class, 1)], 1)).await.unwrap();
    }
    for _ in 0..3 {
        let (classes, maps) = db.played_classes_and_maps(10, &MatchFilter::default()).await.unwrap();
        let names: Vec<(&str, i64)> = classes.iter().map(|(c, n)| (c.as_str(), *n)).collect();
        assert_eq!(names, vec![("sniper", 2), ("spy", 2), ("medic", 1)], "ties by name");
        assert_eq!(maps, vec![("upward".to_string(), 5)]);
    }
    // A player who is in none of them has nothing to filter by.
    let (classes, maps) = db.played_classes_and_maps(99, &MatchFilter::default()).await.unwrap();
    assert!(classes.is_empty() && maps.is_empty());
}

// ---- copying between files -------------------------------------------------

#[tokio::test]
async fn copying_in_takes_what_the_two_share_filters_rows_and_refuses_a_missing_file() {
    use hl_db::TableCopy;
    let dir = temp("copyin");
    // A file from another schema: one table with only some of our columns,
    // and none of the others.
    let theirs = dir.join("theirs.sqlite3");
    let opts = SqliteConnectOptions::new().filename(&theirs).create_if_missing(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
    for sql in [
        "CREATE TABLE etf2l_team (team_id INTEGER PRIMARY KEY, name TEXT, flavour TEXT)",
        "INSERT INTO etf2l_team VALUES (1, 'Zoë ☃', 'x'), (2, 'two', 'y'), (3, 'three', 'z')",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool.close().await;

    let ours = Db::connect(dir.join("ours.sqlite3")).await.unwrap();
    let tables = [
        TableCopy { table: "etf2l_team", filter: Some("team_id <= 2".into()), keep_ours: true },
        // Not in their file at all: skipped, not an error.
        TableCopy { table: "etf2l_competition", filter: None, keep_ours: true },
    ];
    ours.copy_in(&theirs, &tables, &[]).await.unwrap();
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM etf2l_team ORDER BY team_id").fetch_all(ours.pool()).await.unwrap();
    assert_eq!(names, ["Zoë ☃", "two"], "the filter held, and the extra column was left behind");
    // Again: nothing doubles.
    ours.copy_in(&theirs, &tables, &[]).await.unwrap();
    assert_eq!(scalar(&ours, "SELECT COUNT(*) FROM etf2l_team").await, 2);

    // A file that is not there is an error, and does not appear by being asked about.
    let missing = dir.join("missing.sqlite3");
    assert!(ours.copy_in(&missing, &tables, &[]).await.is_err());
    assert!(!missing.exists());
    // And the database still works afterwards: nothing is left attached.
    ours.copy_in(&theirs, &tables, &[]).await.unwrap();
    ours.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}
