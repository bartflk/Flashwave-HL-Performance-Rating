//! What the app is doing, as it does it (Flashy: "the syncs to tell more
//! than reading etf2l 6v6 seasons ... a waterfall 3/4 lines high that fades
//! out, also logging all these things + api statuses and other debug
//! options").
//!
//! Two things, both in memory for this run only (the log file in the data
//! folder keeps the lines across runs):
//!
//! - **The feed**: the latest [`CAP`] lines -- everything the backend logs,
//!   and one line per web request -- numbered, so the window asks for what
//!   is new since the last line it has.
//! - **The sites**: per host, how its requests went -- answered, unchanged
//!   (304), not found, failed -- how long they took, the rate limit it last
//!   reported, and whether it asked us to wait.

use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lines kept: a long sync writes a few thousand.
pub const CAP: usize = 5_000;

/// Whether debug lines go into the feed too (Settings › Activity log).
static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(on: bool) {
    VERBOSE.store(on, Ordering::Relaxed);
}

pub fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// One line of the feed.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub seq: u64,
    /// Unix time in milliseconds.
    pub at: i64,
    /// `error`, `warn`, `info`, `debug`, or `http` for a web request.
    pub level: String,
    /// Where it came from: a site for a request, else the part of the app
    /// (`league`, `sync`, `database`, ...).
    pub source: String,
    pub text: String,
}

/// How one site's requests have gone this run.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub host: String,
    pub requests: u64,
    pub ok: u64,
    pub not_modified: u64,
    pub not_found: u64,
    pub failed: u64,
    /// The last answer: an HTTP status, or `error`.
    pub last_status: Option<String>,
    pub last_at: Option<i64>,
    pub last_path: Option<String>,
    /// Average time to an answer, in milliseconds.
    pub avg_ms: f64,
    /// What its `RateLimit-Remaining` last said, where it sends one.
    pub rate_remaining: Option<u64>,
    /// Until when it asked us to stop (unix ms): nothing is sent before.
    pub resting_until: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Default)]
struct State {
    next: u64,
    lines: VecDeque<Line>,
    sites: BTreeMap<String, Site>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { next: 1, ..Default::default() }))
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Add a line to the feed, with any URL's query values taken out.
pub fn push(level: &str, source: &str, text: impl Into<String>) {
    let text = scrub(&text.into());
    let mut s = state().lock().unwrap_or_else(|p| p.into_inner());
    let seq = s.next;
    s.next += 1;
    s.lines.push_back(Line { seq, at: now_ms(), level: level.to_string(), source: source.to_string(), text });
    while s.lines.len() > CAP {
        s.lines.pop_front();
    }
}

/// The lines after `since` (0 for all kept), oldest first, at most `limit`
/// of the newest.
pub fn since(since: u64, limit: usize) -> Vec<Line> {
    let s = state().lock().unwrap_or_else(|p| p.into_inner());
    let newer: Vec<&Line> = s.lines.iter().filter(|l| l.seq > since).collect();
    newer[newer.len().saturating_sub(limit)..].iter().map(|l| (*l).clone()).collect()
}

/// Every site asked this run, by host.
pub fn sites() -> Vec<Site> {
    let s = state().lock().unwrap_or_else(|p| p.into_inner());
    s.sites.values().cloned().collect()
}

