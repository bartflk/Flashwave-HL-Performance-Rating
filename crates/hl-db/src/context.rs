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

/// The two teams of a classified match, as the match page's header shows
/// them: ETF2L's names, logos and countries where the teams are stored, and
/// the season of an official.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchSides {
    /// The owner's side.
    pub team: Option<SideTeam>,
    pub opp: Option<SideTeam>,
    pub season: Option<i64>,
    pub season_name: Option<String>,
    /// When ETF2L had the official scheduled, unix seconds.
    pub scheduled: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SideTeam {
    /// ETF2L's team id; `None` for a scrim side known only by name.
    pub id: Option<i64>,
    pub name: String,
    pub country: Option<String>,
    pub avatar: Option<String>,
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

/// One competition the owner played officials in, with when.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MyCompetition {
    pub competition: String,
    pub category: Option<String>,
    pub division: Option<String>,
    pub first: i64,
    pub last: i64,
    pub officials: i64,
}

impl Db {
    /// The owner's officials by competition: name, kind, division, first and
    /// last match time and how many (the match list's tournament dividers).
    pub async fn my_competitions(&self) -> Result<Vec<MyCompetition>> {
        Ok(sqlx::query_as(
            "SELECT competition, MAX(category) AS category, MAX(division) AS division,
                    MIN(time) AS first, MAX(time) AS last, COUNT(*) AS officials
             FROM etf2l_match
             WHERE time IS NOT NULL AND competition IS NOT NULL AND competition <> ''
             GROUP BY competition",
        )
        .fetch_all(self.pool())
        .await?)
    }

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
    /// What names a match on a progress line: `(map, played_at, kind,
    /// opponent)` -- "Official vs Kebab · upward · 14 Sept".
    pub async fn match_label(&self, log_id: i64) -> Result<Option<(Option<String>, Option<i64>, Option<String>, Option<String>)>> {
        Ok(sqlx::query_as(
            "SELECT m.map, m.played_at, c.kind, c.opp_team_name FROM match m
               LEFT JOIN match_context c ON c.log_id = m.log_id
              WHERE m.log_id = ?1",
        )
        .bind(log_id)
        .fetch_optional(self.pool())
        .await?)
    }

    /// The ETF2L match a log was matched to by roster, when it was.
    pub async fn context_match_id(&self, log_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar::<_, Option<i64>>("SELECT etf2l_match_id FROM match_context WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?
            .flatten())
    }

