//! The league sample (0033): ETF2L officials from every division, kept apart
//! from the owner's own matches. Which log belongs to which match comes from
//! trends.tf; the division from `etf2l_season_match`; the logs themselves
//! from logs.tf or more.tf. The choosing is `hl_ingest::league_sample`.

use crate::Db;
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;

/// A match's division and tier, playoff matches included: they carry
/// neither, so both come from their competition ("Low Playoffs") and that
/// season's regular matches in the same division. Needs `m` (the match)
/// and `c` (its competition) in scope.
/// A match's division: its own, its competition's, or -- for a final played
/// as a round of the season, with neither (Autumn 2023, Season 32) -- the
/// one its teams played that season.
const DIVISION_SQL: &str = "COALESCE(m.division, NULLIF(c.division, ''),
      (SELECT m3.division FROM etf2l_season_match m3
         JOIN etf2l_competition c3 ON c3.competition_id = m3.competition_id
        WHERE c3.season = c.season AND m3.division IS NOT NULL AND m3.division != ''
          AND (m3.clan1_id IN (m.clan1_id, m.clan2_id) OR m3.clan2_id IN (m.clan1_id, m.clan2_id))
        LIMIT 1))";
const TIER_SQL: &str = "COALESCE(m.tier, (SELECT m2.tier FROM etf2l_season_match m2
      JOIN etf2l_competition c2 ON c2.competition_id = m2.competition_id
      WHERE c2.season = c.season AND m2.division = c.division AND m2.tier IS NOT NULL LIMIT 1))";

/// One ETF2L Highlander log as trends.tf lists it.
pub struct LeagueLogRow<'a> {
    pub log_id: i64,
    pub etf2l_match_id: i64,
    pub map: &'a str,
    pub played_at: i64,
    pub duration_s: Option<i64>,
    pub title: Option<&'a str>,
}

/// A log that could go in the sample, with its match's division.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub log_id: i64,
    pub match_id: i64,
    pub map: String,
    pub played_at: i64,
    pub duration_s: i64,
    /// ETF2L's own tier where it gives one; the division's name on today's
    /// ladder decides first (`select`).
    pub tier: Option<i64>,
    pub division: String,
    /// The match is a Grand Final: its logs are fetched first.
    pub is_final: bool,
}

/// One division's progress, for Settings and `hl league-sample`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierProgress {
    pub tier: i64,
    /// The newest name ETF2L gave this tier ("Premiership", "High", ...).
    pub division: String,
    pub matches: i64,
    pub logs: i64,
    /// Logs with their JSON, from logs.tf and from more.tf.
    pub json_logstf: i64,
    /// From drops.tf: logs.tf's own JSON, Icewind's copy.
    pub json_dropstf: i64,
    pub json_moretf: i64,
    /// Logs with their raw server log, and those logs.tf has none for.
    pub raw: i64,
    pub raw_missing: i64,
    pub maps: i64,
    /// Matches whose ETF2L page (who played) has been read.
    pub rosters: i64,
    pub oldest: Option<i64>,
    pub newest: Option<i64>,
}

