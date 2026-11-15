//! Every active ETF2L ban, for a "Banned" tag beside the player's name
//! wherever the app shows one (Flashy: "we need to add etf2l bans and have
//! them applied if they have them active"; the effect chosen: a badge
//! everywhere, nothing taken out of ratings or awards).
//!
//! ETF2L's `GET /bans?status=active` lists them, a hundred a page, each with
//! the player's SteamID, start, end and reason. Read at most once a
//! [`KEEP_S`], kept in one setting; a ban whose end has passed since is
//! dropped when read.

use crate::sources::Sources;
use anyhow::Result;
use hl_core::SteamId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const SETTING: &str = "etf2l_bans";
/// How long the list is trusted before it is read again.
pub const KEEP_S: i64 = 24 * 3600;
/// ETF2L's "forever": an end at the 32-bit limit.
const FOREVER: i64 = 4_294_967_295;
/// Pages read at the most: about 4,000 bans in all, active a fraction.
const MAX_PAGES: i64 = 60;

/// One active ban.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ban {
    pub account_id: u32,
    pub start: Option<i64>,
    /// `None` for a permanent ban.
    pub end: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    fetched_at: i64,
    bans: Vec<Ban>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The active bans on one page of `/bans`, and its last page number.
pub fn parse_page(json: &str, at: i64) -> (Vec<Ban>, i64) {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return (Vec::new(), 0) };
    let page = &v["bans"];
    let last = page["last_page"].as_i64().unwrap_or(1);
    let bans = page["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["expired"].as_bool() != Some(true))
        .filter_map(|b| {
            let sid = b["steamid64"].as_str().or_else(|| b["steamid"].as_str())?;
            let account_id = SteamId::parse(sid).ok()?.account_id();
            let start = b["start"].as_i64();
            let end = b["end"].as_i64();
            // A ban reverted (its end before its start) or already over is not one.
            if end.is_some_and(|e| e <= at || start.is_some_and(|s| e < s)) {
                return None;
            }
            let reason = b["reason"].as_str().map(str::trim).filter(|r| !r.is_empty()).map(str::to_string);
            Some(Ban { account_id, start, end: end.filter(|e| *e < FOREVER), reason })
        })
        .collect();
    (bans, last)
}

/// Read every active ban from ETF2L and keep them. Their count.
pub async fn refresh(db: &hl_db::Db, sources: &Sources) -> Result<usize> {
    let at = now();
    let mut all: Vec<Ban> = Vec::new();
    let mut page = 1;
    // ETF2L answered page 2 with a 500 on 15 Nov 2026, and the `?` threw away
    // page 1 with it: no Banned tag anywhere, and the next try from scratch.
    // A later page failing keeps what was read, merged with the last list.
    let mut partial = false;
    loop {
        let body = match sources.etf2l_page("/bans?status=active", page).await {
            Ok(Some(b)) => b,
            Ok(None) => break,
            Err(e) if page > 1 => {
                tracing::warn!(page, error = %format!("{e:#}"), "ETF2L bans: a page failed; keeping what was read");
                partial = true;
                break;
            }
            Err(e) => return Err(e),
        };
        let (bans, last) = parse_page(&body, at);
        all.extend(bans);
        if page >= last || page >= MAX_PAGES || sources.etf2l_resting() {
            break;
        }
        page += 1;
    }
    let before: Stored = db.get_setting(SETTING).await?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let fetched_at = if partial {
        // The pages not read keep last time's bans, and the list stays due.
        all.extend(before.bans.iter().cloned());
        before.fetched_at
    } else {
        at
    };
    all.sort_by_key(|b| b.account_id);
    all.dedup_by(|a, b| a.account_id == b.account_id && a.end == b.end);
    db.set_setting(SETTING, &serde_json::to_string(&Stored { fetched_at, bans: all.clone() })?).await?;
    tracing::info!(bans = all.len(), pages = page, partial, "ETF2L bans read");
    Ok(all.len())
}

/// The bans still active now, and whether the list is due to be read again.
pub async fn active(db: &hl_db::Db) -> Result<(Vec<Ban>, bool)> {
    let stored: Stored = db.get_setting(SETTING).await?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let at = now();
    let stale = at - stored.fetched_at > KEEP_S;
    Ok((stored.bans.into_iter().filter(|b| b.end.is_none_or(|e| e > at)).collect(), stale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_of_bans_keeps_the_active_ones() {
        let json = r#"{"bans":{"last_page":3,"data":[
            {"start":100,"end":4294967295,"steamid":"U:1:896222182","steamid64":"76561198856487910","expired":false,"reason":"Cheating"},
            {"start":100,"end":5000,"steamid64":"76561197960265729","expired":false,"reason":"Toxicity "},
            {"start":100,"end":900,"steamid64":"76561197960265730","expired":false,"reason":"over"},
            {"start":100,"end":50,"steamid64":"76561197960265731","expired":false,"reason":"Reverted"},
            {"start":100,"end":6000,"steamid64":"76561197960265732","expired":true,"reason":"expired"}
        ]}}"#;
        let (bans, last) = parse_page(json, 1000);
        assert_eq!(last, 3);
        assert_eq!(bans.len(), 2, "{bans:?}");
        assert_eq!((bans[0].account_id, bans[0].end, bans[0].reason.as_deref()), (896222182, None, Some("Cheating")), "permanent");
        assert_eq!((bans[1].account_id, bans[1].end, bans[1].reason.as_deref()), (1, Some(5000), Some("Toxicity")));
    }
}
