//! The other parts of a match's SourceTV (Flashy: "make it fetch the missing
//! STV parts automatically, give a prompt and also a possibility to check for
//! missing demos").
//!
//! A server splits its recording into files of about eighteen minutes, and
//! demos.tf lists each as a demo of its own; a log is linked to one. A koth
//! match of nine rounds had rounds 4 to 6 on this machine and nothing of the
//! rest: no routes, no round openings, no aim for anyone but the owner. Here:
//! which rounds the SourceTV on this machine covers, and the parts demos.tf
//! has of the others.

use crate::demostf::{walk_player, EARLY_S, LATE_S};
use crate::sources::{DemosTfMeta, Sources};
use anyhow::Result;
use hl_core::maps::map_base;
use hl_core::SteamId;
use hl_db::Db;
use serde::Serialize;
use std::collections::HashSet;

/// A round counts as covered by a recording that began up to this long after
/// it did: the server starts a new file at the round's start, a moment late.
const COVER_SLACK_S: f64 = 5.0;
/// Nor by one that ends this soon after the round began: the next part has it.
const COVER_TAIL_S: f64 = 30.0;

/// The rounds whose start lies inside one of `spans` (`(start, length)` on
/// the log's clock). Pure, for testing: `rounds` are `(number, start, end)`.
pub fn covered_rounds(rounds: &[(i64, f64, f64)], spans: &[(f64, f64)]) -> HashSet<i64> {
    rounds
        .iter()
        .filter(|(_, f, _)| spans.iter().any(|&(s, len)| *f >= s - COVER_SLACK_S && *f <= s + len - COVER_TAIL_S))
        .map(|(n, ..)| *n)
        .collect()
}

/// Which of a match's rounds the SourceTV on this machine covers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub rounds: Vec<i64>,
    pub covered: Vec<i64>,
}

impl Coverage {
    pub fn missing(&self) -> Vec<i64> {
        self.rounds.iter().copied().filter(|r| !self.covered.contains(r)).collect()
    }
}

/// `None` for a match with no SourceTV linked, or not on the real clock. A
/// demo deleted to save space still counts: what it held was read.
pub async fn coverage(db: &Db, log_id: i64) -> Result<Option<Coverage>> {
    let Some(offset) = db.log_clock_offset(log_id).await? else { return Ok(None) };
    let spans: Vec<(f64, f64)> = db
        .demos_for_log(log_id)
        .await?
        .iter()
        .filter(|d| d.kind == "stv")
        .filter_map(|d| Some((d.start_utc? - offset as f64, d.playback_s)))
        .collect();
    if spans.is_empty() {
        return Ok(None);
    }
    let rounds = rounds_of(db, log_id).await?;
    let covered = covered_rounds(&rounds, &spans);
    let mut covered: Vec<i64> = covered.into_iter().collect();
    covered.sort_unstable();
    Ok(Some(Coverage { rounds: rounds.iter().map(|r| r.0).collect(), covered }))
}

async fn rounds_of(db: &Db, log_id: i64) -> Result<Vec<(i64, f64, f64)>> {
    Ok(db.round_spans(log_id).await?.into_iter().map(|(n, s, len)| (n, s as f64, (s + len) as f64)).collect())
}

/// A part of a match's SourceTV on demos.tf.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub demos_tf_id: i64,
    pub name: String,
    /// When the recording ended (demos.tf's upload time), unix seconds.
    pub time: Option<i64>,
    pub duration: Option<i64>,
    /// The match's rounds it covers.
    pub rounds: Vec<i64>,
    /// Its file is already on this machine.
    pub here: bool,
}

/// The parts of this match's SourceTV demos.tf has: demos on its maps whose
/// recording holds one of its rounds' starts. Kept with the match, so a
/// fetch takes them all. Asks demos.tf about the owner when they played,
/// otherwise up to four of its players, until one brings demos.
pub async fn find(db: &Db, sources: &Sources, me: SteamId, log_id: i64) -> Result<Vec<Part>> {
    let Some(offset) = db.log_clock_offset(log_id).await? else { return Ok(Vec::new()) };
    let rounds = rounds_of(db, log_id).await?;
    let (Some(first), Some(last)) = (rounds.first().map(|r| r.1), rounds.last().map(|r| r.2)) else { return Ok(Vec::new()) };
    let mut maps: HashSet<String> = db.round_maps_of(log_id).await?.into_iter().map(|(_, m)| map_base(&m)).collect();
    if maps.is_empty() {
        if let Some(m) = db.match_map(log_id).await? {
            maps.insert(map_base(&m));
        }
    }
    let here: HashSet<i64> = db.demos_tf_ids_here(log_id).await?.into_iter().collect();

    let mut players: Vec<u32> = db.log_time_and_colours(log_id).await?.map(|(_, c)| c.into_iter().map(|(a, _)| a as u32).collect()).unwrap_or_default();
    players.sort_unstable();
    players.dedup();
    let mut ask: Vec<u32> = Vec::new();
    if players.contains(&me.account_id()) {
        ask.push(me.account_id());
    }
    ask.extend(players.iter().copied().filter(|a| *a != me.account_id()).take(4));

    // The match on the real clock: a part ends after its first round began
    // and begins before its last one ended.
    let (from, to) = (first + offset as f64, last + offset as f64);
    for account in ask {
        let listed: Vec<DemosTfMeta> =
            walk_player(sources, &SteamId::from_account_id(account).to_steamid64(), (to as i64) + LATE_S, (from as i64) - EARLY_S).await?;
        let parts = parts_of(&listed, &maps, &rounds, offset as f64, &here);
        if !parts.is_empty() {
            db.add_stv_parts(log_id, &parts.iter().map(|p| p.demos_tf_id).collect::<Vec<_>>()).await?;
            return Ok(parts);
        }
    }
    Ok(Vec::new())
}

