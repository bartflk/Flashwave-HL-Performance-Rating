//! ETF2L sources, and each match's context: official, scrim or pug.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;
use std::collections::HashMap;

/// One ETF2L match, flattened for storage.
pub struct Etf2lMatchRow {
    pub match_id: i64,
    pub competition_id: Option<i64>,
    pub competition: Option<String>,
    pub comp_type: Option<String>,
    pub category: Option<String>,
    pub division: Option<String>,
    pub tier: Option<i64>,
    pub week: Option<i64>,
    pub round: Option<String>,
    pub time: Option<i64>,
    pub clan1: Option<(i64, String)>,
    pub clan2: Option<(i64, String)>,
    pub r1: Option<i64>,
    pub r2: Option<i64>,
    pub default_win: bool,
    pub maps: Vec<String>,
    /// `(account_id, team_id, name)`; mercs have no team.
    pub roster: Vec<(u32, Option<i64>, Option<String>)>,
}

/// An official as the context pass needs it.
pub struct OfficialRow {
    pub match_id: i64,
    pub time: i64,
    pub clan1: (i64, String),
    pub clan2: (i64, String),
    /// Registered players only: `account_id -> team_id`.
    pub roster: HashMap<u32, i64>,
}

/// A kept Highlander match the owner played in, with everyone's side.
pub struct ContextGameRow {
    pub log_id: i64,
    pub played_at: i64,
    pub trends_match: Option<i64>,
    /// `(account_id, team)` for every player, owner included.
    pub players: Vec<(u32, String)>,
}

pub struct ContextRow {
    pub log_id: i64,
    pub kind: &'static str,
    pub etf2l_match_id: Option<i64>,
    pub link_method: Option<&'static str>,
    pub team: Option<(i64, String)>,
    pub opponent: Option<(i64, String)>,
    pub regulars: i64,
}

