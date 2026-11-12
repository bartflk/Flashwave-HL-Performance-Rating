//! What a rebuild (Settings > Data) must do, from the audit of 12 Nov 2026.
//!
//! - One stored trends.tf row that no longer reads must not stop it: the sync
//!   skips such a row (audit SY-7), and the rebuild used to fail on it before
//!   a single log was read.
//! - Every log's player numbers are read again, not only the missing ones,
//!   so a change to how they are read reaches the logs already stored.

use hl_db::Db;
use hl_ingest::classify::classify;
use hl_ingest::normalize::normalize;
use serde_json::Value;

const FIXTURE: &str = "hl_2023_3445078.json";
const LOG_ID: i64 = 3_445_078;

async fn store_fixture(db: &Db) {
    let path = format!("{}/tests/fixtures/{FIXTURE}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect(&path);
    let value: Value = serde_json::from_str(&text).unwrap();
    let log = normalize(LOG_ID, &value).unwrap();
    db.store_log(&log, &text, classify(&log)).await.unwrap();
}

fn trends_row<'a>(log_id: i64, raw: &'a str) -> hl_db::TrendsIndexRow<'a> {
    hl_db::TrendsIndexRow {
        log_id,
        title: None,
        map: None,
        played_at: None,
        duration_s: None,
        format: None,
        league: None,
        etf2l_match_id: None,
        demos_tf_id: None,
        duplicate_of: None,
        raw_json: raw,
    }
}

#[tokio::test]
async fn a_stored_trends_row_that_will_not_read_does_not_stop_a_rebuild() {
    let db = Db::connect_in_memory().await.unwrap();
    store_fixture(&db).await;
    let good = format!(r#"{{"logid":{LOG_ID},"title":"Kebab vs Official","map":"pl_upward","time":1700000000}}"#);
    // `logid` as text: what a changed trends.tf row, or an old damaged one, looks like.
    let bad = r#"{"logid":"not a number"}"#;
    db.upsert_trends_rows(&[trends_row(LOG_ID, &good), trends_row(LOG_ID + 1, bad)]).await.unwrap();

    let mut reprocessed = 0;
    let stats = hl_ingest::reprocess(&db, |p| {
        if let hl_ingest::Progress::Reprocessing { done, total } = p {
            if done == total {
                reprocessed = total;
            }
        }
    })
    .await
    .expect("the rebuild carries on past the row");
    assert_eq!(reprocessed, 1, "the stored log was read again");
    assert!(stats.indexed >= 1);
}

#[tokio::test]
async fn a_rebuild_reads_every_logs_player_numbers_again() {
    let db = Db::connect_in_memory().await.unwrap();
    store_fixture(&db).await;
    assert_eq!(db.logs_with_files().await.unwrap(), vec![LOG_ID]);

    // The first fill writes the rows.
    assert_eq!(hl_ingest::logstat::fill_missing(&db, None, |_, _| {}).await.unwrap(), 1);
    let rows = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM log_stat WHERE log_id = ?1").bind(LOG_ID).fetch_one(db.pool()).await.unwrap()
    };
    let before = rows().await;
    assert!(before > 0);

    // Damage one row's numbers, as a reading that has since been fixed would
    // have left them. fill_missing sees nothing to do; refill_all puts it right.
    sqlx::query("UPDATE log_stat SET kills = -1 WHERE log_id = ?1").bind(LOG_ID).execute(db.pool()).await.unwrap();
    assert_eq!(hl_ingest::logstat::fill_missing(&db, None, |_, _| {}).await.unwrap(), 0, "nothing is missing");

    let mut last = (0, 0);
    let n = hl_ingest::logstat::refill_all(&db, |done, total| last = (done, total)).await.unwrap();
    assert_eq!(n, 1);
    assert_eq!(last, (1, 1), "the bar ends full");
    assert_eq!(rows().await, before, "replaced, not added to");
    let damaged: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM log_stat WHERE log_id = ?1 AND kills < 0").bind(LOG_ID).fetch_one(db.pool()).await.unwrap();
    assert_eq!(damaged, 0);
}