/// The demos among `listed` that are parts of this match: on one of its
/// `maps`, with a file, recording at one of its `rounds`' starts. Pure, for
/// testing.
pub fn parts_of(listed: &[DemosTfMeta], maps: &HashSet<String>, rounds: &[(i64, f64, f64)], offset: f64, here: &HashSet<i64>) -> Vec<Part> {
    let mut out: Vec<Part> = listed
        .iter()
        .filter(|d| d.available())
        .filter(|d| maps.is_empty() || d.map.as_deref().is_some_and(|m| maps.contains(&map_base(m))))
        .filter_map(|d| {
            let (end, len) = (d.time? as f64, d.duration? as f64);
            let covers = covered_rounds(rounds, &[(end - len - offset, len)]);
            if covers.is_empty() {
                return None;
            }
            let mut rs: Vec<i64> = covers.into_iter().collect();
            rs.sort_unstable();
            Some(Part { demos_tf_id: d.id, name: d.name.clone(), time: d.time, duration: d.duration, rounds: rs, here: here.contains(&d.id) })
        })
        .collect();
    out.sort_by_key(|p| p.time);
    out.dedup_by_key(|p| p.demos_tf_id);
    out
}

/// A match whose SourceTV on this machine leaves rounds out.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gappy {
    pub log_id: i64,
    pub map: Option<String>,
    pub played_at: Option<i64>,
    pub rounds: usize,
    pub covered: Vec<i64>,
    pub missing: Vec<i64>,
}

/// Every match with a SourceTV linked that leaves some of its rounds out,
/// newest first. From this machine alone: nothing is asked of demos.tf.
pub async fn gappy(db: &Db) -> Result<Vec<Gappy>> {
    let mut out = Vec::new();
    for (log_id, map, played_at) in db.logs_with_stv().await? {
        let Some(c) = coverage(db, log_id).await? else { continue };
        let missing = c.missing();
        if missing.is_empty() {
            continue;
        }
        out.push(Gappy { log_id, map, played_at, rounds: c.rounds.len(), covered: c.covered, missing });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo(id: i64, map: &str, end: i64, len: i64) -> DemosTfMeta {
        DemosTfMeta { id, url: format!("https://demos.tf/{id}.dem"), name: format!("{id}.dem"), map: Some(map.into()), duration: Some(len), time: Some(end), backend: None }
    }

    #[test]
    fn a_match_split_into_three_recordings() {
        // Nine rounds of 360 s from 1000; the clock is the real one plus 0.
        let rounds: Vec<(i64, f64, f64)> = (0..9).map(|i| (i + 1, 1000.0 + 360.0 * i as f64, 1360.0 + 360.0 * i as f64)).collect();
        // Three parts, each starting a moment after its first round.
        let listed = vec![
            demo(1, "koth_product_final", 1000 + 2 + 1080, 1080),
            demo(2, "koth_product_final", 2080 + 2 + 1080, 1080),
            demo(3, "koth_product_final", 3160 + 2 + 1080, 1080),
            // Another map the same evening, and the same map the night before.
            demo(4, "pl_upward_f12", 1500, 1200),
            demo(5, "koth_product_final", 1000 - 86_400, 1080),
        ];
        let maps: HashSet<String> = ["product".to_string()].into();
        let here: HashSet<i64> = [2].into();
        let parts = parts_of(&listed, &maps, &rounds, 0.0, &here);
        assert_eq!(parts.iter().map(|p| p.demos_tf_id).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(parts[0].rounds, vec![1, 2, 3]);
        assert_eq!(parts[1].rounds, vec![4, 5, 6]);
        assert!(parts[1].here && !parts[0].here);
        // The middle one alone covers rounds 4 to 6.
        let mut c: Vec<i64> = covered_rounds(&rounds, &[(2082.0, 1080.0)]).into_iter().collect();
        c.sort_unstable();
        assert_eq!(c, vec![4, 5, 6]);
    }
}
