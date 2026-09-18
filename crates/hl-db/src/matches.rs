//! The log index, raw log storage, normalized matches, and the match list.

use crate::context::{context_from_row, MatchContext, CONTEXT_COLUMNS};
use crate::Db;
use anyhow::{Context, Result};
use hl_core::matchdata::{Format, NormalizedLog, Team};
use serde::Serialize;
use sqlx::Row;
use std::collections::HashMap;

/// A trends.tf index row, flattened for storage.
pub struct TrendsIndexRow<'a> {
    pub log_id: i64,
    pub title: Option<&'a str>,
    pub map: Option<&'a str>,
    pub played_at: Option<i64>,
    pub duration_s: Option<i64>,
    pub format: Option<&'a str>,
    pub league: Option<&'a str>,
    pub etf2l_match_id: Option<i64>,
    pub demos_tf_id: Option<i64>,
    pub duplicate_of: Option<&'a [i64]>,
    pub raw_json: &'a str,
}

/// A logs.tf search row, flattened for storage.
pub struct LogsTfIndexRow<'a> {
    pub log_id: i64,
    pub title: Option<&'a str>,
    pub map: Option<&'a str>,
    pub played_at: Option<i64>,
    pub player_count: Option<i64>,
    pub raw_json: &'a str,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub indexed: i64,
    pub superseded: i64,
    pub highlander: i64,
    pub sixes: i64,
    pub other: i64,
    pub unclassified: i64,
    pub officials: i64,
    pub fetched: i64,
    pub normalized: i64,
    pub pending: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, Default)]
