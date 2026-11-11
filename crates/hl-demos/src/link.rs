//! Placing logs on the real clock, and linking demos to them.
//!
//! # The clock problem
//!
//! logs.tf round `start_time`s are the game server's local clock written as if
//! it were UTC. A server in CEST is two hours early; one in CET, one hour.
//! Measured on this account: 318 logs at +0h, 169 at +1h, 208 at +2h.
//!
//! The fix needs a true-UTC anchor for when the match *ended*:
//!
//! - a normal log: its own upload time, seconds after the last round;
//! - a combined log: its upload can be days later, so the anchor is the
//!   upload of its last per-round **part** instead, which was prompt.
//!
//! The offset is `anchor - raw end` floored to a whole hour. Flooring (not
//! rounding) keeps the implied upload delay non-negative; measured residual
//! delay after the offset is 16 s median, 19 s at the 90th percentile.

use hl_core::map_base;
use serde::Serialize;

/// A log's round span on the server's raw clock, plus its UTC anchor.
#[derive(Debug, Clone)]
pub struct LogClock {
    pub log_id: i64,
    /// Lowercased map, or `None` when the log recorded none.
    pub map: Option<String>,
    pub raw_start: i64,
    pub raw_end: i64,
    /// True UTC time the match ended, give or take the upload delay.
    pub anchor_utc: i64,
}

impl LogClock {
    /// Whole-hour offset from the server clock to UTC.
    pub fn offset_s(&self) -> i64 {
        (self.anchor_utc - self.raw_end).div_euclid(3600) * 3600
    }

    pub fn start_utc(&self) -> i64 {
        self.raw_start + self.offset_s()
    }

    pub fn end_utc(&self) -> i64 {
        self.raw_end + self.offset_s()
    }

    /// Seconds between the match ending and the anchor. Large values mean the
    /// anchor is not really the match end and the log's placement is suspect.
    pub fn upload_delay_s(&self) -> i64 {
        self.anchor_utc - self.raw_end - self.offset_s()
    }
}

/// A demo's position on the real clock.
#[derive(Debug, Clone)]
pub struct DemoSpan {
    pub demo_id: i64,
    pub map: String,
    pub start_utc: f64,
    pub duration_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub demo_id: i64,
    pub log_id: i64,
    /// How the maps matched: `exact`, `label` (the demo's map appears in a
    /// multi-map log's label like "upward + steel") or `nomap` (the log has no map).
    pub method: &'static str,
    /// Share of the log's rounds that fall inside this demo.
    pub log_share: f64,
}

/// A demo must cover at least this share of the shorter of the two spans.
const MIN_OVERLAP: f64 = 0.5;
/// Stricter when the log has no map and time is the only evidence.
const MIN_OVERLAP_NO_MAP: f64 = 0.8;
/// An anchor this late is not a match end; the log is not placed at all
/// rather than placed wrongly.
const MAX_UPLOAD_DELAY_S: i64 = 30 * 60;

pub fn link(demos: &[DemoSpan], logs: &[LogClock]) -> Vec<Link> {
    let mut out = Vec::new();
    for d in demos {
        if d.duration_s <= 0.0 {
            continue;
        }
        let (ds, de) = (d.start_utc, d.start_utc + d.duration_s);
        for l in logs {
            if l.upload_delay_s() > MAX_UPLOAD_DELAY_S || l.raw_end <= l.raw_start {
                continue;
            }
            let Some(method) = map_match(&d.map, l.map.as_deref()) else { continue };
            let (ls, le) = (l.start_utc() as f64, l.end_utc() as f64);
            let overlap = de.min(le) - ds.max(ls);
            if overlap <= 0.0 {
                continue;
            }
            let share_of_shorter = overlap / d.duration_s.min(le - ls);
            let need = if method == "nomap" { MIN_OVERLAP_NO_MAP } else { MIN_OVERLAP };
            if share_of_shorter >= need {
                out.push(Link {
                    demo_id: d.demo_id,
                    log_id: l.log_id,
                    method,
                    log_share: ((overlap / (le - ls)) * 100.0).round() / 100.0,
                });
            }
        }
    }
    out
}

