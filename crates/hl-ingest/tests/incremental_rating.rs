//! Rating a log as it lands must give the same number as rating everything.
//!
//! A sync scores each log the moment it arrives, against the baselines the
//! last full pass stored, so a match appears in the list with its rating
//! already on it. That is only honest if the number does not move afterwards
//! for no reason — so this pins the two paths together on real logs.

use hl_db::Db;
use hl_ingest::classify::classify;
use hl_ingest::normalize::normalize;
use hl_rating::{Weights, MODEL_VERSION};
use serde_json::Value;

fn load(name: &str) -> (i64, String) {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect(&path);
    let v: Value = serde_json::from_str(&text).expect("fixture is JSON");
    let id = name
        .rsplit('_')
        .next()
        .and_then(|s| s.trim_end_matches(".json").parse().ok())
        .expect("the fixture name ends in its log id");
    let _ = v;
    (id, text)
}

/// Store a log the way a sync would: the index row, the raw JSON, the format.
async fn store(db: &Db, name: &str) -> i64 {
    let (log_id, text) = load(name);
    let value: Value = serde_json::from_str(&text).unwrap();
    let log = normalize(log_id, &value).unwrap();
    db.upsert_logstf_rows(&[hl_db::LogsTfIndexRow {
        log_id,
        title: log.title.as_deref(),
        map: log.map.as_deref(),
        played_at: log.played_at,
        player_count: Some(log.players.len() as i64),
        raw_json: "{}",
    }])
    .await
    .unwrap();
    db.store_raw_log(log_id, &text).await.unwrap();
    db.set_heuristic_format(log_id, classify(&log)).await.unwrap();
    log_id
}

async fn scores(db: &Db, log_id: i64) -> Vec<(i64, String, f64)> {
    sqlx::query_as(
        "SELECT account_id, class, score FROM rating
         WHERE log_id = ?1 AND model_version = ?2 ORDER BY account_id",
    )
    .bind(log_id)
    .bind(MODEL_VERSION)
    .fetch_all(db.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn a_log_rated_on_arrival_scores_the_same_as_one_rated_with_the_rest() {
    let db = Db::connect_in_memory().await.unwrap();
    let (w, _) = Weights::load(std::path::Path::new("does-not-exist.toml"));

    let a = store(&db, "hl_2023_3445078.json").await;
    let b = store(&db, "hl_sub_19p_4114301.json").await;

    // The full pass: builds the baselines and scores everything.
    let full = hl_ingest::rate_all(&db, None, &w, |_| {}).await.unwrap();
    assert!(full.rated > 0, "the fixtures must produce ratings");
    let (was_a, was_b) = (scores(&db, a).await, scores(&db, b).await);
    assert!(!was_a.is_empty() && !was_b.is_empty());

    // One log re-rated on its own, as the fetch loop does it.
    let scored = hl_ingest::rating::rate_logs(&db, &w, &[a]).await.unwrap();
    assert_eq!(scored, was_a.len(), "every performance is scored again");

    assert_eq!(scores(&db, a).await, was_a, "the same numbers");
    assert_eq!(scores(&db, b).await, was_b, "and the other log is untouched");
}

#[tokio::test]
async fn nothing_is_rated_before_there_are_baselines_to_measure_against() {
    let db = Db::connect_in_memory().await.unwrap();
    let (w, _) = Weights::load(std::path::Path::new("does-not-exist.toml"));
    let a = store(&db, "hl_2023_3445078.json").await;

    // A first-ever sync: no full pass has run, so there is no pool yet.
    assert_eq!(hl_ingest::rating::rate_logs(&db, &w, &[a]).await.unwrap(), 0);
    assert!(scores(&db, a).await.is_empty(), "and nothing invented in the meantime");
}
