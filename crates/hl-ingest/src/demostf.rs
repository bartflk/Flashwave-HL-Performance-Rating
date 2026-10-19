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

/// For a log with no map to match on: how far after the demo's time the log
/// may begin, beyond the demo's own length. The demo must have been
/// recording when the log went live, which another match's demo was not.
const UNMAPPED_SLACK_S: i64 = 60;

/// Pages to walk in one sync. demos.tf gives 50 demos a page, so this is
/// 1,000 demos: a regular pugger plays that in a year, and Emiel's 2023
/// official sat 39 pages down (Q52). The walk is resumed where it stopped
/// (see [`Walk`]), so deep history is reached over a few syncs.
const MAX_PAGES: usize = 20;

/// Where the walk through this player's demos.tf list has got to, kept in
/// the settings: `top` is the newest demo's time seen, `reached` the oldest,
/// and every demo between the two has been offered to the logs once.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Walk {
    top: i64,
    reached: i64,
}

const WALK_KEY: &str = "demostf_walk";

impl Walk {
    fn parse(s: &str) -> Option<Walk> {
        let (a, b) = s.split_once(',')?;
        Some(Walk { top: a.trim().parse().ok()?, reached: b.trim().parse().ok()? })
    }
    fn to_setting(self) -> String {
        format!("{},{}", self.top, self.reached)
    }
}

/// The upload time of the oldest demo demos.tf can still hand over (Q62):
/// demo 314574, 14 October 2019. Every demo before it is listed with
/// `backend: "deleted"` and no link, including 312627, which a tester thought
/// was the oldest. Found by bisecting demos.tf's ids on 12 October 2026.
pub const OLDEST_AVAILABLE: i64 = 1_571_085_763;

/// Whether demos.tf can have a downloadable demo of a match played then.
pub fn demostf_reaches(played_at: i64) -> bool {
    played_at >= OLDEST_AVAILABLE - LATE_S
}

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

/// Which log each demo belongs to: `(log_id, demo_id, the demo's map)`.
///
/// By map and time first. Then a log whose map nobody knows (Q30) may take a
/// demo on time alone, from what the logs with a map left over, but only a
/// demo that was recording when the log began: one that started before it and
/// ran past its start. Where two logs could claim one demo, the closer in time
/// takes it, and no log takes two.
pub fn match_demos(logs: &[Unlinked], demos: &[DemosTfMeta]) -> Vec<(i64, i64, Option<String>)> {
    let mut mapped: Vec<(i64, i64, i64)> = Vec::new(); // (gap, log_id, demo_id)
    let mut unmapped: Vec<(i64, i64, i64)> = Vec::new();
    for d in demos {
        // A demo whose file demos.tf no longer has is no download (Q62).
        if d.backend.as_deref() == Some("deleted") {
            continue;
        }
        let (Some(at), Some(map)) = (d.time, d.map.as_deref()) else { continue };
        let map = map_base(map);
        for (log_id, played_at, log_maps) in logs {
            let gap = at - played_at;
            if log_maps.is_empty() {
                let Some(len) = d.duration else { continue };
                if at <= *played_at && *played_at <= at + len + UNMAPPED_SLACK_S {
                    unmapped.push((gap.abs(), *log_id, d.id));
                }
                continue;
            }
            if !log_maps.iter().any(|m| map_base(m) == map) {
                continue;
            }
            if (-EARLY_S..=LATE_S).contains(&gap) {
                mapped.push((gap.abs(), *log_id, d.id));
            }
        }
    }
    let mut used_logs = std::collections::HashSet::new();
    let mut used_demos = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (by_time_alone, mut pairs) in [(false, mapped), (true, unmapped)] {
        // Closest first, then each log and each demo is spoken for once.
        pairs.sort_unstable();
        for (_, log_id, demo_id) in pairs {
            if used_logs.contains(&log_id) || used_demos.contains(&demo_id) {
                continue;
            }
            used_logs.insert(log_id);
            used_demos.insert(demo_id);
            // The demo's map is kept only where the log had none: a log
            // with maps already knows them better than one demo's listing.
            let map = by_time_alone.then(|| demos.iter().find(|d| d.id == demo_id).and_then(|d| d.map.clone())).flatten();
            out.push((log_id, demo_id, map));
        }
    }
    out
}

