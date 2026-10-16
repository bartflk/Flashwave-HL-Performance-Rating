//! Every Highlander team's officials, from ETF2L (Q29): competitions, teams,
//! results, per-map scores and who played. Source data only; the views are
//! built in `hl_ingest::leagues`.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;

pub struct CompetitionRow<'a> {
    pub competition_id: i64,
    pub season: i64,
    pub season_name: &'a str,
    pub division: &'a str,
    pub stage: &'a str,
    pub name: &'a str,
    pub archived: bool,
    pub pool: Option<&'a str>,
}

pub struct SeasonMatchRow<'a> {
    pub match_id: i64,
    pub competition_id: i64,
    pub division: Option<&'a str>,
    pub tier: Option<i64>,
    pub week: Option<i64>,
    pub round: Option<&'a str>,
    pub time: Option<i64>,
    pub clan1_id: i64,
    pub clan2_id: i64,
    pub r1: Option<i64>,
    pub r2: Option<i64>,
    pub default_win: bool,
    pub maps: &'a str,
}

/// A stored competition, as the views use it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Competition {
    pub competition_id: i64,
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub stage: String,
    pub archived: bool,
    pub pool: Vec<String>,
    pub fetched_at: i64,
}

/// One official with its competition and both teams' names.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonMatch {
    pub match_id: i64,
    pub competition_id: i64,
    pub season: i64,
    pub division: String,
    pub tier: Option<i64>,
    pub stage: String,
    pub week: Option<i64>,
    pub round: Option<String>,
    pub time: Option<i64>,
    pub clan1_id: i64,
    pub clan1_name: String,
    pub clan2_id: i64,
    pub clan2_name: String,
    pub r1: Option<i64>,
    pub r2: Option<i64>,
    pub default_win: bool,
    pub maps: Vec<String>,
}

/// One map of a match: rounds each side took.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonMap {
    pub match_id: i64,
    pub map: String,
    pub clan1: i64,
    pub clan2: i64,
}

