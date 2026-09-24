//! The log index, raw log storage, normalized matches, and the match list.

use crate::context::{context_from_row, MatchContext, CONTEXT_COLUMNS};
use crate::Db;
use anyhow::{Context, Result};
use hl_core::config::keys;
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
    /// Old logs the retention window is holding back, none of them officials.
    pub outside_window: i64,
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
    /// Played between these, unix seconds, inclusive. `None` is open.
    pub from: Option<i64>,
    pub to: Option<i64>,
    /// The owner's main class in the match, e.g. `sniper`.
    pub class: Option<String>,
    /// A map, without its version: `koth_product` matches `koth_product_final`.
    /// A combined log counts when any of its maps match.
    pub map: Option<String>,
    pub limit: i64,
    pub offset: i64,
    /// What to order by: one of [`SORTS`]; anything else falls back to `date`.
    /// Every sort but `date` is about the owner's own line, so matches they
    /// did not play in sink to the bottom.
    pub sort: Option<String>,
    /// Smallest first, rather than the usual biggest (or newest) first.
    pub ascending: bool,
    /// The rating model whose scores the `rating` sort and column use.
    pub model_version: String,
}

/// The orderings the match list offers, as `(key, SQL)`. The owner's line is
/// `p`, so `p.kills IS NULL` puts matches they did not play last either way.
pub const SORTS: [(&str, &str); 8] = [
    ("date", "m.played_at"),
    ("kills", "p.kills"),
    ("deaths", "p.deaths"),
    ("assists", "p.assists"),
    ("dmg", "p.dmg"),
    ("dpm", "CASE WHEN COALESCE(p.time_s, 0) > 0 THEN p.dmg * 60.0 / p.time_s END"),
    ("kd", "CASE WHEN COALESCE(p.deaths, 0) > 0 THEN p.kills * 1.0 / p.deaths ELSE p.kills END"),
    ("rating", "r.score"),
];

fn order_by(f: &MatchFilter) -> String {
    let key = f.sort.as_deref().unwrap_or("date");
    let expr = SORTS.iter().find(|(k, _)| *k == key).map_or("m.played_at", |(_, e)| *e);
    let dir = if f.ascending { "ASC" } else { "DESC" };
    // NULLs last whichever way round it is: an unplayed match has no line.
    format!("ORDER BY ({expr}) IS NULL, ({expr}) {dir}, m.played_at DESC, m.log_id DESC")
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
    /// The maps played, in order, when the round-map pass knows them. More
    /// than one for a log combined from several maps.
    pub maps: Vec<String>,
    /// How many per-round logs this one was combined from.
    pub parts: i64,
    /// The owner's rating in this match on their main class, where it has one.
    pub rating: Option<f64>,
}

/// One of the per-round logs a combined log was built from. Kept out of
/// every aggregate (it would double-count), but still worth seeing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartSummary {
    pub log_id: i64,
    pub title: Option<String>,
    pub map: Option<String>,
    pub played_at: Option<i64>,
    pub duration_s: Option<i64>,
    pub player_count: Option<i64>,
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

/// How far back a sync reaches by default.
///
/// Every log ever played is 754 on this account and takes an hour of
/// logs.tf's patience to download; the recent years are what a rating is
/// actually about. Officials are kept whatever their age — a season from 2019
/// is still the games you care about — and Settings can ask for the lot.
pub const KEEP_YEARS: i64 = 2;

