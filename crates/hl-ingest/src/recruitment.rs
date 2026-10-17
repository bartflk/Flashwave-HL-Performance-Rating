//! ETF2L's recruitment posts for the ETF2L tab (Q74, Flashy: "recruitments
//! would be nice to incorporate"): players looking for a team and teams
//! looking for players, Highlander only.
//!
//! The ETF2L API lists them newest first, twenty a page, filtered by
//! `type=Highlander` (which leaves out fun teams). The first few pages are
//! read at most every half hour and kept in `app_config`; a failed read keeps
//! the last list.

use crate::sources::Sources;
use anyhow::Result;
use hl_db::Db;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const KEY: &str = "etf2l_recruitment";
const FRESH: i64 = 30 * 60;
/// Twenty posts a page: sixty of each kind is weeks of Highlander posts.
const PAGES: u32 = 3;

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecruitPost {
    pub id: i64,
    /// The player's name, or the team's.
    pub name: String,
    /// What the player plays, or what the team is looking for.
    pub classes: Vec<String>,
    pub skill: Option<String>,
    pub comments: u32,
    /// When someone last commented, if anyone has.
    pub last_comment: Option<i64>,
    pub avatar: Option<String>,
    /// A player's Steam account, so the app can open their profile.
    pub account_id: Option<u32>,
    /// A team's ETF2L id, so the app can open its page.
    pub team_id: Option<i64>,
    /// The post on etf2l.org.
    pub link: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recruitment {
    pub players: Vec<RecruitPost>,
    pub teams: Vec<RecruitPost>,
}

/// `[U:1:992302338]` as an account number.
fn account(id3: &str) -> Option<u32> {
    id3.trim_matches(|c| c == '[' || c == ']').rsplit(':').next()?.parse().ok()
}

/// The number at the end of `http://api-v2.etf2l.org/team/38059`.
fn last_number(url: &str) -> Option<i64> {
    url.trim_end_matches('/').rsplit('/').next()?.parse().ok()
}

/// One page of `/recruitment/players` or `/recruitment/teams`.
pub fn parse_page(body: &str) -> Option<Vec<RecruitPost>> {
    let v: Value = serde_json::from_str(body).ok()?;
    let page = v.get("recruitment")?;
    // The new API pages under "data"; the old one is a plain list.
    let rows = page.get("data").and_then(Value::as_array).or_else(|| page.as_array())?;
    Some(
        rows.iter()
            .filter(|r| r["type"].as_str() == Some("Highlander"))
            .filter_map(|r| {
                Some(RecruitPost {
                    id: r["id"].as_i64()?,
                    name: crate::catalogue::html_text(r["name"].as_str().unwrap_or("")).trim().to_string(),
                    classes: r["classes"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect(),
                    skill: r["skill"].as_str().filter(|s| !s.is_empty()).map(str::to_string),
                    comments: r["comments"]["count"].as_u64().unwrap_or(0) as u32,
                    last_comment: r["comments"]["last"].as_i64(),
                    avatar: r["steam"]["avatar"].as_str().filter(|a| !a.ends_with('/')).map(str::to_string),
                    account_id: r["urls"]["player"].is_string().then(|| r["steam"]["id3"].as_str().and_then(account)).flatten(),
                    team_id: r["urls"]["team"].as_str().and_then(last_number),
                    link: r["urls"]["recruitment"].as_str().unwrap_or("https://etf2l.org/recruitment/").to_string(),
                })
            })
            .collect(),
    )
}

async fn read(sources: &Sources, kind: &str) -> Result<Vec<RecruitPost>> {
    let mut out: Vec<RecruitPost> = Vec::new();
    for page in 1..=PAGES {
        let Some(body) = sources.etf2l_get_interactive(&format!("/recruitment/{kind}?type=Highlander&page={page}")).await? else { break };
        let Some(rows) = parse_page(&body) else { anyhow::bail!("ETF2L's recruitment did not parse") };
        if rows.is_empty() {
            break;
        }
        for r in rows {
            if !out.iter().any(|o| o.id == r.id) {
                out.push(r);
            }
        }
    }
    Ok(out)
}

/// The latest Highlander recruitment posts, players and teams.
pub async fn latest(db: &Db, sources: &Sources) -> Result<Recruitment> {
    let held: Option<(i64, Recruitment)> = db.get_setting(KEY).await?.and_then(|v| serde_json::from_str(&v).ok());
    if let Some((at, r)) = &held {
        if now() - at < FRESH {
            return Ok(r.clone());
        }
    }
    let fresh = async { Ok::<_, anyhow::Error>(Recruitment { players: read(sources, "players").await?, teams: read(sources, "teams").await? }) }.await;
    match fresh {
        Ok(r) => {
            db.set_setting(KEY, &serde_json::to_string(&(now(), &r))?).await?;
            Ok(r)
        }
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "reading ETF2L's recruitment failed");
            Ok(held.map(|(_, r)| r).unwrap_or_default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_keeps_highlander_posts_with_who_they_are() {
        let body = r#"{"recruitment":{"current_page":1,"data":[
          {"classes":["Spy"],"comments":{"count":2,"last":1792108800},"id":106234,"name":"spooky &amp; co","skill":"Mid/Low",
           "steam":{"avatar":"https://avatars.steamstatic.com/a_full.jpg","id3":"[U:1:992302338]"},"type":"Highlander",
           "urls":{"player":"http://api-v2.etf2l.org/player/138971","recruitment":"https://etf2l.org/recruitment/344714"}},
          {"classes":["Scout"],"comments":{"count":0,"last":null},"id":106230,"name":"sixes","skill":"High","steam":{},"type":"6v6","urls":{}},
          {"classes":["Medic","Scout"],"comments":{"count":0,"last":null},"id":106220,"name":"BEST TROLLS","skill":"",
           "steam":{"avatar":"https://etf2l.org/wp-content/uploads/avatars/","id3":"[U:1:285837700]"},"type":"Highlander",
           "urls":{"team":"http://api-v2.etf2l.org/team/38059","recruitment":"https://etf2l.org/recruitment/344700"}}
        ]}}"#;
        let p = parse_page(body).unwrap();
        assert_eq!(p.len(), 2, "the 6v6 post is left out");
        assert_eq!((p[0].name.as_str(), p[0].account_id, p[0].team_id, p[0].comments), ("spooky & co", Some(992_302_338), None, 2));
        assert_eq!(p[0].classes, vec!["Spy"]);
        assert_eq!((p[1].account_id, p[1].team_id, p[1].skill.as_deref(), p[1].avatar.as_deref()), (None, Some(38059), None, None), "a team: its id, not its leader's account; no empty avatar");
    }
}