/// Every URL in a line with its query's values taken out (audit M3): a
/// warning or an error carries the whole URL, and a key or a SteamID in it
/// would go wherever the copied log is pasted. Request lines were already
/// cut down to their path by [`host_path`].
pub fn scrub(text: &str) -> String {
    if !text.contains("://") || !text.contains('?') {
        return text.to_string();
    }
    text.split(' ')
        .map(|word| {
            let start = word.find("http://").or_else(|| word.find("https://"));
            let Some(start) = start else { return word.to_string() };
            let end = word[start..].find(['"', '\'', ')', '>', ',', ';', '`']).map_or(word.len(), |e| start + e);
            let url = word[start..end].trim_end_matches(['.', ':']);
            match reqwest::Url::parse(url) {
                Ok(u) if u.query().is_some() => {
                    let keys: Vec<String> = u.query_pairs().map(|(k, _)| k.into_owned()).collect();
                    let mut bare = u.clone();
                    bare.set_query(None);
                    word.replacen(url, &format!("{bare}?{}", keys.join("&")), 1)
                }
                _ => word.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A request's host and path, without the query string's values: what the
/// feed shows (no SteamIDs or keys copied into a log someone may share).
pub fn host_path(url: &str) -> (String, String) {
    match reqwest::Url::parse(url) {
        Ok(u) => {
            let host = u.host_str().unwrap_or("?").trim_start_matches("www.").trim_start_matches("api.").to_string();
            let keys: Vec<String> = u.query_pairs().map(|(k, _)| k.into_owned()).collect();
            let path = if keys.is_empty() { u.path().to_string() } else { format!("{}?{}", u.path(), keys.join("&")) };
            (host, path)
        }
        Err(_) => ("?".to_string(), url.to_string()),
    }
}

/// One answered request (or a failed one: `status` `None`).
pub fn request(url: &str, status: Option<u16>, ms: u64, rate_remaining: Option<u64>, error: Option<&str>) {
    let (host, path) = host_path(url);
    let at = now_ms();
    {
        let mut s = state().lock().unwrap_or_else(|p| p.into_inner());
        let site = s.sites.entry(host.clone()).or_insert_with(|| Site { host: host.clone(), ..Default::default() });
        site.avg_ms = (site.avg_ms * site.requests as f64 + ms as f64) / (site.requests + 1) as f64;
        site.requests += 1;
        match status {
            Some(304) => site.not_modified += 1,
            Some(404) => site.not_found += 1,
            Some(c) if (200..300).contains(&c) => site.ok += 1,
            _ => site.failed += 1,
        }
        site.last_status = Some(status.map_or_else(|| "error".to_string(), |c| c.to_string()));
        site.last_at = Some(at);
        site.last_path = Some(path.clone());
        if rate_remaining.is_some() {
            site.rate_remaining = rate_remaining;
        }
        if let Some(e) = error {
            site.last_error = Some(e.to_string());
        }
    }
    let what = match status {
        Some(c) => c.to_string(),
        None => format!("failed: {}", error.unwrap_or("no answer")),
    };
    push("http", &host, format!("{path} · {what} · {ms} ms"));
}

/// A site asked us to wait until `until_ms`.
pub fn resting(url: &str, until_ms: i64) {
    let (host, _) = host_path(url);
    {
        let mut s = state().lock().unwrap_or_else(|p| p.into_inner());
        let site = s.sites.entry(host.clone()).or_insert_with(|| Site { host: host.clone(), ..Default::default() });
        site.resting_until = Some(until_ms);
    }
    let mins = ((until_ms - now_ms()).max(0) + 59_999) / 60_000;
    push("warn", &host, format!("asked us to wait: nothing more is sent for {mins} min"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_line_names_the_site_and_path_but_not_the_query_s_values() {
        assert_eq!(host_path("https://api.etf2l.org/player/123?steamid=7656119&limit=5"), ("etf2l.org".into(), "/player/123?steamid&limit".into()));
        assert_eq!(host_path("https://logs.tf/api/v1/log/42"), ("logs.tf".into(), "/api/v1/log/42".into()));
    }

    #[test]
    fn a_url_anywhere_in_a_line_loses_its_query_values() {
        let line = "error sending request for url (https://api.steampowered.com/x/?key=SECRET&steamids=76561198000000001): timed out";
        let s = scrub(line);
        assert!(!s.contains("SECRET") && !s.contains("76561198000000001"), "{s}");
        assert!(s.contains("https://api.steampowered.com/x/?key&steamids): timed out"), "{s}");
        assert_eq!(scrub("https://logs.tf/api/v1/log/42: HTTP 404"), "https://logs.tf/api/v1/log/42: HTTP 404", "no query, no change");
        assert_eq!(scrub("plain words? none"), "plain words? none");
    }

    #[test]
    fn the_feed_returns_what_is_new_and_the_sites_count_their_answers() {
        let before = since(0, CAP).last().map_or(0, |l| l.seq);
        request("https://feedtest.example/a", Some(200), 100, Some(4), None);
        request("https://feedtest.example/b", Some(304), 300, None, None);
        request("https://feedtest.example/c", None, 50, None, Some("timed out"));
        let new = since(before, CAP);
        assert!(new.iter().any(|l| l.level == "http" && l.text == "/b · 304 · 300 ms"));
        let site = sites().into_iter().find(|s| s.host == "feedtest.example").unwrap();
        assert_eq!((site.requests, site.ok, site.not_modified, site.failed), (3, 1, 1, 1));
        assert_eq!(site.avg_ms, 150.0);
        assert_eq!(site.rate_remaining, Some(4), "kept until a site says otherwise");
        assert_eq!(site.last_error.as_deref(), Some("timed out"));
        assert_eq!(since(before, 1).len(), 1, "the newest only");
    }
}
