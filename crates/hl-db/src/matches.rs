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

/// How many tries a log that keeps failing gets in all (Q77): the first few
/// on consecutive syncs, the rest a week apart, then it is given up.
pub const MAX_FETCH_TRIES: i64 = 6;

/// A log that would not download, with whatever the index knows about it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedLog {
    pub log_id: i64,
    pub attempts: i64,
    pub last_attempt_at: String,
    pub error: String,
    pub title: Option<String>,
    pub map: Option<String>,
    pub played_at: Option<i64>,
    /// drops.tf and logs.tf both said it does not exist: never asked again.
    pub gone: bool,
    /// Gone, or out of tries: listed apart, and left out of "try them all".
    pub given_up: bool,
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
    /// Deleted on logs.tf, or out of tries: no longer asked for (Q77).
    #[serde(default)]
    pub given_up: i64,
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
    /// Only matches with a SourceTV demo on this machine (Q56, Emiel):
    /// downloaded or found in a demo folder, and not deleted.
    pub stv_only: bool,
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
    /// A value per log id, as a JSON object, to order by instead of a
    /// column: the owner's DPM or accuracy as a percentile of their class
    /// (on the map where it has a scale), worked out outside SQL.
    pub order_values: Option<String>,
}

/// The orderings the match list offers, as `(key, SQL)`. The owner's line is
/// `p`, so `p.kills IS NULL` puts matches they did not play last either way.
///
/// Every stat the match list can show has one (Flashy: "i want all the
/// stats possible"): the owner's totals, per minute, per 30 minutes and the
/// ratios.
pub const SORTS: [(&str, &str); 35] = [
    ("date", "m.played_at"),
    ("kills", "p.kills"),
    ("deaths", "p.deaths"),
    ("assists", "p.assists"),
    ("dmg", "p.dmg"),
    ("dpm", "CASE WHEN COALESCE(p.time_s, 0) > 0 THEN p.dmg * 60.0 / p.time_s END"),
    ("kd", "CASE WHEN COALESCE(p.deaths, 0) > 0 THEN p.kills * 1.0 / p.deaths ELSE p.kills END"),
    ("kad", "CASE WHEN COALESCE(p.deaths, 0) > 0 THEN (p.kills + p.assists) * 1.0 / p.deaths ELSE p.kills + p.assists END"),
    ("rating", "r.score"),
    ("time", "p.time_s"),
    ("k30", "CASE WHEN COALESCE(p.time_s, 0) > 0 THEN p.kills * 1800.0 / p.time_s END"),
    ("a30", "CASE WHEN COALESCE(p.time_s, 0) > 0 THEN p.assists * 1800.0 / p.time_s END"),
    ("d30", "CASE WHEN COALESCE(p.time_s, 0) > 0 THEN p.deaths * 1800.0 / p.time_s END"),
    ("dt", "p.dt"),
    ("dtpm", "CASE WHEN COALESCE(p.time_s, 0) > 0 THEN p.dt * 60.0 / p.time_s END"),
    ("hr", "p.hr"),
    ("hrpm", "CASE WHEN COALESCE(p.time_s, 0) > 0 THEN p.hr * 60.0 / p.time_s END"),
    ("heal", "p.heal"),
    ("healpm", "CASE WHEN COALESCE(p.time_s, 0) > 0 THEN p.heal * 60.0 / p.time_s END"),
    ("ubers", "p.ubers"),
    ("drops", "p.drops"),
    ("headshots", "p.headshots"),
    ("headshots_hit", "p.headshots_hit"),
    ("backstabs", "p.backstabs"),
    ("airshots", "p.airshots"),
    ("medkits", "p.medkits"),
    ("medkits_hp", "p.medkits_hp"),
    ("sentries", "p.sentries"),
    ("cpc", "p.cpc"),
    ("ic", "p.ic"),
    ("lks", "p.lks"),
    ("suicides", "p.suicides"),
    ("dmgk", "CASE WHEN COALESCE(p.kills, 0) > 0 THEN p.dmg * 1.0 / p.kills END"),
    ("sentries", "(SELECT SUM(ls.sentry_kills) FROM log_stat ls WHERE ls.log_id = m.log_id AND ls.account_id = p.account_id)"),
    ("acc", "(SELECT CAST(SUM(w.hits) AS REAL) / SUM(w.shots) FROM log_weapon w WHERE w.log_id = m.log_id AND w.account_id = p.account_id AND w.class = p.main_class HAVING SUM(w.shots) >= 30)"),
];

