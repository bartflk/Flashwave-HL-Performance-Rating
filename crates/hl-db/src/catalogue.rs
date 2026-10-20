//! The player catalogue (Q35): every ETF2L official in the database and who
//! played it, for player profiles. Plain loaders; the seasons, divisions and
//! medals are worked out in `hl_ingest::catalogue`, where they can be tested.

use crate::Db;
use anyhow::Result;
use sqlx::Row;
use std::collections::HashMap;

/// One ETF2L official with its competition.
#[derive(Debug, Clone)]
pub struct CatMatch {
    pub match_id: i64,
    pub competition_id: i64,
    pub season: i64,
    pub season_name: String,
    /// The competition's name: the division and stage are read from it
    /// again on load, so a name the parser learned to read later is read.
    pub comp_name: String,
    /// The competition's division ("Low" for "Low Playoffs"), and stage.
    pub comp_division: String,
    pub stage: String,
    /// The match's own division and tier: absent on playoff matches.
    pub division: Option<String>,
    pub tier: Option<i64>,
    pub round: Option<String>,
    pub time: Option<i64>,
    pub clan1: i64,
    pub clan2: i64,
    pub r1: Option<i64>,
    pub r2: Option<i64>,
    pub default_win: bool,
}

/// One player in one official: `(match, account, team, name)`.
pub type RosterRow = (i64, u32, Option<i64>, String);

/// One rated game of anyone's, for profiles and ranks (Q36): the owner's
/// matches carry the full breakdown (`parts`, a JSON list); the league
/// sample's carry the component groups already worked out (`groups`).
#[derive(Debug, Clone)]
pub struct RatedGame {
    pub account_id: u32,
    pub log_id: i64,
    pub played_at: i64,
    pub class: String,
    pub score: f64,
    pub parts: Option<String>,
    pub groups: Option<String>,
    /// An ETF2L official: every league-sample game, and the owner's matches
    /// classified as officials. Ranks count only these.
    pub official: bool,
    /// The ETF2L match it belongs to, where known: an official is one log
    /// in the owner's matches (combined) and one per map in the sample, so
    /// ranks count officials, not logs.
    pub etf2l_match_id: Option<i64>,
    /// One of the logs trends.tf lists for its ETF2L match: a map, not a
    /// combined upload. Where a match has both, the MVPs read these, so no
    /// game is counted twice.
    pub listed: bool,
}

/// ETF2L's page for a player, as kept.
#[derive(Debug, Clone, Default)]
pub struct Etf2lPlayer {
    pub etf2l_id: Option<i64>,
    pub name: Option<String>,
    pub country: Option<String>,
    pub classes: Vec<String>,
    pub avatar: Option<String>,
    pub registered: Option<i64>,
    pub fetched_at: i64,
}