/// Which logs a sync will download: recent, or an official at any age.
///
/// An official is recognised three ways, because no one of them is complete:
/// trends.tf's own tag, the ETF2L match id it carries, and the scheduled time
/// each log was placed against before any of this was downloaded (see
/// `etf2l::match_by_time`). A log with no date at all is kept — not knowing
/// when it was played is not a reason to throw it away.
fn in_window() -> String {
    format!(
        "(i.played_at IS NULL OR i.played_at >= unixepoch() - {KEEP_YEARS} * 365 * 86400
          OR i.etf2l_time_match IS NOT NULL OR i.etf2l_match_id IS NOT NULL OR i.league IS NOT NULL)"
    )
}

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
    /// Logs still to download, newest first.
    ///
    /// Reads the retention setting itself rather than taking it as an
    /// argument: the policy decides what the app does with logs.tf's
    /// patience, and a caller that forgot to pass it would quietly download
    /// twelve years of pugs.
    pub async fn fetch_queue(&self, max_attempts: i64) -> Result<Vec<i64>> {
        let window = if self.all_history().await? { "1 = 1".to_string() } else { in_window() };
        let sql = format!(
            "SELECT i.log_id FROM log_index i
             LEFT JOIN log_raw r         ON r.log_id = i.log_id
             LEFT JOIN log_fetch_error e ON e.log_id = i.log_id
             WHERE r.log_id IS NULL
               AND i.superseded_by IS NULL
               AND ( {EFFECTIVE_FORMAT} = 'highlander'
                  OR ({EFFECTIVE_FORMAT} IS NULL AND COALESCE(i.player_count, 0) >= 16) )
               AND (e.log_id IS NULL OR e.attempts < ?1)
               AND {window}
             ORDER BY i.played_at DESC"
        );
        let rows = sqlx::query(&sql).bind(max_attempts).fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|r| r.get("log_id")).collect())
    }

    /// Whether every log ever played should be downloaded, not just the
    /// recent years and the officials.
    pub async fn all_history(&self) -> Result<bool> {
        Ok(self.get_setting(keys::ALL_HISTORY).await?.as_deref() == Some("1"))
    }

    pub async fn set_all_history(&self, on: bool) -> Result<()> {
        self.set_setting(keys::ALL_HISTORY, if on { "1" } else { "0" }).await
    }

    /// Logs the window is holding back: old, not officials, never downloaded.
    /// What Settings offers to fetch, and what it costs.
    pub async fn outside_window(&self) -> Result<i64> {
        let window = in_window();
        Ok(sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM log_index i
             LEFT JOIN log_raw r ON r.log_id = i.log_id
             WHERE r.log_id IS NULL
               AND i.superseded_by IS NULL
               AND ( {EFFECTIVE_FORMAT} = 'highlander'
                  OR ({EFFECTIVE_FORMAT} IS NULL AND COALESCE(i.player_count, 0) >= 16) )
               AND NOT {window}"
        ))
        .fetch_one(self.pool())
        .await?)
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
        // `fmt` is the first of four: format, kind, from, to.
        // The two queries bind a different number of values, so each says
        // where `class` and `map` sit in its own list.
        let where_sql = |fmt: u8, class_p: u8, map_p: u8| {
            let (kind, from, to) = (fmt + 1, fmt + 2, fmt + 3);
            format!(
                "WHERE i.superseded_by IS NULL
                   AND (?{fmt} IS NULL OR {EFFECTIVE_FORMAT} = ?{fmt})
                   AND (?{kind} IS NULL OR c.kind = ?{kind})
                   AND (?{from} IS NULL OR m.played_at >= ?{from})
                   AND (?{to} IS NULL OR m.played_at <= ?{to})
                   AND (?{class_p} IS NULL OR EXISTS (SELECT 1 FROM match_player mp
                                                      WHERE mp.log_id = m.log_id AND mp.account_id = ?1
                                                        AND mp.main_class = ?{class_p}))
                   AND (?{map_p} IS NULL OR m.map LIKE ?{map_p} || '%'
                        OR EXISTS (SELECT 1 FROM log_segment s
                                   WHERE s.log_id = m.log_id AND s.map LIKE ?{map_p} || '%'))"
            )
        };

        let total: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM match m JOIN log_index i ON i.log_id = m.log_id
             LEFT JOIN match_context c ON c.log_id = m.log_id {}",
            where_sql(2, 6, 7)
        ))
        .bind(me.map(i64::from))
        .bind(&filter.format)
        .bind(&filter.kind)
        .bind(filter.from)
        .bind(filter.to)
        .bind(&filter.class)
        .bind(&filter.map)
        .fetch_one(self.pool())
        .await?;

        let where_sql = where_sql(2, 9, 10);
        let order = order_by(filter);
        let rows = sqlx::query(&format!(
            "SELECT m.log_id, m.played_at, m.map, m.title, m.duration_s,
                    {EFFECTIVE_FORMAT} AS format, i.league, i.etf2l_match_id, i.demos_tf_id,
                    m.red_score, m.blue_score,
                    EXISTS (SELECT 1 FROM demo_link dl WHERE dl.log_id = m.log_id) AS has_demo,
                    (SELECT COUNT(*) FROM log_index pi WHERE pi.superseded_by = m.log_id) AS part_count,
                    (SELECT group_concat(map, '|') FROM
                        (SELECT map FROM log_segment s WHERE s.log_id = m.log_id AND map IS NOT NULL ORDER BY seq)
                    ) AS segment_maps,
                    p.team, p.main_class, p.kills, p.deaths, p.assists, p.dmg, p.time_s,
                    r.score AS my_rating,
                    {CONTEXT_COLUMNS}
             FROM match m
             JOIN log_index i ON i.log_id = m.log_id
             LEFT JOIN match_player p ON p.log_id = m.log_id AND p.account_id = ?1
             LEFT JOIN rating r ON r.log_id = m.log_id AND r.account_id = ?1
                AND r.class = p.main_class AND r.model_version = ?8
             LEFT JOIN match_context c ON c.log_id = m.log_id
             LEFT JOIN etf2l_match e ON e.match_id = c.etf2l_match_id
             {where_sql}
             {order}
             LIMIT ?6 OFFSET ?7"
        ))
        .bind(me.map(i64::from))
        .bind(&filter.format)
        .bind(&filter.kind)
        .bind(filter.from)
        .bind(filter.to)
        .bind(filter.limit)
        .bind(filter.offset)
        .bind(&filter.model_version)
        .bind(&filter.class)
        .bind(&filter.map)
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
                    maps: r
                        .get::<Option<String>, _>("segment_maps")
                        .map(|s| s.split('|').map(str::to_string).collect())
                        .unwrap_or_default(),
                    parts: r.get("part_count"),
                    rating: r.get("my_rating"),
                    me,
                }
            })
            .collect();

        Ok(MatchPage { total, items })
    }

    /// The owner's own matches by class and by map, most played first: what
    /// the match list's filters offer. Maps lose their version, so
    /// `koth_product_final` and `koth_product_rc8` are one entry.
    pub async fn played_classes_and_maps(&self, me: u32) -> Result<(Vec<(String, i64)>, Vec<(String, i64)>)> {
        let classes: Vec<(String, i64)> = sqlx::query_as(
            "SELECT mp.main_class, COUNT(*) FROM match_player mp
             JOIN log_index i ON i.log_id = mp.log_id
             WHERE mp.account_id = ?1 AND mp.main_class IS NOT NULL AND i.superseded_by IS NULL
             GROUP BY mp.main_class ORDER BY COUNT(*) DESC",
        )
        .bind(me)
        .fetch_all(self.pool())
        .await?;

        // A combined log counts once per map it holds, which is what a player
        // means by "my upward games".
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT DISTINCT mp.log_id, COALESCE(s.map, m.map) AS map
             FROM match_player mp
             JOIN log_index i ON i.log_id = mp.log_id
             JOIN match m ON m.log_id = mp.log_id
             LEFT JOIN log_segment s ON s.log_id = mp.log_id
             WHERE mp.account_id = ?1 AND i.superseded_by IS NULL AND COALESCE(s.map, m.map) IS NOT NULL",
        )
        .bind(me)
        .fetch_all(self.pool())
        .await?;
        let mut by_map: HashMap<String, i64> = HashMap::new();
        for (_, map) in rows {
            *by_map.entry(hl_core::map_base(&map)).or_default() += 1;
        }
        let mut maps: Vec<(String, i64)> = by_map.into_iter().collect();
        maps.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Ok((classes, maps))
    }

    /// Every player's name in one match, by account id.
    /// Every player's team in one match, by account id: `Red` or `Blue`.
    pub async fn player_teams(&self, log_id: i64) -> Result<HashMap<u32, String>> {
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT account_id, team FROM match_player WHERE log_id = ?1")
                .bind(log_id)
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(id, team)| (id as u32, team)).collect())
    }

    pub async fn player_names(&self, log_id: i64) -> Result<HashMap<u32, String>> {
        let rows: Vec<(i64, Option<String>)> =
            sqlx::query_as("SELECT account_id, name FROM match_player WHERE log_id = ?1")
                .bind(log_id)
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().filter_map(|(id, name)| Some((id as u32, name?))).collect())
    }

    /// The per-round logs a combined log replaced, oldest first.
    pub async fn parts_of(&self, log_id: i64) -> Result<Vec<PartSummary>> {
        let rows = sqlx::query(
            "SELECT log_id, title, map, played_at, duration_s, player_count
             FROM log_index WHERE superseded_by = ?1 ORDER BY played_at, log_id",
        )
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| PartSummary {
                log_id: r.get("log_id"),
                title: r.get("title"),
                map: r.get("map"),
                played_at: r.get("played_at"),
                duration_s: r.get("duration_s"),
                player_count: r.get("player_count"),
            })
            .collect())
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
            outside_window: if self.all_history().await? { 0 } else { self.outside_window().await? },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(sort: Option<&str>, ascending: bool) -> MatchFilter {
        MatchFilter {
            format: None,
            kind: None,
            from: None,
            to: None,
            limit: 50,
            offset: 0,
            sort: sort.map(str::to_string),
            ascending,
            model_version: "v5".to_string(),
            class: None,
            map: None,
        }
    }

    #[test]
    fn every_sort_is_a_known_column_and_anything_else_falls_back_to_the_date() {
        assert!(order_by(&filter(Some("dpm"), false)).contains("p.dmg * 60.0 / p.time_s"));
        assert!(order_by(&filter(Some("rating"), true)).contains("r.score) ASC"));
        // A sort from outside the list can never reach the query.
        let sneaky = order_by(&filter(Some("1; DROP TABLE match"), false));
        assert!(!sneaky.contains("DROP"), "{sneaky}");
        assert_eq!(sneaky, order_by(&filter(None, false)));
    }

    #[test]
    fn matches_the_owner_did_not_play_sort_last_either_way() {
        for ascending in [false, true] {
            assert!(order_by(&filter(Some("kills"), ascending)).starts_with("ORDER BY (p.kills) IS NULL,"));
        }
    }

    /// A row in the index and nothing else: enough to ask what the sync would
    /// download, which is decided before anything is fetched.
    async fn index(db: &Db, log_id: i64, years_ago: f64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        db.upsert_logstf_rows(&[LogsTfIndexRow {
            log_id,
            title: Some("a game"),
            map: Some("pl_upward_f12"),
            played_at: Some(now - (years_ago * 365.0 * 86400.0) as i64),
            player_count: Some(18),
            raw_json: "{}",
        }])
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn old_pugs_are_left_alone_but_old_officials_are_not() {
        let db = Db::connect_in_memory().await.unwrap();
        index(&db, 1, 0.5).await; // recent
        index(&db, 2, 5.0).await; // old, ordinary
        index(&db, 3, 5.0).await; // old, tagged by trends.tf
        index(&db, 4, 5.0).await; // old, placed against an ETF2L match by time
        sqlx::query("UPDATE log_index SET league = 'etf2l' WHERE log_id = 3")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE log_index SET etf2l_time_match = 92883 WHERE log_id = 4")
            .execute(db.pool())
            .await
            .unwrap();

        let mut queue = db.fetch_queue(3).await.unwrap();
        queue.sort_unstable();
        assert_eq!(queue, vec![1, 3, 4], "the old pug is the only one skipped");
        assert_eq!(db.outside_window().await.unwrap(), 1);

        // Asked for the lot, nothing is held back.
        db.set_all_history(true).await.unwrap();
        let mut queue = db.fetch_queue(3).await.unwrap();
        queue.sort_unstable();
        assert_eq!(queue, vec![1, 2, 3, 4]);
        assert_eq!(db.index_stats().await.unwrap().outside_window, 0, "and nothing is on offer");
    }
}
