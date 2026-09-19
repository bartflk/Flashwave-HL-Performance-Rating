//! Rating every stored performance, and loading a player's profile.
//!
//! Rating is two passes over the stored logs, because a percentile needs the
//! whole pool before any single value can be placed in it:
//!
//! ```text
//! 1. normalize every kept Highlander log, extract every rateable performance
//! 2. build per-class baselines from all of them except the owner's
//! 3. score every performance against those baselines
//! ```
//!
//! No network. Runs after every sync and reprocess.

use crate::normalize::normalize;
use crate::Progress;
use anyhow::{Context, Result};
use hl_core::{SteamId, TfClass};
use hl_db::{Db, RatingRow};
use hl_rating::model::{extract, rate, Baseline, Component, Performance};
use hl_rating::profile::{self, Extra, HistoryRow};
use hl_rating::{Profile, Rating, Weights, MODEL_VERSION};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateSummary {
    pub logs: usize,
    pub performances: usize,
    pub rated: usize,
    /// The owner's rated games.
    pub mine: usize,
}

pub async fn rate_all(
    db: &Db,
    me: Option<SteamId>,
    w: &Weights,
    mut progress: impl FnMut(Progress),
) -> Result<RateSummary> {
    let ids = db.rateable_log_ids().await?;
    let total = ids.len();

    // Pass 1: every rateable performance in every kept Highlander log. Kills
    // from raw logs are valued one by one where a raw log exists.
    let kills = db.all_kills().await?;
    let windows = db.all_round_windows().await?;
    let mut perfs: Vec<(i64, Performance)> = Vec::new();
    for (i, log_id) in ids.iter().copied().enumerate() {
        if i % 25 == 0 {
            progress(Progress::Rating { done: i, total });
        }
        let Some(json) = db.raw_log(log_id).await? else { continue };
        let value: serde_json::Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(log_id, error = %e, "skipping unparseable stored log");
                continue;
            }
        };
        let Ok(log) = normalize(log_id, &value) else { continue };
        let impact = crate::kills::impacts_for(
            kills.get(&log_id).map_or(&[][..], |k| k.as_slice()),
            windows.get(&log_id).map_or(&[][..], |k| k.as_slice()),
            log.map.as_deref(),
            w,
        );
        perfs.extend(
            log.players
                .iter()
                .filter_map(|p| extract(p, &log.flags, w, impact.get(&p.id.account_id())))
                .map(|p| (log_id, p)),
        );
    }

    // Pass 2: the pools, without the owner in them.
    let baseline = Baseline::build(perfs.iter().map(|(_, p)| p), me.map(|m| m.account_id()));
    let stored: Vec<(String, String, Vec<f64>)> = baseline
        .parts()
        .map(|(class, c, vals)| (class.as_str().to_string(), c.key().to_string(), vals.to_vec()))
        .collect();
    db.replace_baselines(MODEL_VERSION, &stored).await?;

    // Pass 3: score everyone, the owner included.
    let mut rows = Vec::with_capacity(perfs.len());
    let mut mine = 0;
    for (log_id, perf) in &perfs {
        let Some(r) = rate(perf, &baseline, w) else { continue };
        if me.is_some_and(|m| m.account_id() == perf.account_id) {
            mine += 1;
        }
        rows.push(RatingRow {
            log_id: *log_id,
            account_id: perf.account_id,
            class: perf.class.as_str(),
            score: r.score,
            minutes: r.minutes,
            parts_json: serde_json::to_string(&r.parts)?,
        });
    }
    db.replace_ratings(MODEL_VERSION, &rows).await?;
    progress(Progress::Rating { done: total, total });

    Ok(RateSummary { logs: total, performances: perfs.len(), rated: rows.len(), mine })
}

/// The stored baselines for the current model; empty until the first rating pass.
pub async fn load_baseline(db: &Db) -> Result<Baseline> {
    let parts = db
        .load_baselines(MODEL_VERSION)
        .await?
        .into_iter()
        .filter_map(|(class, comp, vals)| {
            Some((TfClass::parse(&class).ok()?, Component::parse(&comp)?, vals))
        });
    Ok(Baseline::from_parts(parts))
}

/// Classes the player has rated games on, most played first.
pub async fn rated_classes(db: &Db, me: SteamId) -> Result<Vec<(String, i64)>> {
    db.rated_classes(me.account_id(), MODEL_VERSION).await
}

/// The owner's profile on one class; `kind` narrows it to officials, scrims or pugs.
pub async fn load_profile(
    db: &Db,
    me: SteamId,
    class: TfClass,
    kind: Option<&str>,
) -> Result<Option<Profile>> {
    let rows = db.rating_history(me.account_id(), class.as_str(), MODEL_VERSION).await?;
    let history: Vec<HistoryRow> = rows
        .into_iter()
        .map(|r| {
            let parts = serde_json::from_str(&r.parts_json).context("stored rating parts")?;
            let (mine, theirs) = if r.team == "Red" {
                (r.red_score, r.blue_score)
            } else {
                (r.blue_score, r.red_score)
            };
            let result = match (mine, theirs) {
                (Some(a), Some(b)) if a > b => Some("W"),
                (Some(a), Some(b)) if a < b => Some("L"),
                (Some(_), Some(_)) => Some("T"),
                _ => None,
            };
            Ok(HistoryRow {
                log_id: r.log_id,
                played_at: r.played_at,
                map: r.map,
                title: r.title,
                league: r.league,
                kind: r.kind,
                result: result.map(str::to_string),
                rating: Rating { class, score: r.score, minutes: r.minutes, parts },
            })
        })
        .collect::<Result<_>>()?;

    let contexts = profile::context_splits(&history);
    let history: Vec<HistoryRow> = match kind {
        Some(k) => history.into_iter().filter(|r| r.kind.as_deref() == Some(k)).collect(),
        None => history,
    };
    let Some(mut p) = profile::build(class, history) else {
        return Ok(None);
    };
    p.contexts = contexts;
    p.filter = kind.map(str::to_string);

    // Career records that read straight off the logs, no model involved.
    let mirror = db.vs_totals(me.account_id(), class.as_str(), class.as_str(), MODEL_VERSION).await?;
    if mirror.kills + mirror.deaths > 0 {
        let label = if class == TfClass::Sniper {
            "Sniper duel, career".to_string()
        } else {
            format!("vs enemy {}, career", class.display_name())
        };
        let net = mirror.kills - mirror.deaths;
        p.extras.push(Extra {
            label,
            value: format!("{} – {}", mirror.kills, mirror.deaths),
            detail: Some(format!("{}{} net", if net > 0 { "+" } else { "" }, net)),
            hint: Some(format!(
                "Your kills on the enemy {c} against their kills on you, across every rated game as {c}.",
                c = class.display_name()
            )),
        });
    }
    if class != TfClass::Medic {
        let medic = db.vs_totals(me.account_id(), class.as_str(), "medic", MODEL_VERSION).await?;
        p.extras.push(Extra {
            label: "Medic picks, career".to_string(),
            value: medic.kills.to_string(),
            detail: Some(format!("{:.2} per game", medic.kills as f64 / p.games as f64)),
            hint: Some("Enemy Medics you killed, across every rated game on this class.".to_string()),
        });
    }
    Ok(Some(p))
}
