//! Finding the SourceTV demo of a match demos.tf never got linked to.
//!
//! trends.tf attaches a demos.tf id to 70% of this account's logs and nothing
//! to the other 30%, so a third of matches offered no way to reach their demo
//! — the download button had nothing to download. demos.tf does know about
//! them; a demo carries its map and the second it started, and that is enough
//! to say which log it belongs to.

use crate::sources::{DemosTfMeta, Sources};
use anyhow::Result;
use hl_core::maps::map_base;
use hl_core::SteamId;
use hl_db::Db;
use serde::Serialize;

/// How far apart a demo's time and a log's may be and still be the same match.
///
/// MEASURED, and not what it looked like: demos.tf stamps a demo when the
/// recording *started*, which is when the server loaded the map — before the
/// log, which starts when the match goes live. Against this account's logs
/// the demo runs 5 to 25 minutes early. So the window leans backwards.
///
/// It stays wide on the other side because a combined log is stamped at its
/// first round and its later maps' demos come long after. The map has to
/// match as well, and where two logs could claim one demo the nearer takes
/// it, so width costs little.
const EARLY_S: i64 = 45 * 60;
const LATE_S: i64 = 3 * 3600;

/// Pages to walk before giving up. 100 demos a page, so this is deep history.
const MAX_PAGES: usize = 20;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// Demos demos.tf listed for this player.
    pub listed: usize,
    /// Logs that gained a demos.tf id they did not have.
    pub matched: usize,
}

/// A log with no demo id yet: `(log_id, played_at, maps)`.
///
/// `maps` is a list because a combined log has several, and its own map field
/// is free text the uploader typed — "upward + steel", or an emoji. The maps
/// resolved per round are the ones to match on.
pub type Unlinked = (i64, i64, Vec<String>);

/// Which log each demo belongs to, by map and time.
///
/// A demo is stamped when it was uploaded and a log when it began, so the demo
/// is always *later*; a demo earlier than its log is somebody else's. Where
/// two logs could claim one demo, the closer in time takes it, and no log
/// takes two.
pub fn match_demos(logs: &[Unlinked], demos: &[DemosTfMeta]) -> Vec<(i64, i64)> {
    let mut pairs: Vec<(i64, i64, i64)> = Vec::new(); // (gap, log_id, demo_id)
    for d in demos {
        let (Some(at), Some(map)) = (d.time, d.map.as_deref()) else { continue };
        let map = map_base(map);
        for (log_id, played_at, log_maps) in logs {
            if !log_maps.iter().any(|m| map_base(m) == map) {
                continue;
            }
            let gap = at - played_at;
            if (-EARLY_S..=LATE_S).contains(&gap) {
                pairs.push((gap.abs(), *log_id, d.id));
            }
        }
    }
    // Closest first, then each log and each demo is spoken for once.
    pairs.sort_unstable();
    let mut out = Vec::new();
    let mut used_logs = std::collections::HashSet::new();
    let mut used_demos = std::collections::HashSet::new();
    for (_, log_id, demo_id) in pairs {
        if used_logs.insert(log_id) && used_demos.insert(demo_id) {
            out.push((log_id, demo_id));
        } else {
            used_logs.remove(&log_id);
        }
    }
    out
}

/// Ask demos.tf what it has for this player, and fill in the ids trends.tf
/// never gave us. Stops as soon as a page is older than the oldest log that
/// still needs one.
pub async fn index(db: &Db, sources: &Sources, me: SteamId) -> Result<Found> {
    let logs = db.logs_without_demo_id().await?;
    if logs.is_empty() {
        return Ok(Found::default());
    }
    let oldest = logs.iter().map(|(_, at, _)| *at).min().unwrap_or(0);

    let steamid64 = me.to_steamid64();
    let mut all: Vec<DemosTfMeta> = Vec::new();
    let mut before = None;
    for _ in 0..MAX_PAGES {
        let page = sources.demostf_for_player(&steamid64, before).await?;
        if page.is_empty() {
            break;
        }
        let earliest = page.iter().filter_map(|d| d.time).min().unwrap_or(0);
        all.extend(page);
        // Nothing older than the oldest log we care about is worth asking for.
        if earliest <= oldest {
            break;
        }
        before = Some(earliest);
    }

    let pairs = match_demos(&logs, &all);
    db.set_demos_tf_ids(&pairs).await?;
    Ok(Found { listed: all.len(), matched: pairs.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo(id: i64, map: &str, time: i64) -> DemosTfMeta {
        DemosTfMeta {
            id,
            url: String::new(),
            name: String::new(),
            map: Some(map.into()),
            duration: None,
            time: Some(time),
        }
    }

    #[test]
    fn a_demo_belongs_to_the_log_it_followed_on_the_same_map() {
        let hour = 3600;
        let logs = vec![
            (1, 10 * hour, vec!["pl_upward_f12".to_string()]),
            (2, 12 * hour, vec!["pl_vigil_rc10".to_string()]),
        ];
        let demos = vec![
            // Recording started ten minutes before the log went live.
            demo(100, "pl_upward_f12", 10 * hour - 600),
            // A different version of the same map still counts.
            demo(200, "pl_vigil_rc7", 12 * hour + 2400),
            // An hour and a half early: somebody else's game.
            demo(300, "pl_upward_f12", 8 * hour - 1800),
            // Right map, days later.
            demo(400, "pl_upward_f12", 40 * hour),
        ];
        let mut got = match_demos(&logs, &demos);
        got.sort_unstable();
        assert_eq!(got, vec![(1, 100), (2, 200)]);
    }

    #[test]
    fn the_nearer_log_takes_the_demo_and_no_log_takes_two() {
        let hour = 3600;
        // Two Upward games the same evening, one demo between them.
        let logs = vec![(1, 10 * hour, vec!["pl_upward_f12".into()]), (2, 11 * hour, vec!["pl_upward_f12".into()])];
        let demos = vec![demo(100, "pl_upward_f12", 11 * hour - 300)];
        assert_eq!(match_demos(&logs, &demos), vec![(2, 100)], "the one it actually followed");
    }

    #[test]
    fn a_combined_log_matches_on_any_of_its_maps() {
        let hour = 3600;
        let logs = vec![(1, 10 * hour, vec!["pl_upward_f12".into(), "cp_steel_f12".into()])];
        // Its own map field would read "upward + steel" and match nothing.
        let demos = vec![demo(100, "cp_steel_f12", 10 * hour - 480)];
        assert_eq!(match_demos(&logs, &demos), vec![(1, 100)]);
    }

    #[test]
    fn a_map_that_does_not_match_is_never_claimed() {
        let logs = vec![(1, 0, vec!["koth_product_final".into()])];
        assert!(match_demos(&logs, &[demo(100, "pl_upward_f12", 600)]).is_empty());
    }
}
