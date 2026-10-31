//! Career numbers from trends.tf (Q37, Flashy; PLAN §26).
//!
//! trends.tf's player page holds what the app cannot work out without every
//! log a player ever played: their W-L and winrate in ETF2L Highlander
//! officials, per class with damage per minute, accuracy and hours, their
//! aliases and their teams. Officials only, as the ranks are: a scrim or a
//! pug is not their record. It
//! has no JSON API for this, so the page is read as a browser reads it --
//! once, when a profile is opened (never during a search), and kept a day.
//!
//! Read defensively: each table is found by its heading, and one that is
//! not there is left empty rather than failing the rest. A test holds the
//! parser to a saved copy of a real page.

use crate::sources::Sources;
use anyhow::Result;
use hl_core::SteamId;
use hl_db::Db;
use serde::{Deserialize, Serialize};

/// How long a page read is kept.
const KEEP_S: i64 = 24 * 3600;
/// ...and for a player with no game newer than the copy.
const KEEP_QUIET_S: i64 = 7 * 24 * 3600;

/// The page read: Highlander, ETF2L only -- trends.tf counts a log as a
/// league game when it is linked to a league match, so scrims and pugs are
/// left out (Flashy: "only official matches, scrims do not count").
pub fn page_url(steamid64: &str) -> String {
    crate::http::url(&format!("https://trends.tf/player/{steamid64}/"), [("format", "highlander"), ("league", "etf2l")])
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Career {
    /// Highlander games: wins, losses, ties.
    pub wins: u32,
    pub losses: u32,
    pub ties: u32,
    /// Ties count half, as trends.tf counts them; 0-100.
    pub winrate: Option<f64>,
    /// Seconds played in Highlander.
    pub time_s: i64,
    pub classes: Vec<CareerClass>,
    pub aliases: Vec<(String, u32)>,
    pub teams: Vec<CareerTeam>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CareerClass {
    pub class: String,
    pub wins: u32,
    pub losses: u32,
    pub ties: u32,
    pub winrate: Option<f64>,
    /// Damage per minute.
    pub dpm: Option<f64>,
    /// Accuracy, 0-100.
    pub accuracy: Option<f64>,
    pub time_s: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CareerTeam {
    pub league: String,
    pub team: String,
    pub competitions: String,
    pub division: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CareerView {
    pub career: Option<Career>,
    /// Unix seconds the page was read.
    pub fetched_at: Option<i64>,
    /// Why there is nothing, when there is nothing.
    pub error: Option<String>,
    pub url: String,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A player's trends.tf career, from the day's copy or the page itself.
/// trends.tf being down gives the last copy held, or says so.
pub async fn career(db: &Db, sources: &Sources, account: u32) -> Result<CareerView> {
    let steamid64 = SteamId::from_account_id(account).to_steamid64();
    let url = page_url(&steamid64);
    let held = db.trends_career(account).await?;
    // A day, or a week when no game of theirs the app holds is newer than
    // the copy: trends.tf would show the same.
    let keep = match &held {
        Some((_, at)) if db.player_last_game(account).await?.is_none_or(|t| t <= *at) => KEEP_QUIET_S,
        _ => KEEP_S,
    };
    let key = format!("trends-career:{account}");
    let waiting = crate::backoff::waiting(&key);
    if let Some((json, at)) = &held {
        if now() - at < keep || waiting {
            if let Ok(c) = serde_json::from_str::<Career>(json) {
                return Ok(CareerView { career: Some(c), fetched_at: Some(*at), error: None, url });
            }
        }
    }
    if waiting {
        return Ok(CareerView { career: None, fetched_at: None, error: Some("trends.tf could not be read: it failed a moment ago".into()), url });
    }
    let read = match sources.trends_player_page(&steamid64).await {
        Ok(Some(html)) => Ok(parse(&html)),
        Ok(None) => Err("trends.tf has no page for them".to_string()),
        Err(e) => {
            crate::backoff::failed(&key);
            Err(format!("trends.tf could not be read: {e:#}"))
        }
    };
    match read {
        Ok(c) => {
            crate::backoff::worked(&key);
            let at = now();
            db.put_trends_career(account, &serde_json::to_string(&c)?, at).await?;
            Ok(CareerView { career: Some(c), fetched_at: Some(at), error: None, url })
        }
        Err(error) => {
            tracing::info!(account, %error, "trends.tf career");
            let old = held.and_then(|(json, at)| Some((serde_json::from_str::<Career>(&json).ok()?, at)));
            Ok(CareerView { fetched_at: old.as_ref().map(|o| o.1), career: old.map(|o| o.0), error: Some(error), url })
        }
    }
}

/// The page's tables, as far as they can be read.
pub fn parse(html: &str) -> Career {
    let mut c = Career::default();
    for row in table_after(html, "<h3>Formats</h3>") {
        if row.first().is_some_and(|f| f.eq_ignore_ascii_case("highlander")) {
            (c.wins, c.losses, c.ties) = wlt(row.get(1).map_or("", String::as_str));
            c.winrate = percent(row.get(2));
            c.time_s = duration(row.get(3).map_or("", String::as_str));
        }
    }
    for row in table_after(html, "<h3>Classes</h3>") {
        let Some(class) = row.first().and_then(|n| class_key(n)) else { continue };
        let (wins, losses, ties) = wlt(row.get(1).map_or("", String::as_str));
        c.classes.push(CareerClass {
            class: class.to_string(),
            wins,
            losses,
            ties,
            winrate: percent(row.get(2)),
            dpm: row.get(3).and_then(|x| x.trim().parse().ok()),
            accuracy: percent(row.get(4)),
            time_s: duration(row.last().map_or("", String::as_str)),
        });
    }
    c.classes.sort_by_key(|c| std::cmp::Reverse(c.time_s));
    for row in table_after(html, "<h3>Aliases</h3>") {
        if let (Some(name), Some(n)) = (row.first(), row.get(1).and_then(|n| n.parse().ok())) {
            c.aliases.push((name.clone(), n));
        }
    }
    for row in table_after(html, "<h3>Recent Teams</h3>") {
        if row.len() >= 4 {
            c.teams.push(CareerTeam { league: row[0].clone(), team: row[1].clone(), competitions: row[2].clone(), division: row[3].clone() });
        }
    }
    c
}

/// trends.tf's class names to the app's.
fn class_key(name: &str) -> Option<&'static str> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "scout" => "scout",
        "soldier" => "soldier",
        "pyro" => "pyro",
        "demo" | "demoman" => "demoman",
        "heavy" | "heavyweapons" => "heavyweapons",
        "engineer" => "engineer",
        "medic" => "medic",
        "sniper" => "sniper",
        "spy" => "spy",
        _ => return None,
    })
}

/// The body rows of the first table after `heading`, each a list of cell
/// texts. Empty when the heading or its table is not there.
fn table_after(html: &str, heading: &str) -> Vec<Vec<String>> {
    let Some(at) = html.find(heading) else { return Vec::new() };
    let rest = &html[at..];
    let (Some(start), Some(end)) = (rest.find("<tbody"), rest.find("</tbody>")) else { return Vec::new() };
    if end < start {
        return Vec::new();
    }
    let body = &rest[start..end];
    body.split("<tr").skip(1).map(|tr| tr.split("<td").skip(1).map(|td| text(td.split_once('>').map_or("", |x| x.1))).collect()).filter(|r: &Vec<String>| !r.is_empty()).collect()
}

/// A cell's text: tags dropped, entities read, whitespace collapsed.
fn text(cell: &str) -> String {
    let cell = cell.split("</td>").next().unwrap_or(cell);
    let mut out = String::new();
    let mut in_tag = false;
    for ch in cell.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let out = out
        .replace("&#34;", "\"")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn wlt(s: &str) -> (u32, u32, u32) {
    let n: Vec<u32> = s.split('-').filter_map(|x| x.trim().parse().ok()).collect();
    (n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0), n.get(2).copied().unwrap_or(0))
}

fn percent(s: Option<&String>) -> Option<f64> {
    s?.trim().trim_end_matches('%').parse().ok()
}

/// "236:57:32" or "24:45" to seconds.
fn duration(s: &str) -> i64 {
    s.trim().split(':').filter_map(|x| x.parse::<i64>().ok()).fold(0, |acc, x| acc * 60 + x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_real_page() {
        let html = include_str!("../tests/fixtures/trends_player_76561198099396919.html");
        let c = parse(html);
        assert_eq!((c.wins, c.losses, c.ties), (458, 412, 25));
        assert_eq!(c.winrate, Some(52.57));
        assert_eq!(c.time_s, 255 * 3600 + 60 + 56);
        let sniper = &c.classes[0];
        assert_eq!(sniper.class, "sniper");
        assert_eq!((sniper.wins, sniper.losses, sniper.ties), (429, 391, 22));
        assert_eq!(sniper.dpm, Some(345.0));
        assert_eq!(sniper.accuracy, Some(36.0));
        assert_eq!(sniper.time_s, 236 * 3600 + 57 * 60 + 32);
        // Pyro has no W-L on the page, only its time.
        let pyro = c.classes.iter().find(|x| x.class == "pyro").unwrap();
        assert_eq!((pyro.wins, pyro.losses), (0, 0));
        assert_eq!(c.aliases[0], ("flashy".to_string(), 1045));
        assert!(c.aliases.iter().any(|(n, _)| n == "James \"Flashy\" Wilson"));
        assert_eq!(c.teams[0].team, "DD14");
        assert_eq!(c.teams[0].league, "ETF2L");
    }

    #[test]
    fn a_page_without_its_tables_is_empty_not_an_error() {
        assert_eq!(parse("<html><body>maintenance</body></html>"), Career::default());
    }
}
