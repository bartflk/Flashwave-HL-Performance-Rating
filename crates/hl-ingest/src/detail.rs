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
    Ok(Some(match_detail_from(db, log_id, &value, me, weights).await?))
}

/// The same, from JSON already in hand: a stored log, or a part of a combined
/// one that has no raw server log of its own.
pub async fn match_detail_from(
    db: &Db,
    log_id: i64,
    value: &serde_json::Value,
    me: Option<SteamId>,
    weights: &Weights,
) -> Result<MatchDetail> {
    let log = normalize(log_id, value)?;
    let baseline = crate::rating::load_baseline(db).await?;
    let kills = db.kills_for_log(log_id).await?;
    let windows = db.round_windows(log_id).await?;
    let situations = db.kill_situations(log_id).await?;
    let mut impact = crate::kills::impacts_for(&kills, Some(&situations), &windows, log.map.as_deref(), weights);
    if let Some(rows) = db.fight_counts(Some(log_id)).await?.get(&log_id) {
        crate::kills::attach_fights(&mut impact, rows);
    }
    // Before the first full pass there is no pool, so the breakdown falls
    // back to a scale where an average game is still 1.00.
    let scale = crate::rating::load_scale(db).await?.unwrap_or_default();
    let mut detail = build_detail(&log, me, weights, &baseline, &scale, &impact);

    // Kill markers first: demo enrichment then gives every marker its jump.
    crate::kills::enrich(&log, &kills, me, &mut detail);
    crate::demos::enrich(db, &log, &mut detail).await?;

    detail.parts = db
        .parts_of(log_id)
        .await?
        .into_iter()
        .map(|p| hl_rating::detail::PartView {
            log_id: p.log_id,
            title: p.title,
            map: p.map,
            played_at: p.played_at,
            duration_s: p.duration_s,
            player_count: p.player_count,
        })
        .collect();

    if let Some(info) = db.index_info(log_id).await? {
        detail.format = info.format;
        detail.league = info.league;
        detail.etf2l_match_id = info.etf2l_match_id;
        detail.demos_tf_id = info.demos_tf_id;
    }
    Ok(detail)
}