/// Q61 (Clark): `killed:<class>` sorts by the owner's kills on that class,
/// `diedto:<class>` by their deaths to it, counted from the raw log's kills
/// as logs.tf counts them (inside a round, no feigns). `None` for any other
/// key, or a class that is not one; a match with no raw log counts as NULL
/// and sinks, rather than as a misleading 0.
///
/// Returned as `(joins, expression)`. The count is one grouped pass over
/// the owner's kills, joined in once (Q70): a correlated count per match
/// made SQLite walk all of the owner's kills for every match, 21 s on 757.
fn class_count_sql(f: &MatchFilter) -> Option<(String, &'static str)> {
    let (how, class) = f.sort.as_deref()?.split_once(':')?;
    let class = hl_core::TfClass::parse(class).ok()?.as_str();
    let (mine, theirs) = match how {
        "killed" => ("k.killer", "k.victim_class"),
        "diedto" => ("k.victim", "k.killer_class"),
        _ => return None,
    };
    let joins = format!(
        "LEFT JOIN (SELECT k.log_id, COUNT(*) AS n FROM kill_event k
                    WHERE {mine} = ?1 AND {theirs} = '{class}' AND k.live = 1
                      AND COALESCE(k.custom, '') <> 'feign_death' AND k.killer <> k.victim
                    GROUP BY k.log_id) cc ON cc.log_id = m.log_id
         LEFT JOIN rawlog rl ON rl.log_id = m.log_id"
    );
    Some((joins, "CASE WHEN p.account_id IS NULL OR rl.log_id IS NULL THEN NULL ELSE COALESCE(cc.n, 0) END"))
}