impl Db {
    /// Store what trends.tf lists. A log already held keeps its state.
    pub async fn upsert_league_logs(&self, rows: &[LeagueLogRow<'_>]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        for r in rows {
            sqlx::query(
                "INSERT INTO league_log (log_id, etf2l_match_id, map, played_at, duration_s, title)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (log_id) DO UPDATE SET etf2l_match_id = excluded.etf2l_match_id,
                    map = excluded.map, duration_s = excluded.duration_s, title = excluded.title",
            )
            .bind(r.log_id)
            .bind(r.etf2l_match_id)
            .bind(r.map)
            .bind(r.played_at)
            .bind(r.duration_s)
            .bind(r.title)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// How many logs trends.tf has listed so far, and the oldest and newest
    /// one's time.
    pub async fn league_logs_known(&self) -> Result<(i64, Option<i64>, Option<i64>)> {
        let row = sqlx::query("SELECT COUNT(*) AS n, MIN(played_at) AS oldest, MAX(played_at) AS newest FROM league_log")
            .fetch_one(self.pool())
            .await?;
        Ok((row.get("n"), row.get("oldest"), row.get("newest")))
    }

    /// Every listed log whose match ETF2L has placed in a division.
    pub async fn league_candidates(&self) -> Result<Vec<Candidate>> {
        let rows = sqlx::query(&format!(
            "SELECT * FROM (
               -- A log found by the app's own logs.tf search has no length
               -- until its JSON is in: read it from there, and -1 for a
               -- log not downloaded yet, which stays a candidate (Flashy:
               -- 109 officials' logs left out of the sample, and their
               -- players out of the season tables, for a missing length).
               SELECT l.log_id, l.etf2l_match_id, l.map, l.played_at,
                      COALESCE(l.duration_s,
                               (SELECT json_extract(j.json, '$.length') FROM league_log_json j WHERE j.log_id = l.log_id),
                               -1) AS duration_s,
                      {TIER_SQL} AS tier, {DIVISION_SQL} AS division,
                      (LOWER(REPLACE(COALESCE(m.round, ''), '-', ' ')) LIKE '%grand final%') AS is_final
               FROM league_log l
               JOIN etf2l_season_match m ON m.match_id = l.etf2l_match_id
               JOIN etf2l_competition c ON c.competition_id = m.competition_id
               WHERE m.default_win = 0)
             WHERE division IS NOT NULL AND division != ''"
        ))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Candidate {
                log_id: r.get("log_id"),
                match_id: r.get("etf2l_match_id"),
                map: r.get("map"),
                played_at: r.get("played_at"),
                duration_s: r.get("duration_s"),
                tier: r.get("tier"),
                division: r.get("division"),
                is_final: r.get::<i64, _>("is_final") != 0,
            })
            .collect())
    }

    /// The sample is exactly these logs. What was fetched for a log that
    /// drops out is kept: it costs nothing to hold, and a request to get back.
    pub async fn set_league_picked(&self, log_ids: &[i64]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE league_log SET picked = 0").execute(&mut *tx).await?;
        for id in log_ids {
            sqlx::query("UPDATE league_log SET picked = 1 WHERE log_id = ?1").bind(id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Played officials with no log at all -- trends.tf linked none to them,
    /// and neither is one of the owner's -- whose rosters are read and which
    /// were never searched for on logs.tf: `(match, time, a player of each
    /// team)`, newest first.
    pub async fn league_matches_without_logs(&self, since: i64, until: i64, limit: i64) -> Result<Vec<(i64, i64, i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT m.match_id, m.time,
                    (SELECT p.account_id FROM etf2l_season_player p WHERE p.match_id = m.match_id AND p.team_id = m.clan1_id LIMIT 1) AS a,
                    (SELECT p.account_id FROM etf2l_season_player p WHERE p.match_id = m.match_id AND p.team_id = m.clan2_id LIMIT 1) AS b
               FROM etf2l_season_match m
              WHERE m.default_win = 0 AND COALESCE(m.r1, 0) + COALESCE(m.r2, 0) > 0
                AND m.time BETWEEN ?1 AND ?2
                AND NOT EXISTS (SELECT 1 FROM league_log l WHERE l.etf2l_match_id = m.match_id)
                AND NOT EXISTS (SELECT 1 FROM log_index i WHERE i.etf2l_match_id = m.match_id)
                AND NOT EXISTS (SELECT 1 FROM etf2l_raw r WHERE r.kind = 'log_search' AND r.id = m.match_id)
                AND a IS NOT NULL AND b IS NOT NULL
              ORDER BY m.time DESC LIMIT ?3",
        )
        .bind(since)
        .bind(until)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Pick these logs too, beside what the last choice picked.
    /// Forget the logs linked to these matches that nothing has been
    /// downloaded for yet: an old season's import links again from scratch.
    pub async fn clear_unfetched_league_logs(&self, match_ids: &[i64]) -> Result<u64> {
        let ids = serde_json::to_string(match_ids)?;
        Ok(sqlx::query("DELETE FROM league_log WHERE etf2l_match_id IN (SELECT value FROM json_each(?1)) AND json_source IS NULL")
            .bind(ids)
            .execute(self.pool())
            .await?
            .rows_affected())
    }

    pub async fn pick_league_logs(&self, log_ids: &[i64]) -> Result<()> {
        for id in log_ids {
            sqlx::query("UPDATE league_log SET picked = 1 WHERE log_id = ?1").bind(id).execute(self.pool()).await?;
        }
        Ok(())
    }

    /// Picked logs with no JSON yet, newest first; `include_stand_ins` also
    /// offers those only more.tf has given, to ask logs.tf again.
    pub async fn league_json_todo(&self, limit: i64, include_stand_ins: bool, max_attempts: i64) -> Result<Vec<i64>> {
        let sql = if include_stand_ins {
            "SELECT log_id FROM league_log WHERE picked = 1 AND (json_source IS NULL OR json_source = 'more.tf')
               AND json_attempts < ?2 ORDER BY json_source IS NOT NULL, (etf2l_match_id IN (SELECT match_id FROM etf2l_season_match WHERE LOWER(REPLACE(COALESCE(round, ''), '-', ' ')) LIKE '%grand final%')) DESC, played_at DESC LIMIT ?1"
        } else {
            "SELECT log_id FROM league_log WHERE picked = 1 AND json_source IS NULL
               AND json_attempts < ?2 ORDER BY (etf2l_match_id IN (SELECT match_id FROM etf2l_season_match WHERE LOWER(REPLACE(COALESCE(round, ''), '-', ' ')) LIKE '%grand final%')) DESC, played_at DESC LIMIT ?1"
        };
        Ok(sqlx::query_scalar(sql).bind(limit).bind(max_attempts).fetch_all(self.pool()).await?)
    }

    pub async fn put_league_json(&self, log_id: i64, json: &str, source: &str) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO league_log_json (log_id, json) VALUES (?1, ?2) ON CONFLICT (log_id) DO UPDATE SET json = excluded.json")
            .bind(log_id)
            .bind(json)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE league_log SET json_source = ?2, json_attempts = 0 WHERE log_id = ?1")
            .bind(log_id)
            .bind(source)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// A log neither source could give: counted, so it is not asked forever.
    pub async fn league_json_failed(&self, log_id: i64) -> Result<()> {
        sqlx::query("UPDATE league_log SET json_attempts = json_attempts + 1 WHERE log_id = ?1").bind(log_id).execute(self.pool()).await?;
        Ok(())
    }

    /// Picked logs with JSON but no raw server log tried yet, newest first.
    pub async fn league_raw_todo(&self, limit: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT log_id FROM league_log WHERE picked = 1 AND json_source IS NOT NULL AND raw_state IS NULL
             ORDER BY (etf2l_match_id IN (SELECT match_id FROM etf2l_season_match WHERE LOWER(REPLACE(COALESCE(round, ''), '-', ' ')) LIKE '%grand final%')) DESC, played_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn put_league_raw(&self, log_id: i64, zip: Option<&[u8]>) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        if let Some(zip) = zip {
            sqlx::query("INSERT INTO league_rawlog (log_id, zip) VALUES (?1, ?2) ON CONFLICT (log_id) DO UPDATE SET zip = excluded.zip")
                .bind(log_id)
                .bind(zip)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE league_log SET raw_state = ?2 WHERE log_id = ?1")
            .bind(log_id)
            .bind(if zip.is_some() { "ok" } else { "missing" })
            .execute(&mut *tx)
            .await?;
        // Rated from its JSON alone so far: rated again, now with the
        // fights and kill situations the server log gives.
        if zip.is_some() {
            sqlx::query("DELETE FROM league_rating WHERE log_id = ?1").bind(log_id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Played officials since `since` whose ETF2L page (who played) is still
    /// unread: the sample's own first, then every other, newest first. Every
    /// roster is read, not only the sample's, so every player who played an
    /// official gets their divisions (the player catalogue).
    pub async fn league_matches_without_roster(&self, limit: i64, since: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT m.match_id FROM etf2l_season_match m
             WHERE m.detail_fetched = 0 AND m.default_win = 0 AND m.time >= ?2
               AND COALESCE(m.r1, 0) + COALESCE(m.r2, 0) > 0
             ORDER BY EXISTS (SELECT 1 FROM league_log l WHERE l.etf2l_match_id = m.match_id AND l.picked = 1) DESC,
                      m.time DESC
             LIMIT ?1",
        )
        .bind(limit)
        .bind(since)
        .fetch_all(self.pool())
        .await?)
    }

    /// The player catalogue so far: officials in the window, those whose
    /// roster has been read, and the players they name.
    pub async fn league_catalogue(&self, since: i64) -> Result<(i64, i64, i64)> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS played, SUM(detail_fetched) AS read,
                    (SELECT COUNT(DISTINCT p.account_id) FROM etf2l_season_player p) AS players
             FROM etf2l_season_match
             WHERE default_win = 0 AND time >= ?1 AND COALESCE(r1, 0) + COALESCE(r2, 0) > 0",
        )
        .bind(since)
        .fetch_one(self.pool())
        .await?;
        Ok((row.get("played"), row.get::<Option<i64>, _>("read").unwrap_or(0), row.get("players")))
    }

    /// Per division, what is picked and what has arrived.
    pub async fn league_progress(&self) -> Result<Vec<TierProgress>> {
        let rows = sqlx::query(&format!(
            "WITH picked AS (
               SELECT l.*, {TIER_SQL} AS tier, m.detail_fetched, m.match_id
               FROM league_log l
               JOIN etf2l_season_match m ON m.match_id = l.etf2l_match_id
               JOIN etf2l_competition c ON c.competition_id = m.competition_id
               WHERE l.picked = 1)
             SELECT p.tier,
                    (SELECT m2.division FROM etf2l_season_match m2 WHERE m2.tier = p.tier AND m2.division IS NOT NULL
                       ORDER BY m2.time DESC LIMIT 1) AS division,
                    COUNT(DISTINCT p.etf2l_match_id) AS matches,
                    COUNT(*) AS logs,
                    SUM(p.json_source = 'logs.tf') AS json_logstf,
                    SUM(p.json_source = 'drops.tf') AS json_dropstf,
                    SUM(p.json_source = 'more.tf') AS json_moretf,
                    SUM(p.raw_state = 'ok') AS raw,
                    SUM(p.raw_state = 'missing') AS raw_missing,
                    COUNT(DISTINCT p.map) AS maps,
                    COUNT(DISTINCT CASE WHEN p.detail_fetched = 1 THEN p.match_id END) AS rosters,
                    MIN(p.played_at) AS oldest, MAX(p.played_at) AS newest
             FROM picked p WHERE p.tier IS NOT NULL
             GROUP BY p.tier ORDER BY p.tier"
        ))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| TierProgress {
                tier: r.get("tier"),
                division: r.get::<Option<String>, _>("division").unwrap_or_default(),
                matches: r.get("matches"),
                logs: r.get("logs"),
                json_logstf: r.get::<Option<i64>, _>("json_logstf").unwrap_or(0),
                json_dropstf: r.get::<Option<i64>, _>("json_dropstf").unwrap_or(0),
                json_moretf: r.get::<Option<i64>, _>("json_moretf").unwrap_or(0),
                raw: r.get::<Option<i64>, _>("raw").unwrap_or(0),
                raw_missing: r.get::<Option<i64>, _>("raw_missing").unwrap_or(0),
                maps: r.get("maps"),
                rosters: r.get("rosters"),
                oldest: r.get("oldest"),
                newest: r.get("newest"),
            })
            .collect())
    }

    /// Bytes the sample takes on disk, roughly: its JSON and raw logs.
    pub async fn league_bytes(&self) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT COALESCE((SELECT SUM(LENGTH(json)) FROM league_log_json), 0)
                  + COALESCE((SELECT SUM(LENGTH(zip)) FROM league_rawlog), 0)",
        )
        .fetch_one(self.pool())
        .await?)
    }

    /// How current the league data is: the newest official held (rated, from
    /// the built-in snapshot or downloaded), how many officials are held, and
    /// how many picked logs are still to download.
    pub async fn league_freshness(&self) -> Result<(Option<i64>, i64, i64)> {
        Ok(sqlx::query_as(
            "SELECT (SELECT MAX(l.played_at) FROM league_log l WHERE l.json_source IS NOT NULL),
                    (SELECT COUNT(DISTINCT l.etf2l_match_id) FROM league_log l WHERE l.json_source IS NOT NULL),
                    (SELECT COUNT(*) FROM league_log l WHERE l.picked = 1 AND l.json_source IS NULL)",
        )
        .fetch_one(self.pool())
        .await?)
    }

    /// Each season held: `(season, played matches, matches with a log, logs,
    /// logs downloaded, logs rated)` for the Older seasons panel.
    pub async fn season_coverage(&self, version: &str) -> Result<Vec<(i64, i64, i64, i64, i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT c.season,
                    COUNT(DISTINCT CASE WHEN m.default_win = 0 AND COALESCE(m.r1, 0) + COALESCE(m.r2, 0) > 0 THEN m.match_id END),
                    COUNT(DISTINCT l.etf2l_match_id),
                    COUNT(DISTINCT l.log_id),
                    COUNT(DISTINCT CASE WHEN l.json_source IS NOT NULL THEN l.log_id END),
                    COUNT(DISTINCT r.log_id)
             FROM etf2l_competition c
             JOIN etf2l_season_match m ON m.competition_id = c.competition_id
             LEFT JOIN league_log l ON l.etf2l_match_id = m.match_id
             LEFT JOIN league_rating r ON r.log_id = l.log_id AND r.model_version = ?1
             WHERE c.stage <> 'Cup'
             GROUP BY c.season ORDER BY c.season",
        )
        .bind(version)
        .fetch_all(self.pool())
        .await?)
    }

    /// Sample logs that can join the rating pool: picked, with their JSON,
    /// and not one of the owner's own logs (their officials are in the
    /// sample too, and a game must not be in the pool twice).
    pub async fn league_rateable(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(&format!(
            "SELECT l.log_id FROM league_log l
             WHERE l.picked = 1 AND l.json_source IS NOT NULL AND l.json_source != 'snapshot'
               AND l.log_id NOT IN ({})
             ORDER BY l.log_id",
            crate::OWNER_COVERED
        ))
        .fetch_all(self.pool())
        .await?)
    }

    /// Sample logs downloaded since the last full rating pass: their JSON is
    /// in and nothing of theirs is rated under `version`. One whose server
    /// log is still to come is rated from its JSON and rated again when the
    /// server log lands. Oldest first.
    /// Every league log with its JSON held, whatever brought it: picked for
    /// the sample, caught up with the league, or covered by one of the
    /// owner's own uploads. Each gets a league rating, so every official is
    /// counted map by map for everyone (season tables, awards).
    pub async fn league_logs_with_json(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT l.log_id FROM league_log l
             WHERE l.json_source IS NOT NULL AND l.json_source != 'snapshot'
               AND EXISTS (SELECT 1 FROM league_log_json j WHERE j.log_id = l.log_id)
             ORDER BY l.log_id",
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn league_unrated(&self, version: &str, limit: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(&format!(
            // Every league log with its JSON, as the full pass rates them
            // (`league_logs_with_json`): the season tables count them all.
            "SELECT l.log_id FROM league_log l
             WHERE l.json_source IS NOT NULL AND l.json_source != 'snapshot'
               AND EXISTS (SELECT 1 FROM league_log_json j WHERE j.log_id = l.log_id)
               AND NOT EXISTS (SELECT 1 FROM league_rating r WHERE r.model_version = ?1 AND r.log_id = l.log_id)
             ORDER BY l.log_id LIMIT ?2{}",
            ""
        ))
        .bind(version)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Sample logs held here (JSON downloaded) with no league rating under
    /// `version`: the owner's own officials, which are rated as the owner's
    /// games, and any not rated yet. What the league snapshot rates as
    /// league games before it ships (Q43).
    pub async fn league_logs_without_rating(&self, version: &str) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT l.log_id FROM league_log l
             WHERE l.json_source IS NOT NULL AND l.json_source != 'snapshot'
               AND EXISTS (SELECT 1 FROM league_log_json j WHERE j.log_id = l.log_id)
               AND NOT EXISTS (SELECT 1 FROM league_rating r WHERE r.model_version = ?1 AND r.log_id = l.log_id)
             ORDER BY l.log_id",
        )
        .bind(version)
        .fetch_all(self.pool())
        .await?)
    }

    /// Each sample log's players and whether their side won, for the
    /// validator's league matchups (Q39): `(log_id, account, won, played_at,
    /// division)`. Ties and logs without a score are left out; so are the
    /// owner's own logs, which the validator reads from their matches.
    pub async fn league_decided(&self) -> Result<Vec<(i64, u32, bool, i64, Option<String>)>> {
        let rows = sqlx::query(&format!(
            "WITH s AS (
                 SELECT l.log_id, l.played_at, l.etf2l_match_id,
                        json_extract(j.json, '$.teams.Red.score') AS red,
                        json_extract(j.json, '$.teams.Blue.score') AS blue
                 FROM league_log l JOIN league_log_json j ON j.log_id = l.log_id
                 WHERE l.picked = 1 AND l.log_id NOT IN ({covered})
             )
             SELECT s.log_id, p.account_id, s.played_at,
                    CASE WHEN p.team = 'Red' THEN s.red > s.blue ELSE s.blue > s.red END AS won,
                    COALESCE(m.division, c.division) AS division
             FROM s
             JOIN league_log_player p ON p.log_id = s.log_id
             LEFT JOIN etf2l_season_match m ON m.match_id = s.etf2l_match_id
             LEFT JOIN etf2l_competition c ON c.competition_id = m.competition_id
             WHERE s.red IS NOT NULL AND s.blue IS NOT NULL AND s.red != s.blue AND p.team IN ('Red', 'Blue')",
            covered = crate::OWNER_COVERED
        ))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("log_id"), r.get::<i64, _>("account_id") as u32, r.get::<i64, _>("won") != 0, r.get("played_at"), r.get("division")))
            .collect())
    }

    /// Per ETF2L match: logs listed for it, with JSON, with a server log.
    pub async fn league_logs_by_match(&self) -> Result<std::collections::HashMap<i64, (i64, i64, i64)>> {
        let rows = sqlx::query(
            "SELECT etf2l_match_id, COUNT(*) AS n, SUM(json_source IS NOT NULL) AS j, SUM(raw_state = 'ok') AS r
             FROM league_log GROUP BY etf2l_match_id",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|r| (r.get("etf2l_match_id"), (r.get("n"), r.get::<Option<i64>, _>("j").unwrap_or(0), r.get::<Option<i64>, _>("r").unwrap_or(0)))).collect())
    }

    /// One sample log's JSON and, if it was downloaded, its raw server log.
    pub async fn league_log_files(&self, log_id: i64) -> Result<(Option<String>, Option<Vec<u8>>)> {
        let json = sqlx::query_scalar("SELECT json FROM league_log_json WHERE log_id = ?1").bind(log_id).fetch_optional(self.pool()).await?;
        let zip = sqlx::query_scalar("SELECT zip FROM league_rawlog WHERE log_id = ?1").bind(log_id).fetch_optional(self.pool()).await?;
        Ok((json, zip))
    }

    /// The newest numbered Highlander season ETF2L has listed (one between
    /// two, kept at 100 and up, is not newer).
    pub async fn newest_etf2l_season(&self) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar("SELECT MAX(season) FROM etf2l_competition WHERE season < 100").fetch_one(self.pool()).await?)
    }
}