impl Db {
    pub async fn upsert_competition(&self, c: &CompetitionRow<'_>) -> Result<()> {
        sqlx::query(
            "INSERT INTO etf2l_competition (competition_id, season, season_name, division, stage, name, archived, pool, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())
             ON CONFLICT(competition_id) DO UPDATE SET
                season = excluded.season, season_name = excluded.season_name, division = excluded.division,
                stage = excluded.stage, name = excluded.name, archived = excluded.archived,
                pool = COALESCE(excluded.pool, etf2l_competition.pool), fetched_at = excluded.fetched_at",
        )
        .bind(c.competition_id)
        .bind(c.season)
        .bind(c.season_name)
        .bind(c.division)
        .bind(c.stage)
        .bind(c.name)
        .bind(c.archived)
        .bind(c.pool)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn competitions(&self) -> Result<Vec<Competition>> {
        let rows = sqlx::query(
            "SELECT competition_id, season, season_name, division, stage, archived, pool, fetched_at
             FROM etf2l_competition
             ORDER BY CASE WHEN season >= 100 THEN (season - 100) * 2 + 1 ELSE season * 2 END DESC, competition_id",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Competition {
                competition_id: r.get("competition_id"),
                season: r.get("season"),
                season_name: r.get("season_name"),
                division: r.get("division"),
                stage: r.get("stage"),
                archived: r.get::<i64, _>("archived") != 0,
                pool: r.get::<Option<String>, _>("pool").and_then(|p| serde_json::from_str(&p).ok()).unwrap_or_default(),
                fetched_at: r.get("fetched_at"),
            })
            .collect())
    }

    pub async fn upsert_etf2l_team(&self, team_id: i64, name: &str, country: Option<&str>, avatar: Option<&str>) -> Result<()> {
        sqlx::query(
            "INSERT INTO etf2l_team (team_id, name, country, avatar) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(team_id) DO UPDATE SET name = excluded.name,
                country = COALESCE(excluded.country, etf2l_team.country), avatar = COALESCE(excluded.avatar, etf2l_team.avatar)",
        )
        .bind(team_id)
        .bind(name)
        .bind(country)
        .bind(avatar)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Store a result; a match already read in detail keeps that.
    pub async fn upsert_season_match(&self, m: &SeasonMatchRow<'_>) -> Result<()> {
        sqlx::query(
            "INSERT INTO etf2l_season_match (match_id, competition_id, week, round, time, clan1_id, clan2_id, r1, r2, default_win, maps, division, tier)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(match_id) DO UPDATE SET
                competition_id = excluded.competition_id, division = excluded.division, tier = excluded.tier,
                week = excluded.week, round = excluded.round, time = excluded.time,
                clan1_id = excluded.clan1_id, clan2_id = excluded.clan2_id, r1 = excluded.r1, r2 = excluded.r2,
                default_win = excluded.default_win, maps = excluded.maps",
        )
        .bind(m.match_id)
        .bind(m.competition_id)
        .bind(m.week)
        .bind(m.round)
        .bind(m.time)
        .bind(m.clan1_id)
        .bind(m.clan2_id)
        .bind(m.r1)
        .bind(m.r2)
        .bind(m.default_win)
        .bind(m.maps)
        .bind(m.division)
        .bind(m.tier)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Played matches whose own page has not been read yet, newest first,
    /// from `min_season` on: the league sample (0033) holds years of older
    /// seasons whose pages it reads itself, and a sync should not.
    pub async fn season_matches_without_detail(&self, limit: i64, min_season: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT m.match_id FROM etf2l_season_match m JOIN etf2l_competition c ON c.competition_id = m.competition_id
             WHERE m.detail_fetched = 0 AND m.default_win = 0 AND c.season >= ?2
             ORDER BY m.time DESC LIMIT ?1",
        )
        .bind(limit)
        .bind(min_season)
        .fetch_all(self.pool())
        .await?)
    }

    /// A match's per-map scores and players, replacing what was there.
    pub async fn put_season_match_detail(
        &self,
        match_id: i64,
        maps: &[(i64, &str, i64, i64, bool)],
        players: &[(u32, Option<i64>, &str)],
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM etf2l_season_map WHERE match_id = ?1").bind(match_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM etf2l_season_player WHERE match_id = ?1").bind(match_id).execute(&mut *tx).await?;
        for (order, map, c1, c2, golden) in maps {
            sqlx::query("INSERT OR REPLACE INTO etf2l_season_map (match_id, match_order, map, clan1, clan2, golden_cap) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
                .bind(match_id)
                .bind(order)
                .bind(map)
                .bind(c1)
                .bind(c2)
                .bind(golden)
                .execute(&mut *tx)
                .await?;
        }
        for (account, team, name) in players {
            sqlx::query("INSERT OR REPLACE INTO etf2l_season_player (match_id, account_id, team_id, name) VALUES (?1, ?2, ?3, ?4)")
                .bind(match_id)
                .bind(i64::from(*account))
                .bind(team)
                .bind(name)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE etf2l_season_match SET detail_fetched = 1 WHERE match_id = ?1").bind(match_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Every stored official, with names; `season` narrows to one.
    pub async fn season_matches(&self, season: Option<i64>, team: Option<i64>) -> Result<Vec<SeasonMatch>> {
        let rows = sqlx::query(
            "SELECT m.match_id, m.competition_id, c.season, COALESCE(NULLIF(m.division, ''), c.division) AS division, m.tier, c.stage, m.week, m.round, m.time,
                    m.clan1_id, COALESCE(t1.name, '') AS clan1_name, m.clan2_id, COALESCE(t2.name, '') AS clan2_name,
                    m.r1, m.r2, m.default_win, m.maps
             FROM etf2l_season_match m
             JOIN etf2l_competition c ON c.competition_id = m.competition_id
             LEFT JOIN etf2l_team t1 ON t1.team_id = m.clan1_id
             LEFT JOIN etf2l_team t2 ON t2.team_id = m.clan2_id
             WHERE (?1 IS NULL OR c.season = ?1) AND (?2 IS NULL OR m.clan1_id = ?2 OR m.clan2_id = ?2)
             ORDER BY m.time DESC",
        )
        .bind(season)
        .bind(team)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SeasonMatch {
                match_id: r.get("match_id"),
                competition_id: r.get("competition_id"),
                season: r.get("season"),
                division: r.get("division"),
                tier: r.get("tier"),
                stage: r.get("stage"),
                week: r.get("week"),
                round: r.get("round"),
                time: r.get("time"),
                clan1_id: r.get("clan1_id"),
                clan1_name: r.get("clan1_name"),
                clan2_id: r.get("clan2_id"),
                clan2_name: r.get("clan2_name"),
                r1: r.get("r1"),
                r2: r.get("r2"),
                default_win: r.get::<i64, _>("default_win") != 0,
                maps: r.get::<Option<String>, _>("maps").and_then(|p| serde_json::from_str(&p).ok()).unwrap_or_default(),
            })
            .collect())
    }

    /// Per-map scores for matches read in detail; `team` narrows to one.
    pub async fn season_maps(&self, team: Option<i64>) -> Result<Vec<SeasonMap>> {
        let rows = sqlx::query(
            "SELECT p.match_id, p.map, p.clan1, p.clan2 FROM etf2l_season_map p
             JOIN etf2l_season_match m ON m.match_id = p.match_id
             WHERE ?1 IS NULL OR m.clan1_id = ?1 OR m.clan2_id = ?1
             ORDER BY p.match_id, p.match_order",
        )
        .bind(team)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SeasonMap { match_id: r.get("match_id"), map: r.get("map"), clan1: r.get("clan1"), clan2: r.get("clan2") })
            .collect())
    }

    /// Who played for `team`: `(account, name, matches, last played)`, most first.
    pub async fn season_players(&self, team: i64) -> Result<Vec<(u32, String, i64, i64)>> {
        let rows = sqlx::query(
            "SELECT p.account_id, MAX(p.name) AS name, COUNT(*) AS n, MAX(m.time) AS last
             FROM etf2l_season_player p JOIN etf2l_season_match m ON m.match_id = p.match_id
             WHERE p.team_id = ?1 GROUP BY p.account_id ORDER BY n DESC, last DESC",
        )
        .bind(team)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<i64, _>("account_id") as u32, r.get::<Option<String>, _>("name").unwrap_or_default(), r.get("n"), r.get::<Option<i64>, _>("last").unwrap_or(0)))
            .collect())
    }

    /// Each player's officials for `team` per season: `(account, season, n)`.
    pub async fn season_players_by_season(&self, team: i64) -> Result<Vec<(u32, i64, i64)>> {
        let rows = sqlx::query(
            "SELECT p.account_id, c.season, COUNT(*) AS n
             FROM etf2l_season_player p
             JOIN etf2l_season_match m ON m.match_id = p.match_id
             JOIN etf2l_competition c ON c.competition_id = m.competition_id
             WHERE p.team_id = ?1 GROUP BY p.account_id, c.season",
        )
        .bind(team)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get::<i64, _>("account_id") as u32, r.get("season"), r.get("n"))).collect())
    }

    /// What each player played for this team per season: `(account, season,
    /// class, seconds)`, most time first per account and season.
    pub async fn team_classes_by_season(&self, team: i64) -> Result<Vec<(u32, i64, String, i64)>> {
        let rows = sqlx::query(
            "SELECT sp.account_id, c.season, p.class, SUM(p.seconds) AS s
             FROM etf2l_season_player sp
             JOIN etf2l_season_match m ON m.match_id = sp.match_id
             JOIN etf2l_competition c ON c.competition_id = m.competition_id
             JOIN league_log l ON l.etf2l_match_id = sp.match_id
             JOIN league_log_player p ON p.log_id = l.log_id AND p.account_id = sp.account_id
             WHERE sp.team_id = ?1 AND p.class IS NOT NULL
             GROUP BY sp.account_id, c.season, p.class
             ORDER BY sp.account_id, c.season, s DESC",
        )
        .bind(team)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get::<i64, _>("account_id") as u32, r.get("season"), r.get("class"), r.get("s"))).collect())
    }

    /// What each player played for this team in its officials: `(account,
    /// class, seconds)`, from the logs of its ETF2L matches, most time first
    /// per account (Flashy: the class must be the one played for the team,
    /// not the one played most in the owner's pugs).
    pub async fn team_classes(&self, team: i64) -> Result<Vec<(u32, String, i64)>> {
        let rows = sqlx::query(
            "SELECT sp.account_id, p.class, SUM(p.seconds) AS s
             FROM etf2l_season_player sp
             JOIN league_log l ON l.etf2l_match_id = sp.match_id
             JOIN league_log_player p ON p.log_id = l.log_id AND p.account_id = sp.account_id
             WHERE sp.team_id = ?1 AND p.class IS NOT NULL
             GROUP BY sp.account_id, p.class
             ORDER BY sp.account_id, s DESC",
        )
        .bind(team)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get::<i64, _>("account_id") as u32, r.get("class"), r.get("s"))).collect())
    }

    /// Ratings the pool holds for these accounts: `(account, class, games,
    /// average score)`, most played first per account.
    pub async fn pool_ratings(&self, accounts: &[u32], model_version: &str) -> Result<Vec<(u32, String, i64, f64)>> {
        if accounts.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; accounts.len()].join(",");
        let sql = format!(
            "SELECT account_id, class, COUNT(*) AS n, AVG(score) AS avg FROM rating
             WHERE model_version = ? AND account_id IN ({marks}) GROUP BY account_id, class ORDER BY account_id, n DESC"
        );
        let mut q = sqlx::query(&sql).bind(model_version);
        for a in accounts {
            q = q.bind(i64::from(*a));
        }
        Ok(q.fetch_all(self.pool())
            .await?
            .into_iter()
            .map(|r| (r.get::<i64, _>("account_id") as u32, r.get("class"), r.get("n"), r.get("avg")))
            .collect())
    }

    /// Every player's ETF2L name, from the official rosters already stored:
    /// the owner's matches and the season matches read for the league
    /// pages. No request of its own. The name from the latest match wins,
    /// since players rename.
    pub async fn etf2l_names(&self) -> Result<std::collections::HashMap<u32, String>> {
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT account_id, name FROM (
                 SELECT r.account_id, r.name, COALESCE(m.time, 0) AS t, r.match_id
                 FROM etf2l_roster r LEFT JOIN etf2l_match m ON m.match_id = r.match_id
                 UNION ALL
                 SELECT p.account_id, p.name, COALESCE(s.time, 0), p.match_id
                 FROM etf2l_season_player p LEFT JOIN etf2l_season_match s ON s.match_id = p.match_id
             )
             WHERE name IS NOT NULL AND TRIM(name) <> ''
             ORDER BY account_id, t, match_id",
        )
        .fetch_all(self.pool())
        .await?;
        // Ordered oldest first, so each later row overwrites.
        Ok(rows.into_iter().map(|(a, n)| (a as u32, n.trim().to_string())).collect())
    }

    /// A stored team's name, country and avatar.
    pub async fn etf2l_team(&self, team: i64) -> Result<Option<(String, Option<String>, Option<String>)>> {
        Ok(sqlx::query_as("SELECT name, country, avatar FROM etf2l_team WHERE team_id = ?1").bind(team).fetch_optional(self.pool()).await?)
    }
}

