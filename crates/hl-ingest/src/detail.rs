//! Loading one match for the match page.

use crate::normalize::normalize;
use anyhow::{Context, Result};
use hl_core::SteamId;
use hl_db::Db;
use hl_rating::{build_detail, MatchDetail, Weights};
use serde::Serialize;

/// A match as the page reads it: the scoreboard from the log, plus the two
/// things that come from the database instead.
///
/// This type exists because those two used to be added by the Tauri command
/// and nowhere else. `get_match` wrapped a `MatchDetail` with `context` and
/// `segments` and flattened it, so the page received one object with all
/// three; `get_parts` and `fetch_part` returned a bare `MatchDetail`. The
/// TypeScript declared a single type for both and asserted it at the
/// `invoke` boundary, where nothing checks it — so picking one half of a
/// combined log handed the page an object whose `segments` was `undefined`,
/// and the header's `d.segments.map(...)` threw. Building the page's shape
/// once, here, is what stops the two paths drifting apart again.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchView {
    #[serde(flatten)]
    pub detail: MatchDetail,
    /// Official, scrim or pug, with the ETF2L side of an official.
    pub context: Option<hl_db::MatchContext>,
    /// The maps played, in order, with rounds won on each. Empty for a log
    /// that is one map, which is every part of a combined log.
    pub segments: Vec<hl_db::Segment>,
}

/// Wrap a scoreboard with what the database knows about that log.
pub async fn view_of(db: &Db, log_id: i64, detail: MatchDetail) -> Result<MatchView> {
    Ok(MatchView {
        context: db.match_context(log_id).await?,
        segments: db.segments(log_id).await?,
        detail,
    })
}

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
    let credits = db.kill_credits(log_id).await?;
    crate::kills::attach_shared_swing(&mut impact, &kills, Some(&situations), Some(&credits), weights);
    if let Some(rows) = db.fight_counts(Some(log_id)).await?.get(&log_id) {
        crate::kills::attach_fights(&mut impact, rows);
    }
    if let Some(r) = db.round_impacts(Some(log_id)).await?.get(&log_id) {
        crate::kills::attach_round(&mut impact, r);
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
    // A combined log with no id of its own downloads its parts' (Q52).
    if detail.demos_tf_id.is_none() {
        detail.demos_tf_id = db.stv_ids(log_id).await?.first().copied();
    }
    if detail.demos_tf_id.is_none() {
        detail.etf2l_demos = crate::etf2l_demos::listed(db, log_id).await.unwrap_or(0);
    }
    detail.stand_in = db.stand_in(log_id).await?;
    Ok(detail)
}
