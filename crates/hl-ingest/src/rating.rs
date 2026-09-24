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
    // Pass 1: every rateable performance in every kept Highlander log.
    let (total, perfs) = collect_performances(db, w, &mut progress).await?;

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

/// Rate a few logs now, against the baselines already stored.
///
/// A sync downloads newest first, so a match you played last night appears in
/// the list within seconds — and used to sit there with an empty rating until
/// the whole corpus was re-rated at the end, minutes later. This fills it in
/// as it lands.
///
/// The number is real, not a placeholder. It is measured against the pool as
/// it stood at the last full pass, which a handful of new games barely move;
/// and the components that need the raw server log — openings, trades, kill
/// situations — are *skipped* rather than scored as zero, exactly as they are
/// for the logs logs.tf never had a raw log for. The full pass at the end of
/// the sync refines both.
///
/// Returns how many performances were scored. Does nothing before the first
/// full pass, when there are no baselines to measure against.
pub async fn rate_logs(db: &Db, w: &Weights, log_ids: &[i64]) -> Result<usize> {
    let baseline = load_baseline(db).await?;
    if baseline.is_empty() || log_ids.is_empty() {
        return Ok(0);
    }
    let mut scored = 0;
    for &log_id in log_ids {
        let Some(json) = db.raw_log(log_id).await? else { continue };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else { continue };
        let Ok(log) = normalize(log_id, &value) else { continue };

        // Per log rather than the whole corpus: this runs between fetches.
        let kills = db.kills_for_log(log_id).await?;
        let situations = db.kill_situations(log_id).await?;
        let windows = db.round_windows(log_id).await?;
        let mut impact =
            crate::kills::impacts_for(&kills, Some(&situations), &windows, log.map.as_deref(), w);
        let fights = db.fight_counts(Some(log_id)).await?;
        crate::kills::attach_fights(&mut impact, fights.get(&log_id).map_or(&[][..], |f| f.as_slice()));

        let rows: Vec<RatingRow> = log
            .players
            .iter()
            .filter_map(|p| extract(p, &log.flags, w, impact.get(&p.id.account_id())))
            .filter_map(|perf| {
                let r = rate(&perf, &baseline, w)?;
                Some(RatingRow {
                    log_id,
                    account_id: perf.account_id,
                    class: perf.class.as_str(),
                    score: r.score,
                    minutes: r.minutes,
                    parts_json: serde_json::to_string(&r.parts).ok()?,
                })
            })
            .collect();
        scored += rows.len();
        db.put_ratings_for_log(MODEL_VERSION, log_id, &rows).await?;
    }
    Ok(scored)
}

/// Pass 1 of rating: every rateable performance in every kept Highlander log,
/// with the components `w` names for each class. Kills from raw logs are
/// valued one by one, and fight counts attached, where a raw log exists.
/// Returns the number of logs read and the performances.
pub async fn collect_performances(
    db: &Db,
    w: &Weights,
    mut progress: impl FnMut(Progress),
) -> Result<(usize, Vec<(i64, Performance)>)> {
    let ids = db.rateable_log_ids().await?;
    let total = ids.len();

    let kills = db.all_kills().await?;
    let windows = db.all_round_windows().await?;
    let fights = db.fight_counts(None).await?;
    let situations = db.all_kill_situations().await?;
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
        let mut impact = crate::kills::impacts_for(
            kills.get(&log_id).map_or(&[][..], |k| k.as_slice()),
            situations.get(&log_id),
            windows.get(&log_id).map_or(&[][..], |k| k.as_slice()),
            log.map.as_deref(),
            w,
        );
        crate::kills::attach_fights(&mut impact, fights.get(&log_id).map_or(&[][..], |f| f.as_slice()));
        perfs.extend(
            log.players
                .iter()
                .filter_map(|p| extract(p, &log.flags, w, impact.get(&p.id.account_id())))
                .map(|p| (log_id, p)),
        );
    }

    Ok((total, perfs))
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

/// The owner's profile on one class; `kind` narrows it to officials, scrims
/// or pugs, and `period` to games played between two unix times (a season).
pub async fn load_profile(
    db: &Db,
    me: SteamId,
    class: TfClass,
    kind: Option<&str>,
    period: Option<(i64, i64)>,
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

    let history: Vec<HistoryRow> = match period {
        Some((from, to)) => history.into_iter().filter(|r| r.played_at.is_some_and(|t| t >= from && t <= to)).collect(),
        None => history,
    };
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
    // Career records count every game, so a period shows none.
    if period.is_some() {
        return Ok(Some(p));
    }

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
