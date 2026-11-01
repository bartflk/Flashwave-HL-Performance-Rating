//! ETF2L's other Highlander tournaments, as a section of their own beside
//! the seasons (Flashy: "a section for 1 day cups and smaller tournaments
//! added as a side thing ... maybe a tournaments tab so you could explore
//! them since seasons are the main"): the one-night cups, the Experimental
//! Cups, the Nations' Cups, the Highlander Open and the preseason cups
//! (Flashy: "preseason cups under tournaments").
//!
//! A tournament is every competition ETF2L files under one name: "Highlander
//! Experimental Cup #10" holds its group stage and each bracket's Grand
//! Final and 3rd place match as competitions of their own. Its brackets are
//! the matches' divisions (Mid, Open; a one-night cup's Werewolves, Zombies)
//! or, without one, the competition's own label ("Playoffs"). The winners
//! are whoever won a bracket's Grand Final.

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use hl_db::{Db, ResultRow, TeamRef};
use serde::Serialize;

use crate::leagues::parse_tournament;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tournament {
    pub name: String,
    /// `preseason`, `one-night`, `experimental`, `nations`, `open` or `other`.
    pub kind: &'static str,
    pub first: Option<i64>,
    pub last: Option<i64>,
    pub matches: usize,
    pub teams: usize,
    /// Each bracket's name, in the order its first match was played.
    pub brackets: Vec<String>,
    pub winners: Vec<CupWinner>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CupWinner {
    pub bracket: String,
    pub team: TeamRef,
}

/// One tournament with all its matches, oldest first, each with its bracket.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TournamentPage {
    #[serde(flatten)]
    pub head: Tournament,
    pub results: Vec<CupResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CupResult {
    #[serde(flatten)]
    pub row: ResultRow,
    pub bracket: String,
}

/// What kind of tournament a name is.
pub fn kind_of(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.contains("preseason") {
        "preseason"
    } else if n.contains("nations") {
        "nations"
    } else if n.contains("experimental") {
        "experimental"
    } else if n.contains("one-night") || n.contains("one night") || n.contains("1 day") || n.contains("one-day") {
        "one-night"
    } else if n.contains("highlander open") {
        "open"
    } else {
        "other"
    }
}

/// A match's bracket: its division, or its competition's own label.
fn bracket_of(r: &ResultRow) -> String {
    if let Some(d) = r.division.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        return d.to_string();
    }
    let label = r.competition.split_once(':').or_else(|| r.competition.split_once(" - ")).map_or("", |(_, l)| l.trim());
    if label.is_empty() { "Main".to_string() } else { label.to_string() }
}

fn is_final(round: Option<&str>) -> bool {
    let r = round.unwrap_or("").to_ascii_lowercase();
    r.starts_with("grand final") || r.starts_with("grand-final") || r == "final" || r == "finals"
}

/// Who won a played match, if anyone.
fn winner(r: &ResultRow) -> Option<&TeamRef> {
    match (r.r1, r.r2) {
        (Some(a), Some(b)) if a > b => Some(&r.clan1),
        (Some(a), Some(b)) if b > a => Some(&r.clan2),
        _ => None,
    }
}

/// Every tournament's matches, grouped by tournament, each group oldest first.
fn group(rows: Vec<ResultRow>) -> Vec<(String, Vec<CupResult>)> {
    let mut by: HashMap<String, Vec<CupResult>> = HashMap::new();
    for row in rows {
        let (name, _) = parse_tournament(&row.competition);
        if name.is_empty() {
            continue;
        }
        let bracket = bracket_of(&row);
        by.entry(name).or_default().push(CupResult { row, bracket });
    }
    by.into_iter().collect()
}