/// One official between two teams, from the first team's side (Q48).
#[derive(Debug, Clone)]
pub struct H2hRow {
    pub match_id: i64,
    pub time: Option<i64>,
    pub season: i64,
    pub division: Option<String>,
    pub round: Option<String>,
    pub score_a: Option<i64>,
    pub score_b: Option<i64>,
    pub default_win: bool,
}

impl Db {
    /// Every official between teams `a` and `b`, newest first, scores from
    /// `a`'s side.
    pub async fn head_to_head(&self, a: i64, b: i64) -> Result<Vec<H2hRow>> {
        let rows = sqlx::query(
            "SELECT m.match_id, m.time, c.season, m.division, m.round, m.clan1_id, m.r1, m.r2, m.default_win
               FROM etf2l_season_match m JOIN etf2l_competition c ON c.competition_id = m.competition_id
              WHERE (m.clan1_id = ?1 AND m.clan2_id = ?2) OR (m.clan1_id = ?2 AND m.clan2_id = ?1)
              ORDER BY m.time DESC",
        )
        .bind(a)
        .bind(b)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let first = r.get::<i64, _>("clan1_id") == a;
                let (r1, r2): (Option<i64>, Option<i64>) = (r.get("r1"), r.get("r2"));
                H2hRow {
                    match_id: r.get("match_id"),
                    time: r.get("time"),
                    season: r.get("season"),
                    division: r.get("division"),
                    round: r.get("round"),
                    score_a: if first { r1 } else { r2 },
                    score_b: if first { r2 } else { r1 },
                    default_win: r.get::<Option<i64>, _>("default_win").unwrap_or(0) != 0,
                }
            })
            .collect())
    }
}

