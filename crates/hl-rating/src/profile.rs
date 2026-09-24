//! A player's profile on one class, built from their rated history.

use crate::model::{Component, Rating};
use hl_core::TfClass;
use serde::Serialize;
use std::collections::HashMap;

/// Recent form is the average over this many games.
pub const FORM_WINDOW: usize = 20;
/// The trend line is a rolling average over this many games.
pub const ROLLING_WINDOW: usize = 10;
/// Best and worst lists are this long.
const HIGHLIGHTS: usize = 5;

/// One rated game, as stored.
#[derive(Debug, Clone)]
pub struct HistoryRow {
    pub log_id: i64,
    pub played_at: Option<i64>,
    pub map: Option<String>,
    pub title: Option<String>,
    pub league: Option<String>,
    /// `official`, `scrim` or `pug`.
    pub kind: Option<String>,
    pub result: Option<String>,
    pub rating: Rating,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub class: TfClass,
    pub games: usize,
    pub career_avg: f64,
    /// Average over the last `FORM_WINDOW` games.
    pub form_avg: f64,
    /// Average over the `FORM_WINDOW` games before that; `None` without enough history.
    pub prev_form_avg: Option<f64>,
    pub win_rate: Option<f64>,
    /// Chronological, oldest first.
    pub trend: Vec<TrendPoint>,
    pub components: Vec<ComponentSummary>,
    pub best: Vec<GameRef>,
    pub worst: Vec<GameRef>,
    pub recent: Vec<GameRef>,
    pub form_window: usize,
    pub rolling_window: usize,
    /// Class-specific totals the caller fills in (the sniper duel record, ...).
    pub extras: Vec<Extra>,
    /// The same class split by kind of game, always over every game, so the
    /// split stays visible while the rest of the profile is filtered.
    pub contexts: Vec<ContextSplit>,
    /// The kind of game the profile is filtered to, if any.
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSplit {
    pub kind: String,
    pub games: usize,
    pub avg: f64,
    pub win_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub log_id: i64,
    pub played_at: Option<i64>,
    pub map: Option<String>,
    pub score: f64,
    /// Rolling average ending at this game; `None` until the window fills.
    pub rolling: Option<f64>,
    pub result: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentSummary {
    pub component: Component,
    pub label: String,
    pub unit: String,
    pub weight: f64,
    /// Average percentile over recent form.
    pub form_pct: f64,
    pub career_pct: f64,
    /// Average raw value over recent form, in `unit`.
    pub form_raw: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRef {
    pub log_id: i64,
    pub played_at: Option<i64>,
    pub map: Option<String>,
    pub title: Option<String>,
    pub league: Option<String>,
    pub kind: Option<String>,
    pub result: Option<String>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Extra {
    pub label: String,
    pub value: String,
    /// A short second line under the value.
    pub detail: Option<String>,
    pub hint: Option<String>,
}

/// `rows` in any order; `None` when there is nothing rated.
pub fn build(class: TfClass, mut rows: Vec<HistoryRow>) -> Option<Profile> {
    if rows.is_empty() {
        return None;
    }
    rows.sort_by_key(|r| (r.played_at.unwrap_or(0), r.log_id));
    let n = rows.len();

    let scores: Vec<f64> = rows.iter().map(|r| r.rating.score).collect();
    let recent_start = n.saturating_sub(FORM_WINDOW);
    let form = &rows[recent_start..];
    let prev = (recent_start >= FORM_WINDOW).then(|| &rows[recent_start - FORM_WINDOW..recent_start]);

    let trend = rows
        .iter()
        .enumerate()
        .map(|(i, r)| TrendPoint {
            log_id: r.log_id,
            played_at: r.played_at,
            map: r.map.clone(),
            score: r.rating.score,
            rolling: (i + 1 >= ROLLING_WINDOW)
                .then(|| round2(mean(&scores[i + 1 - ROLLING_WINDOW..=i]))),
            result: r.result.clone(),
            kind: r.kind.clone(),
        })
        .collect();

    let win_rate = win_rate(&rows);

    let mut by_score: Vec<&HistoryRow> = rows.iter().collect();
    by_score.sort_by(|a, b| b.rating.score.total_cmp(&a.rating.score));

    Some(Profile {
        class,
        games: n,
        career_avg: round2(mean(&scores)),
        form_avg: round2(mean(&form.iter().map(|r| r.rating.score).collect::<Vec<_>>())),
        prev_form_avg: prev.map(|p| round2(mean(&p.iter().map(|r| r.rating.score).collect::<Vec<_>>()))),
        win_rate,
        trend,
        components: components(&rows, form),
        best: by_score.iter().take(HIGHLIGHTS).map(|r| game_ref(r)).collect(),
        worst: by_score.iter().rev().take(HIGHLIGHTS).map(|r| game_ref(r)).collect(),
        recent: rows.iter().rev().take(10).map(game_ref).collect(),
        form_window: FORM_WINDOW,
        rolling_window: ROLLING_WINDOW,
        extras: Vec::new(),
        contexts: Vec::new(),
        filter: None,
    })
}

/// Ties do not count towards a win rate; `None` with no decided games.
fn win_rate(rows: &[HistoryRow]) -> Option<f64> {
    let decided = rows.iter().filter(|r| matches!(r.result.as_deref(), Some("W" | "L"))).count();
    let won = rows.iter().filter(|r| r.result.as_deref() == Some("W")).count();
    (decided > 0).then(|| round1(won as f64 / decided as f64 * 100.0))
}

/// Games, average and win rate per kind of game, in a fixed order.
pub fn context_splits(rows: &[HistoryRow]) -> Vec<ContextSplit> {
    ["official", "scrim", "pug"]
        .into_iter()
        .filter_map(|kind| {
            let of: Vec<HistoryRow> = rows.iter().filter(|r| r.kind.as_deref() == Some(kind)).cloned().collect();
            (!of.is_empty()).then(|| ContextSplit {
                kind: kind.to_string(),
                games: of.len(),
                avg: round2(mean(&of.iter().map(|r| r.rating.score).collect::<Vec<_>>())),
                win_rate: win_rate(&of),
            })
        })
        .collect()
}

/// Per component: where the player sits recently and over their career.
/// Ordered by weight, so the parts that move the rating most come first.
fn components(all: &[HistoryRow], form: &[HistoryRow]) -> Vec<ComponentSummary> {
    struct Acc {
        label: String,
        unit: String,
        weight_sum: f64,
        career: Vec<f64>,
        form: Vec<f64>,
        form_raw: Vec<f64>,
    }
    let form_ids: std::collections::HashSet<i64> = form.iter().map(|r| r.log_id).collect();
    let mut acc: HashMap<Component, Acc> = HashMap::new();

    for r in all {
        for p in &r.rating.parts {
            let a = acc.entry(p.component).or_insert_with(|| Acc {
                label: p.label.clone(),
                unit: p.unit.clone(),
                weight_sum: 0.0,
                career: Vec::new(),
                form: Vec::new(),
                form_raw: Vec::new(),
            });
            a.weight_sum += p.weight;
            a.career.push(p.percentile);
            if form_ids.contains(&r.log_id) {
                a.form.push(p.percentile);
                a.form_raw.push(p.raw);
            }
        }
    }

    let mut out: Vec<ComponentSummary> = acc
        .into_iter()
        .map(|(c, a)| ComponentSummary {
            component: c,
            weight: round2(a.weight_sum / a.career.len() as f64),
            career_pct: round1(mean(&a.career)),
            form_pct: round1(if a.form.is_empty() { mean(&a.career) } else { mean(&a.form) }),
            form_raw: round2(mean(&a.form_raw)),
            label: a.label,
            unit: a.unit,
        })
        .collect();
    out.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    out
}

fn game_ref(r: &HistoryRow) -> GameRef {
    GameRef {
        log_id: r.log_id,
        played_at: r.played_at,
        map: r.map.clone(),
        title: r.title.clone(),
        league: r.league.clone(),
        kind: r.kind.clone(),
        result: r.result.clone(),
        score: r.rating.score,
    }
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Part;

    fn row(i: i64, score: f64, result: &str) -> HistoryRow {
        HistoryRow {
            log_id: i,
            played_at: Some(1_000 + i),
            map: None,
            title: None,
            league: None,
            kind: Some(if i % 2 == 0 { "scrim" } else { "pug" }.to_string()),
            result: Some(result.to_string()),
            rating: Rating {
                class: TfClass::Sniper,
                score,
                minutes: 30.0,
                parts: vec![Part {
                    component: Component::Duel,
                    label: "Sniper duel".into(),
                    unit: "per 10 min".into(),
                    raw: 1.0,
                    percentile: score,
                    weight: 1.0,
                }],
            },
        }
    }

    #[test]
    fn form_is_the_most_recent_games_even_when_given_out_of_order() {
        // 30 games: the first 10 score 20, the last 20 score 80. Reversed input.
        let rows: Vec<_> = (0..30).rev().map(|i| row(i, if i < 10 { 20.0 } else { 80.0 }, "W")).collect();
        let p = build(TfClass::Sniper, rows).unwrap();
        assert_eq!(p.form_avg, 80.0);
        assert_eq!(p.career_avg, 60.0);
        assert!(p.prev_form_avg.is_none(), "30 games cannot fill two 20-game windows");
    }

    #[test]
    fn rolling_average_waits_for_a_full_window() {
        let rows: Vec<_> = (0..12).map(|i| row(i, 50.0, "W")).collect();
        let p = build(TfClass::Sniper, rows).unwrap();
        assert!(p.trend[ROLLING_WINDOW - 2].rolling.is_none());
        assert_eq!(p.trend[ROLLING_WINDOW - 1].rolling, Some(50.0));
    }

    #[test]
    fn ties_do_not_count_towards_win_rate() {
        let rows = vec![row(1, 50.0, "W"), row(2, 50.0, "L"), row(3, 50.0, "T")];
        assert_eq!(build(TfClass::Sniper, rows).unwrap().win_rate, Some(50.0));
    }

    #[test]
    fn splits_by_kind_in_a_fixed_order() {
        let rows = vec![row(1, 40.0, "L"), row(2, 80.0, "W"), row(3, 60.0, "W"), row(4, 60.0, "T")];
        let s = context_splits(&rows);
        assert_eq!(s.iter().map(|c| c.kind.as_str()).collect::<Vec<_>>(), ["scrim", "pug"]);
        assert_eq!((s[0].games, s[0].avg, s[0].win_rate), (2, 70.0, Some(100.0)));
        assert_eq!((s[1].games, s[1].avg, s[1].win_rate), (2, 50.0, Some(50.0)));
    }

    #[test]
    fn best_and_worst_are_ordered() {
        let rows: Vec<_> = (0..8).map(|i| row(i, i as f64 * 10.0, "W")).collect();
        let p = build(TfClass::Sniper, rows).unwrap();
        assert_eq!(p.best[0].score, 70.0);
        assert_eq!(p.worst[0].score, 0.0);
    }
}
