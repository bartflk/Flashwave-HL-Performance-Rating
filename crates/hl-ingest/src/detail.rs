//! Loading one match for the match page.

use crate::normalize::normalize;
use anyhow::{Context, Result};
use hl_core::SteamId;
use hl_db::Db;
use hl_rating::{build_detail, MatchDetail, Weights};

/// Build the match page for a stored log, or `None` if it has not been fetched.
///
/// Normalizes straight from the stored raw JSON rather than reassembling the
/// normalized tables: it takes milliseconds, and it guarantees the page shows
/// exactly what the current parser reads from the source.
pub async fn match_detail(
    db: &Db,
    log_id: i64,
    me: Option<SteamId>,
    weights: &Weights,
) -> Result<Option<MatchDetail>> {
    let Some(json) = db.raw_log(log_id).await? else {
        return Ok(None);
    };
    let value: serde_json::Value =
        serde_json::from_str(&json).with_context(|| format!("log {log_id} is not valid JSON"))?;
    let log = normalize(log_id, &value)?;
    let baseline = crate::rating::load_baseline(db).await?;
    let kills = db.kills_for_log(log_id).await?;
    let impact = crate::kills::impacts_for(&kills, log.map.as_deref(), weights);
    let mut detail = build_detail(&log, me, weights, &baseline, &impact);

    // Kill markers first: demo enrichment then gives every marker its jump.
    crate::kills::enrich(&log, &kills, me, &mut detail);
    crate::demos::enrich(db, &log, &mut detail).await?;

    if let Some(info) = db.index_info(log_id).await? {
        detail.format = info.format;
        detail.league = info.league;
        detail.etf2l_match_id = info.etf2l_match_id;
        detail.demos_tf_id = info.demos_tf_id;
    }
    Ok(Some(detail))
}
