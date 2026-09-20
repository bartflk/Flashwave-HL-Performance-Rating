//! Scoreboards for the logs a combined log was built from.
//!
//! One upload can hold three maps and an hour of play. The combined log is
//! what gets rated — counting its parts as well would double every kill — but
//! a scoreboard per map is what a player actually wants to read.
//!
//! logs.tf keeps the parts as ordinary logs, so each one normalizes and
//! scores exactly like a match of its own. Their JSON is stored in
//! `part_raw`, fetched during a sync or on demand here.

use crate::detail::match_detail_from;
use anyhow::{Context, Result};
use hl_core::SteamId;
use hl_db::Db;
use hl_rating::{MatchDetail, Weights};
use serde::Serialize;

/// One part of a combined log, with its own scoreboard where the data is
/// stored. `detail` is `None` until the part has been fetched.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartScore {
    pub log_id: i64,
    pub title: Option<String>,
    pub map: Option<String>,
    pub played_at: Option<i64>,
    pub duration_s: Option<i64>,
    pub detail: Option<MatchDetail>,
    /// The rounds of the combined log this part covers. logs.tf copies rounds
    /// verbatim when it combines, so a part's round and its copy in the
    /// combined log start on the same second.
    pub parent_rounds: Vec<i64>,
}

/// Rounds are matched by their start time; a second either way is rounding,
/// not a different round.
const ROUND_SLACK_S: i64 = 2;

/// Every part of `log_id`, oldest first, with the scoreboards already stored
/// and the rounds each one covers in the combined log.
pub async fn scores(db: &Db, log_id: i64, me: Option<SteamId>, w: &Weights) -> Result<Vec<PartScore>> {
    let parts = db.parts_of(log_id).await?;
    let parent = db.round_spans(log_id).await?;
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        let detail = detail_of(db, p.log_id, me, w).await?;
        out.push(PartScore {
            parent_rounds: parent_rounds(db, p.log_id, &parent).await?,
            detail,
            log_id: p.log_id,
            title: p.title,
            map: p.map,
            played_at: p.played_at,
            duration_s: p.duration_s,
        });
    }
    Ok(out)
}

/// Which of the combined log's rounds came from this part, by start time.
async fn parent_rounds(db: &Db, part_id: i64, parent: &[(i64, i64, i64)]) -> Result<Vec<i64>> {
    let Some(json) = db.part_raw(part_id).await? else { return Ok(Vec::new()) };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else { return Ok(Vec::new()) };
    let Ok(log) = crate::normalize::normalize(part_id, &value) else { return Ok(Vec::new()) };
    let starts: Vec<i64> = log.rounds.iter().filter_map(|r| r.start_time).collect();
    Ok(parent
        .iter()
        .filter(|(_, start, _)| starts.iter().any(|s| (s - start).abs() <= ROUND_SLACK_S))
        .map(|(num, ..)| *num)
        .collect())
}

/// Fetch one part's log from logs.tf, store it, and score it. Does nothing
/// but score it when it is already stored.
pub async fn fetch(db: &Db, sources: &crate::Sources, part_id: i64, me: Option<SteamId>, w: &Weights) -> Result<Option<MatchDetail>> {
    if db.part_raw(part_id).await?.is_none() {
        let json = sources
            .logstf_log(part_id)
            .await
            .with_context(|| format!("fetching log {part_id} from logs.tf"))?;
        db.store_part_raw(part_id, &json).await?;
    }
    detail_of(db, part_id, me, w).await
}

/// Build a part's match page from its stored JSON. `None` when it has not
/// been fetched, or its JSON cannot be read.
async fn detail_of(db: &Db, part_id: i64, me: Option<SteamId>, w: &Weights) -> Result<Option<MatchDetail>> {
    let Some(json) = db.part_raw(part_id).await? else { return Ok(None) };
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(part_id, error = %e, "stored part is not valid JSON");
            return Ok(None);
        }
    };
    // A part is an ordinary log: it normalizes, and its players are rated
    // against the same pool as everyone else.
    match match_detail_from(db, part_id, &value, me, w).await {
        Ok(d) => Ok(Some(d)),
        Err(e) => {
            tracing::warn!(part_id, error = %format!("{e:#}"), "part could not be scored");
            Ok(None)
        }
    }
}