fn summarise(name: String, results: &[CupResult]) -> Tournament {
    let times: Vec<i64> = results.iter().map(|r| r.row.time).filter(|&t| t > 0).collect();
    let teams: BTreeSet<i64> = results.iter().flat_map(|r| [r.row.clan1.id, r.row.clan2.id]).collect();
    let mut brackets: Vec<String> = Vec::new();
    for r in results {
        if !brackets.contains(&r.bracket) {
            brackets.push(r.bracket.clone());
        }
    }
    // The last Grand Final of each bracket.
    let mut winners: Vec<CupWinner> = Vec::new();
    for r in results.iter().rev().filter(|r| is_final(r.row.round.as_deref())) {
        if winners.iter().any(|w| w.bracket == r.bracket) {
            continue;
        }
        if let Some(team) = winner(&r.row) {
            winners.push(CupWinner { bracket: r.bracket.clone(), team: team.clone() });
        }
    }
    winners.sort_by_key(|w| brackets.iter().position(|b| *b == w.bracket));
    Tournament {
        kind: kind_of(&name),
        name,
        first: times.iter().min().copied(),
        last: times.iter().max().copied(),
        matches: results.len(),
        teams: teams.len(),
        brackets,
        winners,
    }
}

/// Every tournament the league data holds, newest first.
pub async fn list(db: &Db) -> Result<Vec<Tournament>> {
    let mut out: Vec<Tournament> = group(db.cup_results().await?).into_iter().map(|(name, rs)| summarise(name, &rs)).collect();
    out.sort_by(|a, b| b.last.cmp(&a.last).then(a.name.cmp(&b.name)));
    Ok(out)
}

/// One tournament by its name.
pub async fn page(db: &Db, name: &str) -> Result<Option<TournamentPage>> {
    Ok(group(db.cup_results().await?)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(n, results)| TournamentPage { head: summarise(n, &results), results }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, time: i64, competition: &str, division: Option<&str>, round: &str, a: (i64, i64), score: (i64, i64)) -> ResultRow {
        let team = |id: i64| TeamRef { id, name: format!("T{id}"), avatar: None };
        ResultRow {
            match_id: id,
            time,
            season: 36,
            division: division.map(str::to_string),
            tier: None,
            round: Some(round.to_string()),
            week: None,
            stage: "Cup".into(),
            competition: competition.into(),
            clan1: team(a.0),
            clan2: team(a.1),
            r1: Some(score.0),
            r2: Some(score.1),
            default_win: false,
        }
    }

    #[test]
    fn a_cups_competitions_are_one_tournament_with_each_brackets_winner() {
        let rows = vec![
            row(1, 10, "Highlander Experimental Cup #10", Some("Mid"), "Round 1", (1, 2), (2, 1)),
            row(2, 11, "Highlander Experimental Cup #10", Some("Open"), "Round 1", (3, 4), (0, 2)),
            row(3, 20, "Highlander Experimental Cup #10: Mid Grand Final", Some("Mid"), "Grand Final", (1, 5), (1, 3)),
            row(4, 21, "Highlander Experimental Cup #10: Open Grand Final", Some("Open"), "Grand Final", (4, 6), (3, 0)),
            row(5, 30, "High-Tide Highlander One-Night-Cup", Some("Tuna"), "Grand Final", (7, 8), (2, 0)),
            row(6, 40, "Highlander Nations Cup #9: Playoffs", None, "Grand-Finals", (9, 10), (0, 2)),
        ];
        let mut t: Vec<Tournament> = group(rows).into_iter().map(|(n, r)| summarise(n, &r)).collect();
        t.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<&str> = t.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["High-Tide Highlander One-Night-Cup", "Highlander Experimental Cup #10", "Highlander Nations Cup #9"]);
        let ec = &t[1];
        assert_eq!((ec.kind, ec.matches, ec.teams, ec.first, ec.last), ("experimental", 4, 6, Some(10), Some(21)));
        assert_eq!(ec.brackets, ["Mid", "Open"]);
        let w: Vec<(&str, i64)> = ec.winners.iter().map(|w| (w.bracket.as_str(), w.team.id)).collect();
        assert_eq!(w, [("Mid", 5), ("Open", 4)]);
        assert_eq!(t[0].kind, "one-night");
        assert_eq!((t[2].kind, t[2].brackets[0].as_str(), t[2].winners[0].team.id), ("nations", "Playoffs", 10));
        assert_eq!(kind_of("Highlander Season 25 Preseason Cup"), "preseason");
    }
}