/// One played official, any team's, for the ETF2L tab's results (Q74).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRow {
    pub match_id: i64,
    pub time: i64,
    pub season: i64,
    pub division: Option<String>,
    pub tier: Option<i64>,
    pub round: Option<String>,
    pub week: Option<i64>,
    pub clan1: TeamRef,
    pub clan2: TeamRef,
    pub r1: Option<i64>,
    pub r2: Option<i64>,
    pub default_win: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRef {
    pub id: i64,
    pub name: String,
    pub avatar: Option<String>,
}

/// A team found by name (Q74's search), with how much of the league it has
/// played.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamHit {
    pub team_id: i64,
    pub name: String,
    pub country: Option<String>,
    pub avatar: Option<String>,
    pub officials: i64,
    /// The last season it played, and when.
    pub last_season: Option<i64>,
    pub last_played: Option<i64>,
}

/// One official's map, from its own page: rounds each side took.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchMap {
    pub map: String,
    pub clan1: i64,
    pub clan2: i64,
    pub golden_cap: bool,
}

/// Who played an official, and for which side.
#[derive(Debug, Clone)]
pub struct MatchPlayerRow {
    pub account_id: u32,
    pub team_id: Option<i64>,
    pub name: Option<String>,
}

/// A log of an official: the owner's (openable in the app) or the league
/// sample's.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchLog {
    pub log_id: i64,
    pub map: Option<String>,
    /// In the owner's matches, so the app can open it.
    pub stored: bool,
}