/// Ask demos.tf what it has for this player, and fill in the ids trends.tf
/// never gave us.
///
/// First the demos uploaded since the last walk, down to its newest; then,
/// with the pages left, on down from where the last walk stopped, until a
/// page is older than the oldest log that still needs a demo. A demo is
/// uploaded up to a few hours after its log, so the new pages reach a day
/// past the old top.
pub async fn index(db: &Db, sources: &Sources, me: SteamId) -> Result<Found> {
    let logs = db.logs_without_demo_id().await?;
    if logs.is_empty() {
        return Ok(Found::default());
    }
    // Nothing older than demos.tf's oldest file is worth walking to (Q62).
    let oldest = logs.iter().map(|(_, at, _)| *at).min().unwrap_or(0).max(OLDEST_AVAILABLE - LATE_S);
    let was = db.get_setting(WALK_KEY).await?.as_deref().and_then(Walk::parse);

    let steamid64 = me.to_steamid64();
    let mut all: Vec<DemosTfMeta> = Vec::new();
    let mut pages = 0;
    let new_floor = was.map_or(oldest, |w| (w.top - 86_400).max(oldest));
    let fresh = walk(sources, &steamid64, None, new_floor, &mut pages, &mut all).await?;
    let top = all.iter().filter_map(|d| d.time).max().or(was.map(|w| w.top)).unwrap_or(0);
    let mut reached = match (fresh, was) {
        // Met the last walk: the two join up, down to where it had reached.
        (Some(e), Some(w)) if e <= new_floor => w.reached.min(e),
        (Some(e), _) => e,
        (None, w) => w.map_or(top, |w| w.reached),
    };
    if reached > oldest && pages < MAX_PAGES {
        if let Some(e) = walk(sources, &steamid64, Some(reached), oldest, &mut pages, &mut all).await? {
            reached = reached.min(e);
        }
    }
    db.set_setting(WALK_KEY, &Walk { top, reached }.to_setting()).await?;

    let pairs = match_demos(&logs, &all);
    db.set_demos_tf_ids(&pairs).await?;
    Ok(Found { listed: all.len(), matched: pairs.len() })
}

/// Ask demos.tf again for one match (Flashy: "a refresh button so i can see
/// if demos.tf updated ... for games after i loaded the log"): the pages of
/// your demos around its time, matched as a sync would, for the log and any
/// parts folded into it that have no demo yet. Leaves the sync's walk alone.
pub async fn index_one(db: &Db, sources: &Sources, me: SteamId, log_id: i64) -> Result<Found> {
    let ids: std::collections::HashSet<i64> = db.log_and_parts(log_id).await?.into_iter().collect();
    let logs: Vec<Unlinked> = db.logs_without_demo_id().await?.into_iter().filter(|(id, _, _)| ids.contains(id)).collect();
    let Some(latest) = logs.iter().map(|(_, at, _)| *at).max() else {
        return Ok(Found::default());
    };
    let floor = logs.iter().map(|(_, at, _)| *at).min().unwrap_or(latest) - EARLY_S;
    let mut all = Vec::new();
    let mut pages = 0;
    // A demo goes up to LATE_S after its log; the page starting just past
    // that, and on down to EARLY_S before the first part.
    walk(sources, &me.to_steamid64(), Some(latest + LATE_S + 1), floor, &mut pages, &mut all).await?;
    let pairs = match_demos(&logs, &all);
    db.set_demos_tf_ids(&pairs).await?;
    Ok(Found { listed: all.len(), matched: pairs.len() })
}

