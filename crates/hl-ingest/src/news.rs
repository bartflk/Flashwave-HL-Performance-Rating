//! ETF2L's Highlander news for the ETF2L tab (Q74, Flashy; PLAN §27).
//!
//! ETF2L's site is WordPress, and its posts come whole from the WordPress
//! API: `/wp-json/wp/v2/posts`, category 18 being Highlander. The list is
//! read at most every half hour and kept in `app_config`, the posts' HTML
//! with it, so a post opens without another request.

use crate::sources::Sources;
use anyhow::Result;
use hl_db::Db;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const KEY: &str = "etf2l_news";
const FRESH: i64 = 30 * 60;
/// ETF2L's "Highlander" category.
const HIGHLANDER: i64 = 18;
const PER_PAGE: usize = 12;

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// One post as the list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    pub id: i64,
    /// Unix seconds.
    pub time: i64,
    pub title: String,
    pub link: String,
    /// The post's opening, as plain text.
    pub excerpt: String,
    /// The first picture in the post, usually its banner.
    pub image: Option<String>,
}

/// One post to read: the list's entry and its HTML as WordPress gives it.
/// The app builds the page from the HTML itself, keeping only the safe parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsPost {
    #[serde(flatten)]
    pub item: NewsItem,
    pub html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Held {
    at: i64,
    items: Vec<NewsItem>,
    /// Each post's HTML, by id, for reading it in the app.
    #[serde(default)]
    html: Vec<(i64, String)>,
}

/// The first `<img src>` in some HTML.
fn first_image(html: &str) -> Option<String> {
    let at = html.find("<img")?;
    let tag = &html[at..at + html[at..].find('>')?];
    let src = tag.find(" src=\"")? + 6;
    let end = tag[src..].find('"')?;
    let url = crate::catalogue::html_text(&tag[src..src + end]);
    url.starts_with("https://").then_some(url)
}

/// WordPress's `date_gmt`, "2026-10-12T01:14:47", as unix seconds.
fn gmt(s: &str) -> Option<i64> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok().map(|d| d.and_utc().timestamp())
}

/// The posts in one answer of the WordPress API, newest first.
pub fn parse(body: &str) -> Option<Vec<(NewsItem, String)>> {
    let v: Value = serde_json::from_str(body).ok()?;
    let mut out = Vec::new();
    for p in v.as_array()? {
        let (Some(id), Some(time)) = (p["id"].as_i64(), p["date_gmt"].as_str().and_then(gmt)) else { continue };
        let html = p["content"]["rendered"].as_str().unwrap_or("").to_string();
        let mut excerpt = crate::catalogue::html_text(p["excerpt"]["rendered"].as_str().unwrap_or("")).trim().to_string();
        // ETF2L's posts open with a "banner by …" credit under the picture,
        // which the excerpt keeps without the picture or the name.
        if excerpt.get(..10).is_some_and(|p| p.eq_ignore_ascii_case("banner by ")) {
            excerpt = excerpt[10..].trim_start().to_string();
        }
        out.push((
            NewsItem {
                id,
                time,
                title: crate::catalogue::html_text(p["title"]["rendered"].as_str().unwrap_or("")).trim().to_string(),
                link: p["link"].as_str().unwrap_or("").to_string(),
                excerpt,
                image: first_image(&html),
            },
            html,
        ));
    }
    out.sort_by_key(|(n, _)| std::cmp::Reverse(n.time));
    Some(out)
}

async fn held(db: &Db) -> Result<Option<Held>> {
    Ok(db.get_setting(KEY).await?.and_then(|v| serde_json::from_str(&v).ok()))
}

/// The latest Highlander posts. Best effort: a failed read keeps the last
/// list, and no list at all is an empty one.
pub async fn latest(db: &Db, sources: &Sources) -> Result<Vec<NewsItem>> {
    let held = held(db).await?;
    if let Some(h) = &held {
        if now() - h.at < FRESH {
            return Ok(h.items.clone());
        }
    }
    let url = crate::http::url(
        "https://etf2l.org/wp-json/wp/v2/posts",
        [
            ("categories", HIGHLANDER.to_string().as_str()),
            ("per_page", PER_PAGE.to_string().as_str()),
            ("_fields", "id,date_gmt,title,link,excerpt,content"),
        ],
    );
    match sources.fetch_text_interactive(&url).await.map(|b| b.as_deref().and_then(parse)) {
        Ok(Some(posts)) => {
            let fresh = Held { at: now(), items: posts.iter().map(|(n, _)| n.clone()).collect(), html: posts.into_iter().map(|(n, h)| (n.id, h)).collect() };
            db.set_setting(KEY, &serde_json::to_string(&fresh)?).await?;
            Ok(fresh.items)
        }
        other => {
            if let Err(e) = other {
                tracing::warn!(error = %format!("{e:#}"), "reading ETF2L's news failed");
            }
            Ok(held.map(|h| h.items).unwrap_or_default())
        }
    }
}

/// One post, from the kept list when it is there, else asked for alone
/// (an older post linked from somewhere).
pub async fn post(db: &Db, sources: &Sources, id: i64) -> Result<Option<NewsPost>> {
    if let Some(h) = held(db).await? {
        if let (Some(item), Some((_, html))) = (h.items.iter().find(|n| n.id == id), h.html.iter().find(|(i, _)| *i == id)) {
            return Ok(Some(NewsPost { item: item.clone(), html: html.clone() }));
        }
    }
    let url = crate::http::url(
        "https://etf2l.org/wp-json/wp/v2/posts",
        [("include", id.to_string().as_str()), ("_fields", "id,date_gmt,title,link,excerpt,content")],
    );
    let Some(body) = sources.fetch_text_interactive(&url).await? else { return Ok(None) };
    Ok(parse(&body).and_then(|p| p.into_iter().next()).map(|(item, html)| NewsPost { item, html }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_reads_with_its_banner_and_without_the_credit() {
        let body = r#"[
          {"id":2,"date_gmt":"2026-10-12T01:14:47","link":"https://etf2l.org/2026/10/12/b/",
           "title":{"rendered":"Rule Changes &amp; Tiers"},
           "excerpt":{"rendered":"<p>banner by With the release of final tiers [&hellip;]</p>\n"},
           "content":{"rendered":"<figure><img loading=\"lazy\" src=\"https://etf2l.org/wp-content/uploads/b.png\" alt=\"\"></figure><p>Text</p>"}},
          {"id":1,"date_gmt":"2026-10-09T01:33:05","link":"https://etf2l.org/a/","title":{"rendered":"Older"},
           "excerpt":{"rendered":"<p>Banner by Plain</p>"},"content":{"rendered":"<p>No picture</p>"}}
        ]"#;
        let p = parse(body).unwrap();
        assert_eq!(p.len(), 2);
        let (n, html) = &p[0];
        assert_eq!(n.title, "Rule Changes & Tiers");
        assert_eq!(n.time, 1_791_767_687);
        assert_eq!(n.excerpt, "With the release of final tiers […]");
        assert_eq!(n.image.as_deref(), Some("https://etf2l.org/wp-content/uploads/b.png"));
        assert!(html.contains("<p>Text</p>"));
        assert_eq!(p[1].0.image, None);
        assert_eq!(p[1].0.excerpt, "Plain", "the credit in any case");
    }
}