/// What the match list and match page show about a match's context.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchContext {
    /// `official`, `scrim` or `pug`.
    pub kind: String,
    pub etf2l_match_id: Option<i64>,
    /// `trends` or `roster`: how an official was recognised.
    pub link_method: Option<String>,
    pub team_name: Option<String>,
    pub opp_name: Option<String>,
    pub regulars: i64,
    /// Present when the ETF2L match itself is stored.
    pub official: Option<OfficialInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialInfo {
    pub competition: Option<String>,
    pub category: Option<String>,
    pub division: Option<String>,
    pub tier: Option<i64>,
    pub week: Option<i64>,
    pub round: Option<String>,
    /// ETF2L's own score, from the owner's side when their side is known.
    pub score: Option<(i64, i64)>,
    pub default_win: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCounts {
    pub officials: i64,
    pub scrims: i64,
    pub pugs: i64,
    /// Officials recognised from rosters, not tagged by trends.tf.
    pub roster_officials: i64,
    pub etf2l_matches: i64,
    pub etf2l_player: Option<i64>,
    pub last_fetch: Option<i64>,
}

/// One teammate appearance: the owner's game joined with a teammate's line.
pub struct MateRow {
    pub log_id: i64,
    pub account_id: u32,
    pub name: Option<String>,
    pub main_class: Option<String>,
    pub time_s: i64,
}

/// One of the owner's games, for teammate and team summaries.
pub struct OwnGameRow {
    pub log_id: i64,
    pub played_at: i64,
    pub kind: String,
    pub team_id: Option<i64>,
    pub team_name: Option<String>,
    /// `W`, `L` or `T`.
    pub result: String,
    /// The owner's rating in this game, when rated.
    pub score: Option<f64>,
}

/// SQL for the owner's result from a joined `match_player p` and `match m`.
pub(crate) const RESULT_SQL: &str = "CASE
        WHEN (CASE p.team WHEN 'Red' THEN m.red_score ELSE m.blue_score END)
           > (CASE p.team WHEN 'Red' THEN m.blue_score ELSE m.red_score END) THEN 'W'
        WHEN (CASE p.team WHEN 'Red' THEN m.red_score ELSE m.blue_score END)
           < (CASE p.team WHEN 'Red' THEN m.blue_score ELSE m.red_score END) THEN 'L'
        ELSE 'T' END";

/// Columns for [`MatchContext`], from `match_context c LEFT JOIN etf2l_match e`.
pub(crate) const CONTEXT_COLUMNS: &str = "c.kind AS c_kind, c.etf2l_match_id AS c_match,
    c.link_method AS c_method, c.team_id AS c_team_id, c.team_name AS c_team, c.opp_team_name AS c_opp,
    c.regulars AS c_regulars, e.match_id AS e_id, e.competition AS e_comp, e.category AS e_cat,
    e.division AS e_div, e.tier AS e_tier, e.week AS e_week, e.round AS e_round,
    e.clan1_id AS e_clan1, e.r1 AS e_r1, e.r2 AS e_r2, e.default_win AS e_dw";

/// Read [`CONTEXT_COLUMNS`] back; `None` when the match has no context row.
pub(crate) fn context_from_row(r: &sqlx::sqlite::SqliteRow) -> Option<MatchContext> {
    let kind: Option<String> = r.get("c_kind");
    let kind = kind?;
    let official = r.get::<Option<i64>, _>("e_id").map(|_| {
        let team: Option<i64> = r.get("c_team_id");
        let clan1: Option<i64> = r.get("e_clan1");
        let (r1, r2): (Option<i64>, Option<i64>) = (r.get("e_r1"), r.get("e_r2"));
        let score = match (r1, r2, team) {
            (Some(a), Some(b), Some(t)) if Some(t) == clan1 => Some((a, b)),
            (Some(a), Some(b), Some(_)) => Some((b, a)),
            _ => None,
        };
        OfficialInfo {
            competition: r.get("e_comp"),
            category: r.get("e_cat"),
            division: r.get("e_div"),
            tier: r.get("e_tier"),
            week: r.get("e_week"),
            round: r.get("e_round"),
            score,
            default_win: r.get::<i64, _>("e_dw") != 0,
        }
    });
    Some(MatchContext {
        kind,
        etf2l_match_id: r.get("c_match"),
        link_method: r.get("c_method"),
        team_name: r.get("c_team"),
        opp_name: r.get("c_opp"),
        regulars: r.get("c_regulars"),
        official,
    })
}

impl Db {
    // ---- ETF2L sources -----------------------------------------------------

    pub async fn store_etf2l_raw(&self, kind: &str, id: i64, json: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO etf2l_raw (kind, id, fetched_at, json) VALUES (?1, ?2, unixepoch(), ?3)
             ON CONFLICT (kind, id) DO UPDATE SET fetched_at = excluded.fetched_at, json = excluded.json",
        )
        .bind(kind)
        .bind(id)
        .bind(json)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// `(id, fetched_at, json)` for every stored source of one kind.
    pub async fn etf2l_raw(&self, kind: &str) -> Result<Vec<(i64, i64, String)>> {
        let rows = sqlx::query("SELECT id, fetched_at, json FROM etf2l_raw WHERE kind = ?1 ORDER BY id")
            .bind(kind)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.into_iter().map(|r| (r.get("id"), r.get("fetched_at"), r.get("json"))).collect())
    }

    /// ETF2L match ids trends.tf attached to kept Highlander logs.
    pub async fn trends_etf2l_ids(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT DISTINCT etf2l_match_id FROM log_index
             WHERE etf2l_match_id IS NOT NULL AND superseded_by IS NULL
               AND COALESCE(format_override, format) = 'highlander'",
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn replace_etf2l_matches(&self, rows: &[Etf2lMatchRow]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM etf2l_roster").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM etf2l_match").execute(&mut *tx).await?;
        for m in rows {
            sqlx::query(
                "INSERT INTO etf2l_match (match_id, competition_id, competition, comp_type, category,
                    division, tier, week, round, time, clan1_id, clan1_name, clan2_id, clan2_name,
                    r1, r2, default_win, maps)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            )
            .bind(m.match_id)
            .bind(m.competition_id)
            .bind(&m.competition)
            .bind(&m.comp_type)
            .bind(&m.category)
            .bind(&m.division)
            .bind(m.tier)
            .bind(m.week)
            .bind(&m.round)
            .bind(m.time)
            .bind(m.clan1.as_ref().map(|c| c.0))
            .bind(m.clan1.as_ref().map(|c| c.1.as_str()))
            .bind(m.clan2.as_ref().map(|c| c.0))
            .bind(m.clan2.as_ref().map(|c| c.1.as_str()))
            .bind(m.r1)
            .bind(m.r2)
            .bind(m.default_win)
            .bind(serde_json::to_string(&m.maps)?)
            .execute(&mut *tx)
            .await?;
            for (account, team, name) in &m.roster {
                sqlx::query(
                    "INSERT OR IGNORE INTO etf2l_roster (match_id, account_id, team_id, name)
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .bind(m.match_id)
                .bind(*account as i64)
                .bind(team)
                .bind(name)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    // ---- the context pass ----------------------------------------------------

    /// Highlander officials with both clans known, and their registered players.
    pub async fn official_rows(&self) -> Result<Vec<OfficialRow>> {
        let matches = sqlx::query(
            "SELECT match_id, time, clan1_id, clan1_name, clan2_id, clan2_name FROM etf2l_match
             WHERE comp_type = 'Highlander' AND time IS NOT NULL
               AND clan1_id IS NOT NULL AND clan2_id IS NOT NULL",
        )
        .fetch_all(self.pool())
        .await?;
        let roster = sqlx::query("SELECT match_id, account_id, team_id FROM etf2l_roster WHERE team_id IS NOT NULL")
            .fetch_all(self.pool())
            .await?;
        let mut by_match: HashMap<i64, HashMap<u32, i64>> = HashMap::new();
        for r in roster {
            by_match
                .entry(r.get("match_id"))
                .or_default()
                .insert(r.get::<i64, _>("account_id") as u32, r.get("team_id"));
        }
        Ok(matches
            .into_iter()
            .map(|r| {
                let id: i64 = r.get("match_id");
                OfficialRow {
                    match_id: id,
                    time: r.get("time"),
                    clan1: (r.get("clan1_id"), r.get::<Option<String>, _>("clan1_name").unwrap_or_default()),
                    clan2: (r.get("clan2_id"), r.get::<Option<String>, _>("clan2_name").unwrap_or_default()),
                    roster: by_match.remove(&id).unwrap_or_default(),
                }
            })
            .collect())
    }

    /// Every kept Highlander match the owner played, oldest first.
    pub async fn context_games(&self, me: u32) -> Result<Vec<ContextGameRow>> {
        let games = sqlx::query(
            "SELECT m.log_id, COALESCE(m.played_at, 0) AS played_at, i.etf2l_match_id
             FROM match m
             JOIN log_index i    ON i.log_id = m.log_id
             JOIN match_player p ON p.log_id = m.log_id AND p.account_id = ?1
             WHERE i.superseded_by IS NULL AND COALESCE(i.format_override, i.format) = 'highlander'
             ORDER BY m.played_at, m.log_id",
        )
        .bind(me as i64)
        .fetch_all(self.pool())
        .await?;
        let players = sqlx::query(
            "SELECT q.log_id, q.account_id, q.team FROM match_player q
             WHERE q.log_id IN (SELECT log_id FROM match_player WHERE account_id = ?1)",
        )
        .bind(me as i64)
        .fetch_all(self.pool())
        .await?;
        let mut by_log: HashMap<i64, Vec<(u32, String)>> = HashMap::new();
        for r in players {
            by_log
                .entry(r.get("log_id"))
                .or_default()
                .push((r.get::<i64, _>("account_id") as u32, r.get("team")));
        }
        Ok(games
            .into_iter()
            .map(|r| {
                let log_id: i64 = r.get("log_id");
                ContextGameRow {
                    log_id,
                    played_at: r.get("played_at"),
                    trends_match: r.get("etf2l_match_id"),
                    players: by_log.remove(&log_id).unwrap_or_default(),
                }
            })
            .collect())
    }

    pub async fn replace_match_context(&self, rows: &[ContextRow]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM match_context").execute(&mut *tx).await?;
        for c in rows {
            sqlx::query(
                "INSERT INTO match_context (log_id, kind, etf2l_match_id, link_method, team_id,
                    team_name, opp_team_id, opp_team_name, regulars)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(c.log_id)
            .bind(c.kind)
            .bind(c.etf2l_match_id)
            .bind(c.link_method)
            .bind(c.team.as_ref().map(|t| t.0))
            .bind(c.team.as_ref().map(|t| t.1.as_str()))
            .bind(c.opponent.as_ref().map(|t| t.0))
            .bind(c.opponent.as_ref().map(|t| t.1.as_str()))
            .bind(c.regulars)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    // ---- reads -------------------------------------------------------------

    pub async fn match_context(&self, log_id: i64) -> Result<Option<MatchContext>> {
        let row = sqlx::query(&format!(
            "SELECT {CONTEXT_COLUMNS} FROM match_context c
             LEFT JOIN etf2l_match e ON e.match_id = c.etf2l_match_id
             WHERE c.log_id = ?1"
        ))
        .bind(log_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.as_ref().and_then(context_from_row))
    }

    pub async fn context_counts(&self, etf2l_player_key: &str) -> Result<ContextCounts> {
        let r = sqlx::query(
            "SELECT
                COALESCE(SUM(kind = 'official'), 0) AS officials,
                COALESCE(SUM(kind = 'scrim'), 0) AS scrims,
                COALESCE(SUM(kind = 'pug'), 0) AS pugs,
                COALESCE(SUM(link_method = 'roster'), 0) AS roster_officials,
                (SELECT COUNT(*) FROM etf2l_match) AS etf2l_matches,
                (SELECT MAX(fetched_at) FROM etf2l_raw) AS last_fetch
             FROM match_context",
        )
        .fetch_one(self.pool())
        .await?;
        Ok(ContextCounts {
            officials: r.get("officials"),
            scrims: r.get("scrims"),
            pugs: r.get("pugs"),
            roster_officials: r.get("roster_officials"),
            etf2l_matches: r.get("etf2l_matches"),
            last_fetch: r.get("last_fetch"),
            etf2l_player: self.get_setting(etf2l_player_key).await?.and_then(|s| s.parse().ok()),
        })
    }

    /// The owner's classified games, with result and rating, oldest first.
    pub async fn own_games(&self, me: u32, version: &str) -> Result<Vec<OwnGameRow>> {
        let rows = sqlx::query(&format!(
            "SELECT c.log_id, COALESCE(m.played_at, 0) AS played_at, c.kind, c.team_id, c.team_name,
                    {RESULT_SQL} AS result, r.score
             FROM match_context c
             JOIN match m        ON m.log_id = c.log_id
             JOIN match_player p ON p.log_id = c.log_id AND p.account_id = ?1
             LEFT JOIN rating r  ON r.log_id = c.log_id AND r.account_id = ?1 AND r.model_version = ?2
             ORDER BY m.played_at, c.log_id"
        ))
        .bind(me as i64)
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| OwnGameRow {
                log_id: r.get("log_id"),
                played_at: r.get("played_at"),
                kind: r.get("kind"),
                team_id: r.get("team_id"),
                team_name: r.get("team_name"),
                result: r.get("result"),
                score: r.get("score"),
            })
            .collect())
    }

    /// Everyone who was on the owner's side in a classified game.
    pub async fn mate_rows(&self, me: u32) -> Result<Vec<MateRow>> {
        let rows = sqlx::query(
            "SELECT q.log_id, q.account_id, q.name, q.main_class, q.time_s
             FROM match_context c
             JOIN match_player p ON p.log_id = c.log_id AND p.account_id = ?1
             JOIN match_player q ON q.log_id = c.log_id AND q.team = p.team AND q.account_id != ?1",
        )
        .bind(me as i64)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| MateRow {
                log_id: r.get("log_id"),
                account_id: r.get::<i64, _>("account_id") as u32,
                name: r.get("name"),
                main_class: r.get("main_class"),
                time_s: r.get("time_s"),
            })
            .collect())
    }
}
