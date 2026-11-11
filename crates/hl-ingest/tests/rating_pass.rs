//! The full rating pass over a temp database: what it stores must be finite,
//! unique, on the pool's own scale, and the same when run twice.

use hl_db::Db;
use hl_ingest::classify::classify;
use hl_ingest::normalize::normalize;
use hl_rating::{Weights, MODEL_VERSION};
use serde_json::Value;

const FIXTURES: [(&str, i64); 4] = [
    ("hl_2014_steamid2_295610.json", 295610),
    ("hl_2023_3445078.json", 3445078),
    ("hl_sub_19p_4114301.json", 4114301),
    ("hl_combined_6rounds_4109131.json", 4109131),
];

async fn store(db: &Db, name: &str, log_id: i64) {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect(&path);
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
}

fn weights() -> Weights {
    Weights::load(std::path::Path::new("does-not-exist.toml")).0
}

async fn ratings(db: &Db) -> Vec<(i64, i64, String, f64, f64)> {
    sqlx::query_as("SELECT log_id, account_id, class, score, minutes FROM rating WHERE model_version = ?1 ORDER BY log_id, account_id, class")
        .bind(MODEL_VERSION)
        .fetch_all(db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_pass_over_nothing_rates_nothing_and_does_not_fail() {
    let db = Db::connect_in_memory().await.unwrap();
    let s = hl_ingest::rate_all(&db, None, &weights(), |_| {}).await.unwrap();
    assert_eq!((s.logs, s.performances, s.rated, s.mine), (0, 0, 0, 0));
    assert!(ratings(&db).await.is_empty());
    assert!(hl_ingest::rating::load_scale(&db).await.unwrap().is_none(), "no scale is stored from no games");
}

#[tokio::test]
async fn what_a_pass_stores_is_finite_unique_and_centred_on_one() {
    let db = Db::connect_in_memory().await.unwrap();
    for (name, id) in FIXTURES {
        store(&db, name, id).await;
    }
    let s = hl_ingest::rate_all(&db, None, &weights(), |_| {}).await.unwrap();
    assert_eq!(s.logs, 4);
    assert!(s.rated > 40 && s.rated <= s.performances, "{s:?}");
    let rows = ratings(&db).await;
    assert_eq!(rows.len(), s.rated);
    let mut seen = std::collections::HashSet::new();
    for (log, account, class, score, minutes) in &rows {
        assert!(seen.insert((*log, *account)), "one rating per player per log: {log} {account}");
        assert!(score.is_finite() && *score >= 0.0, "{log} {account} {class}: {score}");
        assert!(*minutes >= weights().general.min_minutes, "a rated game is a long enough one: {minutes}");
    }
    let mean = rows.iter().map(|r| r.3).sum::<f64>() / rows.len() as f64;
    assert!((mean - 1.0).abs() < 0.05, "the pool is its own average: {mean}");
    let scale = hl_ingest::rating::load_scale(&db).await.unwrap().expect("a scale is stored");
    assert!(scale.pooled.sd > 0.0 && scale.pooled.mean.is_finite());
}

#[tokio::test]
async fn a_second_pass_changes_nothing() {
    let db = Db::connect_in_memory().await.unwrap();
    for (name, id) in FIXTURES {
        store(&db, name, id).await;
    }
    let w = weights();
    hl_ingest::rate_all(&db, None, &w, |_| {}).await.unwrap();
    let first = ratings(&db).await;
    hl_ingest::rate_all(&db, None, &w, |_| {}).await.unwrap();
    assert_eq!(ratings(&db).await, first, "rating twice is rating once");
}

#[tokio::test]
async fn a_single_log_is_rated_against_itself_without_dividing_by_nothing() {
    let db = Db::connect_in_memory().await.unwrap();
    store(&db, "hl_sub_19p_4114301.json", 4114301).await;
    let s = hl_ingest::rate_all(&db, None, &weights(), |_| {}).await.unwrap();
    for r in ratings(&db).await {
        assert!(r.3.is_finite(), "{r:?}");
    }
    assert_eq!(ratings(&db).await.len(), s.rated);
}
