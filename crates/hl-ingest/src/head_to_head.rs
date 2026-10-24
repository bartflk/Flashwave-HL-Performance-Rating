//! Two players in one match, log by log (Flashy, October 2026: "I want more
//! granular info and a breakdown on the ratings that I can inspect"): each
//! log's rating with every part of it -- raw number, percentile against the
//! pool on that class and map, weight, share of the score -- beside impact,
//! kills, deaths, damage, who they killed, and how often they killed each
//! other.
//!
//! The logs are the match's own per-map logs where the league holds them,
//! duplicates and combined uploads left out as in the season tables; else
//! the one log asked for. Each is rated again here exactly as the rating
//! pass rates it, against the stored baselines and scale.

use crate::rawlog;
use anyhow::Result;
use hl_db::Db;
use hl_rating::model::{rate, Part};
use hl_rating::Weights;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Side {
    pub account_id: u32,
    pub team: String,
    pub class: Option<String>,
    pub minutes: f64,
    /// The rating on the 1.00 scale, and the weighted percentile it came from.
    pub rating: Option<f64>,
    pub score: Option<f64>,
    pub parts: Vec<Part>,
    /// Round-win chance added, in rounds, and per 30 minutes.
    pub impact: Option<f64>,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub dmg: i64,
    /// Kills on each class, with what a kill of that class is worth.
    pub victims: Vec<(String, i64, f64)>,
    /// Kills on the other player of the pair.
    pub on_other: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct H2hLog {
    pub log_id: i64,
    pub map: Option<String>,
    pub minutes: f64,
    /// Which colour won the log, where it has a winner.
    pub winner: Option<String>,
    pub sides: [Option<Side>; 2],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadToHead {
    pub accounts: [u32; 2],
    pub logs: Vec<H2hLog>,
    /// The rating scale: rating = 1 + (score - mean) / sd x 0.25.
    pub scale_mean: f64,
    pub scale_sd: f64,
}

pub async fn head_to_head(db: &Db, w: &Weights, log_id: i64, a: u32, b: u32) -> Result<HeadToHead> {
    // The match's own logs, as the season tables count them.
    let mut ids: Vec<i64> = Vec::new();
    if let Some(m) = db.etf2l_match_of(log_id).await? {
        let skip = crate::data_audit::excluded_logs(db).await?;
        ids = db.league_logs_of_match(m).await?.into_iter().filter(|l| !skip.contains(l)).collect();
    }
    if ids.is_empty() {
        ids.push(log_id);
    }
    let baseline = crate::rating::load_baseline(db).await?;
    let scale = crate::rating::load_scale(db).await?.unwrap_or_default();
    let mut logs = Vec::new();
    for id in ids {
        let (json, zip) = match db.league_log_files(id).await? {
            (Some(j), z) => (j, z),
            _ => match db.raw_log(id).await? {
                Some(j) => (j, db.rawlog(id).await?),
                None => continue,
            },
        };
        let w2 = w.clone();
        let (j2, z2) = (json.clone(), zip.clone());
        let perfs = tokio::task::spawn_blocking(move || crate::league_rating::rate_one(id, &j2, z2.as_deref(), &w2)).await??;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let log = crate::normalize::normalize(id, &value)?;
        // Who killed whom, from the server log.
        let mut duel: HashMap<(u32, u32), i64> = HashMap::new();
        if let Some(z) = zip.as_deref() {
            if let Ok(text) = rawlog::unzip(z) {
                for k in rawlog::parse(&text).kills.iter().filter(|k| k.counts() && k.killer.team != k.victim.team) {
                    *duel.entry((k.killer.account, k.victim.account)).or_default() += 1;
                }
            }
        }
        let impact: HashMap<u32, f64> = db.log_stats_of(id).await?.into_iter().filter(|s| s.main).filter_map(|s| Some((s.account_id as u32, s.impact?))).collect();
        let side = |acc: u32, other: u32| -> Option<Side> {
            let p = log.players.iter().find(|p| p.id.account_id() == acc)?;
            let class = p.main_class();
            let line = class.and_then(|c| p.classes.iter().find(|l| l.class == c));
            let perf = perfs.iter().find(|x| x.account_id == acc);
            let r = perf.and_then(|x| rate(x, &baseline, w));
            let mut victims: Vec<(String, i64, f64)> =
                p.vs.iter().filter(|v| v.kills > 0).map(|v| (v.other_class.as_str().to_string(), v.kills, w.victim(v.other_class))).collect();
            victims.sort_by(|x, y| y.1.cmp(&x.1));
            Some(Side {
                account_id: acc,
                team: format!("{:?}", p.team),
                class: class.map(|c| c.as_str().to_string()),
                minutes: line.map_or(0.0, |l| l.time_s as f64 / 60.0),
                rating: r.as_ref().map(|r| scale.rating(r.score)),
                score: r.as_ref().map(|r| r.score),
                parts: r.map(|r| r.parts).unwrap_or_default(),
                impact: impact.get(&acc).copied(),
                kills: line.map_or(p.stats.kills, |l| l.kills),
                deaths: line.map_or(p.stats.deaths, |l| l.deaths),
                assists: line.map_or(p.stats.assists, |l| l.assists),
                dmg: line.map_or(p.stats.dmg, |l| l.dmg),
                victims,
                on_other: duel.get(&(acc, other)).copied().unwrap_or(0),
            })
        };
        let winner = match log.red_score.cmp(&log.blue_score) {
            std::cmp::Ordering::Greater => Some("Red".to_string()),
            std::cmp::Ordering::Less => Some("Blue".to_string()),
            _ => None,
        };
        logs.push(H2hLog { log_id: id, map: log.map.clone(), minutes: log.duration_s as f64 / 60.0, winner, sides: [side(a, b), side(b, a)] });
    }
    Ok(HeadToHead { accounts: [a, b], logs, scale_mean: scale.mean, scale_sd: scale.sd })
}
