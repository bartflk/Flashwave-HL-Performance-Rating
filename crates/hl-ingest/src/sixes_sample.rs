//! The 6v6 league sample (the 6v6 plan, P5): every ETF2L 6v6 official's
//! logs, for the 6v6 rating's pools, as Highlander's league sample is for
//! Highlander's.
//!
//! **Off until switched on.** It is hours of requests in the background,
//! so it waits for the person to start it (question 7 asked for "as much as
//! possible"; how far back is [`SEASONS_BACK`]).
//!
//! **Finding them** ([`discover`]): the 6v6 seasons' results from ETF2L
//! (the 6v6 league pass, deeper), and trends.tf's list of ETF2L 6v6 logs,
//! each with the ETF2L match it belongs to.
//!
//! **Downloading them** ([`step`]): one log's JSON a step, newest first,
//! from drops.tf (by Icewind), the caller waiting between steps. A log
//! drops.tf has not got is asked again on later passes, then left.
//!
//! Its own table (0050): nothing here reaches Highlander's sample, its
//! queue or its rating.

use crate::sources::Sources;
use anyhow::Result;
use hl_db::{Db, SixesLeagueLogRow};
use serde::Serialize;

/// 6v6 seasons read back: two a year, six years, as Highlander's sample.
pub const SEASONS_BACK: i64 = 12;
/// trends.tf listed back this far.
const MAX_YEARS: i64 = 6;
const YEAR_S: i64 = 365 * 24 * 3600;
/// How often the lists are read again for new officials.
const REDISCOVER_S: i64 = 24 * 3600;
/// A log drops.tf has not got after this many tries is left alone.
pub const MAX_ATTEMPTS: i64 = 3;

const KEY_ON: &str = "sixes_sample_on";
const KEY_DISCOVERED: &str = "sixes_sample_discovered_at";
const KEY_RETRY: &str = "sixes_sample_retry_at";

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub async fn enabled(db: &Db) -> Result<bool> {
    Ok(hl_core::format::SIXES_ENABLED && db.get_setting(KEY_ON).await?.as_deref() == Some("1"))
}

pub async fn set_enabled(db: &Db, on: bool) -> Result<()> {
    db.set_setting(KEY_ON, if on { "1" } else { "0" }).await
}

/// What one step did.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Discovered { listed: usize, competitions: usize },
    Json { log_id: i64 },
    Missing { log_id: i64 },
    Failed { what: String },
    Done,
}

/// The seasons' results, then trends.tf's ETF2L 6v6 logs down to the
/// window's start, or to a week before the newest held once the back
/// catalogue is in.
pub async fn discover(db: &Db, sources: &Sources) -> Result<(usize, usize)> {
    let etf2l = crate::leagues::fetch_sixes(db, sources, SEASONS_BACK, 0, |_, _| {}).await?;
    let (known, oldest, newest) = db.sixes_league_span().await?;
    let floor = now() - MAX_YEARS * YEAR_S;
    let back_held = known > 0 && oldest.is_some_and(|o| o <= floor + 60 * 24 * 3600);
    let stop_at = if back_held { newest.map_or(floor, |n| (n - 7 * 24 * 3600).max(floor)) } else { floor };
    let mut listed = 0;
    let mut path: Option<String> = None;
    for _ in 0..2000 {
        let (rows, next) = sources.trends_league_page_of(path.as_deref(), "sixes").await?;
        if rows.is_empty() {
            break;
        }
        let keep: Vec<SixesLeagueLogRow> = rows
            .iter()
            .filter(|r| r.duplicate_of.as_ref().is_none_or(|d| d.is_empty()))
            .filter_map(|r| {
                Some(SixesLeagueLogRow {
                    log_id: r.logid,
                    etf2l_match_id: r.matchid?,
                    map: r.map.as_deref()?,
                    played_at: r.time?,
                    duration_s: r.duration,
                    title: r.title.as_deref(),
                })
            })
            .collect();
        listed += keep.len();
        db.upsert_sixes_league_logs(&keep).await?;
        if rows.iter().filter_map(|r| r.time).min().unwrap_or(0) < stop_at {
            break;
        }
        match next {
            Some(n) => path = Some(n),
            None => break,
        }
    }
    db.set_setting(KEY_DISCOVERED, &now().to_string()).await?;
    Ok((listed, etf2l.competitions))
}

/// One unit of work: discovery when due (daily), else one log's JSON.
pub async fn step(db: &Db, sources: &Sources) -> Result<Step> {
    let due = db.get_setting(KEY_DISCOVERED).await?.and_then(|s| s.parse::<i64>().ok()).is_none_or(|at| now() - at > REDISCOVER_S);
    let resting = db.get_setting(KEY_RETRY).await?.and_then(|s| s.parse::<i64>().ok()).is_some_and(|at| now() < at);
    if due && !resting {
        match discover(db, sources).await {
            Ok((listed, competitions)) => return Ok(Step::Discovered { listed, competitions }),
            // trends.tf or ETF2L down must not stop the downloads.
            Err(e) => {
                tracing::warn!(error = %format!("{e:#}"), "6v6 sample discovery failed; listing again in an hour");
                db.set_setting(KEY_RETRY, &(now() + 3600).to_string()).await?;
            }
        }
    }
    let Some(log_id) = db.sixes_league_todo(1, MAX_ATTEMPTS).await?.first().copied() else { return Ok(Step::Done) };
    match sources.drops_log(log_id).await {
        Ok(Some(body)) => {
            db.put_sixes_league_json(log_id, &body, crate::sources::DROPS).await?;
            Ok(Step::Json { log_id })
        }
        Ok(None) => {
            db.sixes_league_failed(log_id).await?;
            Ok(Step::Missing { log_id })
        }
        // drops.tf not answering is not the log's fault: no attempt spent.
        Err(e) => Ok(Step::Failed { what: format!("drops.tf: {e:#}") }),
    }
}

/// Where the 6v6 sample stands, for the ETF2L tab in 6v6.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub seasons_back: i64,
    pub counts: hl_db::SixesSampleCounts,
    pub discovered_at: Option<i64>,
}

pub async fn status(db: &Db) -> Result<Status> {
    Ok(Status {
        enabled: enabled(db).await?,
        seasons_back: SEASONS_BACK,
        counts: db.sixes_league_counts(MAX_ATTEMPTS).await?,
        discovered_at: db.get_setting(KEY_DISCOVERED).await?.and_then(|s| s.parse().ok()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_sample_is_off_until_switched_on() {
        let db = Db::connect_in_memory().await.unwrap();
        assert!(!enabled(&db).await.unwrap());
        set_enabled(&db, true).await.unwrap();
        assert_eq!(enabled(&db).await.unwrap(), hl_core::format::SIXES_ENABLED, "and never in a build with 6v6 off");
        let s = status(&db).await.unwrap();
        assert_eq!((s.counts.listed, s.seasons_back), (0, SEASONS_BACK));
    }
}
