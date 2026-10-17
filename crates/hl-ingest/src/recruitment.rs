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

// ---- One post, read from its page (Flashy: "is it possible to get the
// actual post?"). Neither ETF2L API carries a post's text, its date or its
// comments; the post's own page on etf2l.org has them all, so the app reads
// that page when a post is opened. If ETF2L changes the page, the parts not
// found are simply missing and the card still links to the post.

/// A post as its page shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecruitDetail {
    pub link: String,
    /// When it was posted, unix seconds.
    pub posted: Option<i64>,
    /// Who posted a team's post.
    pub posted_by: Option<String>,
    pub last_online: Option<i64>,
    /// The message as HTML; the app builds it into the page itself.
    pub html: String,
    /// A player's team history, newest first.
    pub history: Vec<TeamMove>,
    pub comments: Vec<RecruitComment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMove {
    /// "Joined" or "Left".
    pub action: String,
    pub team: String,
    pub team_id: Option<i64>,
    pub time: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecruitComment {
    pub author: String,
    /// The commenter's teams, as ETF2L shows them after the name.
    pub teams: String,
    pub time: Option<i64>,
    pub html: String,
}

/// The text between `a` and the next `b`, from `from` on.
fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let i = s.find(a)? + a.len();
    let j = s[i..].find(b)? + i;
    Some(&s[i..j])
}

/// Every `datetime="…"` in a piece of HTML, as unix seconds.
fn datetimes(s: &str) -> Vec<i64> {
    s.match_indices("datetime=\"")
        .filter_map(|(i, m)| {
            let rest = &s[i + m.len()..];
            let v = &rest[..rest.find('"')?];
            chrono::DateTime::parse_from_rfc3339(v).ok().map(|d| d.timestamp())
        })
        .collect()
}

