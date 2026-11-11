//! Tests for the sync audit's fixes: writes that belong together land together
//! (SY-1, SY-2), a log the sources lack is asked for on a schedule (API-3),
//! and the rest of the small bookkeeping the audit added.

use crate::{AimRow, ChatRow, DeathRow, Db, KillRow, LogsTfIndexRow, PathRow};
use hl_core::matchdata::{Format, LogFlags, NormalizedLog};

fn log(id: i64) -> NormalizedLog {
    NormalizedLog {
        log_id: id,
        title: Some("a game".into()),
        map: Some("pl_upward".into()),
        played_at: Some(1_000),
        duration_s: 1_200,
        red_score: 1,
        blue_score: 0,
        flags: LogFlags::default(),
        players: Vec::new(),
        rounds: Vec::new(),
    }
}

/// A log the index lists, so the fetch queue can see it.
async fn indexed(db: &Db, id: i64) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    db.upsert_logstf_rows(&[LogsTfIndexRow { log_id: id, title: None, map: None, played_at: Some(now - 3_600), player_count: Some(18), raw_json: "{}" }]).await.unwrap();
}

#[tokio::test]
async fn a_log_is_stored_whole_or_not_at_all() {
    let db = Db::connect_in_memory().await.unwrap();
    db.store_log(&log(1), "{}", Format::Highlander).await.unwrap();
    assert_eq!(db.raw_log(1).await.unwrap().as_deref(), Some("{}"));
    let matches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM match WHERE log_id = 1").fetch_one(db.pool()).await.unwrap();
    assert_eq!(matches, 1);

    // A failure between the raw JSON and the match rows (here: a table the
    // match write needs is gone) takes the raw JSON back with it, so the log
    // is still in the queue and not a stored log that was never read.
    sqlx::query("DROP TABLE match_event").execute(db.pool()).await.unwrap();
    assert!(db.store_log(&log(2), "{}", Format::Highlander).await.is_err());
    assert_eq!(db.raw_log(2).await.unwrap(), None, "the raw JSON must not outlive the failed match write");
}