/// Walk the list down from `before` until a page reaches `floor` or the
/// pages run out: the oldest time seen, `i64::MIN` at the list's end.
async fn walk(
    sources: &Sources,
    steamid64: &str,
    mut before: Option<i64>,
    floor: i64,
    pages: &mut usize,
    all: &mut Vec<DemosTfMeta>,
) -> Result<Option<i64>> {
    let mut earliest = None;
    while *pages < MAX_PAGES {
        *pages += 1;
        let page = sources.demostf_for_player(steamid64, before).await?;
        let Some(e) = page.iter().filter_map(|d| d.time).min() else {
            return Ok(Some(i64::MIN));
        };
        all.extend(page);
        earliest = Some(e);
        if e <= floor {
            break;
        }
        before = Some(e);
    }
    Ok(earliest)
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
            backend: None,
        }
    }

    #[test]
    fn a_demo_demostf_no_longer_has_is_never_matched() {
        let gone = DemosTfMeta { backend: Some("deleted".into()), ..demo(312627, "cp_sunshine", 1_570_463_416) };
        let logs = vec![(1, 1_570_463_000, vec!["cp_sunshine".into()])];
        assert!(match_demos(&logs, &[gone]).is_empty());
        assert!(!demostf_reaches(1_500_000_000));
        assert!(demostf_reaches(1_790_000_000));
    }

    /// `(log, demo)` pairs, without the map, for the older tests.
    fn ids(v: Vec<(i64, i64, Option<String>)>) -> Vec<(i64, i64)> {
        v.into_iter().map(|(l, d, _)| (l, d)).collect()
    }

    fn long(id: i64, map: &str, time: i64, len: i64) -> DemosTfMeta {
        DemosTfMeta { duration: Some(len), ..demo(id, map, time) }
    }

    #[test]
    fn a_log_with_no_map_takes_the_demo_that_was_recording_when_it_began() {
        let hour = 3600;
        let logs = vec![(1, 10 * hour, vec![]), (2, 12 * hour, vec!["pl_upward_f12".into()])];
        let demos = vec![
            // Started ten minutes before log 1, ran 40: recording when it began.
            long(100, "pl_vigil_rc10", 10 * hour - 600, 2400),
            // Ended before log 1 began: another match.
            long(101, "koth_product_final", 10 * hour - 3000, 1800),
            long(200, "pl_upward_f12", 12 * hour - 600, 2400),
        ];
        let mut got = match_demos(&logs, &demos);
        got.sort_unstable();
        assert_eq!(got, vec![(1, 100, Some("pl_vigil_rc10".into())), (2, 200, None)], "only the log with no map keeps the demo's");
    }

    #[test]
    fn a_log_with_a_map_keeps_its_demo_from_an_unmapped_neighbour() {
        let hour = 3600;
        // Both logs began while demo 100 was recording; the one whose map
        // matches takes it, and the unmapped one gets nothing.
        let logs = vec![(1, 10 * hour, vec![]), (2, 10 * hour + 300, vec!["pl_vigil_rc10".into()])];
        let demos = vec![long(100, "pl_vigil_rc10", 10 * hour - 60, 3000)];
        assert_eq!(ids(match_demos(&logs, &demos)), vec![(2, 100)]);
    }

    #[test]
    fn with_no_length_a_demo_is_never_given_to_a_log_with_no_map() {
        let logs = vec![(1, 1000, vec![])];
        assert!(match_demos(&logs, &[demo(100, "pl_vigil_rc10", 900)]).is_empty());
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
        let mut got = ids(match_demos(&logs, &demos));
        got.sort_unstable();
        assert_eq!(got, vec![(1, 100), (2, 200)]);
    }

    #[test]
    fn the_nearer_log_takes_the_demo_and_no_log_takes_two() {
        let hour = 3600;
        // Two Upward games the same evening, one demo between them.
        let logs = vec![(1, 10 * hour, vec!["pl_upward_f12".into()]), (2, 11 * hour, vec!["pl_upward_f12".into()])];
        let demos = vec![demo(100, "pl_upward_f12", 11 * hour - 300)];
        assert_eq!(ids(match_demos(&logs, &demos)), vec![(2, 100)], "the one it actually followed");
    }

    #[test]
    fn a_combined_log_matches_on_any_of_its_maps() {
        let hour = 3600;
        let logs = vec![(1, 10 * hour, vec!["pl_upward_f12".into(), "cp_steel_f12".into()])];
        // Its own map field would read "upward + steel" and match nothing.
        let demos = vec![demo(100, "cp_steel_f12", 10 * hour - 480)];
        assert_eq!(ids(match_demos(&logs, &demos)), vec![(1, 100)]);
    }

    #[test]
    fn a_combined_logs_parts_each_take_their_own_demo() {
        // Emiel's S29 0FO vs Xenon (Q52): three parts uploaded seconds after
        // demos.tf got each recording, and the combined log six minutes on.
        let maps = || vec!["pl_vigil_rc10".to_string()];
        let logs = vec![(3426234, 1685563743, maps()), (3426174, 1685561253, maps()), (3426198, 1685562225, maps()), (3426219, 1685563377, maps())];
        let demos = vec![demo(990239, "pl_vigil_rc10", 1685561256), demo(990254, "pl_vigil_rc10", 1685562227), demo(990266, "pl_vigil_rc10", 1685563380)];
        let mut got = ids(match_demos(&logs, &demos));
        got.sort_unstable();
        assert_eq!(got, vec![(3426174, 990239), (3426198, 990254), (3426219, 990266)]);
    }

    #[test]
    fn the_walk_is_kept_as_two_times() {
        let w = Walk { top: 1_790_000_000, reached: 1_685_000_000 };
        assert_eq!(Walk::parse(&w.to_setting()), Some(w));
        assert_eq!(Walk::parse("nonsense"), None);
    }

    #[test]
    fn a_map_that_does_not_match_is_never_claimed() {
        let logs = vec![(1, 0, vec!["koth_product_final".into()])];
        assert!(match_demos(&logs, &[demo(100, "pl_upward_f12", 600)]).is_empty());
    }
}