const RESULT_SELECT: &str = "SELECT m.match_id, m.time, c.season, m.division, m.tier, m.round, m.week, m.r1, m.r2, m.default_win,
        m.clan1_id, t1.name AS n1, t1.avatar AS a1, m.clan2_id, t2.name AS n2, t2.avatar AS a2
   FROM etf2l_season_match m
   JOIN etf2l_competition c ON c.competition_id = m.competition_id
   LEFT JOIN etf2l_team t1 ON t1.team_id = m.clan1_id
   LEFT JOIN etf2l_team t2 ON t2.team_id = m.clan2_id";

fn result_row(r: &sqlx::sqlite::SqliteRow) -> ResultRow {
    ResultRow {
        match_id: r.get("match_id"),
        // An official not yet given a date has none.
        time: r.get::<Option<i64>, _>("time").unwrap_or(0),
        season: r.get("season"),
        division: r.get("division"),
        tier: r.get("tier"),
        round: r.get("round"),
        week: r.get("week"),
        clan1: TeamRef { id: r.get("clan1_id"), name: r.get::<Option<String>, _>("n1").unwrap_or_default(), avatar: r.get("a1") },
        clan2: TeamRef { id: r.get("clan2_id"), name: r.get::<Option<String>, _>("n2").unwrap_or_default(), avatar: r.get("a2") },
        r1: r.get("r1"),
        r2: r.get("r2"),
        default_win: r.get::<i64, _>("default_win") != 0,
    }
}

