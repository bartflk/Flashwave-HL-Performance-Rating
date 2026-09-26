//! Looking other people up (Q14).
//!
//! Every stored Highlander log holds seventeen players besides the owner,
//! and all of them are already rated — the pool the rating is measured
//! against is built from exactly these performances. So a page about
//! somebody else needs no new model and no new fetching: it is a different
//! `WHERE account_id` on tables that are already there.
//!
//! What it cannot show is anything the owner did not play in. A stranger's
//! record here is their record *in your matches*, which is a smaller and
//! more honest claim than "their record", and every view says so.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;

/// Non-superseded Highlander logs that were actually read: the same set the
/// rest of the app counts, so a player's games here match the match list.
const KEPT: &str = "SELECT i.log_id FROM log_index i JOIN log_raw r ON r.log_id = i.log_id
     WHERE i.superseded_by IS NULL AND COALESCE(i.format_override, i.format) = 'highlander'";

/// One row of the search results.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerHit {
    pub account_id: u32,
    /// The most recent name they used.
    pub name: String,
    /// Games of theirs in your database.
    pub games: i64,
    pub last_seen: Option<i64>,
    /// Their most played class.
    pub top_class: Option<String>,
}

/// The header of a player's page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSummary {
    pub account_id: u32,
    pub steamid64: String,
    pub name: String,
    /// Every other name they have appeared under, newest first.
    pub also_known_as: Vec<String>,
    pub games: i64,
    pub first_seen: Option<i64>,
    pub last_seen: Option<i64>,
    /// Games where they were on the owner's team, and where they were not.
    pub with_you: i64,
    pub against_you: i64,
    /// Of the games against you, the ones your side won.
    pub you_beat_them: i64,
    pub they_beat_you: i64,
    /// Classes they are rated on, most played first: class, games, average.
    pub classes: Vec<PlayerClass>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerClass {
    pub class: String,
    pub games: i64,
    pub avg: f64,
}

impl Db {
    /// Find players by name, or by any form of Steam ID.
    ///
    /// Names are matched against *every* name a player has used, not only
    /// their current one: people rename constantly and the whole point is
    /// to find someone you remember.
    pub async fn search_players(&self, query: &str, limit: i64) -> Result<Vec<PlayerHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        // A Steam ID in any notation finds exactly one person.
        let by_id = hl_core::SteamId::parse(q).ok().map(|s| s.account_id() as i64);
        let like = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));

        let sql = format!(
            "WITH kept AS ({KEPT}),
             theirs AS (
                 SELECT p.account_id, p.log_id, m.played_at
                 FROM match_player p
                 JOIN kept k ON k.log_id = p.log_id
                 JOIN match m ON m.log_id = p.log_id
                 WHERE p.account_id IN (
                     SELECT account_id FROM match_player
                     WHERE (?1 IS NOT NULL AND account_id = ?1)
                        OR (?1 IS NULL AND name LIKE ?2 ESCAPE '\\')
                 )
             )
             SELECT t.account_id,
                    COUNT(*) AS games,
                    MAX(t.played_at) AS last_seen,
                    (SELECT x.name FROM match_player x
                      JOIN match mx ON mx.log_id = x.log_id
                      WHERE x.account_id = t.account_id
                      ORDER BY mx.played_at DESC LIMIT 1) AS name,
                    (SELECT x.main_class FROM match_player x
                      WHERE x.account_id = t.account_id AND x.main_class IS NOT NULL
                      GROUP BY x.main_class ORDER BY COUNT(*) DESC LIMIT 1) AS top_class
             FROM theirs t
             GROUP BY t.account_id
             ORDER BY games DESC
             LIMIT ?3"
        );
        let rows = sqlx::query(&sql).bind(by_id).bind(&like).bind(limit).fetch_all(self.pool()).await?;
        Ok(rows
            .into_iter()
            .map(|r| PlayerHit {
                account_id: r.get::<i64, _>("account_id") as u32,
                name: r.get::<Option<String>, _>("name").unwrap_or_else(|| "unknown".into()),
                games: r.get("games"),
                last_seen: r.get("last_seen"),
                top_class: r.get("top_class"),
            })
            .collect())
    }

    /// The header of one player's page: who they are, and how you have met.
    pub async fn player_summary(&self, account_id: u32, owner: u32, version: &str) -> Result<Option<PlayerSummary>> {
        let sql = format!(
            "WITH kept AS ({KEPT}),
             theirs AS (
                 SELECT p.log_id, p.team, m.played_at, m.red_score, m.blue_score
                 FROM match_player p
                 JOIN kept k ON k.log_id = p.log_id
                 JOIN match m ON m.log_id = p.log_id
                 WHERE p.account_id = ?1
             ),
             met AS (
                 SELECT t.*, o.team AS your_team
                 FROM theirs t
                 JOIN match_player o ON o.log_id = t.log_id AND o.account_id = ?2
             )
             SELECT (SELECT COUNT(*) FROM theirs) AS games,
                    (SELECT MIN(played_at) FROM theirs) AS first_seen,
                    (SELECT MAX(played_at) FROM theirs) AS last_seen,
                    (SELECT COUNT(*) FROM met WHERE team = your_team) AS with_you,
                    (SELECT COUNT(*) FROM met WHERE team <> your_team) AS against_you,
                    (SELECT COUNT(*) FROM met WHERE team <> your_team AND
                        ((your_team = 'Red' AND red_score > blue_score) OR
                         (your_team = 'Blue' AND blue_score > red_score))) AS you_beat_them,
                    (SELECT COUNT(*) FROM met WHERE team <> your_team AND
                        ((your_team = 'Red' AND red_score < blue_score) OR
                         (your_team = 'Blue' AND blue_score < red_score))) AS they_beat_you"
        );
        let r = sqlx::query(&sql)
            .bind(account_id as i64)
            .bind(owner as i64)
            .fetch_one(self.pool())
            .await?;
        let games: i64 = r.get("games");
        if games == 0 {
            return Ok(None);
        }

        let names: Vec<String> = sqlx::query(
            "SELECT x.name, MAX(m.played_at) AS seen
             FROM match_player x JOIN match m ON m.log_id = x.log_id
             WHERE x.account_id = ?1 AND x.name IS NOT NULL AND x.name <> ''
             GROUP BY x.name ORDER BY seen DESC",
        )
        .bind(account_id as i64)
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();

        let classes: Vec<PlayerClass> = sqlx::query(
            "SELECT class, COUNT(*) AS n, AVG(score) AS avg FROM rating
             WHERE account_id = ?1 AND model_version = ?2
             GROUP BY class ORDER BY n DESC",
        )
        .bind(account_id as i64)
        .bind(version)
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|r| PlayerClass {
            class: r.get("class"),
            games: r.get("n"),
            avg: (r.get::<f64, _>("avg") * 100.0).round() / 100.0,
        })
        .collect();

        let id = hl_core::SteamId::from_account_id(account_id);
        Ok(Some(PlayerSummary {
            account_id,
            steamid64: id.to_steamid64(),
            name: names.first().cloned().unwrap_or_else(|| "unknown".into()),
            also_known_as: names.into_iter().skip(1).take(6).collect(),
            games,
            first_seen: r.get("first_seen"),
            last_seen: r.get("last_seen"),
            with_you: r.get("with_you"),
            against_you: r.get("against_you"),
            you_beat_them: r.get("you_beat_them"),
            they_beat_you: r.get("they_beat_you"),
            classes,
        }))
    }
}