impl Db {
    pub async fn catalogue_matches(&self) -> Result<Vec<CatMatch>> {
        let rows = sqlx::query(
            "SELECT m.match_id, m.competition_id, c.season, c.season_name, c.name AS comp_name, c.division AS comp_division, c.stage,
                    m.division, m.tier, m.round, m.time, m.clan1_id, m.clan2_id, m.r1, m.r2, m.default_win
             FROM etf2l_season_match m JOIN etf2l_competition c ON c.competition_id = m.competition_id",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| CatMatch {
                match_id: r.get("match_id"),
                competition_id: r.get("competition_id"),
                season: r.get("season"),
                season_name: r.get("season_name"),
                comp_name: r.get("comp_name"),
                comp_division: r.get("comp_division"),
                stage: r.get("stage"),
                division: r.get("division"),
                tier: r.get("tier"),
                round: r.get("round"),
                time: r.get("time"),
                clan1: r.get("clan1_id"),
                clan2: r.get("clan2_id"),
                r1: r.get("r1"),
                r2: r.get("r2"),
                default_win: r.get::<i64, _>("default_win") != 0,
            })
            .collect())
    }

    /// Everyone who played each official, for the matches `account` is
    /// given (or every match without it).
    pub async fn catalogue_rosters(&self, account: Option<u32>) -> Result<Vec<RosterRow>> {
        let rows = sqlx::query(
            "SELECT match_id, account_id, team_id, COALESCE(name, '') AS name FROM etf2l_season_player
             WHERE ?1 IS NULL OR match_id IN (SELECT match_id FROM etf2l_season_player WHERE account_id = ?1)",
        )
        .bind(account.map(i64::from))
        .fetch_all(self.pool())
        .await?;
        let guessed = self.merc_sides(None).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let (m, a): (i64, i64) = (r.get("match_id"), r.get("account_id"));
                let team: Option<i64> = r.get::<Option<i64>, _>("team_id").or_else(|| guessed.get(&(m, a)).copied());
                (m, a as u32, team, r.get("name"))
            })
            .collect())
    }

    /// ETF2L lists a merc -- someone playing for a team they are not on
    /// yet -- with no team (Flashy's first DD14 official). Their team is the
    /// one whose players were on their side in the match's logs: for one
    /// match, or every match without it. Keyed by `(match, account)`.
    pub async fn merc_sides(&self, match_id: Option<i64>) -> Result<HashMap<(i64, i64), i64>> {
        let guessed: HashMap<(i64, i64), i64> = {
            let rows = sqlx::query(
                "WITH merc AS (SELECT match_id, account_id FROM etf2l_season_player WHERE team_id IS NULL AND (?1 IS NULL OR match_id = ?1)),
                 side AS (
                   SELECT l.log_id, l.etf2l_match_id AS match_id, p.account_id, p.team FROM league_log l
                     JOIN league_log_player p ON p.log_id = l.log_id WHERE l.etf2l_match_id IN (SELECT match_id FROM merc)
                   UNION
                   SELECT c.log_id, c.etf2l_match_id, p.account_id, p.team FROM match_context c
                     JOIN match_player p ON p.log_id = c.log_id WHERE c.etf2l_match_id IN (SELECT match_id FROM merc)
                 )
                 SELECT me.match_id, me.account_id, o.team_id, COUNT(*) AS n
                 FROM merc
                 JOIN side me ON me.match_id = merc.match_id AND me.account_id = merc.account_id
                 JOIN side mate ON mate.log_id = me.log_id AND mate.team = me.team AND mate.account_id != me.account_id
                 JOIN etf2l_season_player o ON o.match_id = me.match_id AND o.account_id = mate.account_id AND o.team_id IS NOT NULL
                 GROUP BY me.match_id, me.account_id, o.team_id
                 ORDER BY n ASC",
            )
            .bind(match_id)
            .fetch_all(self.pool())
            .await?;
            // Ascending, so the team seen most is inserted last and kept.
            rows.into_iter().map(|r| ((r.get("match_id"), r.get("account_id")), r.get("team_id"))).collect()
        };
        Ok(guessed)
    }

    /// Who played on which colour in the logs of `matches`: `(match, log,
    /// account, "Red" | "Blue")`, from the league sample's logs and the
    /// owner's. The MVPs read a player's side from it when ETF2L's roster
    /// does not place them (a merc, a ringer it never listed).
    pub async fn log_colours(&self, matches: &[i64]) -> Result<Vec<(i64, i64, u32, String)>> {
        let ids = serde_json::to_string(matches)?;
        let rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(
            "SELECT l.etf2l_match_id, p.log_id, p.account_id, p.team FROM league_log l JOIN league_log_player p ON p.log_id = l.log_id
              WHERE l.etf2l_match_id IN (SELECT value FROM json_each(?1)) AND p.team IS NOT NULL
             UNION
             SELECT c.etf2l_match_id, p.log_id, p.account_id, p.team FROM match_context c JOIN match_player p ON p.log_id = c.log_id
              WHERE c.etf2l_match_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(ids)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(m, l, a, t)| (m, l, a as u32, t)).collect())
    }

    /// The mercs of every match: `(match, account)` for each player ETF2L
    /// lists with no team -- someone playing for a team they are not on.
    pub async fn catalogue_mercs(&self) -> Result<Vec<(i64, u32)>> {
        let rows: Vec<(i64, i64)> = sqlx::query_as("SELECT match_id, account_id FROM etf2l_season_player WHERE team_id IS NULL")
            .fetch_all(self.pool())
            .await?;
        Ok(rows.into_iter().map(|(m, a)| (m, a as u32)).collect())
    }

    /// A match's date and everyone in it, for their divisions (Q38).
    pub async fn match_accounts(&self, log_id: i64) -> Result<Option<(i64, Vec<u32>)>> {
        let Some(played_at) = sqlx::query_scalar::<_, Option<i64>>("SELECT played_at FROM match WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?
            .flatten()
        else {
            return Ok(None);
        };
        let accounts: Vec<i64> = sqlx::query_scalar("SELECT account_id FROM match_player WHERE log_id = ?1").bind(log_id).fetch_all(self.pool()).await?;
        Ok(Some((played_at, accounts.into_iter().map(|a| a as u32).collect())))
    }

    /// Every ETF2L team: name and avatar.
    pub async fn etf2l_teams(&self) -> Result<HashMap<i64, (String, Option<String>)>> {
        let rows = sqlx::query("SELECT team_id, name, avatar FROM etf2l_team").fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|r| (r.get("team_id"), (r.get("name"), r.get("avatar")))).collect())
    }

    /// Games per main class, for every account: from the owner's matches
    /// and the league sample together.
    pub async fn main_class_games(&self) -> Result<HashMap<u32, HashMap<String, i64>>> {
        let rows = sqlx::query(&format!(
            "SELECT account_id, main_class AS class, COUNT(*) AS n FROM match_player
               WHERE main_class IS NOT NULL GROUP BY account_id, main_class
             UNION ALL
             SELECT account_id, class, COUNT(*) FROM league_log_player
               WHERE class IS NOT NULL AND log_id NOT IN ({covered})
               GROUP BY account_id, class",
            covered = crate::OWNER_COVERED
        ))
        .fetch_all(self.pool())
        .await?;
        let mut out: HashMap<u32, HashMap<String, i64>> = HashMap::new();
        for r in rows {
            *out.entry(r.get::<i64, _>("account_id") as u32).or_default().entry(r.get("class")).or_default() += r.get::<i64, _>("n");
        }
        Ok(out)
    }

    /// Every name each player has appeared under in the owner's matches,
    /// newest first: for search, beside the ETF2L names.
    pub async fn log_names(&self) -> Result<HashMap<u32, Vec<String>>> {
        let rows = sqlx::query(
            "SELECT p.account_id, p.name FROM match_player p JOIN match m ON m.log_id = p.log_id
             WHERE p.name IS NOT NULL GROUP BY p.account_id, p.name ORDER BY MAX(m.played_at) DESC",
        )
        .fetch_all(self.pool())
        .await?;
        let mut out: HashMap<u32, Vec<String>> = HashMap::new();
        for r in rows {
            out.entry(r.get::<i64, _>("account_id") as u32).or_default().push(r.get("name"));
        }
        Ok(out)
    }

    /// League-sample logs with JSON whose players are not indexed yet.
    pub async fn league_logs_unindexed(&self, limit: i64) -> Result<Vec<(i64, String)>> {
        let rows = sqlx::query(
            "SELECT j.log_id, j.json FROM league_log_json j
             WHERE NOT EXISTS (SELECT 1 FROM league_log_player p WHERE p.log_id = j.log_id) LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get("log_id"), r.get("json"))).collect())
    }

    /// One log's players: `(account, team, main class, seconds)`.
    pub async fn put_league_players(&self, log_id: i64, players: &[(u32, Option<&str>, Option<&str>, i64)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM league_log_player WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        for (account, team, class, seconds) in players {
            sqlx::query("INSERT INTO league_log_player (log_id, account_id, team, class, seconds) VALUES (?1, ?2, ?3, ?4, ?5)")
                .bind(log_id)
                .bind(i64::from(*account))
                .bind(team)
                .bind(class)
                .bind(seconds)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The league sample's ratings for a model: `(log, account, class,
    /// score, minutes, groups JSON)`, replacing the model's old ones.
    pub async fn replace_league_ratings(&self, version: &str, rows: &[(i64, u32, &str, f64, f64, String)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM league_rating WHERE model_version = ?1").bind(version).execute(&mut *tx).await?;
        Self::insert_league_ratings(&mut tx, version, rows).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Adds sample ratings, keeping the rest: a log rated as it arrives.
    pub async fn put_league_ratings(&self, version: &str, rows: &[(i64, u32, &str, f64, f64, String)]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        Self::insert_league_ratings(&mut tx, version, rows).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn insert_league_ratings(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        version: &str,
        rows: &[(i64, u32, &str, f64, f64, String)],
    ) -> Result<()> {
        for (log_id, account, class, score, minutes, groups) in rows {
            sqlx::query(
                "INSERT OR REPLACE INTO league_rating (model_version, log_id, account_id, class, score, minutes, groups)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(version)
            .bind(log_id)
            .bind(i64::from(*account))
            .bind(class)
            .bind(score)
            .bind(minutes)
            .bind(groups)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Every rated game -- the owner's matches and the league sample -- for
    /// one account, or everyone's. A game in both is counted once, from the
    /// owner's matches.
    pub async fn rated_games(&self, version: &str, account: Option<u32>) -> Result<Vec<RatedGame>> {
        let rows = sqlx::query(&format!(
            "SELECT r.account_id, r.log_id, COALESCE(m.played_at, 0) AS played_at, r.class, r.score,
                    r.parts AS parts, NULL AS groups,
                    -- An official however it is known, not only as one of
                    -- the owner's: a league log stored here (opened from an
                    -- ETF2L page, or a combined log's parts found on logs.tf)
                    -- is still the official it was, or every player in it
                    -- loses one from the season tables (Flashy: S36 High's
                    -- Sniper table thinning out as matches were opened).
                    (EXISTS (SELECT 1 FROM match_context c WHERE c.log_id = r.log_id AND c.kind = 'official')
                     OR EXISTS (SELECT 1 FROM league_log l WHERE l.log_id = r.log_id)
                     OR EXISTS (SELECT 1 FROM log_index p JOIN league_log l ON l.log_id = p.log_id WHERE p.superseded_by = r.log_id)
                     OR EXISTS (SELECT 1 FROM log_index i WHERE i.log_id = r.log_id AND i.etf2l_match_id IS NOT NULL)
                     OR EXISTS (SELECT 1 FROM etf2l_raw x WHERE x.kind = 'league_sides' AND x.id = r.log_id)) AS official,
                    -- The owner's logs of an official, linked to its match
                    -- however it is known: their own context, the league's
                    -- list, trends.tf's index, or -- for a combined log,
                    -- the one kept -- the per-map logs it replaced (S31
                    -- Low's final had only that).
                    COALESCE((SELECT c.etf2l_match_id FROM match_context c WHERE c.log_id = r.log_id),
                             (SELECT l.etf2l_match_id FROM league_log l WHERE l.log_id = r.log_id),
                             (SELECT i.etf2l_match_id FROM log_index i WHERE i.log_id = r.log_id),
                             (SELECT i.etf2l_match_id FROM log_index i WHERE i.superseded_by = r.log_id AND i.etf2l_match_id IS NOT NULL LIMIT 1),
                             (SELECT l.etf2l_match_id FROM log_index p JOIN league_log l ON l.log_id = p.log_id WHERE p.superseded_by = r.log_id LIMIT 1),
                             (SELECT json_extract(x.json, '$.matchId') FROM etf2l_raw x WHERE x.kind = 'league_sides' AND x.id = r.log_id)) AS etf2l_match_id,
                    EXISTS (SELECT 1 FROM league_log l WHERE l.log_id = r.log_id) AS listed
             FROM rating r JOIN match m ON m.log_id = r.log_id
             WHERE r.model_version = ?1 AND (?2 IS NULL OR r.account_id = ?2)
             UNION ALL
             SELECT r.account_id, r.log_id, l.played_at, r.class, r.score, NULL, r.groups, 1, l.etf2l_match_id, 1
             FROM league_rating r JOIN league_log l ON l.log_id = r.log_id
             WHERE r.model_version = ?1 AND (?2 IS NULL OR r.account_id = ?2)
               AND r.log_id NOT IN ({covered})",
            covered = crate::OWNER_COVERED
        ))
        .bind(version)
        .bind(account.map(i64::from))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| RatedGame {
                account_id: r.get::<i64, _>("account_id") as u32,
                log_id: r.get("log_id"),
                played_at: r.get("played_at"),
                class: r.get("class"),
                score: r.get("score"),
                parts: r.get("parts"),
                groups: r.get("groups"),
                official: r.get::<i64, _>("official") != 0,
                etf2l_match_id: r.get("etf2l_match_id"),
                listed: r.get::<i64, _>("listed") != 0,
            })
            .collect())
    }

    /// A player's trends.tf career as last read (Q37): `(json, fetched_at)`.
    pub async fn trends_career(&self, account: u32) -> Result<Option<(String, i64)>> {
        Ok(sqlx::query_as("SELECT career, fetched_at FROM trends_career WHERE account_id = ?1").bind(i64::from(account)).fetch_optional(self.pool()).await?)
    }

    pub async fn put_trends_career(&self, account: u32, career: &str, at: i64) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO trends_career (account_id, career, fetched_at) VALUES (?1, ?2, ?3)")
            .bind(i64::from(account))
            .bind(career)
            .bind(at)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn etf2l_player(&self, account: u32) -> Result<Option<Etf2lPlayer>> {
        let Some(r) = sqlx::query("SELECT * FROM etf2l_player WHERE account_id = ?1").bind(i64::from(account)).fetch_optional(self.pool()).await? else {
            return Ok(None);
        };
        Ok(Some(Etf2lPlayer {
            etf2l_id: r.get("etf2l_id"),
            name: r.get("name"),
            country: r.get("country"),
            classes: r.get::<Option<String>, _>("classes").and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default(),
            avatar: r.get("avatar"),
            registered: r.get("registered"),
            fetched_at: r.get("fetched_at"),
        }))
    }

    pub async fn put_etf2l_player(&self, account: u32, p: &Etf2lPlayer) -> Result<()> {
        sqlx::query(
            "INSERT INTO etf2l_player (account_id, etf2l_id, name, country, classes, avatar, registered, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (account_id) DO UPDATE SET etf2l_id = excluded.etf2l_id, name = excluded.name,
                country = excluded.country, classes = excluded.classes, avatar = excluded.avatar,
                registered = excluded.registered, fetched_at = excluded.fetched_at",
        )
        .bind(i64::from(account))
        .bind(p.etf2l_id)
        .bind(&p.name)
        .bind(&p.country)
        .bind(serde_json::to_string(&p.classes)?)
        .bind(&p.avatar)
        .bind(p.registered)
        .bind(p.fetched_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