    /// Every stored log of one ETF2L match, by either link.
    pub async fn logs_of_etf2l_match(&self, match_id: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT log_id FROM log_index WHERE etf2l_match_id = ?1 AND superseded_by IS NULL
             UNION SELECT log_id FROM match_context WHERE etf2l_match_id = ?1
             ORDER BY 1",
        )
        .bind(match_id)
        .fetch_all(self.pool())
        .await?)
    }

    /// One stored ETF2L response: `(fetched_at, json)`.
    pub async fn etf2l_raw_one(&self, kind: &str, id: i64) -> Result<Option<(i64, String)>> {
        Ok(sqlx::query_as("SELECT fetched_at, json FROM etf2l_raw WHERE kind = ?1 AND id = ?2")
            .bind(kind)
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

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

    /// Every Highlander official ETF2L scheduled: `(match_id, time)`.
    pub async fn etf2l_match_times(&self) -> Result<Vec<(i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT match_id, time FROM etf2l_match
             WHERE comp_type = 'Highlander' AND time IS NOT NULL
             ORDER BY time",
        )
        .fetch_all(self.pool())
        .await?)
    }

    /// When every kept log was played: `(log_id, played_at)`. Indexed rows,
    /// so this answers for logs that have not been downloaded yet.
    pub async fn index_times(&self) -> Result<Vec<(i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT log_id, played_at FROM log_index
             WHERE superseded_by IS NULL AND played_at IS NOT NULL
             ORDER BY played_at",
        )
        .fetch_all(self.pool())
        .await?)
    }

    /// Record which ETF2L match each log sits inside the scheduled time of.
    /// Replaces the lot: a log that no longer matches must lose its mark.
    pub async fn set_etf2l_time_matches(&self, pairs: &[(i64, i64)]) -> Result<usize> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE log_index SET etf2l_time_match = NULL WHERE etf2l_time_match IS NOT NULL")
            .execute(&mut *tx)
            .await?;
        for (log_id, match_id) in pairs {
            sqlx::query("UPDATE log_index SET etf2l_time_match = ?2 WHERE log_id = ?1")
                .bind(log_id)
                .bind(match_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(pairs.len())
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
        // Kinds set by hand (Q63) win over the pass's, which is kept as
        // what "Automatic" goes back to.
        sqlx::query(
            "UPDATE match_kind_override SET
                 was_kind = (SELECT c.kind FROM match_context c WHERE c.log_id = match_kind_override.log_id),
                 was_method = (SELECT c.link_method FROM match_context c WHERE c.log_id = match_kind_override.log_id)
             WHERE log_id IN (SELECT log_id FROM match_context)",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE match_context SET
                 kind = (SELECT o.kind FROM match_kind_override o WHERE o.log_id = match_context.log_id),
                 link_method = 'manual'
             WHERE log_id IN (SELECT log_id FROM match_kind_override)",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Set a match's kind by hand (Q63), or with `None` give it back to the
    /// context pass. `false` when the match has no context to set: one the
    /// owner did not play, or not Highlander.
    pub async fn set_match_kind(&self, log_id: i64, kind: Option<&str>) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        let current: Option<(String, Option<String>)> = sqlx::query_as("SELECT kind, link_method FROM match_context WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some((now_kind, now_method)) = current else { return Ok(false) };
        let held: Option<(String, Option<String>)> = sqlx::query_as("SELECT was_kind, was_method FROM match_kind_override WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(&mut *tx)
            .await?;
        match kind {
            Some(kind) => {
                anyhow::ensure!(matches!(kind, "official" | "scrim" | "pug"), "not a kind of match: {kind}");
                // What the pass decided: kept from the first time it was overridden.
                let (was_kind, was_method) = held.unwrap_or((now_kind, now_method));
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
                sqlx::query(
                    "INSERT INTO match_kind_override (log_id, kind, was_kind, was_method, set_at) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(log_id) DO UPDATE SET kind = excluded.kind, set_at = excluded.set_at",
                )
                .bind(log_id)
                .bind(kind)
                .bind(&was_kind)
                .bind(&was_method)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                sqlx::query("UPDATE match_context SET kind = ?2, link_method = 'manual' WHERE log_id = ?1")
                    .bind(log_id)
                    .bind(kind)
                    .execute(&mut *tx)
                    .await?;
            }
            None => {
                if let Some((was_kind, was_method)) = held {
                    sqlx::query("UPDATE match_context SET kind = ?2, link_method = ?3 WHERE log_id = ?1")
                        .bind(log_id)
                        .bind(was_kind)
                        .bind(was_method)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("DELETE FROM match_kind_override WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
                }
            }
        }
        tx.commit().await?;
        Ok(true)
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

    /// Both sides of a classified match, for the match page's header.
    pub async fn match_sides(&self, log_id: i64) -> Result<Option<MatchSides>> {
        let row = sqlx::query(
            "SELECT c.team_id, COALESCE(ta.name, c.team_name) AS team_name, ta.country AS team_country, ta.avatar AS team_avatar,
                    c.opp_team_id, COALESCE(tb.name, c.opp_team_name) AS opp_name, tb.country AS opp_country, tb.avatar AS opp_avatar,
                    comp.season, comp.season_name, e.time AS scheduled
             FROM match_context c
             LEFT JOIN etf2l_team ta         ON ta.team_id = c.team_id
             LEFT JOIN etf2l_team tb         ON tb.team_id = c.opp_team_id
             LEFT JOIN etf2l_match e         ON e.match_id = c.etf2l_match_id
             LEFT JOIN etf2l_competition comp ON comp.competition_id = e.competition_id
             WHERE c.log_id = ?1",
        )
        .bind(log_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| {
            let side = |id: &str, name: &str, country: &str, avatar: &str| {
                let name: Option<String> = r.get(name);
                name.filter(|n| !n.is_empty()).map(|name| SideTeam { id: r.get(id), name, country: r.get(country), avatar: r.get(avatar) })
            };
            MatchSides {
                team: side("team_id", "team_name", "team_country", "team_avatar"),
                opp: side("opp_team_id", "opp_name", "opp_country", "opp_avatar"),
                season: r.get("season"),
                season_name: r.get("season_name"),
                scheduled: r.get("scheduled"),
            }
        }))
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

#[cfg(test)]
mod kind_override_tests {
    use crate::Db;

    use super::ContextRow;

    fn pug(log_id: i64) -> ContextRow {
        ContextRow { log_id, kind: "pug", etf2l_match_id: None, link_method: None, team: None, opponent: None, regulars: 1 }
    }

    async fn kind_of(db: &Db, log_id: i64) -> (String, Option<String>) {
        sqlx::query_as("SELECT kind, link_method FROM match_context WHERE log_id = ?1").bind(log_id).fetch_one(db.pool()).await.unwrap()
    }

    #[tokio::test]
    async fn a_kind_set_by_hand_outlasts_the_pass_and_automatic_undoes_it() {
        let db = Db::connect_in_memory().await.unwrap();
        db.replace_match_context(&[pug(1), pug(2)]).await.unwrap();

        assert!(db.set_match_kind(1, Some("scrim")).await.unwrap());
        assert_eq!(kind_of(&db, 1).await, ("scrim".into(), Some("manual".into())));

        // The pass runs again, still calling it a pug: the hand's kind stays.
        db.replace_match_context(&[pug(1), pug(2)]).await.unwrap();
        assert_eq!(kind_of(&db, 1).await.0, "scrim");
        assert_eq!(kind_of(&db, 2).await.0, "pug", "the others are the pass's");

        // Automatic: back to what the pass said.
        assert!(db.set_match_kind(1, None).await.unwrap());
        assert_eq!(kind_of(&db, 1).await, ("pug".into(), None));
        db.replace_match_context(&[pug(1)]).await.unwrap();
        assert_eq!(kind_of(&db, 1).await.0, "pug");

        // No context, nothing to set; and only the three kinds.
        assert!(!db.set_match_kind(99, Some("scrim")).await.unwrap());
        assert!(db.set_match_kind(1, Some("lobby")).await.is_err());
    }
}