fn text(s: &str) -> String {
    crate::catalogue::html_text(s).split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_detail(page: &str, link: &str) -> Option<RecruitDetail> {
    let start = page.find("<div class=\"rec\"")?;
    let rest = &page[start..];
    // The post ends where the page's next section begins.
    let end = ["<h2>Team History", "<h2>Previous Recruitment", "<h3 id=\"comments\"", "<h2 id=\"respond\""].iter().filter_map(|m| rest.find(m)).min().unwrap_or(rest.len());
    let rec = &rest[..end];
    let small = between(rec, "<small>", "</small>").unwrap_or("");
    let times = datetimes(small);
    let posted_by = between(small, "Posted by ", ":").map(text).filter(|s| !s.is_empty());
    // The message: after the dates, before the profile link that closes it.
    let after = rec.find("</small>").map_or("", |i| &rec[i + "</small>".len()..]);
    // The first of those links (`class="small"`: profile, Steam) ends it.
    let body = after.find("class=\"small\"").and_then(|k| after[..k].rfind("<a href=")).map_or(after, |i| &after[..i]);
    let html = body.trim().trim_start_matches("<p>").trim_end_matches("</p>").trim().to_string();

    let mut history = Vec::new();
    if let Some(table) = between(&page[start..], "<h2>Team History</h2>", "</table>") {
        for row in table.split("<tr class=\"small").skip(1) {
            let cells: Vec<&str> = row.split("<td>").skip(1).collect();
            if cells.len() < 4 {
                continue;
            }
            let team_cell = cells[1];
            history.push(TeamMove {
                action: text(cells[0]),
                team: text(team_cell),
                team_id: between(team_cell, "/teams/", "\"").and_then(|n| n.trim_end_matches('/').parse().ok()),
                time: datetimes(cells[3]).first().copied(),
            });
        }
    }

    let mut comments = Vec::new();
    if let Some(list) = between(&page[start..], "<ol class=\"commentlist\">", "</ol>") {
        for item in list.split("<li ").skip(1) {
            let cite = between(item, "<cite>", "</cite>").map(text).unwrap_or_default();
            let (author, teams) = match cite.split_once(':') {
                Some((a, t)) => (a.trim().to_string(), t.trim().to_string()),
                None => (cite.clone(), String::new()),
            };
            let meta_end = item.find("class=\"postmeta\"").and_then(|i| item[i..].find("</p>").map(|j| i + j + "</p>".len()));
            let body = meta_end.map_or("", |i| &item[i..]);
            let body = body.rfind("</li>").map_or(body, |j| &body[..j]);
            comments.push(RecruitComment { author, teams, time: datetimes(item).first().copied(), html: body.trim().to_string() });
        }
    }

    Some(RecruitDetail {
        link: link.to_string(),
        posted: times.first().copied(),
        posted_by,
        last_online: times.get(1).copied(),
        html,
        history,
        comments,
    })
}

/// One post's page, read when it is opened. `id` is the number in its link.
pub async fn detail(sources: &Sources, id: i64) -> Result<Option<RecruitDetail>> {
    let link = format!("https://etf2l.org/recruitment/{id}/");
    let Some(page) = sources.fetch_text_interactive(&link).await? else { return Ok(None) };
    Ok(parse_detail(&page, &link))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player's post as etf2l.org drew it on 17 October 2026, cut down.
    const PLAYER_PAGE: &str = r##"<div id="content"><div class="post"><h1>Recruitment</h1>
    <div class="rec" style="width:97%">
        <span style="float:right"><img src="/images/icons/16x16_Spy.png" alt="Spy" title="Spy" />&nbsp;&nbsp;<strong>Mid/Low Skill</strong>, <strong>Highlander</strong></span>
        <h2><img src="https://etf2l.org/images/flags/France.gif" alt="France" title="France" /> spooky        </h2>
        <small>Posted: <time class="fuzzy" title="Sat, 17 Oct 2026 02:09:43 +0200" datetime="2026-10-17T02:09:43+02:00">15 minutes ago</time> | Last Online: <time class="fuzzy" title="Sat, 17 Oct 2026 01:16:23 +0200" datetime="2026-10-17T01:16:23+02:00">1 hour ago</time></small>
        <p>
    <p>keen spy main seeking a team to tryhard and be serious</p>
<p>can comm in EN / FR</p>
        </p>
    <a href="https://etf2l.org/forum/user/138971" class="small"> User Profile</a> | <a href="http://steamcommunity.com/profiles/[U:1:992302338]" class="small">SteamID: U:1:992302338</a>    </div>
        <h2>Team History</h2>
        <table class="fix">
            <tr class="fix_title"><td>Action</td><td>Team</td><td>By</td><td>Date</td></tr>
                    <tr class="small ">
                        <td>Joined</td>
                        <td><a href="https://etf2l.org/teams/37906">4NIKE [6v6]</a></td>
                        <td><a href="https://etf2l.org/forum/user/153922">SpicyFire</a></td>
                        <td><time class="fuzzy" title="Thu, 24 Sep 2026 12:54:56 +0200" datetime="2026-09-24T12:54:56+02:00">3 weeks ago</time></td>
                    </tr>
        </table>
    <h3 id="comments">1 Comment</h3>
    <ol class="commentlist">
        <li class="alt" id="comment-891684">
            <cite><a href="https://etf2l.org/forum/user/95939">MEGA BABOON</a>: <a href="https://etf2l.org/teams/37846">MNHT</a></cite> said:<br>
            <p class="postmeta"> <a href="#comment-891684" title=""><time class="brief" title="Fri, 16 Oct 2026 19:28:40 +0200" datetime="2026-10-16T19:28:40+02:00"><span class="date">Yesterday</span>, <span class="time">19:28</span></time></a> </p>
            <p>beast</p>
        </li>
    </ol>
    <h2 id="respond">Leave a Reply</h2></div></div>"##;

    #[test]
    fn a_post_page_gives_its_message_dates_history_and_comments() {
        let d = parse_detail(PLAYER_PAGE, "https://etf2l.org/recruitment/344714/").unwrap();
        assert_eq!(d.posted, Some(1_792_195_783), "2026-10-17 02:09:43 +02:00");
        assert_eq!(d.last_online, Some(1_792_192_583));
        assert_eq!(d.posted_by, None, "a player posts as themselves");
        assert!(d.html.contains("keen spy main") && d.html.contains("EN / FR"), "{}", d.html);
        assert!(!d.html.contains("User Profile"), "the profile link is not the message");
        assert_eq!(d.history.len(), 1);
        assert_eq!((d.history[0].action.as_str(), d.history[0].team.as_str(), d.history[0].team_id), ("Joined", "4NIKE [6v6]", Some(37906)));
        assert_eq!(d.comments.len(), 1);
        let c = &d.comments[0];
        assert_eq!((c.author.as_str(), c.teams.as_str(), c.html.as_str()), ("MEGA BABOON", "MNHT", "<p>beast</p>"));
        assert_eq!(c.time, Some(1_792_171_720));
    }

    #[test]
    fn a_team_post_says_who_posted_it() {
        let page = r#"<div class="rec"><h2>2 CAKED UP</h2><small>Posted by wavy: <time datetime="2026-10-15T22:16:13+02:00">1 day ago</time> | Last Online: <time datetime="2026-10-16T23:36:11+02:00">3 hours ago</time></small>
        <p><p>TRIALS</p><p>@777wavy on discord</p></p><a href="https://etf2l.org/teams/35790" class="small"> Team Profile</a></div><h2>Previous Recruitment Posts</h2>"#;
        let d = parse_detail(page, "x").unwrap();
        assert_eq!(d.posted_by.as_deref(), Some("wavy"));
        assert!(d.html.contains("TRIALS") && !d.html.contains("Team Profile"));
        assert!(d.history.is_empty() && d.comments.is_empty());
        assert!(parse_detail("<html>no post here</html>", "x").is_none(), "a changed page reads as nothing, not as garbage");
    }

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
