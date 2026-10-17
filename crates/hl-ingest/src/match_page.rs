//! An official's own page on etf2l.org, for the ETF2L tab's match page (Flashy:
//! "an overview of the match page like ETF2L, comments and other info").
//!
//! ETF2L's API has a match's teams, maps and players but not its schedule,
//! its per-map results with their logs, its demos or its comments; the
//! match's page has all of them, so the app reads that page when a match is
//! opened. If the page changes, what is not found is left out.

use crate::recruitment::{between, datetimes, parse_comments, text, RecruitComment};
use crate::sources::Sources;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchPage {
    pub link: String,
    /// The round's window, unix seconds.
    pub round_from: Option<i64>,
    pub round_to: Option<i64>,
    pub schedule_deadline: Option<i64>,
    pub result_deadline: Option<i64>,
    pub scheduled: Option<i64>,
    pub submitted: Option<i64>,
    /// Who submitted the result, as ETF2L writes it: "obi (:pregnant_man:)".
    pub submitted_by: Option<String>,
    pub maps: Vec<PageMap>,
    pub demos: Vec<PageDemo>,
    pub comments: Vec<RecruitComment>,
    /// The match itself as the page heads it, for an official older than
    /// the seasons the app has read (a player's or team's old officials).
    pub competition: Option<String>,
    pub round: Option<String>,
    pub teams: Vec<PageTeam>,
    /// The final score, in the page's team order; none before it is played.
    pub score: Option<(i64, i64)>,
    /// The logs in the maps that are in the owner's matches, so the app can
    /// open them itself.
    #[serde(default)]
    pub stored_logs: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageTeam {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMap {
    /// None while the map is still to be decided.
    pub map: Option<String>,
    pub image: Option<String>,
    /// Rounds each side took, in the page's order (clan1 first).
    pub score: Option<(i64, i64)>,
    /// The logs.tf logs linked to this map.
    pub logs: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDemo {
    /// "Map 2 (pl_vigil_rc10): First Person by piro (54 Downloads)".
    pub label: String,
    pub link: String,
}

/// "1  - 0" as (1, 0); "[XX] - [XX]" as nothing.
fn score(s: &str) -> Option<(i64, i64)> {
    let (a, b) = text(s).split_once('-').map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

pub fn parse(page: &str, link: &str) -> Option<MatchPage> {
    let start = page.find("<div id=\"content\">")?;
    let body = &page[start..];
    let body = &body[..body.find("<div id=\"sidebars\">").unwrap_or(body.len())];
    // Everything before the comments is the match; the comments come after.
    let head = &body[..body.find("<h3 id=\"comments\"").unwrap_or(body.len())];
    let round = between(head, "Round commencing:", "</h4>").map(datetimes).unwrap_or_default();
    let deadlines = between(head, "Schedule deadline:", "</h4>").map(|s| (s, s.find("Result deadline:")));
    let (schedule_deadline, result_deadline) = match deadlines {
        Some((s, Some(i))) => (datetimes(&s[..i]).first().copied(), datetimes(&s[i..]).first().copied()),
        Some((s, None)) => (datetimes(s).first().copied(), None),
        None => (None, None),
    };
    let sched = between(head, "Scheduled:", "</h4>").unwrap_or("");
    let (scheduled, submitted, submitted_by) = match sched.find("Results submitted:") {
        Some(i) => (
            datetimes(&sched[..i]).first().copied(),
            datetimes(&sched[i..]).first().copied(),
            sched[i..].rfind("by ").map(|j| text(&sched[i + j + 3..])).filter(|s| !s.is_empty()),
        ),
        None => (datetimes(sched).first().copied(), None, None),
    };

    let mut maps = Vec::new();
    if let Some(block) = between(head, "<div class=\"maps\">", "<div style=\"clear: both\">") {
        for m in block.split("<div class=\"map\">").skip(1) {
            let name = between(m, "<h2>", "</h2>").map(text).unwrap_or_default();
            let decided = !name.is_empty() && name != "TBD";
            let image = between(m, "<img src=\"", "\"").filter(|s| s.starts_with("https://etf2l.org/images/maps/")).map(str::to_string);
            let score = m.split("<p").skip(1).find(|p| !p.starts_with(" class=\"s\"")).and_then(|p| between(p, ">", "</p>")).and_then(score);
            let logs = m.match_indices("logs.tf/").filter_map(|(i, s)| m[i + s.len()..].split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).collect();
            maps.push(PageMap { map: decided.then_some(name), image: if decided { image } else { None }, score, logs });
        }
    }

    let mut demos = Vec::new();
    if let Some(list) = between(head, "<h2>Demos</h2>", "</ul>") {
        for li in list.split("<li>").skip(1) {
            let label = text(li.split("</li>").next().unwrap_or(li));
            if let Some(link) = between(li, "href=\"", "\"").filter(|l| l.starts_with("https://etf2l.org/")) {
                demos.push(PageDemo { label, link: link.to_string() });
            }
        }
    }

    // The page's head: competition, division and round, the two teams and
    // the score under them.
    let competition = between(head, "<h1 class=\"c\">", "</h1>").map(text).filter(|s| !s.is_empty());
    let round_name = head.split("<h3 class=\"c\">").skip(1).filter_map(|h| h.split("</h3>").next()).map(text).last().filter(|s| !s.is_empty());
    let mut teams = Vec::new();
    let mut score_line = None;
    if let Some(result) = between(head, "<p class=\"result\">", "</p>") {
        for part in result.split("href=\"https://etf2l.org/teams/").skip(1) {
            let id = part.split('"').next().and_then(|n| n.trim_end_matches('/').parse().ok());
            let name = between(part, ">", "</a>").map(text);
            if let (Some(id), Some(name)) = (id, name) {
                teams.push(PageTeam { id, name });
            }
        }
        score_line = result.rsplit("<br>").next().and_then(score);
    }

    Some(MatchPage {
        competition,
        round: round_name,
        teams,
        score: score_line,
        stored_logs: Vec::new(),
        link: link.to_string(),
        round_from: round.first().copied(),
        round_to: round.get(1).copied(),
        schedule_deadline,
        result_deadline,
        scheduled,
        submitted,
        submitted_by,
        maps,
        demos,
        comments: parse_comments(body),
    })
}

/// One official's page, read when it is opened; its logs that are in the
/// owner's matches marked so the app opens them itself.
pub async fn page(db: &hl_db::Db, sources: &Sources, match_id: i64) -> Result<Option<MatchPage>> {
    let link = format!("https://etf2l.org/matches/{match_id}/");
    let Some(html) = sources.fetch_text_interactive(&link).await? else { return Ok(None) };
    let Some(mut p) = parse(&html, &link) else { return Ok(None) };
    for id in p.maps.iter().flat_map(|m| m.logs.iter().copied()).collect::<Vec<_>>() {
        if db.has_match(id).await? {
            p.stored_logs.push(id);
        }
    }
    Ok(Some(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DD14 against :pregnant_man:, Season 36 High, as etf2l.org drew it on
    /// 17 October 2026, cut down: played, two maps with logs, a demo.
    const PLAYED: &str = r##"<div id="content"><div class="post">
    <h1 class="c">Highlander Season 36 (Autumn 2026): High</h1><h3 class="c">Round 6</h3>
        <h4 class="c">
            Round commencing: <time class="brief" datetime="2026-09-17T00:00:00+02:00">17 Sep</time> - <time class="brief" datetime="2026-09-22T23:59:00+02:00">22 Sep</time><br>
        </h4>
    <h4 class="c">Schedule deadline: <time class="brief" datetime="2026-09-15T23:59:00+02:00">15 Sep</time> | Result deadline: <time class="brief" datetime="2026-09-22T23:59:00+02:00">22 Sep</time></h4>
    <h4 class="c">
        <img src="https://etf2l.org/wp-content/themes/etf2lwide/images/calendar.png" align="top" alt="Date" /> Scheduled: <time class="brief" datetime="2026-09-17T21:15:00+02:00">17 Sep</time>
              <br>Results submitted: <time class="brief" datetime="2026-09-17T21:59:03+02:00">17 Sep</time>              <br>by obi (:pregnant_man:)        </h4>
        <div class="maps">
            <div class="map">
                    <h2>pl_vigil_rc10</h2>
                        <img src="https://etf2l.org/images/maps/pl_vigil_rc10.jpg" alt="pl_vigil_rc10" />
                    <p>1  - 0</p>
                                    <p class="s">[Screenshots: N\A ]</p>
                <p class="s">[Logs:  <a href="https://logs.tf/4122215" target="_blank">A</a> ]</p>
            </div>
            <div class="map">
                    <h2>pl_vigil_rc10</h2>
                        <img src="https://etf2l.org/images/maps/pl_vigil_rc10.jpg" alt="pl_vigil_rc10" />
                    <p>1  - 0</p>
                <p class="s">[Logs:  <a href="https://logs.tf/4122233" target="_blank">A</a> ]</p>
            </div>
            <div style="clear: both"></div>
        </div>
        <p class="result">
        <img src="/images/flags/Germany.gif" alt="Germany" /> <span class="winr"><a href="https://etf2l.org/teams/37863">:pregnant_man:</a></span>
        <b>vs</b>
        <img src="/images/flags/France.gif" alt="France" /> <span><a href="https://etf2l.org/teams/37805">DD14</a></span>
        <br>
    6 - 0    </p>
    <h2>Demos</h2>
    <ul>
         <li>Map 2 (pl_vigil_rc10): <a href="https://etf2l.org/wp-content/uploads/demos/17782">First Person</a> by <a href="https://etf2l.org/forum/user/139825">piro</a> (54 Downloads)</li>
    </ul>
    <h3 id="comments">1 Comment</h3>
    <ol class="commentlist">
        <li class="alt" id="comment-891376">
            <cite><a href="http://www.etf2l.org" class="url" rel="ugc external nofollow">ETF2L Staff</a></cite> said:<br>
            <p class="postmeta"> <a href="#comment-891376" title=""><time class="brief" datetime="2026-10-12T18:53:57+02:00">Mon</time></a> </p>
            <p>Welcome to the Grand Final!</p>
        </li>
    </ol></div></div><div id="sidebars">"##;

    #[test]
    fn a_played_match_page_gives_its_schedule_maps_demos_and_comments() {
        let p = parse(PLAYED, "https://etf2l.org/matches/92900/").unwrap();
        assert_eq!((p.round_from, p.round_to), (Some(1_789_596_000), Some(1_790_114_340)));
        assert_eq!((p.schedule_deadline, p.result_deadline), (Some(1_789_509_540), Some(1_790_114_340)));
        assert_eq!((p.scheduled, p.submitted), (Some(1_789_672_500), Some(1_789_675_143)));
        assert_eq!(p.submitted_by.as_deref(), Some("obi (:pregnant_man:)"));
        assert_eq!(p.maps.len(), 2);
        assert_eq!(p.maps[0].map.as_deref(), Some("pl_vigil_rc10"));
        assert_eq!(p.maps[0].score, Some((1, 0)));
        assert_eq!(p.maps[1].logs, vec![4_122_233]);
        assert_eq!(p.maps[0].image.as_deref(), Some("https://etf2l.org/images/maps/pl_vigil_rc10.jpg"));
        assert_eq!(p.demos.len(), 1);
        assert!(p.demos[0].label.starts_with("Map 2 (pl_vigil_rc10): First Person by piro"), "{}", p.demos[0].label);
        assert_eq!(p.comments.len(), 1);
        assert!(p.comments[0].staff && p.comments[0].author == "ETF2L Staff");
        assert_eq!(p.competition.as_deref(), Some("Highlander Season 36 (Autumn 2026): High"));
        assert_eq!(p.round.as_deref(), Some("Round 6"));
        assert_eq!(p.teams.iter().map(|t| (t.id, t.name.as_str())).collect::<Vec<_>>(), vec![(37863, ":pregnant_man:"), (37805, "DD14")]);
        assert_eq!(p.score, Some((6, 0)));
    }

    #[test]
    fn a_match_to_come_has_its_maps_to_be_decided() {
        let page = r#"<div id="content"><h4 class="c"> Scheduled: <time datetime="2026-10-18T20:35:00+02:00">18 Oct</time> </h4>
        <div class="maps"><div class="map"><h2><span title="To Be Decided">TBD</span></h2>
        <img src="https://etf2l.org/wp-content/themes/etf2lwide/images/map_placeholder.png" alt="" /><p>[XX] - [XX]</p></div><div style="clear: both"></div></div>"#;
        let p = parse(page, "x").unwrap();
        assert_eq!(p.scheduled, Some(1_792_348_500));
        assert_eq!((p.maps[0].map.clone(), p.maps[0].score, p.maps[0].image.clone()), (None, None, None));
        assert!(p.comments.is_empty() && p.demos.is_empty() && p.submitted.is_none());
        assert!(parse("<html>nothing</html>", "x").is_none());
    }
}
