//! The tournaments the owner played, for the match list's dividers (Flashy:
//! "a timeline separation where it separates timelines by the tournaments
//! played ... it kinda gets lost in the sea of other logs"). A season's
//! group stages and playoffs are one tournament: ETF2L names them
//! "Highlander Season 33 (Spring 2025): Group Stage #1" and so on, and the
//! part before the colon is the tournament.

use anyhow::Result;
use hl_db::Db;
use serde::Serialize;
use std::collections::BTreeMap;

/// A week before the first official: the scrims that prepared for it.
const BEFORE_S: i64 = 7 * 86_400;
/// The day after the last.
const AFTER_S: i64 = 86_400;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tournament {
    pub name: String,
    /// "season", "cup" or "other".
    pub kind: String,
    pub division: Option<String>,
    /// First and last official, unix seconds.
    pub first: i64,
    pub last: i64,
    /// The stretch of the match list it heads: a week before the first
    /// official to the day after the last.
    pub from: i64,
    pub to: i64,
    pub officials: i64,
}

pub async fn mine(db: &Db) -> Result<Vec<Tournament>> {
    let mut by: BTreeMap<String, Tournament> = BTreeMap::new();
    for c in db.my_competitions().await? {
        let name = c.competition.split(':').next().unwrap_or(&c.competition).trim().to_string();
        let kind = match c.category.as_deref() {
            Some(k) if k.to_ascii_lowercase().contains("season") => "season",
            Some(k) if k.to_ascii_lowercase().contains("cup") => "cup",
            _ if name.to_ascii_lowercase().contains("cup") => "cup",
            _ => "other",
        };
        let t = by.entry(name.clone()).or_insert_with(|| Tournament {
            name,
            kind: kind.to_string(),
            division: None,
            first: c.first,
            last: c.last,
            from: 0,
            to: 0,
            officials: 0,
        });
        t.first = t.first.min(c.first);
        t.last = t.last.max(c.last);
        t.officials += c.officials;
        // The division of the regular stage; playoffs name none.
        if t.division.is_none() {
            t.division = c.division.filter(|d| !d.is_empty());
        }
    }
    let mut out: Vec<Tournament> = by
        .into_values()
        .map(|mut t| {
            t.from = t.first - BEFORE_S;
            t.to = t.last + AFTER_S;
            t
        })
        .collect();
    out.sort_by_key(|t| std::cmp::Reverse(t.first));
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_seasons_stages_are_one_tournament() {
        let names = ["Highlander Season 33 (Spring 2025): Group Stage #1", "Highlander Season 33 (Spring 2025): Low Playoffs", "Highlander Experimental Cup #10"];
        let base: Vec<&str> = names.iter().map(|n| n.split(':').next().unwrap().trim()).collect();
        assert_eq!(base[0], base[1]);
        assert_eq!(base[2], "Highlander Experimental Cup #10");
    }
}