impl Db {
    /// The latest played officials of every team, newest first: a score in,
    /// or a default win.
    pub async fn recent_results(&self, limit: i64) -> Result<Vec<ResultRow>> {
        let rows = sqlx::query(&format!(
            "{RESULT_SELECT}
              WHERE m.time IS NOT NULL AND (m.r1 IS NOT NULL OR m.default_win <> 0)
              ORDER BY m.time DESC
              LIMIT ?1"
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.iter().map(result_row).collect())
    }

    /// One official, played or not, as the results list has it.
    pub async fn league_match(&self, match_id: i64) -> Result<Option<ResultRow>> {
        let row = sqlx::query(&format!("{RESULT_SELECT} WHERE m.match_id = ?1")).bind(match_id).fetch_optional(self.pool()).await?;
        Ok(row.as_ref().map(result_row))
    }

    /// An official's maps in the order played.
    pub async fn league_match_maps(&self, match_id: i64) -> Result<Vec<MatchMap>> {
        let rows = sqlx::query("SELECT map, clan1, clan2, golden_cap FROM etf2l_season_map WHERE match_id = ?1 ORDER BY match_order")
            .bind(match_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| MatchMap { map: r.get("map"), clan1: r.get("clan1"), clan2: r.get("clan2"), golden_cap: r.get::<i64, _>("golden_cap") != 0 })
            .collect())
    }

