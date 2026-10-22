//! The league sample in the rating pool (Flashy, September 2026).
//!
//! A rating says how a game compares with the pool of games on the same
//! class. That pool was only the owner's own matches -- a few hundred logs
//! from wherever the owner plays, with the owner left out so nobody was
//! compared with themselves. The league sample adds officials from every
//! ETF2L division, so the pool is the league: "1.00" is a typical league
//! game, and the owner is in it like everyone else (one Sniper among a few
//! thousand, where they had been half the Sniper pool of their own matches).
//!
//! Each sample log goes through exactly what an owner's log goes through --
//! kills valued by victim, map and situation, the fights pass (openings,
//! trades, Fight KAST, caps), the shared swing -- only in memory, from the
//! downloaded JSON and raw server log, because none of it is stored: the
//! sample's games are never written to the owner's match or rating tables.

use crate::fights::{analyse_on, counts, kill_credits};
use crate::normalize::normalize;
use crate::rawlog;
use crate::state::GameState;
use anyhow::Result;
use hl_db::{Db, RoundWindow};
use hl_rating::model::extract;
use hl_rating::{Performance, Weights};
use std::collections::HashMap;

/// Every rateable performance in the league sample, with the log it is in.
pub async fn performances(db: &Db, w: &Weights) -> Result<Vec<(i64, Performance)>> {
    let ids = db.league_rateable().await?;
    let mut out = Vec::new();
    for log_id in ids {
        let (Some(json), zip) = db.league_log_files(log_id).await? else { continue };
        let w = w.clone();
        // Parsing a raw log and its fights is ~10 ms of CPU: off the async
        // threads, like the other passes over raw logs.
        let perfs = tokio::task::spawn_blocking(move || rate_one(log_id, &json, zip.as_deref(), &w)).await?;
        match perfs {
            Ok(p) => out.extend(p.into_iter().map(|p| (log_id, p))),
            Err(e) => tracing::warn!(log_id, error = %format!("{e:#}"), "league sample log not rated"),
        }
    }
    Ok(out)
}

/// Rates up to `limit` sample logs downloaded since the last full pass,
/// against its baselines and scale, as [`crate::rating::rate_logs`] does for
/// the owner's: a season the downloader has just reached is ranked as its
/// logs land, not at the next Sync (Flashy: "#1 of 1" in AFA 2025).
///
/// `tried` holds logs tried this run that gave no rating: one with no
/// rateable player stays unrated and is not read again. Returns the logs read (0 when there
/// is nothing new, or no full pass yet to measure against).
pub async fn rate_new(db: &Db, w: &Weights, tried: &mut std::collections::HashSet<i64>, limit: i64) -> Result<usize> {
    let ids: Vec<i64> = db
        .league_unrated(hl_rating::MODEL_VERSION, limit + tried.len() as i64)
        .await?
        .into_iter()
        .filter(|id| !tried.contains(id))
        .take(limit as usize)
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    let baseline = crate::rating::load_baseline(db).await?;
    let Some(scale) = crate::rating::load_scale(db).await? else { return Ok(0) };
    if baseline.is_empty() {
        return Ok(0);
    }
    for &log_id in &ids {
        // Kept only for a log that gives no rating: one rated is not picked
        // again, until its server log lands and it is rated anew.
        tried.insert(log_id);
        let (Some(json), zip) = db.league_log_files(log_id).await? else { continue };
        let w2 = w.clone();
        let perfs = match tokio::task::spawn_blocking(move || rate_one(log_id, &json, zip.as_deref(), &w2)).await? {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(log_id, error = %format!("{e:#}"), "league sample log not rated");
                continue;
            }
        };
        let rows: Vec<(i64, u32, &str, f64, f64, String)> = perfs
            .iter()
            .filter_map(|p| {
                let r = hl_rating::model::rate(p, &baseline, w)?.scaled(&scale);
                Some((log_id, p.account_id, p.class.as_str(), r.score, r.minutes, hl_rating::guide::group_scores_json(&r.parts)))
            })
            .collect();
        if !rows.is_empty() {
            tried.remove(&log_id);
        }
        db.put_league_ratings(hl_rating::MODEL_VERSION, &rows).await?;
    }
    // Their numbers for the season tables; older logs are filled at startup.
    crate::logstat::fill(db, &ids).await?;
    Ok(ids.len())
}

/// One sample log's performances: its JSON, and its raw log where there is one.
pub fn rate_one(log_id: i64, json: &str, zip: Option<&[u8]>, w: &Weights) -> Result<Vec<Performance>> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let log = normalize(log_id, &value)?;
    let pool_map = log.map.as_deref().map(hl_core::maps::map_base);

    let mut impact = HashMap::new();
    if let Some(zip) = zip {
        let mut raw = rawlog::parse(&rawlog::unzip(zip)?);
        // The fights pass works on the server's own clock, as it does for a
        // stored raw log; each kill's index is its seq either way.
        let gs = GameState::build(&raw);
        let map = log.map.clone();
        let f = analyse_on(&raw, &gs, &|_| map.clone());
        let situations: HashMap<i64, (i8, i8)> = crate::situation::kill_states(&raw, &gs, &f.tags)
            .into_iter()
            .enumerate()
            .filter_map(|(seq, s)| s.map(|(k, _)| (seq as i64, (k.diff, k.adv))))
            .collect();
        let mut credits: HashMap<i64, Vec<(u32, f64)>> = HashMap::new();
        for (seq, account, share) in kill_credits(&raw) {
            credits.entry(seq).or_default().push((account, share));
        }
        let fight_rows: Vec<(u32, [u32; 14])> = f.players.iter().map(|s| (s.account_id, counts(s))).collect();

        // Kills are valued in logs.tf's round-time frame, where the round
        // windows are: shift them as storing them would.
        let logstf_starts: Vec<i64> = log.rounds.iter().filter_map(|r| r.start_time).collect();
        if let Some(shift) = rawlog::frame_offset(&raw.round_starts, &logstf_starts) {
            raw.shift(shift);
        }
        let kills = crate::kills::stored_kills(&raw);
        let windows: Vec<RoundWindow> = log
            .rounds
            .iter()
            .filter_map(|r| Some(RoundWindow { start: r.start_time?, length: r.length_s.unwrap_or(0), map: log.map.clone() }))
            .collect();
        impact = crate::kills::impacts_for(&kills, Some(&situations), &windows, log.map.as_deref(), w);
        crate::kills::attach_shared_swing(&mut impact, &kills, Some(&situations), Some(&credits), w);
        crate::kills::attach_fights(&mut impact, &fight_rows);
    }

    Ok(log
        .players
        .iter()
        .filter_map(|p| extract(p, &log.flags, w, impact.get(&p.id.account_id()), pool_map.as_deref()))
        .collect())
}