pub struct IndexInfo {
    pub format: Option<String>,
    pub league: Option<String>,
    pub etf2l_match_id: Option<i64>,
    pub demos_tf_id: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct MatchFilter {
    /// `None` means every format.
    pub format: Option<String>,
    /// `official`, `scrim` or `pug`; `None` means every kind.
    pub kind: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchSummary {
    pub log_id: i64,
    pub played_at: Option<i64>,
    pub map: Option<String>,
    pub title: Option<String>,
    pub duration_s: Option<i64>,
    pub format: Option<String>,
    pub league: Option<String>,
    pub etf2l_match_id: Option<i64>,
    pub demos_tf_id: Option<i64>,
    pub red_score: Option<i64>,
    pub blue_score: Option<i64>,
    /// A demo on this machine is linked to the match.
    pub has_demo: bool,
    /// The owner's line, when they appear in the log.
    pub me: Option<MyLine>,
    /// Official, scrim or pug; `None` for matches outside the context pass
    /// (other formats, or ones the owner did not play).
    pub context: Option<MatchContext>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MyLine {
    pub team: String,
    pub main_class: Option<String>,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub dmg: i64,
    pub time_s: i64,
    /// `W`, `L` or `T` from the owner's side.
    pub result: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchPage {
    pub total: i64,
    pub items: Vec<MatchSummary>,
}

/// SQL for the format a log is treated as. A manual override always wins.
const EFFECTIVE_FORMAT: &str = "COALESCE(i.format_override, i.format)";

impl Db {
    // ---- index -----------------------------------------------------------

    pub async fn upsert_trends_rows(&self, rows: &[TrendsIndexRow<'_>]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        for r in rows {
            let dup = r.duplicate_of.filter(|d| !d.is_empty()).map(|d| {
                serde_json::to_string(d).expect("a slice of i64 always serializes")
            });
            sqlx::query(
                "INSERT INTO log_index
                    (log_id, title, map, played_at, duration_s, trends_json,
                     format, league, etf2l_match_id, demos_tf_id, duplicate_of,
                     classified_by, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                         CASE WHEN ?7 IS NULL THEN NULL ELSE 'trends' END, datetime('now'))
                 ON CONFLICT(log_id) DO UPDATE SET
                    title          = COALESCE(excluded.title, NULLIF(log_index.title, '')),
                    map            = COALESCE(excluded.map, NULLIF(log_index.map, '')),
                    played_at      = COALESCE(excluded.played_at, log_index.played_at),
                    duration_s     = COALESCE(excluded.duration_s, log_index.duration_s),
                    trends_json    = excluded.trends_json,
                    format         = COALESCE(excluded.format, log_index.format),
                    league         = excluded.league,
                    etf2l_match_id = excluded.etf2l_match_id,
                    demos_tf_id    = excluded.demos_tf_id,
                    duplicate_of   = excluded.duplicate_of,
                    classified_by  = CASE WHEN excluded.format IS NOT NULL THEN 'trends'
                                          ELSE log_index.classified_by END,
                    updated_at     = excluded.updated_at",
            )
            .bind(r.log_id)
            .bind(r.title)
            .bind(r.map)
            .bind(r.played_at)
            .bind(r.duration_s)
            .bind(r.raw_json)
            .bind(r.format)
            .bind(r.league)
            .bind(r.etf2l_match_id)
            .bind(r.demos_tf_id)
            .bind(dup)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("upserting trends index row {}", r.log_id))?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_logstf_rows(&self, rows: &[LogsTfIndexRow<'_>]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        for r in rows {
            // trends.tf is the better source for everything but player count,
            // so logs.tf only fills gaps.
            sqlx::query(
                "INSERT INTO log_index (log_id, title, map, played_at, player_count, logstf_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(log_id) DO UPDATE SET
                    title        = COALESCE(NULLIF(log_index.title, ''), excluded.title),
                    map          = COALESCE(NULLIF(log_index.map, ''), excluded.map),
                    played_at    = COALESCE(log_index.played_at, excluded.played_at),
                    player_count = excluded.player_count,
                    logstf_json  = excluded.logstf_json",
            )
            .bind(r.log_id)
            .bind(r.title)
            .bind(r.map)
            .bind(r.played_at)
            .bind(r.player_count)
            .bind(r.raw_json)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("upserting logs.tf index row {}", r.log_id))?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Everything dedupe needs: id, duration and the parts it claims.
    pub async fn dedupe_inputs(&self) -> Result<Vec<(i64, i64, Vec<i64>)>> {
        let rows = sqlx::query("SELECT log_id, duration_s, duplicate_of FROM log_index")
            .fetch_all(self.pool())
            .await?;
        rows.into_iter()
            .map(|r| {
                let dup: Option<String> = r.get("duplicate_of");
                let parts = match dup {
                    Some(s) => serde_json::from_str(&s).context("stored duplicate_of is not a JSON array")?,
                    None => Vec::new(),
                };
                Ok((r.get("log_id"), r.get::<Option<i64>, _>("duration_s").unwrap_or(0), parts))
            })
            .collect()
    }

    /// Replace every supersession in one transaction. Recomputed from scratch
    /// each time, so it can never drift from the index.
    pub async fn apply_supersessions(&self, map: &HashMap<i64, i64>) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE log_index SET superseded_by = NULL")
            .execute(&mut *tx)
            .await?;
        for (log_id, keep) in map {
            sqlx::query("UPDATE log_index SET superseded_by = ?2 WHERE log_id = ?1")
                .bind(log_id)
                .bind(keep)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Logs worth fetching in full: kept (not superseded), Highlander or not
    /// yet classified but plausibly so, not already stored, and not failing
    /// permanently. Newest first, so a sync surfaces recent matches soonest.
    pub async fn fetch_queue(&self, max_attempts: i64) -> Result<Vec<i64>> {
        let sql = format!(
            "SELECT i.log_id FROM log_index i
             LEFT JOIN log_raw r         ON r.log_id = i.log_id
             LEFT JOIN log_fetch_error e ON e.log_id = i.log_id
             WHERE r.log_id IS NULL
               AND i.superseded_by IS NULL
               AND ( {EFFECTIVE_FORMAT} = 'highlander'
                  OR ({EFFECTIVE_FORMAT} IS NULL AND COALESCE(i.player_count, 0) >= 16) )
               AND (e.log_id IS NULL OR e.attempts < ?1)
             ORDER BY i.played_at DESC"
        );
        let rows = sqlx::query(&sql).bind(max_attempts).fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|r| r.get("log_id")).collect())
    }

    /// Highest `updated` timestamp seen from trends.tf, for incremental sync.
    pub async fn trends_cursor(&self) -> Result<Option<i64>> {
        let v: Option<String> =
            sqlx::query_scalar("SELECT cursor FROM sync_state WHERE source = 'trends'")
                .fetch_optional(self.pool())
                .await?
                .flatten();
        Ok(v.and_then(|s| s.parse().ok()))
    }

    // ---- raw logs --------------------------------------------------------

    pub async fn store_raw_log(&self, log_id: i64, json: &str) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO log_raw (log_id, json) VALUES (?1, ?2)
             ON CONFLICT(log_id) DO UPDATE SET json = excluded.json, fetched_at = datetime('now')",
        )
        .bind(log_id)
        .bind(json)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM log_fetch_error WHERE log_id = ?1")
            .bind(log_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn record_fetch_error(&self, log_id: i64, error: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO log_fetch_error (log_id, error) VALUES (?1, ?2)
             ON CONFLICT(log_id) DO UPDATE SET
                attempts = log_fetch_error.attempts + 1,
                last_attempt_at = datetime('now'),
                error = excluded.error",
        )
        .bind(log_id)
        .bind(error)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn raw_log_ids(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT log_id FROM log_raw ORDER BY log_id")
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn raw_log(&self, log_id: i64) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT json FROM log_raw WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?)
    }

    /// Stored trends.tf rows, for re-deriving the index without the network.
    pub async fn trends_raw_rows(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT trends_json FROM log_index WHERE trends_json IS NOT NULL")
            .fetch_all(self.pool())
            .await?)
    }

    /// The format a log currently resolves to, if any.
    pub async fn effective_format(&self, log_id: i64) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(&format!(
            "SELECT {EFFECTIVE_FORMAT} FROM log_index i WHERE i.log_id = ?1"
        ))
        .bind(log_id)
        .fetch_optional(self.pool())
        .await?
        .flatten())
    }

    /// Record our own classification — only where trends.tf gave none.
    pub async fn set_heuristic_format(&self, log_id: i64, format: Format) -> Result<()> {
        sqlx::query(
            "UPDATE log_index SET format = ?2, classified_by = 'heuristic'
             WHERE log_id = ?1 AND (format IS NULL OR classified_by = 'heuristic')",
        )
        .bind(log_id)
        .bind(format.as_str())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    // ---- normalized matches ------------------------------------------------

    /// Replace everything normalized for one log. Idempotent: running it twice
    /// leaves the same rows, which is what makes `reprocess` safe.
    pub async fn write_match(&self, log: &NormalizedLog) -> Result<()> {
        let id = log.log_id;
        let mut tx = self.pool().begin().await?;

        // Explicit child deletes rather than relying on ON DELETE CASCADE, so
        // correctness does not hinge on the foreign_keys pragma.
        for table in [
            "match_event",
            "match_round",
            "match_class_vs",
            "match_player_class",
            "match_player",
            "match",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE log_id = ?1"))
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }

        let f = &log.flags;
        sqlx::query(
            "INSERT INTO match (log_id, title, map, played_at, duration_s, red_score, blue_score,
                round_count, has_real_damage, has_accuracy, has_hs, has_hs_hit, has_bs, has_cp,
                has_dt, has_as, has_hr)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        )
        .bind(id)
        .bind(&log.title)
        .bind(&log.map)
        .bind(log.played_at)
        .bind(log.duration_s)
        .bind(log.red_score)
        .bind(log.blue_score)
        .bind(log.rounds.len() as i64)
        .bind(f.real_damage)
        .bind(f.accuracy)
        .bind(f.hs)
        .bind(f.hs_hit)
        .bind(f.bs)
        .bind(f.cp)
        .bind(f.dt)
        .bind(f.airshots)
        .bind(f.hr)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("inserting match {id}"))?;

        for p in &log.players {
            let account = p.id.account_id() as i64;
            let s = &p.stats;
            sqlx::query(
                "INSERT INTO match_player (log_id, account_id, name, team, main_class, time_s,
                    kills, deaths, assists, suicides, dmg, dmg_real, dt, dt_real, hr, heal, ubers,
                    drops, headshots, headshots_hit, backstabs, medkits, medkits_hp, sentries,
                    cpc, ic, lks, airshots)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                    ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)",
            )
            .bind(id)
            .bind(account)
            .bind(&p.name)
            .bind(p.team.as_str())
            .bind(p.main_class().map(|c| c.as_str()))
            .bind(p.total_time())
            .bind(s.kills)
            .bind(s.deaths)
            .bind(s.assists)
            .bind(s.suicides)
            .bind(s.dmg)
            .bind(s.dmg_real)
            .bind(s.dt)
            .bind(s.dt_real)
            .bind(s.hr)
            .bind(s.heal)
            .bind(s.ubers)
            .bind(s.drops)
            .bind(s.headshots)
            .bind(s.headshots_hit)
            .bind(s.backstabs)
            .bind(s.medkits)
            .bind(s.medkits_hp)
            .bind(s.sentries)
            .bind(s.cpc)
            .bind(s.ic)
            .bind(s.lks)
            .bind(s.airshots)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("inserting player {} in match {id}", p.id))?;

            for c in &p.classes {
                sqlx::query(
                    "INSERT INTO match_player_class
                        (log_id, account_id, class, time_s, kills, assists, deaths, dmg)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .bind(id)
                .bind(account)
                .bind(c.class.as_str())
                .bind(c.time_s)
                .bind(c.kills)
                .bind(c.assists)
                .bind(c.deaths)
                .bind(c.dmg)
                .execute(&mut *tx)
                .await?;
            }

            for v in &p.vs {
                sqlx::query(
                    "INSERT INTO match_class_vs
                        (log_id, account_id, other_class, kills, deaths, assists)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .bind(id)
                .bind(account)
                .bind(v.other_class.as_str())
                .bind(v.kills)
                .bind(v.deaths)
                .bind(v.assists)
                .execute(&mut *tx)
                .await?;
            }
        }

        for r in &log.rounds {
            sqlx::query(
                "INSERT INTO match_round (log_id, round_num, start_time, length_s, winner,
                    firstcap, red_kills, blue_kills, red_dmg, blue_dmg, red_ubers, blue_ubers,
                    colours_swapped)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )
            .bind(id)
            .bind(r.round_num)
            .bind(r.start_time)
            .bind(r.length_s)
            .bind(r.winner.map(Team::as_str))
            .bind(r.firstcap.map(Team::as_str))
            .bind(r.red.kills)
            .bind(r.blue.kills)
            .bind(r.red.dmg)
            .bind(r.blue.dmg)
            .bind(r.red.ubers)
            .bind(r.blue.ubers)
            .bind(r.colours_swapped)
            .execute(&mut *tx)
            .await?;

            for (seq, e) in r.events.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO match_event
                        (log_id, round_num, seq, at_s, kind, team, player, killer, medigun, point)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )
                .bind(id)
                .bind(r.round_num)
                .bind(seq as i64)
                .bind(e.at_s)
                .bind(&e.kind)
                .bind(e.team.map(Team::as_str))
                .bind(e.player.map(|s| s.account_id() as i64))
                .bind(e.killer.map(|s| s.account_id() as i64))
                .bind(&e.medigun)
                .bind(e.point)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await.with_context(|| format!("committing match {id}"))?;
        Ok(())
    }

    // ---- reads -------------------------------------------------------------

    pub async fn list_matches(&self, me: Option<u32>, filter: &MatchFilter) -> Result<MatchPage> {
        // Placeholder numbers are parameters so the count query and the page
        // query can share one WHERE clause with different binding layouts.
        let where_sql = |fmt: u8, kind: u8| {
            format!(
                "WHERE i.superseded_by IS NULL
                   AND (?{fmt} IS NULL OR {EFFECTIVE_FORMAT} = ?{fmt})
                   AND (?{kind} IS NULL OR c.kind = ?{kind})"
            )
        };

        let total: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM match m JOIN log_index i ON i.log_id = m.log_id
             LEFT JOIN match_context c ON c.log_id = m.log_id {}",
            where_sql(1, 2)
        ))
        .bind(&filter.format)
        .bind(&filter.kind)
        .fetch_one(self.pool())
        .await?;

        let where_sql = where_sql(2, 3);
        let rows = sqlx::query(&format!(
            "SELECT m.log_id, m.played_at, m.map, m.title, m.duration_s,
                    {EFFECTIVE_FORMAT} AS format, i.league, i.etf2l_match_id, i.demos_tf_id,
                    m.red_score, m.blue_score,
                    EXISTS (SELECT 1 FROM demo_link dl WHERE dl.log_id = m.log_id) AS has_demo,
                    p.team, p.main_class, p.kills, p.deaths, p.assists, p.dmg, p.time_s,
                    {CONTEXT_COLUMNS}
             FROM match m
             JOIN log_index i ON i.log_id = m.log_id
             LEFT JOIN match_player p ON p.log_id = m.log_id AND p.account_id = ?1
             LEFT JOIN match_context c ON c.log_id = m.log_id
             LEFT JOIN etf2l_match e ON e.match_id = c.etf2l_match_id
             {where_sql}
             ORDER BY m.played_at DESC, m.log_id DESC
             LIMIT ?4 OFFSET ?5"
        ))
        .bind(me.map(i64::from))
        .bind(&filter.format)
        .bind(&filter.kind)
        .bind(filter.limit)
        .bind(filter.offset)
        .fetch_all(self.pool())
        .await?;

        let items = rows
            .into_iter()
            .map(|r| {
                let red: Option<i64> = r.get("red_score");
                let blue: Option<i64> = r.get("blue_score");
                let team: Option<String> = r.get("team");
                let me = team.map(|team| {
                    let (mine, theirs) = if team == "Red" { (red, blue) } else { (blue, red) };
                    let result = match (mine, theirs) {
                        (Some(a), Some(b)) if a > b => "W",
                        (Some(a), Some(b)) if a < b => "L",
                        _ => "T",
                    };
                    MyLine {
                        team,
                        main_class: r.get("main_class"),
                        kills: r.get("kills"),
                        deaths: r.get("deaths"),
                        assists: r.get("assists"),
                        dmg: r.get("dmg"),
                        time_s: r.get("time_s"),
                        result: result.to_string(),
                    }
                });
                MatchSummary {
                    log_id: r.get("log_id"),
                    played_at: r.get("played_at"),
                    map: r.get("map"),
                    title: r.get("title"),
                    duration_s: r.get("duration_s"),
                    format: r.get("format"),
                    league: r.get("league"),
                    etf2l_match_id: r.get("etf2l_match_id"),
                    demos_tf_id: r.get("demos_tf_id"),
                    red_score: red,
                    blue_score: blue,
                    has_demo: r.get::<i64, _>("has_demo") != 0,
                    context: context_from_row(&r),
                    me,
                }
            })
            .collect();

        Ok(MatchPage { total, items })
    }

    /// The index context for one log: what trends.tf knows about it.
    pub async fn index_info(&self, log_id: i64) -> Result<Option<IndexInfo>> {
        let row = sqlx::query(&format!(
            "SELECT {EFFECTIVE_FORMAT} AS format, i.league, i.etf2l_match_id, i.demos_tf_id
             FROM log_index i WHERE i.log_id = ?1"
        ))
        .bind(log_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|r| IndexInfo {
            format: r.get("format"),
            league: r.get("league"),
            etf2l_match_id: r.get("etf2l_match_id"),
            demos_tf_id: r.get("demos_tf_id"),
        }))
    }

    pub async fn index_stats(&self) -> Result<IndexStats> {
        let q = |sql: String| async move {
            sqlx::query_scalar::<_, i64>(&sql).fetch_one(self.pool()).await
        };
        let kept = "i.superseded_by IS NULL";
        Ok(IndexStats {
            indexed: q("SELECT COUNT(*) FROM log_index".into()).await?,
            superseded: q("SELECT COUNT(*) FROM log_index WHERE superseded_by IS NOT NULL".into()).await?,
            highlander: q(format!("SELECT COUNT(*) FROM log_index i WHERE {kept} AND {EFFECTIVE_FORMAT} = 'highlander'")).await?,
            sixes: q(format!("SELECT COUNT(*) FROM log_index i WHERE {kept} AND {EFFECTIVE_FORMAT} = 'sixes'")).await?,
            other: q(format!("SELECT COUNT(*) FROM log_index i WHERE {kept} AND {EFFECTIVE_FORMAT} NOT IN ('highlander','sixes')")).await?,
            unclassified: q(format!("SELECT COUNT(*) FROM log_index i WHERE {kept} AND {EFFECTIVE_FORMAT} IS NULL")).await?,
            // Officials the owner played, counted once per log like the match list.
            officials: q("SELECT COUNT(*) FROM match_context WHERE kind = 'official'".into()).await?,
            fetched: q("SELECT COUNT(*) FROM log_raw".into()).await?,
            normalized: q("SELECT COUNT(*) FROM match".into()).await?,
            pending: self.fetch_queue(3).await?.len() as i64,
            failed: q("SELECT COUNT(*) FROM log_fetch_error WHERE attempts >= 3".into()).await?,
        })
    }
}