fn map_match(demo_map: &str, log_map: Option<&str>) -> Option<&'static str> {
    let Some(log_map) = log_map else { return Some("nomap") };
    let dm = demo_map.to_ascii_lowercase();
    let lm = log_map.to_ascii_lowercase();
    if dm == lm {
        return Some("exact");
    }
    let base = map_base(&dm);
    (!base.is_empty() && lm.contains(&base)).then_some("label")
}

/// The demo tick for a moment in a log.
///
/// `lead_s` rewinds a few seconds so the jump shows the lead-up, not the
/// aftermath. `None` when the moment falls outside the recording.
pub fn tick_for(
    event_raw_time: i64,
    log_offset_s: i64,
    demo_start_utc: f64,
    tick_rate: f64,
    total_ticks: i64,
    lead_s: f64,
) -> Option<i64> {
    let at = (event_raw_time + log_offset_s) as f64 - demo_start_utc - lead_s;
    let tick = (at * tick_rate).round() as i64;
    // Allow the lead to run before tick 0; only the event itself must be inside.
    let event_tick = ((at + lead_s) * tick_rate).round() as i64;
    (0..=total_ticks).contains(&event_tick).then_some(tick.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(id: i64, map: Option<&str>, raw_start: i64, raw_end: i64, anchor: i64) -> LogClock {
        LogClock { log_id: id, map: map.map(str::to_string), raw_start, raw_end, anchor_utc: anchor }
    }

    /// The real case: two swiftwater logs recorded in one 62-minute demo, on a
    /// server two hours behind UTC.
    #[test]
    fn a_cest_server_is_placed_two_hours_later() {
        // Raw rounds 17:31:29 -> 18:01:52 "UTC"; uploaded at 20:02:10 real UTC.
        let l = log(4122234, Some("pl_swiftwater_final1"), 1_789_666_289, 1_789_668_112, 1_789_675_330);
        assert_eq!(l.offset_s(), 7200);
        assert_eq!(l.upload_delay_s(), 18);
    }

    /// Real values: flashwav2026-09-17_21-00-24.dem and the two logs in it.
    #[test]
    fn one_demo_can_hold_two_matches() {
        let demo = DemoSpan { demo_id: 1, map: "pl_swiftwater_final1".into(), start_utc: 1_789_671_625.0, duration_s: 3712.0 };
        let logs = [
            log(4122209, Some("pl_swiftwater_final1"), 1_789_665_009, 1_789_666_197, 1_789_673_414),
            log(4122234, Some("pl_swiftwater_final1"), 1_789_666_289, 1_789_668_112, 1_789_675_330),
        ];
        let links = link(&[demo], &logs);
        let ids: Vec<i64> = links.iter().map(|l| l.log_id).collect();
        assert_eq!(ids, vec![4122209, 4122234]);
        assert!(links.iter().all(|l| l.log_share == 1.0));
    }

    #[test]
    fn a_different_map_never_links() {
        let demo = DemoSpan { demo_id: 1, map: "pl_badwater".into(), start_utc: 0.0, duration_s: 3600.0 };
        assert!(link(&[demo], &[log(9, Some("pl_upward_f12"), 100, 1000, 1010)]).is_empty());
    }

    #[test]
    fn multi_map_labels_match_by_base_name() {
        assert_eq!(map_match("pl_upward_f12", Some("upward + steel")), Some("label"));
        assert_eq!(map_match("cp_steel_f12", Some("upward + steel")), Some("label"));
        assert_eq!(map_match("koth_product_final", Some("upward + steel")), None);
        assert_eq!(map_match("pl_vigil_rc10", None), Some("nomap"));
    }

    #[test]
    fn map_base_strips_prefix_and_version() {
        assert_eq!(map_base("pl_upward_f12"), "upward");
        assert_eq!(map_base("koth_product_final"), "product");
        assert_eq!(map_base("pl_vigil_rc10"), "vigil");
        assert_eq!(map_base("koth_proot_b5b"), "proot");
        assert_eq!(map_base("pl_swiftwater_final1"), "swiftwater");
        assert_eq!(map_base("cp_process"), "process");
    }

    #[test]
    fn a_late_anchor_places_nothing_rather_than_placing_wrongly() {
        // Uploaded two and a half days after the match: no trustworthy offset.
        let l = log(1, Some("pl_upward_f12"), 0, 3000, 3000 + 60 * 3600 + 40 * 60);
        let demo = DemoSpan { demo_id: 1, map: "pl_upward_f12".into(), start_utc: 0.0, duration_s: 99_999.0 };
        assert!(l.upload_delay_s() > MAX_UPLOAD_DELAY_S);
        assert!(link(&[demo], &[l]).is_empty());
    }

    fn demo(map: &str, start: f64, len: f64) -> DemoSpan {
        DemoSpan { demo_id: 1, map: map.into(), start_utc: start, duration_s: len }
    }

    #[test]
    fn the_overlap_threshold_is_inclusive_at_half_of_the_shorter_span() {
        // Log on the real clock: 1000..2000 (offset 0, uploaded 5 s after).
        let l = log(7, Some("pl_upward_f12"), 1000, 2000, 2005);
        // A 1000 s demo starting at 1500 overlaps 500 s: half of the shorter (both 1000 s).
        assert_eq!(link(&[demo("pl_upward_f12", 1500.0, 1000.0)], &[l.clone()]).len(), 1);
        // One second less overlap: no link.
        assert!(link(&[demo("pl_upward_f12", 1501.0, 1000.0)], &[l.clone()]).is_empty());
        // Touching but not overlapping.
        assert!(link(&[demo("pl_upward_f12", 2000.0, 1000.0)], &[l.clone()]).is_empty());
        assert!(link(&[demo("pl_upward_f12", 0.0, 1000.0)], &[l]).is_empty());
    }

    #[test]
    fn a_short_demo_inside_a_long_match_is_judged_against_itself() {
        let l = log(7, Some("pl_upward_f12"), 0, 3600, 3610);
        let links = link(&[demo("pl_upward_f12", 1000.0, 600.0)], &[l]);
        assert_eq!(links.len(), 1);
        // But it holds only a sixth of the match.
        assert_eq!(links[0].log_share, 0.17);
    }

    #[test]
    fn a_log_with_no_map_needs_a_stricter_overlap() {
        let l = log(7, None, 1000, 2000, 2005);
        // 70% of the shorter: enough with a map, not without one.
        assert!(link(&[demo("anything", 1300.0, 1000.0)], &[l.clone()]).is_empty());
        let links = link(&[demo("anything", 1200.0, 1000.0)], &[l]);
        assert_eq!(links[0].method, "nomap");
    }

    #[test]
    fn empty_and_backwards_spans_never_link() {
        let l = log(7, Some("pl_upward_f12"), 1000, 2000, 2005);
        assert!(link(&[], &[l.clone()]).is_empty());
        assert!(link(&[demo("pl_upward_f12", 1000.0, 1000.0)], &[]).is_empty());
        assert!(link(&[demo("pl_upward_f12", 1000.0, 0.0)], &[l.clone()]).is_empty());
        assert!(link(&[demo("pl_upward_f12", 1000.0, -5.0)], &[l]).is_empty());
        // A log whose end is not after its start.
        let flat = log(8, Some("pl_upward_f12"), 1000, 1000, 1005);
        assert!(link(&[demo("pl_upward_f12", 900.0, 300.0)], &[flat]).is_empty());
    }

    #[test]
    fn the_upload_delay_limit_is_inclusive() {
        let demo = demo("pl_upward_f12", 0.0, 3000.0);
        let on_time = log(1, Some("pl_upward_f12"), 100, 1000, 1000 + MAX_UPLOAD_DELAY_S);
        let late = log(2, Some("pl_upward_f12"), 100, 1000, 1000 + MAX_UPLOAD_DELAY_S + 1);
        assert_eq!(link(&[demo.clone()], &[on_time]).len(), 1);
        assert!(link(&[demo], &[late]).is_empty());
    }

    /// The same raw end time in winter (CET, one hour behind UTC) and summer
    /// (CEST, two hours): the whole-hour floor puts each on the right side,
    /// also when the match crosses midnight on the server.
    #[test]
    fn winter_and_summer_servers_get_their_own_hour() {
        // Raw end 23:58:00 on the server's clock, read as UTC.
        let raw_end = 1_798_588_680_i64 / 86_400 * 86_400 + 23 * 3600 + 58 * 60;
        let winter = log(1, None, raw_end - 1800, raw_end, raw_end + 3600 + 25);
        let summer = log(2, None, raw_end - 1800, raw_end, raw_end + 7200 + 25);
        assert_eq!((winter.offset_s(), winter.upload_delay_s()), (3600, 25));
        assert_eq!((summer.offset_s(), summer.upload_delay_s()), (7200, 25));
        // A server already on UTC.
        let utc = log(3, None, raw_end - 1800, raw_end, raw_end + 25);
        assert_eq!((utc.offset_s(), utc.upload_delay_s()), (0, 25));
        // An upload a second after the match, with a server two hours behind,
        // crossing midnight in UTC: still the right hour.
        let crossing = log(4, None, raw_end - 1800, raw_end, raw_end + 7200 + 2);
        assert_eq!(crossing.end_utc() - raw_end, 7200);
        assert_eq!(crossing.start_utc() - (raw_end - 1800), 7200);
    }

    #[test]
    fn a_demo_is_matched_to_a_log_in_the_other_time_zone_only_by_the_real_clock() {
        // The same raw times, two servers an hour apart. The demo was recorded
        // at the real time of the CET one; it must not link to the CEST one.
        let cet = log(1, Some("pl_upward_f12"), 1000, 2000, 2000 + 3600 + 20);
        let cest = log(2, Some("pl_upward_f12"), 1000, 2000, 2000 + 7200 + 20);
        let links = link(&[demo("pl_upward_f12", 1000.0 + 3600.0, 1000.0)], &[cet, cest]);
        assert_eq!(links.iter().map(|l| l.log_id).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn map_matching_edges() {
        assert_eq!(map_match("PL_UPWARD_F12", Some("pl_upward_f12")), Some("exact"));
        // A different version of the same map is a label match, not exact.
        assert_eq!(map_match("pl_upward_f10", Some("pl_upward_f12")), Some("label"));
        assert_eq!(map_match("pl_upward_f10", Some("")), None);
        // A demo with no map matches nothing that names one.
        assert_eq!(map_match("", Some("pl_upward_f12")), None);
    }

    #[test]
    fn ticks_at_the_edges_of_the_recording() {
        // Exactly at the start and at the last tick: inside. One past: outside.
        assert_eq!(tick_for(1000, 0, 1000.0, 66.0, 6600, 0.0), Some(0));
        assert_eq!(tick_for(1100, 0, 1000.0, 66.0, 6600, 0.0), Some(6600));
        assert_eq!(tick_for(1101, 0, 1000.0, 66.0, 6600, 0.0), None);
        // The log's offset moves the moment into the demo's clock.
        assert_eq!(tick_for(1000 - 7200, 7200, 1000.0, 66.0, 6600, 0.0), Some(0));
        // A demo with no ticks holds only tick 0.
        assert_eq!(tick_for(1000, 0, 1000.0, 66.0, 0, 0.0), Some(0));
        assert_eq!(tick_for(1001, 0, 1000.0, 66.0, 0, 0.0), None);
    }

    #[test]
    fn ticks_rewind_by_the_lead_but_stay_in_the_demo() {
        // Event 120 s into the recording at 66.67 tick/s, 5 s lead.
        let t = tick_for(1120, 0, 1000.0, 66.67, 100_000, 5.0).unwrap();
        assert_eq!(t, (115.0_f64 * 66.67).round() as i64);
        // Before the recording started: no jump.
        assert!(tick_for(900, 0, 1000.0, 66.67, 100_000, 5.0).is_none());
        // Near the start, the lead is clamped at tick 0.
        assert_eq!(tick_for(1002, 0, 1000.0, 66.67, 100_000, 5.0), Some(0));
    }
}