#[tokio::test]
async fn a_raw_log_without_its_match_is_fetched_again() {
    let db = Db::connect_in_memory().await.unwrap();
    indexed(&db, 5).await;
    // Raw JSON only: what a cancelled sync used to leave.
    sqlx::query("INSERT INTO log_raw (log_id, json) VALUES (5, '{}')").execute(db.pool()).await.unwrap();
    assert_eq!(db.fetch_queue(3).await.unwrap(), vec![5]);
    db.store_log(&log(5), "{}", Format::Highlander).await.unwrap();
    assert!(db.fetch_queue(3).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_raw_server_log_is_stored_with_its_kills_or_not_at_all() {
    let db = Db::connect_in_memory().await.unwrap();
    let kill = KillRow {
        at_raw: 1,
        round_num: 1,
        live: true,
        killer: 1,
        killer_team: None,
        killer_class: None,
        victim: 2,
        victim_team: None,
        victim_class: None,
        weapon: "scattergun",
        custom: None,
        assister: None,
        killer_pos: None,
        victim_pos: None,
    };
    let chat = [ChatRow { at_raw: 1, account: Some(1), team_chat: false, message: "gg" }];
    db.store_rawlog_with_kills(1, b"zip", &[kill], &chat, true).await.unwrap();
    assert_eq!(db.rawlog(1).await.unwrap().as_deref(), Some(&b"zip"[..]));
    assert_eq!(db.kills_for_log(1).await.unwrap().len(), 1);
    assert!(!db.rawlog_unaligned(1).await.unwrap());

    // Unaligned is remembered, and a later aligned write clears it.
    db.store_rawlog_with_kills(1, b"zip", &[], &[], false).await.unwrap();
    assert!(db.rawlog_unaligned(1).await.unwrap());
    db.set_rawlog_aligned(1, true).await.unwrap();
    assert!(!db.rawlog_unaligned(1).await.unwrap());

    // The chat write fails after the zip and kills went in: none of it stays,
    // so the log is still in the raw-log queue.
    sqlx::query("DROP TABLE chat_event").execute(db.pool()).await.unwrap();
    assert!(db.store_rawlog_with_kills(2, b"zip", &[], &[], true).await.is_err());
    assert_eq!(db.rawlog(2).await.unwrap(), None, "the zip must not outlive the failed kills write");
}

fn aim_row() -> AimRow {
    AimRow {
        demo_id: 1,
        tick: 10,
        shooter: 1,
        at_raw: None,
        round_num: None,
        victim: Some(2),
        error_deg: 1.0,
        before_deg: 1.0,
        dx_deg: 0.0,
        dy_deg: 0.0,
        before_dx_deg: 0.0,
        before_dy_deg: 0.0,
        path: Vec::new(),
        flick_deg: 0.0,
        range_units: 100.0,
        height: 0.0,
        victim_seen: true,
        shooter_seen: true,
        headshot: false,
    }
}

fn path_row() -> PathRow {
    PathRow { demo_id: 1, seq: 0, account_id: 1, from_tick: 0, to_tick: 10, round_num: None, died: false, points: Vec::new(), caps: Vec::new() }
}

fn death_row() -> DeathRow {
    DeathRow {
        demo_id: 1,
        tick: 10,
        who: 1,
        at_raw: None,
        round_num: None,
        killer: Some(2),
        killer_range: None,
        killer_dx_deg: None,
        killer_dy_deg: None,
        nearest_mate: None,
        mates_near: 0,
        scoped: false,
        seen: false,
    }
}

#[tokio::test]
async fn aim_deaths_and_paths_land_together_and_the_log_is_marked_last() {
    let db = Db::connect_in_memory().await.unwrap();
    db.replace_aim_all(1, 14, &[aim_row()], &[death_row()], &[(1, 1, 100, 10)], &[path_row()]).await.unwrap();
    assert!(db.aim_logs(14).await.unwrap().contains(&1));
    assert_eq!(db.paths_for_log(1).await.unwrap().len(), 1);

    // The paths write fails (the table is gone): the aim write that would have
    // marked log 2 as read at this version never lands either, so it is read
    // again next time instead of keeping new aim beside missing paths.
    sqlx::query("DROP TABLE demo_path").execute(db.pool()).await.unwrap();
    assert!(db.replace_aim_all(2, 14, &[aim_row()], &[death_row()], &[], &[path_row()]).await.is_err());
    assert!(!db.aim_logs(14).await.unwrap().contains(&2));
    let aim: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demo_aim WHERE log_id = 2").fetch_one(db.pool()).await.unwrap();
    let deaths: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demo_death WHERE log_id = 2").fetch_one(db.pool()).await.unwrap();
    assert_eq!((aim, deaths), (0, 0));
}

#[tokio::test]
async fn a_log_the_sources_lack_is_asked_for_again_after_a_day_then_a_week_then_a_month() {
    let db = Db::connect_in_memory().await.unwrap();
    indexed(&db, 7).await;
    assert_eq!(db.fetch_queue(3).await.unwrap(), vec![7]);

    db.record_log_miss(7).await.unwrap();
    assert!(db.fetch_queue(3).await.unwrap().is_empty(), "not asked again at once");
    let ago = |s: i64| format!("UPDATE log_source_miss SET last_miss_at = unixepoch() - {s} WHERE log_id = 7");
    sqlx::query(&ago(86_400 + 60)).execute(db.pool()).await.unwrap();
    assert_eq!(db.fetch_queue(3).await.unwrap(), vec![7], "a day later it is asked for again");

    db.record_log_miss(7).await.unwrap();
    sqlx::query(&ago(2 * 86_400)).execute(db.pool()).await.unwrap();
    assert!(db.fetch_queue(3).await.unwrap().is_empty(), "the second wait is a week");
    sqlx::query(&ago(7 * 86_400 + 60)).execute(db.pool()).await.unwrap();
    assert_eq!(db.fetch_queue(3).await.unwrap(), vec![7]);

    db.record_log_miss(7).await.unwrap();
    sqlx::query(&ago(8 * 86_400)).execute(db.pool()).await.unwrap();
    assert!(db.fetch_queue(3).await.unwrap().is_empty(), "the third wait is a month");

    // Asking by hand ends the wait, and a stored log forgets it.
    db.clear_fetch_error(7).await.unwrap();
    assert_eq!(db.fetch_queue(3).await.unwrap(), vec![7]);
    db.record_log_miss(7).await.unwrap();
    db.store_log(&log(7), "{}", Format::Highlander).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM log_source_miss").fetch_one(db.pool()).await.unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn a_zip_that_would_not_open_is_asked_for_within_the_hour_not_the_week() {
    let db = Db::connect_in_memory().await.unwrap();
    db.upsert_logstf_rows(&[LogsTfIndexRow { log_id: 9, title: None, map: None, played_at: Some(1), player_count: Some(18), raw_json: "{}" }]).await.unwrap();
    sqlx::query("UPDATE log_index SET format = 'highlander' WHERE log_id = 9").execute(db.pool()).await.unwrap();
    db.store_log(&log(9), "{}", Format::Highlander).await.unwrap();
    assert_eq!(db.rawlog_queue().await.unwrap(), vec![9]);

    db.mark_rawlog_missing(9, "unreadable zip").await.unwrap();
    assert!(db.rawlog_queue().await.unwrap().is_empty());
    sqlx::query("UPDATE rawlog_missing SET checked_at = unixepoch() - 3700").execute(db.pool()).await.unwrap();
    assert_eq!(db.rawlog_queue().await.unwrap(), vec![9], "a cut-off download is tried again within the hour");

    // A log the source simply does not have keeps the week.
    db.mark_rawlog_missing(9, "drops.tf has no raw log").await.unwrap();
    sqlx::query("UPDATE rawlog_missing SET checked_at = unixepoch() - 3700").execute(db.pool()).await.unwrap();
    assert!(db.rawlog_queue().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_demo_that_failed_to_parse_is_skipped_until_the_file_changes() {
    let db = Db::connect_in_memory().await.unwrap();
    assert_eq!(db.demo_parse_failure(4, 1000, 50).await.unwrap(), None);
    db.record_demo_parse_failure(4, 1000, 50, "bad header").await.unwrap();
    assert_eq!(db.demo_parse_failure(4, 1000, 50).await.unwrap().as_deref(), Some("bad header"));
    // A different size, or a later modification, is a different file.
    assert_eq!(db.demo_parse_failure(4, 2000, 50).await.unwrap(), None);
    assert_eq!(db.demo_parse_failure(4, 1000, 51).await.unwrap(), None);
    db.clear_demo_parse_failure(4).await.unwrap();
    assert_eq!(db.demo_parse_failure(4, 1000, 50).await.unwrap(), None);
}

#[tokio::test]
async fn a_log_whose_demo_timeline_is_old_is_read_again() {
    let db = Db::connect_in_memory().await.unwrap();
    sqlx::query("INSERT INTO demo (demo_id, path, file_name, playdemo_arg, kind, size_bytes, mtime, map, server, recorder, playback_s, ticks)
                 VALUES (1, 'p', 'f', 'f', 'stv', 1, 1, 'm', '', '', 1.0, 1)")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO demo_link (demo_id, log_id, method, log_share) VALUES (1, 30, 'time', 1.0)").execute(db.pool()).await.unwrap();
    sqlx::query("INSERT INTO demo_timeline (demo_id, version, tick_rate, stride, head, samples, changes, objects, events, raw_bytes, stored_bytes)
                 VALUES (1, 1, 66.0, 1, '', x'', x'', x'', x'', 0, 0)")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.logs_with_old_timeline(2).await.unwrap(), vec![30]);
    assert!(db.logs_with_old_timeline(1).await.unwrap().is_empty());
}