fn order_by(f: &MatchFilter) -> String {
    let key = f.sort.as_deref().unwrap_or("date");
    let by_class = class_count_sql(f);
    let by_value = "json_extract(?11, '$.\"' || m.log_id || '\"')";
    let expr = if f.order_values.is_some() {
        by_value
    } else {
        by_class.as_ref().map(|(_, e)| *e).unwrap_or_else(|| SORTS.iter().find(|(k, _)| *k == key).map_or("m.played_at", |(_, e)| *e))
    };
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
    /// Under a `killed:` or `diedto:` sort (Q61): the owner's kills on that
    /// class, or deaths to it. `None` otherwise, or with no raw log.
    pub class_count: Option<i64>,
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
    /// The map as the stat scales name it, for the DPM and accuracy colours.
    pub map_key: Option<String>,
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
    pub dt: i64,
    pub hr: i64,
    pub heal: i64,
    pub ubers: i64,
    pub drops: i64,
    pub headshots: i64,
    pub headshots_hit: i64,
    pub backstabs: i64,
    pub airshots: i64,
    pub medkits: i64,
    pub medkits_hp: i64,
    pub sentries: i64,
    pub cpc: i64,
    pub ic: i64,
    pub lks: i64,
    pub suicides: i64,
    /// Hits over shots on the main class, where the log has 30 shots or more.
    pub accuracy: Option<f64>,
    /// Sentries destroyed, from the server log; `None` without one.
    pub sentry_kills: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchPage {
    pub total: i64,
    pub items: Vec<MatchSummary>,
}

/// SQL for the format a log is treated as. A manual override always wins.
const EFFECTIVE_FORMAT: &str = "COALESCE(i.format_override, i.format)";

/// A match `m` with a SourceTV demo on this machine (Q56).
const STV_ONLY_SQL: &str = "AND EXISTS (SELECT 1 FROM demo_link dl JOIN demo d ON d.demo_id = dl.demo_id
                                  WHERE dl.log_id = m.log_id AND d.kind = 'stv' AND d.deleted_at IS NULL)";

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
/// A log worth downloading by its format: Highlander, unclassified with a
/// Highlander headcount, or trends.tf's "other" with one. trends.tf says
/// "other" for a Highlander game a sub joined (19-24 players in the log),
/// and our classifier, counting each side's classes, is the one that can
/// tell (audit, Oct 2026: such games were never downloaded).
const WORTH_FETCHING: &str = "( COALESCE(i.format_override, i.format) = 'highlander'
       OR (COALESCE(i.format_override, i.format) IS NULL AND COALESCE(i.player_count, 0) >= 16)
       OR (i.format_override IS NULL AND i.format = 'other' AND i.player_count BETWEEN 16 AND 24) )";

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
                    -- trends.tf's 'other' never undoes what our classifier
                    -- or a combine's parts placed.
                    format         = CASE WHEN excluded.format = 'other' AND log_index.classified_by IN ('heuristic', 'parts')
                                          THEN log_index.format ELSE COALESCE(excluded.format, log_index.format) END,
                    league         = excluded.league,
                    etf2l_match_id = excluded.etf2l_match_id,
                    demos_tf_id    = excluded.demos_tf_id,
                    duplicate_of   = excluded.duplicate_of,
                    classified_by  = CASE WHEN excluded.format = 'other' AND log_index.classified_by IN ('heuristic', 'parts')
                                          THEN log_index.classified_by
                                          WHEN excluded.format IS NOT NULL THEN 'trends'
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

    /// The combined log `log_id` is a part of, if any (Q71).
    pub async fn superseded_by(&self, log_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar("SELECT superseded_by FROM log_index WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?
            .flatten())
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
        // A combined log trends.tf could not place -- no single map, so
        // "other" or nothing -- is the format its parts agree on. ivg, 24 Oct
        // 2026: four Highlander maps uploaded again as "STEAMHAPPY MIX" came
        // back as "other", the four were hidden behind it as its parts, and
        // the evening was never downloaded. trends.tf's own label wins
        // whenever it gives one of the formats; a person's override always.
        sqlx::query(
            "UPDATE log_index SET classified_by = 'parts',
                    format = (SELECT MIN(p.format) FROM log_index p WHERE p.superseded_by = log_index.log_id)
             WHERE (format IS NULL OR format NOT IN ('highlander', 'sixes'))
               AND EXISTS (SELECT 1 FROM log_index p WHERE p.superseded_by = log_index.log_id)
               AND NOT EXISTS (SELECT 1 FROM log_index p WHERE p.superseded_by = log_index.log_id
                                 AND (p.format IS NULL OR p.format NOT IN ('highlander', 'sixes')))
               AND (SELECT COUNT(DISTINCT p.format) FROM log_index p WHERE p.superseded_by = log_index.log_id) = 1",
        )
        .execute(&mut *tx)
        .await?;
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
             -- A log stored from more.tf is asked of logs.tf again (0031).
             WHERE (r.log_id IS NULL OR i.log_id IN (SELECT log_id FROM log_stand_in))
               AND i.superseded_by IS NULL
               AND {WORTH_FETCHING}
               -- Never a deleted log (Q77); a failing one on its first tries,
               -- then once a week until it has had them all.
               AND (e.log_id IS NULL OR (e.gone = 0 AND (e.attempts < ?1
                    OR (e.attempts < ?2 AND e.last_attempt_at <= datetime('now', '-7 days')))))
               AND {window}
             ORDER BY i.played_at DESC"
        );
        let rows = sqlx::query(&sql).bind(max_attempts).bind(MAX_FETCH_TRIES).fetch_all(self.pool()).await?;
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

    /// Whether a downloaded STV demo is deleted once every match it covers
    /// has been read (PLAN Q23). Off unless asked for.
    pub async fn auto_delete_demos(&self) -> Result<bool> {
        Ok(self.get_setting(keys::AUTO_DELETE_DEMOS).await?.as_deref() == Some("1"))
    }

    pub async fn set_auto_delete_demos(&self, on: bool) -> Result<()> {
        self.set_setting(keys::AUTO_DELETE_DEMOS, if on { "1" } else { "0" }).await
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
               AND {WORTH_FETCHING}
               AND NOT {window}"
        ))
        .fetch_one(self.pool())
        .await?)
    }

    /// Highest `updated` timestamp seen from trends.tf, for incremental sync.
    /// When a log was played, as the index has it.
    pub async fn log_played_at(&self, log_id: i64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar("SELECT played_at FROM log_index WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?
            .flatten())
    }

    /// Whether a source has ever been read in full: the logs.tf search runs
    /// over the whole history once, on the first sync.
    pub async fn source_synced(&self, source: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT 1 FROM sync_state WHERE source = ?1 AND last_error IS NULL")
            .bind(source)
            .fetch_optional(self.pool())
            .await?
            .is_some())
    }

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

    /// Every log that would not download, newest first.
    ///
    /// A sync used to say "2 failed" and leave it there: no way to see which
    /// two, why, or what to do (KamikaZe, September 2026). Everything needed
    /// was already stored — this just asks for it.
    pub async fn failed_logs(&self) -> Result<Vec<FailedLog>> {
        let rows = sqlx::query(
            "SELECT e.log_id, e.attempts, e.last_attempt_at, e.error, e.gone,
                    i.title, i.map, i.played_at
             FROM log_fetch_error e
             LEFT JOIN log_index i ON i.log_id = e.log_id
             -- One rescued from more.tf is stored: nothing is missing.
             WHERE e.log_id NOT IN (SELECT log_id FROM log_raw)
             ORDER BY COALESCE(i.played_at, 0) DESC, e.log_id DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| FailedLog {
                log_id: r.get("log_id"),
                attempts: r.get("attempts"),
                last_attempt_at: r.get("last_attempt_at"),
                error: r.get("error"),
                title: r.get("title"),
                map: r.get("map"),
                played_at: r.get("played_at"),
                gone: r.get::<i64, _>("gone") != 0,
                given_up: r.get::<i64, _>("gone") != 0 || r.get::<i64, _>("attempts") >= MAX_FETCH_TRIES,
            })
            .collect())
    }

    /// Forget a log's failures, so the next sync tries it again from scratch.
    pub async fn clear_fetch_error(&self, log_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM log_fetch_error WHERE log_id = ?1")
            .bind(log_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// The same for every failure still being tried: what "try them all
    /// again" means. A deleted log, or one out of tries, stays given up (Q77):
    /// trying it again is one log at a time, by hand.
    pub async fn clear_all_fetch_errors(&self) -> Result<u64> {
        Ok(sqlx::query("DELETE FROM log_fetch_error WHERE gone = 0 AND attempts < ?1")
            .bind(MAX_FETCH_TRIES)
            .execute(self.pool())
            .await?
            .rows_affected())
    }

    /// One more failed try. `gone` when both sources said the log does not
    /// exist: it is then never asked for again (Q77).
    pub async fn record_fetch_error(&self, log_id: i64, error: &str, gone: bool) -> Result<()> {
        sqlx::query(
            "INSERT INTO log_fetch_error (log_id, error, gone) VALUES (?1, ?2, ?3)
             ON CONFLICT(log_id) DO UPDATE SET
                attempts = log_fetch_error.attempts + 1,
                last_attempt_at = datetime('now'),
                error = excluded.error,
                gone = excluded.gone",
        )
        .bind(log_id)
        .bind(error)
        .bind(gone)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Every deleted log, for the supersessions: a deleted combined log must
    /// not hide its parts, which may still be there (Q77).
    pub async fn gone_logs(&self) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT log_id FROM log_fetch_error WHERE gone = 1").fetch_all(self.pool()).await?)
    }

    /// Deleted logs whose more.tf copy has not been asked for yet, at most
    /// `limit`, newest first.
    pub async fn gone_rescue_todo(&self, limit: i64) -> Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT e.log_id FROM log_fetch_error e LEFT JOIN log_index i ON i.log_id = e.log_id
              WHERE e.gone = 1 AND e.rescue_tried = 0 AND e.log_id NOT IN (SELECT log_id FROM log_raw)
              ORDER BY COALESCE(i.played_at, 0) DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn mark_rescue_tried(&self, ids: &[i64]) -> Result<()> {
        for id in ids {
            sqlx::query("UPDATE log_fetch_error SET rescue_tried = 1 WHERE log_id = ?1").bind(id).execute(self.pool()).await?;
        }
        Ok(())
    }

    /// Whether the index knows this log: a sync has seen it, whatever its format.
    pub async fn is_indexed(&self, log_id: i64) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT 1 FROM log_index WHERE log_id = ?1")
            .bind(log_id)
            .fetch_optional(self.pool())
            .await?
            .is_some())
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

    /// Record our own classification -- where trends.tf gave none, or could
    /// only say "other": it says that for a Highlander game a sub joined,
    /// which our classifier, counting each side's classes, places (audit,
    /// Oct 2026: such games were never shown, and importing one by hand left
    /// it hidden too).
    pub async fn set_heuristic_format(&self, log_id: i64, format: Format) -> Result<()> {
        sqlx::query(
            "UPDATE log_index SET format = ?2, classified_by = 'heuristic'
             WHERE log_id = ?1 AND (format IS NULL OR classified_by = 'heuristic'
                                    OR (classified_by = 'trends' AND format = 'other'))",
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
        let stv = if filter.stv_only { STV_ONLY_SQL } else { "" };
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
                                   WHERE s.log_id = m.log_id AND s.map LIKE ?{map_p} || '%')) {stv}"
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
        let (class_joins, class_count) = class_count_sql(filter).unwrap_or_else(|| (String::new(), "NULL"));
        let rows = sqlx::query(&format!(
            "SELECT m.log_id, m.played_at, m.map, m.title, m.duration_s,
                    ({class_count}) AS class_count,
                    {EFFECTIVE_FORMAT} AS format, i.league, i.etf2l_match_id, i.demos_tf_id,
                    m.red_score, m.blue_score,
                    EXISTS (SELECT 1 FROM demo_link dl WHERE dl.log_id = m.log_id) AS has_demo,
                    (SELECT COUNT(*) FROM log_index pi WHERE pi.superseded_by = m.log_id) AS part_count,
                    (SELECT group_concat(map, '|') FROM
                        (SELECT map FROM log_segment s WHERE s.log_id = m.log_id AND map IS NOT NULL ORDER BY seq)
                    ) AS segment_maps,
                    p.team, p.main_class, p.kills, p.deaths, p.assists, p.dmg, p.time_s,
                    p.dt, p.hr, p.heal, p.ubers, p.drops, p.headshots, p.headshots_hit, p.backstabs, p.airshots, p.medkits, p.medkits_hp, p.sentries, p.cpc, p.ic, p.lks, p.suicides,
                    r.score AS my_rating,
                    (SELECT SUM(ls.sentry_kills) FROM log_stat ls WHERE ls.log_id = m.log_id AND ls.account_id = p.account_id) AS my_sentries,
                    (SELECT CAST(SUM(w.hits) AS REAL) / SUM(w.shots) FROM log_weapon w WHERE w.log_id = m.log_id AND w.account_id = p.account_id AND w.class = p.main_class HAVING SUM(w.shots) >= 30) AS my_accuracy,
                    {CONTEXT_COLUMNS}
             FROM match m
             JOIN log_index i ON i.log_id = m.log_id
             LEFT JOIN match_player p ON p.log_id = m.log_id AND p.account_id = ?1
             LEFT JOIN rating r ON r.log_id = m.log_id AND r.account_id = ?1
                AND r.class = p.main_class AND r.model_version = ?8
             LEFT JOIN match_context c ON c.log_id = m.log_id
             LEFT JOIN etf2l_match e ON e.match_id = c.etf2l_match_id
             {class_joins}
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
        .bind(filter.order_values.as_deref().unwrap_or("{}"))
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
                        dt: r.get("dt"),
                        hr: r.get("hr"),
                        heal: r.get("heal"),
                        ubers: r.get("ubers"),
                        drops: r.get("drops"),
                        headshots: r.get("headshots"),
                        headshots_hit: r.get("headshots_hit"),
                        backstabs: r.get("backstabs"),
                        airshots: r.get("airshots"),
                        medkits: r.get("medkits"),
                        medkits_hp: r.get("medkits_hp"),
                        sentries: r.get("sentries"),
                        cpc: r.get("cpc"),
                        ic: r.get("ic"),
                        lks: r.get("lks"),
                        suicides: r.get("suicides"),
                        accuracy: r.get("my_accuracy"),
                        sentry_kills: r.get("my_sentries"),
                    }
                });
                let maps: Vec<String> = r
                    .get::<Option<String>, _>("segment_maps")
                    .map(|s| s.split('|').map(str::to_string).collect())
                    .unwrap_or_default();
                let parts: i64 = r.get("part_count");
                // The map the stat scales place this match on: its one map,
                // none for a log over several.
                let bases: std::collections::BTreeSet<String> = maps.iter().map(|x| hl_core::maps::map_base(x)).collect();
                let map_key = match bases.len() {
                    1 => bases.into_iter().next(),
                    0 if parts == 0 => r.get::<Option<String>, _>("map").map(|x| hl_core::maps::map_base(&x)),
                    _ => None,
                };
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
                    maps,
                    parts,
                    map_key,
                    rating: r.get("my_rating"),
                    class_count: r.get("class_count"),
                    me,
                }
            })
            .collect();

        Ok(MatchPage { total, items })
    }

    /// The owner's own matches by class and by map, most played first: what
    /// the match list's filters offer. Maps lose their version, so
    /// `koth_product_final` and `koth_product_rc8` are one entry.
    /// The classes and maps the match list can filter to, each with how many
    /// matches it holds under the list's other filters (Q59, Emiel): the
    /// class counts follow the format, the kind, the period and the map, and
    /// the map counts follow those and the class. `filter`'s paging and sort
    /// are not used.
    pub async fn played_classes_and_maps(&self, me: u32, filter: &MatchFilter) -> Result<(Vec<(String, i64)>, Vec<(String, i64)>)> {
        // ?1 me, ?2 format, ?3 kind, ?4 from, ?5 to, ?6 class, ?7 map; the
        // class counts leave out the class, and the map counts the map.
        let shared = format!(
            "i.superseded_by IS NULL
               AND (?2 IS NULL OR {EFFECTIVE_FORMAT} = ?2)
               AND (?3 IS NULL OR c.kind = ?3)
               AND (?4 IS NULL OR m.played_at >= ?4)
               AND (?5 IS NULL OR m.played_at <= ?5) {}",
            if filter.stv_only { STV_ONLY_SQL } else { "" }
        );
        let by_map = "(?7 IS NULL OR m.map LIKE ?7 || '%'
               OR EXISTS (SELECT 1 FROM log_segment s2 WHERE s2.log_id = m.log_id AND s2.map LIKE ?7 || '%'))";
        let classes: Vec<(String, i64)> = sqlx::query_as(&format!(
            "SELECT mp.main_class, COUNT(*) FROM match_player mp
             JOIN log_index i ON i.log_id = mp.log_id
             JOIN match m ON m.log_id = mp.log_id
             LEFT JOIN match_context c ON c.log_id = mp.log_id
             WHERE mp.account_id = ?1 AND mp.main_class IS NOT NULL AND {shared} AND {by_map}
               AND (?6 IS NULL OR 1)
             GROUP BY mp.main_class ORDER BY COUNT(*) DESC"
        ))
        .bind(me)
        .bind(&filter.format)
        .bind(&filter.kind)
        .bind(filter.from)
        .bind(filter.to)
        .bind(&filter.class)
        .bind(&filter.map)
        .fetch_all(self.pool())
        .await?;

        // A combined log counts once per map it holds, which is what a player
        // means by "my upward games".
        let rows: Vec<(i64, String)> = sqlx::query_as(&format!(
            "SELECT DISTINCT mp.log_id, COALESCE(s.map, m.map) AS map
             FROM match_player mp
             JOIN log_index i ON i.log_id = mp.log_id
             JOIN match m ON m.log_id = mp.log_id
             LEFT JOIN match_context c ON c.log_id = mp.log_id
             LEFT JOIN log_segment s ON s.log_id = mp.log_id
             WHERE mp.account_id = ?1 AND {shared} AND COALESCE(s.map, m.map) IS NOT NULL
               AND (?6 IS NULL OR mp.main_class = ?6)
               AND (?7 IS NULL OR 1)"
        ))
        .bind(me)
        .bind(&filter.format)
        .bind(&filter.kind)
        .bind(filter.from)
        .bind(filter.to)
        .bind(&filter.class)
        .bind(&filter.map)
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
            // Failing and still being tried; the given-up are counted apart
            // and no longer called failures (Q77).
            failed: q(format!(
                "SELECT COUNT(*) FROM log_fetch_error WHERE gone = 0 AND attempts >= 3 AND attempts < {MAX_FETCH_TRIES}
                   AND log_id NOT IN (SELECT log_id FROM log_raw)"
            ))
            .await?,
            given_up: q(format!(
                "SELECT COUNT(*) FROM log_fetch_error WHERE (gone = 1 OR attempts >= {MAX_FETCH_TRIES})
                   AND log_id NOT IN (SELECT log_id FROM log_raw)"
            ))
            .await?,
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
            stv_only: false,
            order_values: None,
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

    #[tokio::test]
    async fn a_highlander_game_with_a_sub_trends_calls_other_is_fetched_and_shown() {
        // Audit, Oct 2026: trends.tf says "other" for a Highlander log a sub
        // joined (20 players); it was never downloaded, and importing it by
        // hand left it hidden.
        let db = Db::connect_in_memory().await.unwrap();
        index(&db, 1, 0.01).await;
        sqlx::query("UPDATE log_index SET player_count = 20 WHERE log_id = 1").execute(db.pool()).await.unwrap();
        let trends = |format: &'static str| TrendsIndexRow {
            log_id: 1,
            title: Some("a game"),
            map: Some("pl_upward_f12"),
            played_at: None,
            duration_s: Some(1800),
            format: Some(format),
            league: None,
            etf2l_match_id: None,
            demos_tf_id: None,
            duplicate_of: None,
            raw_json: "{}",
        };
        db.upsert_trends_rows(&[trends("other")]).await.unwrap();
        assert_eq!(db.fetch_queue(3).await.unwrap(), vec![1], "queued for our classifier to see");

        db.set_heuristic_format(1, Format::Highlander).await.unwrap();
        assert_eq!(db.effective_format(1).await.unwrap().as_deref(), Some("highlander"));
        // trends.tf saying "other" again on a later sync does not undo it.
        db.upsert_trends_rows(&[trends("other")]).await.unwrap();
        assert_eq!(db.effective_format(1).await.unwrap().as_deref(), Some("highlander"));
        // trends.tf's own formats still win.
        db.upsert_trends_rows(&[trends("sixes")]).await.unwrap();
        assert_eq!(db.effective_format(1).await.unwrap().as_deref(), Some("sixes"));
    }

    #[tokio::test]
    async fn a_combined_log_trends_cannot_place_takes_its_parts_format() {
        // ivg, 24 Oct 2026: a Highlander evening's four maps and a combine
        // of them that trends.tf called "other"; the parts are hidden behind
        // it, so unless it counts as Highlander nothing of that night is
        // downloaded.
        let db = Db::connect_in_memory().await.unwrap();
        for id in 1..=5 {
            index(&db, id, 0.01).await;
        }
        sqlx::query("UPDATE log_index SET format = 'highlander', classified_by = 'trends' WHERE log_id <= 4")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE log_index SET format = 'other', classified_by = 'trends', map = NULL WHERE log_id = 5")
            .execute(db.pool())
            .await
            .unwrap();
        let parts: HashMap<i64, i64> = (1..=4).map(|p| (p, 5)).collect();
        db.apply_supersessions(&parts).await.unwrap();
        assert_eq!(db.fetch_queue(3).await.unwrap(), vec![5], "the combine is fetched, its parts are not");
        let (format, by): (String, String) = sqlx::query_as("SELECT format, classified_by FROM log_index WHERE log_id = 5")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!((format.as_str(), by.as_str()), ("highlander", "parts"));

        // Parts that disagree leave it as trends.tf said.
        sqlx::query("UPDATE log_index SET format = 'other' WHERE log_id = 5").execute(db.pool()).await.unwrap();
        sqlx::query("UPDATE log_index SET format = 'sixes' WHERE log_id = 4").execute(db.pool()).await.unwrap();
        db.apply_supersessions(&parts).await.unwrap();
        let format: String = sqlx::query_scalar("SELECT format FROM log_index WHERE log_id = 5").fetch_one(db.pool()).await.unwrap();
        assert_eq!(format, "other");
    }

    #[tokio::test]
    async fn a_deleted_log_is_never_asked_again_and_a_failing_one_waits_a_week() {
        // Q77 (Flashy): 23 logs logs.tf had deleted were listed and retried
        // for good.
        let db = Db::connect_in_memory().await.unwrap();
        for id in 1..=6 {
            index(&db, id, 0.5).await;
        }
        db.record_fetch_error(1, "https://logs.tf/api/v1/log/1: HTTP 404 Not Found", true).await.unwrap();
        for _ in 0..2 {
            db.record_fetch_error(2, "timed out", false).await.unwrap(); // 2 tries: tried again
        }
        for _ in 0..3 {
            db.record_fetch_error(3, "timed out", false).await.unwrap(); // 3 tries, just now: waits
        }
        for _ in 0..3 {
            db.record_fetch_error(4, "timed out", false).await.unwrap(); // 3 tries, a week ago: tried
        }
        sqlx::query("UPDATE log_fetch_error SET last_attempt_at = datetime('now', '-8 days') WHERE log_id = 4")
            .execute(db.pool())
            .await
            .unwrap();
        for _ in 0..MAX_FETCH_TRIES {
            db.record_fetch_error(5, "timed out", false).await.unwrap(); // out of tries
        }
        sqlx::query("UPDATE log_fetch_error SET last_attempt_at = datetime('now', '-30 days') WHERE log_id = 5")
            .execute(db.pool())
            .await
            .unwrap();

        let mut queue = db.fetch_queue(3).await.unwrap();
        queue.sort_unstable();
        assert_eq!(queue, vec![2, 4, 6], "not the deleted one, not the one waiting its week, not the one out of tries");

        let failed = db.failed_logs().await.unwrap();
        let given_up: Vec<i64> = failed.iter().filter(|f| f.given_up).map(|f| f.log_id).collect();
        assert_eq!(given_up.len(), 2);
        assert!(given_up.contains(&1) && given_up.contains(&5));
        assert!(failed.iter().find(|f| f.log_id == 1).unwrap().gone);
        let stats = db.index_stats().await.unwrap();
        assert_eq!((stats.failed, stats.given_up), (2, 2), "3 and 4 are failing, 1 and 5 given up");

        // "Try them all again" leaves the given-up alone.
        assert_eq!(db.clear_all_fetch_errors().await.unwrap(), 3);
        let left: Vec<i64> = db.failed_logs().await.unwrap().iter().map(|f| f.log_id).collect();
        assert_eq!(left.len(), 2);

        // Rescued from more.tf: stored, so no longer listed or rescued.
        assert_eq!(db.gone_rescue_todo(10).await.unwrap(), vec![1]);
        db.store_raw_log(1, "{}").await.unwrap();
        assert!(db.failed_logs().await.unwrap().iter().all(|f| f.log_id != 1));
        assert!(db.gone_rescue_todo(10).await.unwrap().is_empty());
        assert_eq!(db.gone_logs().await.unwrap(), Vec::<i64>::new(), "storing it cleared its failure");
    }

    #[tokio::test]
    async fn the_filter_counts_take_every_filter() {
        // The SQL holds together with every filter set and with none (Q59).
        let db = Db::connect_in_memory().await.unwrap();
        let all = MatchFilter::default();
        let some = MatchFilter {
            format: Some("highlander".into()),
            kind: Some("official".into()),
            from: Some(0),
            to: Some(i64::MAX),
            class: Some("sniper".into()),
            map: Some("pl_upward".into()),
            stv_only: true,
            ..Default::default()
        };
        for f in [&all, &some] {
            let (classes, maps) = db.played_classes_and_maps(1, f).await.unwrap();
            assert!(classes.is_empty() && maps.is_empty());
        }
        // And the list itself, with the STV filter (Q56) and a class sort (Q61).
        let stv = MatchFilter { stv_only: true, ..filter(None, false) };
        assert_eq!(db.list_matches(Some(1), &stv).await.unwrap().total, 0);
        for key in ["killed:soldier", "diedto:heavyweapons"] {
            db.list_matches(Some(1), &filter(Some(key), false)).await.unwrap();
        }
    }

    #[test]
    fn a_class_sort_names_a_real_class() {
        assert!(class_count_sql(&filter(Some("killed:soldier"), false)).unwrap().0.contains("k.victim_class = 'soldier'"));
        assert!(class_count_sql(&filter(Some("diedto:heavyweapons"), false)).unwrap().0.contains("k.killer_class = 'heavy'"));
        // Nothing but a class goes into the SQL.
        assert!(class_count_sql(&filter(Some("killed:x' OR 1=1 --"), false)).is_none());
        assert!(class_count_sql(&filter(Some("hugged:spy"), false)).is_none());
        assert!(class_count_sql(&filter(Some("kills"), false)).is_none());
    }
}