    /// Who played an official.
    pub async fn league_match_players(&self, match_id: i64) -> Result<Vec<MatchPlayerRow>> {
        let rows = sqlx::query("SELECT account_id, team_id, name FROM etf2l_season_player WHERE match_id = ?1 ORDER BY team_id, name")
            .bind(match_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| MatchPlayerRow { account_id: r.get::<i64, _>("account_id") as u32, team_id: r.get("team_id"), name: r.get("name") })
            .collect())
    }

    /// An official's logs: the owner's, linked by their context, and the
    /// league sample's; each once, the owner's first.
    pub async fn league_match_logs(&self, match_id: i64) -> Result<Vec<MatchLog>> {
        let rows = sqlx::query(
            "SELECT c.log_id, m.map, 1 AS stored, COALESCE(m.played_at, 0) AS at FROM match_context c LEFT JOIN match m ON m.log_id = c.log_id
              WHERE c.etf2l_match_id = ?1
             UNION
             SELECT l.log_id, l.map, EXISTS (SELECT 1 FROM match m WHERE m.log_id = l.log_id), l.played_at FROM league_log l
              WHERE l.etf2l_match_id = ?1 AND l.log_id NOT IN (SELECT log_id FROM match_context WHERE etf2l_match_id = ?1)
             ORDER BY at",
        )
        .bind(match_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| MatchLog { log_id: r.get("log_id"), map: r.get("map"), stored: r.get::<i64, _>("stored") != 0 })
            .collect())
    }

    /// Teams whose name holds `query`, any case: exact and prefix matches
    /// first, then the ones that played most recently.
    pub async fn search_teams(&self, query: &str, limit: i64) -> Result<Vec<TeamHit>> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        // `!` escapes, so a % or _ in a name is a character, not a wildcard.
        let like = format!("%{}%", q.replace('!', "!!").replace('%', "!%").replace('_', "!_"));
        let rows = sqlx::query(
            "SELECT t.team_id, t.name, t.country, t.avatar,
                    (SELECT COUNT(*) FROM etf2l_season_match m WHERE m.clan1_id = t.team_id OR m.clan2_id = t.team_id) AS officials,
                    (SELECT MAX(c.season) FROM etf2l_season_match m JOIN etf2l_competition c ON c.competition_id = m.competition_id
                      WHERE (m.clan1_id = t.team_id OR m.clan2_id = t.team_id) AND c.season < 100) AS last_season,
                    (SELECT MAX(m.time) FROM etf2l_season_match m WHERE m.clan1_id = t.team_id OR m.clan2_id = t.team_id) AS last_played
               FROM etf2l_team t
              WHERE lower(t.name) LIKE ?1 ESCAPE '!'
              ORDER BY (lower(t.name) = ?2) DESC, (lower(t.name) LIKE ?3 ESCAPE '!') DESC, last_played DESC
              LIMIT ?4",
        )
        .bind(&like)
        .bind(&q)
        .bind(format!("{}%", &like[1..like.len() - 1]))
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| TeamHit {
                team_id: r.get("team_id"),
                name: r.get("name"),
                country: r.get("country"),
                avatar: r.get("avatar"),
                officials: r.get("officials"),
                last_season: r.get("last_season"),
                last_played: r.get("last_played"),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::Db;

    #[tokio::test]
    async fn etf2l_names_take_the_latest_from_either_roster() {
        let db = Db::connect_in_memory().await.unwrap();
        for sql in [
            "INSERT INTO etf2l_match (match_id, time) VALUES (1, 100), (2, 300)",
            "INSERT INTO etf2l_roster (match_id, account_id, name) VALUES (1, 7, 'old'), (2, 7, 'owner era'), (1, 8, '  '), (1, 9, 'nine')",
            "INSERT INTO etf2l_season_match (match_id, competition_id, time, clan1_id, clan2_id) VALUES (10, 1, 200, 1, 2), (11, 1, 400, 1, 2)",
            "INSERT INTO etf2l_season_player (match_id, account_id, name) VALUES (10, 7, 'between'), (11, 9, 'nine renamed'), (10, 8, 'eight')",
        ] {
            sqlx::query(sql).execute(db.pool()).await.unwrap();
        }
        let names = db.etf2l_names().await.unwrap();
        assert_eq!(names.get(&7).map(String::as_str), Some("owner era"), "time 300 beats 200 and 100");
        assert_eq!(names.get(&8).map(String::as_str), Some("eight"), "a blank name is skipped");
        assert_eq!(names.get(&9).map(String::as_str), Some("nine renamed"));
    }

    #[tokio::test]
    async fn results_are_newest_first_and_teams_found_by_name() {
        let db = Db::connect_in_memory().await.unwrap();
        for sql in [
            "INSERT INTO etf2l_competition (competition_id, season, season_name, division, stage, name, fetched_at) VALUES (1, 36, 'Autumn 2026', 'Open', 'regular', 'HL 36', 0)",
            "INSERT INTO etf2l_team (team_id, name) VALUES (1, 'Comp Chaos'), (2, 'Chaos Theory'), (3, 'BRETT'), (4, '100%_real')",
            "INSERT INTO etf2l_season_match (match_id, competition_id, time, clan1_id, clan2_id, r1, r2) VALUES (10, 1, 100, 1, 3, 4, 0), (11, 1, 300, 2, 3, 1, 3), (12, 1, 500, 1, 2, NULL, NULL)",
        ] {
            sqlx::query(sql).execute(db.pool()).await.unwrap();
        }
        let r = db.recent_results(10).await.unwrap();
        assert_eq!(r.iter().map(|x| x.match_id).collect::<Vec<_>>(), vec![11, 10], "the unplayed one is left out");
        assert_eq!((r[0].clan1.name.as_str(), r[0].r2), ("Chaos Theory", Some(3)));
        let t = db.search_teams("chaos", 10).await.unwrap();
        assert_eq!(t.iter().map(|x| x.team_id).collect::<Vec<_>>(), vec![2, 1], "prefix first");
        assert_eq!((t[1].officials, t[1].last_season), (2, Some(36)));
        assert_eq!(db.search_teams("%", 10).await.unwrap().len(), 1, "a % is a character, not a wildcard");
    }
}
