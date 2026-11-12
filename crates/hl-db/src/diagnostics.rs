//! What Settings > Diagnostics reads: each source's last sync, and the
//! owner's ratings under each model the database still holds.

use crate::Db;
use anyhow::Result;
use serde::Serialize;

/// One source's row of `sync_state`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SourceState {
    pub source: String,
    pub cursor: Option<String>,
    /// SQLite's `datetime('now')`: UTC, "YYYY-MM-DD HH:MM:SS".
    pub last_run_at: Option<String>,
    pub last_error: Option<String>,
}

/// One class's games rated under both models.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClassShift {
    pub class: String,
    pub games: i64,
    pub before: f64,
    pub after: f64,
}

/// One game whose rating moved, for the "moved the most" list.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GameShift {
    pub log_id: i64,
    pub class: String,
    pub map: Option<String>,
    pub played_at: Option<i64>,
    pub before: f64,
    pub after: f64,
}

/// "v14" -> 14, so "v9" sorts before "v14". A version that is not `v<n>` sorts first.
pub fn model_number(version: &str) -> i64 {
    version.strip_prefix('v').and_then(|n| n.parse().ok()).unwrap_or(-1)
}

impl Db {
    pub async fn sync_states(&self) -> Result<Vec<SourceState>> {
        Ok(sqlx::query_as("SELECT source, cursor, last_run_at, last_error FROM sync_state ORDER BY source")
            .fetch_all(self.pool())
            .await?)
    }

    /// Every model version with the owner's ratings, and how many, oldest first.
    pub async fn rating_versions(&self, account_id: u32) -> Result<Vec<(String, i64)>> {
        let mut rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT model_version, COUNT(*) FROM rating WHERE account_id = ?1 GROUP BY model_version")
                .bind(i64::from(account_id))
                .fetch_all(self.pool())
                .await?;
        rows.sort_by(|a, b| model_number(&a.0).cmp(&model_number(&b.0)).then_with(|| a.0.cmp(&b.0)));
        Ok(rows)
    }

    /// The owner's games rated under both `before` and `after`: each class's
    /// average under each, and the `top` games that moved the most.
    pub async fn model_shift(&self, account_id: u32, before: &str, after: &str, top: i64) -> Result<(Vec<ClassShift>, Vec<GameShift>)> {
        let classes: Vec<(String, i64, f64, f64)> = sqlx::query_as(
            "SELECT a.class, COUNT(*), AVG(a.score), AVG(b.score)
               FROM rating a
               JOIN rating b ON b.log_id = a.log_id AND b.account_id = a.account_id
                            AND b.class = a.class AND b.model_version = ?3
              WHERE a.account_id = ?1 AND a.model_version = ?2
              GROUP BY a.class ORDER BY COUNT(*) DESC, a.class",
        )
        .bind(i64::from(account_id))
        .bind(before)
        .bind(after)
        .fetch_all(self.pool())
        .await?;
        let games: Vec<(i64, String, Option<String>, Option<i64>, f64, f64)> = sqlx::query_as(
            "SELECT a.log_id, a.class, m.map, m.played_at, a.score, b.score
               FROM rating a
               JOIN rating b ON b.log_id = a.log_id AND b.account_id = a.account_id
                            AND b.class = a.class AND b.model_version = ?3
               LEFT JOIN match m ON m.log_id = a.log_id
              WHERE a.account_id = ?1 AND a.model_version = ?2
              ORDER BY ABS(b.score - a.score) DESC, a.log_id DESC, a.class
              LIMIT ?4",
        )
        .bind(i64::from(account_id))
        .bind(before)
        .bind(after)
        .bind(top)
        .fetch_all(self.pool())
        .await?;
        Ok((
            classes.into_iter().map(|(class, games, before, after)| ClassShift { class, games, before, after }).collect(),
            games
                .into_iter()
                .map(|(log_id, class, map, played_at, before, after)| GameShift { log_id, class, map, played_at, before, after })
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(log_id: i64, account_id: u32, class: &'static str, score: f64) -> crate::RatingRow<'static> {
        crate::RatingRow { log_id, account_id, class, score, minutes: 30.0, parts_json: "{}".into() }
    }

    #[tokio::test]
    async fn a_model_shift_pairs_the_same_games_under_both_models() {
        let db = Db::connect_in_memory().await.unwrap();
        // v13: three of the owner's games and one of someone else's.
        db.replace_ratings("v13", &[row(1, 7, "medic", 0.6), row(2, 7, "medic", 1.0), row(3, 7, "soldier", 1.5), row(1, 9, "medic", 2.0)])
            .await
            .unwrap();
        // v14: games 1 and 3 again, and game 4, which v13 never rated.
        db.replace_ratings("v14", &[row(1, 7, "medic", 1.2), row(3, 7, "soldier", 1.3), row(4, 7, "medic", 1.0), row(1, 9, "medic", 0.1)])
            .await
            .unwrap();

        let versions = db.rating_versions(7).await.unwrap();
        assert_eq!(versions, vec![("v13".to_string(), 3), ("v14".to_string(), 3)]);

        let (classes, games) = db.model_shift(7, "v13", "v14", 10).await.unwrap();
        // Only games rated under both, and only the owner's.
        assert_eq!(classes.len(), 2);
        let medic = classes.iter().find(|c| c.class == "medic").unwrap();
        assert_eq!(medic.games, 1);
        assert!((medic.before - 0.6).abs() < 1e-9 && (medic.after - 1.2).abs() < 1e-9);
        // The biggest move first: medic +0.6, then soldier -0.2.
        assert_eq!(games.iter().map(|g| g.log_id).collect::<Vec<_>>(), vec![1, 3]);

        let (_, top) = db.model_shift(7, "v13", "v14", 1).await.unwrap();
        assert_eq!(top.len(), 1, "the list is capped");
    }

    #[tokio::test]
    async fn versions_sort_by_number_not_by_text() {
        let db = Db::connect_in_memory().await.unwrap();
        for v in ["v14", "v9", "v10"] {
            db.replace_ratings(v, &[row(1, 7, "scout", 1.0)]).await.unwrap();
        }
        let order: Vec<String> = db.rating_versions(7).await.unwrap().into_iter().map(|(v, _)| v).collect();
        assert_eq!(order, ["v9", "v10", "v14"]);
    }

    #[tokio::test]
    async fn source_states_read_back_what_a_sync_recorded() {
        let db = Db::connect_in_memory().await.unwrap();
        assert!(db.sync_states().await.unwrap().is_empty());
        db.record_sync("trends", Some("100"), Some("timed out")).await.unwrap();
        db.record_sync("logstf_search", None, None).await.unwrap();
        let s = db.sync_states().await.unwrap();
        assert_eq!(s.iter().map(|x| x.source.as_str()).collect::<Vec<_>>(), ["logstf_search", "trends"]);
        assert_eq!(s[1].last_error.as_deref(), Some("timed out"));
        assert!(s[1].last_run_at.is_some());
    }

    #[test]
    fn model_numbers_sort_as_numbers() {
        let mut v = vec!["v14", "v9", "v10", "weird"];
        v.sort_by_key(|s| model_number(s));
        assert_eq!(v, ["weird", "v9", "v10", "v14"]);
    }
}
