//! What the season data check (`hl_ingest::data_audit`) reads: a season's
//! played officials, ETF2L's maps for them, and every log held of them.

use crate::{Db, Result};
use sqlx::Row;

/// A played official.
pub struct AuditMatch {
    pub id: i64,
    pub stage: String,
    pub division: String,
    pub tier: Option<i64>,
    pub week: Option<i64>,
    pub round: Option<String>,
    pub clan1: i64,
    pub clan2: i64,
    /// ETF2L's map list (JSON array), where known.
    pub maps: Option<String>,
}

/// A league sample log of a match.
pub struct AuditLog {
    pub match_id: i64,
    pub log_id: i64,
    pub map: String,
    /// Upload time (the end) and length, seconds.
    pub end: i64,
    pub duration: i64,
    pub players: i64,
    pub has_json: bool,
    pub rated: bool,
    pub score: Option<(i64, i64)>,
}

impl Db {
    /// Every played official of a season, cups left out, oldest first.
    pub async fn audit_matches(&self, season: i64) -> Result<Vec<AuditMatch>> {
        Ok(sqlx::query(
            "SELECT m.match_id, c.stage, COALESCE(NULLIF(m.division, ''), c.division), m.tier, m.week, m.round, m.clan1_id, m.clan2_id, m.maps
             FROM etf2l_season_match m JOIN etf2l_competition c ON c.competition_id = m.competition_id
             WHERE c.season = ?1 AND c.stage != 'Cup' AND m.default_win = 0 AND COALESCE(m.r1, 0) + COALESCE(m.r2, 0) > 0
             ORDER BY m.time",
        )
        .bind(season)
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|r| AuditMatch {
            id: r.get(0),
            stage: r.get(1),
            division: r.get::<Option<String>, _>(2).unwrap_or_default(),
            tier: r.get(3),
            week: r.get(4),
            round: r.get(5),
            clan1: r.get(6),
            clan2: r.get(7),
            maps: r.get(8),
        })
        .collect())
    }

    /// ETF2L's per-map results of the given matches: `(match, map, clan1, clan2)`, in order.
    pub async fn audit_maps(&self, matches: &[i64]) -> Result<Vec<(i64, String, i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT match_id, map, clan1, clan2 FROM etf2l_season_map
             WHERE match_id IN (SELECT value FROM json_each(?1)) ORDER BY match_id, match_order",
        )
        .bind(serde_json::to_string(matches)?)
        .fetch_all(self.pool())
        .await?)
    }

    /// The league sample's logs of the given matches.
    pub async fn audit_logs(&self, matches: &[i64], version: &str) -> Result<Vec<AuditLog>> {
        Ok(sqlx::query(
            "SELECT l.etf2l_match_id, l.log_id, l.map, l.played_at, COALESCE(l.duration_s, 0),
                    (SELECT COUNT(*) FROM league_log_player p WHERE p.log_id = l.log_id),
                    EXISTS (SELECT 1 FROM league_log_json j WHERE j.log_id = l.log_id) OR l.json_source = 'snapshot',
                    EXISTS (SELECT 1 FROM league_rating r WHERE r.log_id = l.log_id AND r.model_version = ?2),
                    (SELECT json_extract(j.json, '$.teams.Red.score') FROM league_log_json j WHERE j.log_id = l.log_id),
                    (SELECT json_extract(j.json, '$.teams.Blue.score') FROM league_log_json j WHERE j.log_id = l.log_id)
             FROM league_log l WHERE l.etf2l_match_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(serde_json::to_string(matches)?)
        .bind(version)
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|r| {
            let (red, blue): (Option<i64>, Option<i64>) = (r.get(8), r.get(9));
            AuditLog {
                match_id: r.get(0),
                log_id: r.get(1),
                map: r.get(2),
                end: r.get(3),
                duration: r.get(4),
                players: r.get(5),
                has_json: r.get::<i64, _>(6) != 0,
                rated: r.get::<i64, _>(7) != 0,
                score: red.zip(blue),
            }
        })
        .collect())
    }

    /// The owner's own logs of the given matches: `(match, log, map, end, duration)`.
    pub async fn audit_own_logs(&self, matches: &[i64]) -> Result<Vec<(i64, i64, String, i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT c.etf2l_match_id, m.log_id, COALESCE(m.map, ''), m.played_at, COALESCE(m.duration_s, 0)
             FROM match_context c JOIN match m ON m.log_id = c.log_id WHERE c.etf2l_match_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(serde_json::to_string(matches)?)
        .fetch_all(self.pool())
        .await?)
    }

    /// Pairs of league logs of one match whose minutes overlap by more than
    /// a minute: `(a, a's map, a's seconds, b, b's map, b's seconds)`, each
    /// pair once (a < b).
    pub async fn league_overlaps(&self) -> Result<Vec<(i64, String, i64, i64, String, i64)>> {
        Ok(sqlx::query_as(
            "SELECT a.log_id, a.map, COALESCE(a.duration_s, 0), b.log_id, b.map, COALESCE(b.duration_s, 0)
             FROM league_log a JOIN league_log b ON a.etf2l_match_id = b.etf2l_match_id AND a.log_id < b.log_id
             WHERE MIN(a.played_at, b.played_at) - MAX(a.played_at - COALESCE(a.duration_s, 0), b.played_at - COALESCE(b.duration_s, 0)) > 60",
        )
        .fetch_all(self.pool())
        .await?)
    }

    /// The ETF2L match a log belongs to, however it is known: the league's
    /// list, the owner's own context, trends.tf's index, or a combined log's
    /// parts.
    pub async fn etf2l_match_of(&self, log_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT COALESCE(
                (SELECT l.etf2l_match_id FROM league_log l WHERE l.log_id = ?1),
                (SELECT c.etf2l_match_id FROM match_context c WHERE c.log_id = ?1),
                (SELECT i.etf2l_match_id FROM log_index i WHERE i.log_id = ?1),
                (SELECT i.etf2l_match_id FROM log_index i WHERE i.superseded_by = ?1 AND i.etf2l_match_id IS NOT NULL LIMIT 1))",
        )
        .bind(log_id)
        .fetch_one(self.pool())
        .await?)
    }

    /// A match's league logs, in the order they were played.
    pub async fn league_logs_of_match(&self, match_id: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT log_id FROM league_log WHERE etf2l_match_id = ?1 ORDER BY played_at")
            .bind(match_id)
            .fetch_all(self.pool())
            .await?)
    }

    /// Every ETF2L team's name.
    pub async fn team_names(&self) -> Result<Vec<(i64, String)>> {
        Ok(sqlx::query_as("SELECT team_id, name FROM etf2l_team").fetch_all(self.pool()).await?)
    }
}
