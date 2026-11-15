//! Which of the owner's matches were RGL matches (Flashy chose "B now":
//! RGL context on the owner's matches -- the season, division, week and
//! opponent, and the way to the RGL match page -- without counting them as
//! officials, so no rating or pool changes).
//!
//! Gently, as every RGL read must be (Flashy: "make sure to properly use the
//! api to not overburden them"): at most once a [`KEEP_S`], one profile read
//! for the owner -- an owner RGL does not know costs that and nothing more --
//! then their newest [`MAX_MATCHES`] RGL matches, each match's logs found the
//! way the RGL match page finds them and kept on disk for good there. The
//! links are the found logs the owner has stored.

use crate::rgl;
use crate::sources::Sources;
use anyhow::Result;
use hl_core::SteamId;
use hl_db::Db;
use serde::{Deserialize, Serialize};

const SETTING: &str = "rgl_own_links";
/// How long the links are trusted before RGL is asked again.
pub const KEEP_S: i64 = 24 * 3600;
/// The newest RGL matches looked at.
const MAX_MATCHES: usize = 20;

/// One of the owner's logs that was an RGL match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RglLink {
    pub log_id: i64,
    pub match_id: i64,
    pub season_name: Option<String>,
    pub division: Option<String>,
    /// "Week 1 - Vigil".
    pub week: Option<String>,
    pub team: String,
    pub opponent: String,
    pub won: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    fetched_at: i64,
    links: Vec<RglLink>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The links as last worked out, and whether they are due again.
pub async fn stored(db: &Db) -> Result<(Vec<RglLink>, bool)> {
    let s: Stored = db.get_setting(SETTING).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    Ok((s.links, now() - s.fetched_at > KEEP_S))
}

/// Work the links out again from RGL and store them.
pub async fn refresh(db: &Db, sources: &Sources, me: SteamId) -> Result<usize> {
    let steam = me.to_steamid64();
    let mut links = Vec::new();
    if rgl::player(sources, &steam).await?.is_some() {
        let more = rgl::player_more(db, sources, &steam).await?;
        for f in more.form.iter().take(MAX_MATCHES) {
            let Some(mm) = rgl::match_more(db, sources, f.game.match_id).await? else { continue };
            for l in mm.logs.iter().filter(|l| l.stored) {
                links.push(link_of(l.log_id, f.team_id, &f.game));
            }
        }
    }
    let n = links.len();
    db.set_setting(SETTING, &serde_json::to_string(&Stored { fetched_at: now(), links })?).await?;
    tracing::info!(links = n, "RGL matches among your logs");
    Ok(n)
}

fn link_of(log_id: i64, team_id: i64, m: &rgl::RglMatch) -> RglLink {
    let ours = m.teams.iter().find(|t| t.team_id == team_id);
    let theirs = m.teams.iter().find(|t| t.team_id != team_id);
    RglLink {
        log_id,
        match_id: m.match_id,
        season_name: m.season_name.clone(),
        division: m.division_name.clone(),
        week: m.match_name.clone(),
        team: ours.map(|t| t.team_name.clone()).unwrap_or_default(),
        opponent: theirs.map(|t| t.team_name.clone()).unwrap_or_default(),
        won: m.winner.map(|w| w == team_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_names_the_owner_s_team_and_the_other() {
        let m: rgl::RglMatch = serde_json::from_str(
            r#"{"matchId":7,"matchDate":null,"matchName":"Week 3 - Product","divisionName":"Advanced","seasonName":"HL Season 25","winner":2,
                "teams":[{"teamId":1,"teamName":"Us"},{"teamId":2,"teamName":"Them"}]}"#,
        )
        .unwrap();
        let l = link_of(99, 1, &m);
        assert_eq!((l.team.as_str(), l.opponent.as_str(), l.won, l.week.as_deref()), ("Us", "Them", Some(false), Some("Week 3 - Product")));
    }
}
