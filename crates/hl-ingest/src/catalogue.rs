//! Player profiles (Q35, Flashy; PLAN §26): who everyone in the league is,
//! from the ETF2L officials the league job has downloaded and who played
//! them.
//!
//! **Seasons.** Each official says who played it and for which team; a
//! player's season is the teams they played for, the division each was in,
//! and the record. Playoff matches carry no division of their own, so theirs
//! comes from the competition's name ("Low Playoffs") and its tier from that
//! season's regular matches in the same division.
//!
//! **Medals.** Worked out, not fetched -- ETF2L's API has no awards. For
//! each season and division: a playoff Grand Final gives gold to its winner
//! and silver to its loser, and a 3rd Place match gives bronze. A division
//! with no playoffs (Premiership, High and Mid in several seasons) is settled
//! by its final table -- won, then map score -- and only once the season is
//! over. A player earns their team's medal by playing at least one official
//! for it that season.
//!
//! **Which division a player is** (Flashy, ETF2L's own rule): in a season,
//! a division counts for them if they played at least [`MIN_FOR_DIVISION`]
//! officials in it, or played in the Grand Final of the division one below
//! (the finalists move up). Where both apply, the higher one. One merc game
//! in Premiership makes nobody a Premiership player. Their highest division
//! is the best one that counted in any season; every per-season division
//! -- the tags on a match, the ranks -- follows the same rule.

use crate::leagues::season_order;
use crate::sources::Sources;
use anyhow::{Context, Result};
use hl_core::SteamId;
use hl_db::{CatMatch, Db, Etf2lPlayer};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Officials in a division in one season before it counts as theirs.
pub const MIN_FOR_DIVISION: usize = 3;
/// ETF2L's page for a player is fetched again after this long.
const ETF2L_PLAYER_KEEP_S: i64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: i64,
    pub name: String,
    pub avatar: Option<String>,
}

/// One team a player played for in one season.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSeason {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: Option<i64>,
    pub team: Team,
    pub played: u32,
    pub won: u32,
    pub lost: u32,
    /// 1, 2 or 3 when the team took a medal that season.
    pub place: Option<u8>,
    /// Only ever a merc for this team that season: not on its roster.
    pub merc: bool,
    /// On the roster, but ETF2L has them leaving before the team's last
    /// match of the season: the team's medal is not theirs (Q48).
    pub left_early: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Medal {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: Option<i64>,
    /// 1 gold, 2 silver, 3 bronze.
    pub place: u8,
    pub team: Team,
    /// "Grand Final 5-4", "3rd place match 6-0", "League table".
    pub how: String,
}

/// An event's MVP (Flashy: "whoever played the finals and did the best in
/// the grand final", as HLTV names one). One per class for each Grand Final
/// -- the best player of the class on either finalist -- and one for the
/// event, the best of the winning team's.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mvp {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: Option<i64>,
    pub class: String,
    pub account_id: u32,
    pub team: Team,
    /// Their team won the final.
    pub won: bool,
    /// The Season MVP, the division's one headline award.
    pub event: bool,
    /// `"season"` (the Season MVP: the champions' best), `"evp"` (one of the
    /// division's best seasons, any team) or `"class"` (the top of the
    /// class's season table). See [`Catalogue::mvps`].
    pub kind: String,
    /// What it was decided on: for the Season MVP the blend of season,
    /// final and playoffs; for the others the season rating.
    pub score: f64,
    /// The season on that class: one rating per official, playoffs counting
    /// [`PLAYOFF_WEIGHT`] and the final [`FINAL_WEIGHT`], pulled towards 1.00
    /// by [`SHRINK_OFFICIALS`]. Its officials and the maps in them.
    pub season_rating: f64,
    pub officials: usize,
    pub season_maps: usize,
    /// The Grand Final and the playoff run before it (the Season MVP's).
    pub final_rating: Option<f64>,
    pub final_maps: usize,
    pub playoffs_rating: Option<f64>,
    pub playoffs_maps: usize,
}

/// Pseudo-officials at 1.00 that a season rating is pulled towards, so four
/// good officials cannot beat eight nearly as good. Measured (Flashy's
/// audit, 24 Oct 2026): one official varies by 0.19 around a Sniper's level
/// and Snipers' levels differ by 0.11, so k = 0.19^2/0.11^2 = 3.0 (2.7 over
/// every class); it was 2, chosen.
pub const SHRINK_OFFICIALS: f64 = 3.0;
/// How much an official's rating is adjusted for the strength of the team
/// faced: measured within players, each 0.10 of opponent strength (their
/// season's average official rating) costs 0.075 of rating; adjusting made
/// a season agree with itself between its halves 0.48 -> 0.56. An official
/// against a team at 1.10 counts 0.075 more, one against 0.90 0.075 less.
pub const OPPONENT_SLOPE: f64 = 0.75;
/// Pseudo-maps at 1.00 that a Grand Final's or a playoff run's own figure
/// is pulled towards, by the real maps in it.
pub const SHRINK_MAPS: f64 = 4.0;
/// How much a playoff official and a Grand Final count in a season, as the
/// harder games.
pub const PLAYOFF_WEIGHT: f64 = 1.5;
pub const FINAL_WEIGHT: f64 = 2.0;
/// Officials on the class (or, for the Season MVP, in the season) to be
/// eligible for an award: never fewer than this...
pub const AWARD_MIN_OFFICIALS: usize = 4;
/// ...and at least this share of the most officials anyone in the division
/// played that season, so a long season asks for more than a short one
/// (Flashy: S31 Low's best Sniper had 4 officials of a season where the
/// champions' Sniper played 7; Premiership plays fewer games than Low).
pub const AWARD_SHARE_OF_SEASON: f64 = 0.6;

/// Who may take a division's awards, worked out once per call.
#[derive(Debug, Default)]
pub struct AwardRules {
    /// `(season, tier)` -> officials needed.
    pub need: HashMap<(i64, i64), usize>,
    /// `(season, team)`: teams that played a playoff match that season.
    pub playoff_teams: std::collections::HashSet<(i64, i64)>,
    /// `(season, tier)`: divisions that had playoffs at all.
    pub has_playoffs: std::collections::HashSet<(i64, i64)>,
    /// `(season, tier, team)`: the division's medal teams -- gold, silver,
    /// bronze. The best of a class comes from these (Flashy: "only grabbing
    /// the snipers from the top 3 teams"); a division with no medals yet has
    /// no best of a class yet.
    pub podium: std::collections::HashSet<(i64, i64, i64)>,
}

impl AwardRules {
    pub fn need(&self, season: i64, tier: i64) -> usize {
        self.need.get(&(season, tier)).copied().unwrap_or(AWARD_MIN_OFFICIALS)
    }
    /// Enough officials on the class: EVPs, and the first half of the class award.
    pub fn enough(&self, l: &SeasonLine) -> bool {
        l.officials.len() >= self.need(l.season, l.tier)
    }
    /// Best of the class: enough officials, on one of the division's three
    /// medal teams.
    pub fn class_award(&self, l: &SeasonLine) -> bool {
        self.enough(l) && self.podium.contains(&(l.season, l.tier, l.team))
    }
}
/// The Season MVP's blend: season, Grand Final, the rest of the playoffs.
/// With no other playoff games, their share goes to the season.
pub const SEASON_MVP_WEIGHTS: (f64, f64, f64) = (0.50, 0.35, 0.15);
/// EVPs per division: five, three in a division of fewer than eight teams.
pub const EVPS: usize = 5;
pub const EVPS_SMALL: usize = 3;
pub const EVPS_SMALL_BELOW_TEAMS: usize = 8;

/// A weighted average pulled towards 1.00 by [`SHRINK_MAPS`].
pub fn shrunk(sum_weighted: f64, sum_weights: f64) -> f64 {
    (sum_weighted + SHRINK_MAPS) / (sum_weights + SHRINK_MAPS)
}

/// One official in a player's season: its logs weighed by minutes played,
/// so a map uploaded as three logs counts as much as one uploaded whole
/// (Flashy: Noe's 23 "maps" were 13 maps in 7 officials).
#[derive(Debug, Clone, Default)]
pub struct OfficialGame {
    pub score_minutes: f64,
    pub minutes: f64,
    /// [`PLAYOFF_WEIGHT`], [`FINAL_WEIGHT`] or 1.
    pub stage: f64,
    /// The maps played in it, by base name.
    pub maps: std::collections::HashSet<String>,
    pub logs: usize,
}

impl OfficialGame {
    pub fn rating(&self) -> f64 {
        if self.minutes > 0.0 { self.score_minutes / self.minutes } else { 1.0 }
    }
}

/// One player's season on one class, in one division.
#[derive(Debug, Clone, Default)]
pub struct SeasonLine {
    pub season: i64,
    pub tier: i64,
    pub division: String,
    pub class: String,
    pub account_id: u32,
    /// Their team that season (the one they played most for).
    pub team: i64,
    pub officials: std::collections::HashSet<i64>,
    /// Official -> how they played it.
    pub games: HashMap<i64, OfficialGame>,
    /// The opponents' strength over the officials that had one: sum and count.
    pub opponents: (f64, usize),
}

impl SeasonLine {
    /// One rated log into its official.
    pub fn add(&mut self, g: &hl_db::RatedGame, stage: f64) {
        let key = g.etf2l_match_id.unwrap_or(-g.log_id);
        self.officials.insert(key);
        let e = self.games.entry(key).or_insert_with(|| OfficialGame { stage, ..Default::default() });
        let m = if g.minutes > 0.0 { g.minutes } else { 1.0 };
        e.score_minutes += g.score * m;
        e.minutes += m;
        e.logs += 1;
        for name in g.map.as_deref().unwrap_or("").split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|n| n.contains('_')) {
            e.maps.insert(hl_core::maps::map_base(name));
        }
    }
    /// The real maps played, over every official (an official with no map
    /// name counts one).
    pub fn maps(&self) -> usize {
        self.games.values().map(|o| o.maps.len().max(1)).sum()
    }
    pub fn logs(&self) -> usize {
        self.games.values().map(|o| o.logs).sum()
    }
    /// The average strength of the teams faced, where known.
    pub fn opponent_strength(&self) -> Option<f64> {
        (self.opponents.1 > 0).then(|| self.opponents.0 / self.opponents.1 as f64)
    }

    /// The plain average over their officials.
    pub fn average(&self) -> f64 {
        if self.games.is_empty() { 1.0 } else { self.games.values().map(OfficialGame::rating).sum::<f64>() / self.games.len() as f64 }
    }
    /// The season rating: one rating per official, weighted by stage and
    /// pulled towards 1.00 by [`SHRINK_OFFICIALS`].
    pub fn rating(&self) -> f64 {
        let (sw, w) = self.games.values().fold((0.0, 0.0), |(sw, w), o| (sw + o.stage * o.rating(), w + o.stage));
        (sw + SHRINK_OFFICIALS) / (w + SHRINK_OFFICIALS)
    }
}

/// How much of an MVP's score is the Grand Final; the rest is the playoff
/// run before it. The final decides it, as HLTV's does, but one good map
/// against four bad ones does not.
pub const MVP_FINAL_WEIGHT: f64 = 0.7;

/// The playoff run counts in full only over this many maps; fewer count for
/// less, the final for the rest (Flashy's MVP audit: two playoff maps at 1.57
/// out-weighed a final 0.20 better).
pub const MVP_PLAYOFF_FULL_MAPS: usize = 4;

/// An MVP's score: the final, and the playoff run before it in proportion to
/// how much of it there is.
pub fn mvp_score(final_rating: f64, playoffs: Option<f64>, playoff_maps: usize) -> f64 {
    let Some(p) = playoffs else { return final_rating };
    let share = (1.0 - MVP_FINAL_WEIGHT) * (playoff_maps.min(MVP_PLAYOFF_FULL_MAPS) as f64 / MVP_PLAYOFF_FULL_MAPS as f64);
    (1.0 - share) * final_rating + share * p
}

/// One official a player played.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Official {
    pub match_id: i64,
    pub time: Option<i64>,
    pub season: i64,
    /// ETF2L's name for the competition ("Highlander Experimental Cup #10").
    pub competition: String,
    pub division: String,
    pub tier: Option<i64>,
    pub stage: String,
    pub round: Option<String>,
    pub team: Team,
    pub opponent: Team,
    pub score_for: Option<i64>,
    pub score_against: Option<i64>,
    pub won: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Division {
    pub name: String,
    pub tier: i64,
}

/// One row of a search.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub account_id: u32,
    pub name: String,
    pub highest: Option<Division>,
    pub main_class: Option<String>,
    /// Gold, silver, bronze.
    pub medals: [u32; 3],
    pub officials: u32,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub account_id: u32,
    pub steamid64: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub country: Option<String>,
    pub avatar: Option<String>,
    /// The classes they signed up as on ETF2L.
    pub declared_classes: Vec<String>,
    /// Games per main class in the logs held, most first.
    pub played_classes: Vec<(String, i64)>,
    pub main_class: Option<String>,
    pub current: Option<PlayerSeason>,
    pub highest: Option<Division>,
    pub medals: Vec<Medal>,
    /// The events they were MVP of, on their class or overall.
    pub mvps: Vec<Mvp>,
    /// Newest season first.
    pub seasons: Vec<PlayerSeason>,
    /// Newest first.
    pub officials: Vec<Official>,
    pub etf2l_id: Option<i64>,
    /// When they signed up on ETF2L, unix seconds.
    pub registered: Option<i64>,
    /// ETF2L's title for them: "Player", "Admin", ...
    pub etf2l_title: Option<String>,
    /// The ETF2L teams they are on now, every format: fun teams included.
    pub etf2l_teams: Vec<Etf2lRoster>,
    pub bans: Vec<Etf2lBan>,
}

/// A team on a player's ETF2L page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Etf2lRoster {
    pub id: i64,
    pub name: String,
    pub tag: Option<String>,
    /// "Highlander", "6v6", "Highlander Fun Team", ...
    pub kind: Option<String>,
    pub country: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Etf2lBan {
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub reason: Option<String>,
}

/// Everything the profiles are worked out from, loaded once per request.
pub struct Catalogue {
    matches: HashMap<i64, CatMatch>,
    /// match -> (account, team, name)
    rosters: HashMap<i64, Vec<(u32, Option<i64>, String)>>,
    teams: HashMap<i64, (String, Option<String>)>,
    /// (season, division name) -> tier, from regular matches; and division
    /// name -> tier over every season, for what a season does not settle.
    tiers: HashMap<(i64, String), i64>,
    tier_by_name: HashMap<String, i64>,
    newest_season: i64,
    /// `(match, account)`: played as a merc, for a team they were not on.
    mercs: std::collections::HashSet<(i64, u32)>,
    /// `(team, account)` -> their joins (true) and leaves, oldest first,
    /// from ETF2L's transfers (Q48).
    transfers: HashMap<(i64, u32), Vec<(i64, bool)>>,
    /// `(season, team)` -> the time of the team's last decided match.
    season_end: HashMap<(i64, i64), i64>,
    /// Final -> log -> (account, colour): the side each player was on, read
    /// where the roster does not say (Flashy: mercs can be MVP too).
    final_colours: HashMap<i64, HashMap<i64, Vec<(u32, String)>>>,
    /// `(season, division)` -> ETF2L's own final placings, `(place, team)`,
    /// from the league table's `ach` once the season is over.
    official_places: HashMap<(i64, String), Vec<(u8, i64)>>,
}

/// ETF2L's placings in a league table: `(division, place, team)` for every
/// row it marked 1st, 2nd or 3rd (`ach`). Empty while the season runs.
pub fn table_places(json: &str) -> Vec<(String, u8, i64)> {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let Some(tables) = v.get("tables").and_then(Value::as_object) else { return Vec::new() };
    let mut out = Vec::new();
    for (division, rows) in tables {
        for r in rows.as_array().into_iter().flatten() {
            let place = r.get("ach").and_then(|a| a.as_i64().or_else(|| a.as_str().and_then(|s| s.parse().ok())));
            if let (Some(place @ 1..=3), Some(team)) = (place, r.get("id").and_then(Value::as_i64)) {
                out.push((division.clone(), place as u8, team));
            }
        }
    }
    out
}

/// The rounds a medal is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoundKind {
    Final,
    Third,
    Semi,
    LowerFinal,
}

impl Catalogue {
    pub async fn load(db: &Db) -> Result<Catalogue> {
        let mut matches: HashMap<i64, CatMatch> = db.catalogue_matches().await?.into_iter().map(|m| (m.match_id, m)).collect();
        // A competition's stage and division as the parser reads its name
        // now: one stored before the parser knew "3rd Place Match" was kept
        // as a regular division called "Premiership 3rd Place Match".
        for m in matches.values_mut() {
            if let Some((division, stage)) = crate::leagues::division_and_stage(&m.comp_name).filter(|_| !m.comp_name.is_empty()) {
                m.comp_division = division;
                m.stage = stage;
            }
        }
        let mut rosters: HashMap<i64, Vec<(u32, Option<i64>, String)>> = HashMap::new();
        for (m, a, t, n) in db.catalogue_rosters(None).await? {
            rosters.entry(m).or_default().push((a, t, n));
        }
        let mut tiers: HashMap<(i64, String), i64> = HashMap::new();
        let mut votes: HashMap<String, HashMap<i64, u32>> = HashMap::new();
        for m in matches.values() {
            if let (Some(d), Some(t)) = (&m.division, m.tier) {
                tiers.entry((m.season, d.clone())).or_insert(t);
                *votes.entry(d.clone()).or_default().entry(t).or_default() += 1;
            }
        }
        let tier_by_name = votes.into_iter().filter_map(|(d, v)| Some((d, v.into_iter().max_by_key(|(_, n)| *n)?.0))).collect();
        // The newest numbered season: 134 (AFA 2025) is older than 35.
        let newest_season = matches.values().map(|m| m.season).filter(|s| *s < crate::leagues::OFF_SEASON).max().unwrap_or(0);
        let mercs = db.catalogue_mercs().await?.into_iter().collect();
        let mut transfers: HashMap<(i64, u32), Vec<(i64, bool)>> = HashMap::new();
        for (team, account, time, joined) in db.catalogue_transfers().await? {
            transfers.entry((team, account)).or_default().push((time, joined));
        }
        let season_end = Self::season_ends(&matches);
        let finals: Vec<i64> = matches.values().filter(|m| Self::is_final(m)).map(|m| m.match_id).collect();
        let mut final_colours: HashMap<i64, HashMap<i64, Vec<(u32, String)>>> = HashMap::new();
        for (m, log, account, colour) in db.log_colours(&finals).await? {
            final_colours.entry(m).or_default().entry(log).or_default().push((account, colour));
        }
        // ETF2L's own placings, per season and division.
        let seasons: HashMap<i64, i64> = db.competitions().await?.into_iter().map(|c| (c.competition_id, c.season)).collect();
        let mut official_places: HashMap<(i64, String), Vec<(u8, i64)>> = HashMap::new();
        for (competition, _, json) in db.etf2l_raw("tables").await? {
            let Some(&season) = seasons.get(&competition) else { continue };
            for (division, place, team) in table_places(&json) {
                official_places.entry((season, Self::ungrouped(&division))).or_default().push((place, team));
            }
        }
        Ok(Catalogue { matches, rosters, teams: db.etf2l_teams().await?, tiers, tier_by_name, newest_season, mercs, transfers, season_end, final_colours, official_places })
    }

    /// Each team's last decided match of each season.
    fn season_ends(matches: &HashMap<i64, CatMatch>) -> HashMap<(i64, i64), i64> {
        let mut out: HashMap<(i64, i64), i64> = HashMap::new();
        for m in matches.values().filter(|m| Self::played(m) || m.default_win) {
            let Some(time) = m.time else { continue };
            for team in [m.clan1, m.clan2] {
                let e = out.entry((m.season, team)).or_insert(time);
                *e = (*e).max(time);
            }
        }
        out
    }

    /// Whether a player joined a team after `after` and by the team's last
    /// match of the season, by ETF2L's transfers, and was still on it then:
    /// a sub who played as a merc before joining.
    fn joined_after(&self, account: u32, season: i64, team: i64, after: i64) -> bool {
        let (Some(events), Some(end)) = (self.transfers.get(&(team, account)), self.season_end.get(&(season, team))) else { return false };
        events.iter().any(|(at, joined)| *joined && *at >= after && at <= end) && self.on_roster_at_end(account, season, team)
    }

    /// Whether a player was still on a team's roster at its last match of
    /// the season, by ETF2L's transfers. Without transfers for them and
    /// that team, or with none from before then, they are taken to be.
    fn on_roster_at_end(&self, account: u32, season: i64, team: i64) -> bool {
        let (Some(events), Some(end)) = (self.transfers.get(&(team, account)), self.season_end.get(&(season, team))) else { return true };
        events.iter().rev().find(|(at, _)| at <= end).is_none_or(|(_, joined)| *joined)
    }

    /// Medals withheld because the player had left first:
    /// `(season, team, account, name)`, for checking (`hl transfers medals`).
    pub fn withheld_medals(&self) -> Vec<(i64, i64, u32, String)> {
        let medal_teams: std::collections::HashSet<(i64, i64)> =
            self.medals().into_iter().flat_map(|((season, _), list)| list.into_iter().map(move |(_, team, _)| (season, team))).collect();
        let mut out: std::collections::BTreeSet<(i64, i64, u32, String)> = Default::default();
        for (match_id, roster) in &self.rosters {
            let Some(m) = self.matches.get(match_id).filter(|m| Self::played(m)) else { continue };
            for (account, team, name) in roster {
                let Some(team) = *team else { continue };
                if medal_teams.contains(&(m.season, team)) && !self.mercs.contains(&(*match_id, *account)) && !self.on_roster_at_end(*account, m.season, team) {
                    out.insert((m.season, team, *account, name.clone()));
                }
            }
        }
        out.into_iter().collect()
    }

    /// A season's own name ("Autumn 2026"), for the daily ETF2L check.
    pub fn season_title(&self, season: i64) -> String {
        self.season_name(season)
    }

    /// A team's name, for the CLI's audits.
    pub fn team_name(&self, id: i64) -> String {
        self.team(id).name
    }

    pub fn team(&self, id: i64) -> Team {
        let (name, avatar) = self.teams.get(&id).cloned().unwrap_or_else(|| (format!("Team {id}"), None));
        Team { id, name, avatar }
    }

    /// A match's division, and its tier where one can be found.
    fn division_of(&self, m: &CatMatch) -> (String, Option<i64>) {
        let name = m.division.clone().unwrap_or_else(|| m.comp_division.clone());
        // Today's ladder first; ETF2L's own tier only for a name that is not
        // a division.
        let tier = crate::leagues::canonical_tier(&name)
            .or(m.tier)
            .or_else(|| self.tiers.get(&(m.season, name.clone())).copied())
            .or_else(|| self.tier_by_name.get(&name).copied());
        (name, tier)
    }

    /// The team's result in a played match: `(for, against, won)`.
    fn result_for(m: &CatMatch, team: i64) -> (Option<i64>, Option<i64>, Option<bool>) {
        let (f, a) = if m.clan1 == team { (m.r1, m.r2) } else { (m.r2, m.r1) };
        let won = match (f, a) {
            (Some(f), Some(a)) if f != a => Some(f > a),
            _ => None,
        };
        (f, a, won)
    }

    /// The division a medal is for, as ETF2L ran it: "Division 1" and
    /// "Division 2" each had their own final, so each has its own medals,
    /// though today's ladder calls both High (Flashy's audit: the merged name
    /// gave S25-S30 two golds a division). Groups of one division share it
    /// ("Division 2A" and "2B" are Division 2, "Open B" is Open), and a
    /// division today's ladder knows is called by today's name. A final with
    /// no division of its own (Autumn 2023's Grand Final is a round of the
    /// season) takes the one its teams played that season.
    fn medal_division(&self, m: &CatMatch) -> String {
        let (name, _) = self.division_of(m);
        let name = Self::ungrouped(&name);
        if Self::numbered(&name).is_some() {
            return name;
        }
        let mut name = name;
        if crate::leagues::canonical_tier(&name).is_none() {
            let mut seen: HashMap<String, u32> = HashMap::new();
            for x in self.matches.values().filter(|x| x.season == m.season && x.stage == "regular" && x.match_id != m.match_id) {
                if [x.clan1, x.clan2].iter().any(|t| *t == m.clan1 || *t == m.clan2) {
                    let (d, _) = self.division_of(x);
                    if crate::leagues::canonical_tier(&d).is_some() {
                        *seen.entry(d).or_default() += 1;
                    }
                }
            }
            if let Some((d, _)) = seen.into_iter().max_by_key(|(_, n)| *n) {
                name = d;
            }
        }
        let name = Self::ungrouped(&name);
        // Numbered divisions, and S33's Freshest beside its Fresh: each
        // had its own final.
        if Self::numbered(&name).is_some() || name.eq_ignore_ascii_case("freshest") {
            return name;
        }
        crate::leagues::canonical_tier(&name).and_then(|t| crate::leagues::TIER_NAMES.get(t as usize)).map_or(name, |n| n.to_string())
    }

    /// The tier of the division one above a match's, on its season's own
    /// ladder: "Division 2" -> Division 1, "Division 1" -> Premiership, a
    /// named division one tier up today. `None` above the top.
    fn division_above(&self, m: &CatMatch) -> Option<i64> {
        let name = self.medal_division(m);
        match Self::numbered(&name) {
            Some(1) => Some(0),
            Some(n) => crate::leagues::canonical_tier(&format!("Division {}", n - 1)),
            None => crate::leagues::canonical_tier(&name).or_else(|| self.division_of(m).1).filter(|t| *t > 0).map(|t| t - 1),
        }
    }

    /// "Division 2A" -> "Division 2", "Open B" -> "Open", "Premiership
    /// Division" -> "Premiership": one division's groups, one name.
    fn ungrouped(name: &str) -> String {
        let n = name.trim();
        let n = n.strip_suffix(" Division").unwrap_or(n);
        match n.rsplit_once(' ') {
            Some((head, last)) if last.len() == 1 && last.chars().all(|c| c.is_ascii_alphabetic()) => head.to_string(),
            Some((head, last))
                if last.len() >= 2 && last[..last.len() - 1].chars().all(|c| c.is_ascii_digit()) && last.ends_with(|c: char| c.is_ascii_alphabetic()) =>
            {
                format!("{head} {}", &last[..last.len() - 1])
            }
            _ => n.to_string(),
        }
    }

    /// The number of a "Division 3" / "Div 3", if it is one.
    fn numbered(name: &str) -> Option<u32> {
        let d = name.to_ascii_lowercase();
        d.strip_prefix("division ").or_else(|| d.strip_prefix("div ")).and_then(|n| n.trim().parse().ok())
    }

    /// What a round is, whatever ETF2L called it that season: "Grand Final",
    /// "Grand-Final", "Division 1 Grand Final", "3rd Place " (with the
    /// space), "Semi-Finals", a double-elimination "Lower Bracket Final".
    fn round_kind(round: Option<&str>) -> Option<RoundKind> {
        let r = round?.to_ascii_lowercase().replace('-', " ");
        let r = r.split_whitespace().collect::<Vec<_>>().join(" ");
        if r.contains("grand final") || r == "final" || r.starts_with("final ") {
            Some(RoundKind::Final)
        } else if r.contains("3rd place") || r.contains("third place") || r.contains("bronze") {
            Some(RoundKind::Third)
        } else if r.contains("lower bracket final") {
            Some(RoundKind::LowerFinal)
        } else if r.contains("semi") && !r.contains("bracket") {
            Some(RoundKind::Semi)
        } else {
            None
        }
    }

    /// A league final: a Playoffs competition's final, a "Grand Final"
    /// competition, or -- in the seasons without a separate playoff
    /// competition (Autumn 2023) -- a regular match whose round is the Grand
    /// Final. A preseason cup's final is not the league's.
    /// Everyone in a final's logs on the finalist whose roster players shared
    /// their colour, by the most logs. A log whose colours the rosters do not
    /// settle -- both finalists' players on one side, or none -- says nothing.
    fn sides_from_logs(logs: Option<&HashMap<i64, Vec<(u32, String)>>>, roster: &HashMap<u32, i64>, a: i64, b: i64) -> HashMap<u32, i64> {
        let mut votes: HashMap<(u32, i64), u32> = HashMap::new();
        for players in logs.into_iter().flat_map(|l| l.values()) {
            let mut by_colour: HashMap<&str, (u32, u32)> = HashMap::new();
            for (account, colour) in players {
                match roster.get(account) {
                    Some(t) if *t == a => by_colour.entry(colour.as_str()).or_default().0 += 1,
                    Some(t) if *t == b => by_colour.entry(colour.as_str()).or_default().1 += 1,
                    _ => {}
                }
            }
            let team_of_colour = |c: &str| {
                let (x, y) = by_colour.get(c).copied().unwrap_or_default();
                if x > y {
                    Some(a)
                } else if y > x {
                    Some(b)
                } else {
                    None
                }
            };
            let (mut red, mut blue) = (team_of_colour("Red"), team_of_colour("Blue"));
            // Two teams, two colours: one side known is both known, for a
            // final whose lineup ETF2L lists none of one side's players in.
            let other = |t: i64| if t == a { b } else { a };
            match (red, blue) {
                (Some(r), None) => blue = Some(other(r)),
                (None, Some(x)) => red = Some(other(x)),
                _ => {}
            }
            if red.is_none() || red == blue {
                continue;
            }
            for (account, colour) in players {
                let team = if colour == "Red" { red } else { blue };
                if let Some(t) = team {
                    *votes.entry((*account, t)).or_default() += 1;
                }
            }
        }
        let mut out: HashMap<u32, (i64, u32)> = HashMap::new();
        for ((account, team), n) in votes {
            let e = out.entry(account).or_insert((team, 0));
            if n > e.1 {
                *e = (team, n);
            }
        }
        out.into_iter().map(|(a, (t, _))| (a, t)).collect()
    }

    fn is_final(m: &CatMatch) -> bool {
        m.stage != "Cup" && (m.stage == "Grand Final" || Self::round_kind(m.round.as_deref()) == Some(RoundKind::Final))
    }

    fn played(m: &CatMatch) -> bool {
        !m.default_win && m.r1.unwrap_or(0) + m.r2.unwrap_or(0) > 0
    }

    /// Every medal of every season and division: `(season, division) ->
    /// [(place, team, how)]`. Where ETF2L has published a division's final
    /// placings, those are the medals, with the result that decided each
    /// kept beside it; until then, the app's own reading of the playoffs.
    pub fn medals(&self) -> BTreeMap<(i64, String), Vec<(u8, i64, String)>> {
        let mut out = self.medals_from_results();
        for (key, places) in &self.official_places {
            let ours = out.remove(key).unwrap_or_default();
            let mut confirmed: Vec<(u8, i64, String)> = places
                .iter()
                .map(|(place, team)| {
                    let how = ours
                        .iter()
                        .find(|(p, t, _)| p == place && t == team)
                        .map(|(_, _, how)| format!("{how}, confirmed by ETF2L"))
                        .unwrap_or_else(|| "ETF2L's final standings".to_string());
                    (*place, *team, how)
                })
                .collect();
            confirmed.sort_by_key(|(p, t, _)| (*p, *t));
            out.insert(key.clone(), confirmed);
        }
        out
    }

    /// Where the medals read from the results and ETF2L's own placings
    /// disagree: `(season, division, ours, ETF2L's)`, each `(place, team)`.
    /// For `hl medals-audit`.
    pub fn medal_checks(&self) -> Vec<(i64, String, Vec<(u8, i64)>, Vec<(u8, i64)>)> {
        let ours = self.medals_from_results();
        let mut out = Vec::new();
        for (key, places) in &self.official_places {
            let mut theirs = places.clone();
            theirs.sort_unstable();
            let mut mine: Vec<(u8, i64)> = ours.get(key).map(|v| v.iter().map(|(p, t, _)| (*p, *t)).collect()).unwrap_or_default();
            mine.sort_unstable();
            if mine != theirs {
                out.push((key.0, key.1.clone(), mine, theirs));
            }
        }
        out.sort_by(|a, b| season_order(b.0).cmp(&season_order(a.0)).then(a.1.cmp(&b.1)));
        out
    }

    /// The number of divisions ETF2L has placed, for the audit.
    pub fn official_divisions(&self) -> usize {
        self.official_places.len()
    }

    /// The medals as the results read them: finals, 3rd-place matches,
    /// semi-finals, and the table where a division had no playoffs.
    fn medals_from_results(&self) -> BTreeMap<(i64, String), Vec<(u8, i64, String)>> {
        let mut out: BTreeMap<(i64, String), Vec<(u8, i64, String)>> = BTreeMap::new();
        // A forfeit decides a final as surely as a match does (AFA 2025's
        // Division 1 Grand Final was won by default): it says so.
        let score = |m: &CatMatch, winner: i64| {
            let (f, a, _) = Self::result_for(m, winner);
            format!("{}-{}{}", f.unwrap_or(0), a.unwrap_or(0), if m.default_win { " by default" } else { "" })
        };
        let winner_loser = |m: &CatMatch| -> Option<(i64, i64)> {
            match (m.r1?, m.r2?) {
                (a, b) if a > b => Some((m.clan1, m.clan2)),
                (a, b) if b > a => Some((m.clan2, m.clan1)),
                _ => None,
            }
        };
        let decided = |m: &&CatMatch| Self::played(m) || m.default_win;
        for m in self.matches.values().filter(decided) {
            let key = (m.season, self.medal_division(m));
            if Self::is_final(m) {
                if let Some((w, l)) = winner_loser(m) {
                    let e = out.entry(key).or_default();
                    e.push((1, w, format!("Grand Final {}", score(m, w))));
                    e.push((2, l, format!("Grand Final {}", score(m, l))));
                }
            } else if m.stage == "Cup" {
                continue;
            } else if m.stage == "3rd Place" || Self::round_kind(m.round.as_deref()) == Some(RoundKind::Third) {
                if let Some((w, _)) = winner_loser(m) {
                    out.entry(key).or_default().push((3, w, format!("3rd place match {}", score(m, w))));
                }
            } else if Self::round_kind(m.round.as_deref()) == Some(RoundKind::LowerFinal) {
                // Double elimination: losing the lower bracket's final is
                // third outright.
                if let Some((_, l)) = winner_loser(m) {
                    out.entry(key).or_default().push((3, l, format!("lower bracket final {}", score(m, l))));
                }
            }
        }
        // No 3rd-place match: the two losing semi-finalists share third, as
        // ETF2L gives it (Flashy's S34 Mid: lost the semi-final 3-6, and
        // there was no match for third).
        let mut semis: BTreeMap<(i64, String), Vec<(i64, String)>> = BTreeMap::new();
        // Semi-finals in a Playoffs competition, or -- in the seasons whose
        // playoffs were rounds of the season ("Top Tiers") -- in the season.
        for m in self.matches.values().filter(|m| decided(m) && m.stage != "Cup") {
            if Self::round_kind(m.round.as_deref()) == Some(RoundKind::Semi) {
                if let Some((_, l)) = winner_loser(m) {
                    semis.entry((m.season, self.medal_division(m))).or_default().push((l, format!("joint 3rd: semi-final {}", score(m, l))));
                }
            }
        }
        for (key, losers) in semis {
            if let Some(e) = out.get_mut(&key) {
                if !e.iter().any(|(p, _, _)| *p == 3) {
                    e.extend(losers.into_iter().map(|(team, how)| (3u8, team, how)));
                }
            }
        }
        // Divisions without a final: the table, once the season is over.
        let mut tables: HashMap<(i64, String), HashMap<i64, (u32, i64)>> = HashMap::new();
        for m in self.matches.values().filter(|m| Self::played(m) && m.stage == "regular" && season_order(m.season) < season_order(self.newest_season)) {
            let t = tables.entry((m.season, self.medal_division(m))).or_default();
            for team in [m.clan1, m.clan2] {
                let (f, a, won) = Self::result_for(m, team);
                let e = t.entry(team).or_default();
                e.0 += u32::from(won == Some(true));
                e.1 += f.unwrap_or(0) - a.unwrap_or(0);
            }
        }
        for (key, table) in tables {
            if out.contains_key(&key) || table.len() < 4 {
                continue;
            }
            let mut rows: Vec<(i64, (u32, i64))> = table.into_iter().collect();
            rows.sort_by(|a, b| (b.1).cmp(&a.1).then(a.0.cmp(&b.0)));
            let e = out.entry(key).or_default();
            for (i, (team, _)) in rows.into_iter().take(3).enumerate() {
                e.push((i as u8 + 1, team, "League table".to_string()));
            }
        }
        out
    }

    /// Every league Grand Final played (not forfeited): `(season, division,
    /// match)`, newest first. What the MVPs and gold medals are read from.
    pub fn finals(&self) -> Vec<(i64, String, &CatMatch)> {
        let mut v: Vec<(i64, String, &CatMatch)> =
            self.matches.values().filter(|m| Self::played(m) && Self::is_final(m)).map(|m| (m.season, self.medal_division(m), m)).collect();
        v.sort_by_key(|(s, d, _)| (std::cmp::Reverse(season_order(*s)), d.clone()));
        v
    }

    /// A season's own name, not a tournament's kept with it.
    fn season_name(&self, s: i64) -> String {
        let of = |m: &&CatMatch| m.season == s;
        self.matches
            .values()
            .filter(of)
            .find(|m| m.stage != "Cup")
            .or_else(|| self.matches.values().find(of))
            .map(|m| m.season_name.clone())
            .unwrap_or_default()
    }

    /// How much an official counts in a season: a Grand Final
    /// [`FINAL_WEIGHT`], another playoff game [`PLAYOFF_WEIGHT`], else 1.
    pub fn stage_weight(&self, match_id: Option<i64>) -> f64 {
        match match_id.and_then(|id| self.matches.get(&id)) {
            Some(m) if Self::is_final(m) => FINAL_WEIGHT,
            Some(m) if matches!(m.stage.as_str(), "Playoffs" | "3rd Place") || Self::round_kind(m.round.as_deref()).is_some() => PLAYOFF_WEIGHT,
            _ => 1.0,
        }
    }

    /// Every player's season on every class, in the division they played
    /// that season: officials only, a cup left out. Keyed `(season, tier,
    /// class, account)`.
    pub fn season_lines(&self, games: &[hl_db::RatedGame]) -> HashMap<(i64, i64, String, u32), SeasonLine> {
        self.season_lines_with(games, games)
    }

    /// [`Catalogue::season_lines`] over `games`, the opponents' strength
    /// measured on `all`: a table of the playoffs alone still knows how
    /// strong a team was over the whole season.
    pub fn season_lines_with(&self, games: &[hl_db::RatedGame], all: &[hl_db::RatedGame]) -> HashMap<(i64, i64, String, u32), SeasonLine> {
        let windows = self.season_windows();
        let divisions = self.played_divisions();
        let strength = self.team_strength(all);
        let mut out: HashMap<(i64, i64, String, u32), SeasonLine> = HashMap::new();
        for g in games.iter().filter(|g| g.official) {
            let Some(season) = game_season(self, &windows, g) else { continue };
            let Some((tier, division, team)) = divisions.get(&(g.account_id, season)) else { continue };
            let w = self.stage_weight(g.etf2l_match_id);
            // The opponent's strength, from their other officials that season.
            let opp = g.etf2l_match_id.and_then(|id| {
                let m = self.matches.get(&id)?;
                let mine = self.rosters.get(&id)?.iter().find(|(a, _, _)| *a == g.account_id)?.1?;
                let other = if mine == m.clan1 { m.clan2 } else if mine == m.clan2 { m.clan1 } else { return None };
                strength.get(&(m.season, other)).and_then(|by| {
                    let rest: Vec<f64> = by.iter().filter(|(mid, _)| **mid != id).map(|(_, r)| *r).collect();
                    (rest.len() >= 3).then(|| rest.iter().sum::<f64>() / rest.len() as f64)
                })
            });
            let mut adjusted = g.clone();
            if let Some(s) = opp {
                adjusted.score += OPPONENT_SLOPE * (s - 1.0);
            }
            let line = out.entry((season, *tier, g.class.clone(), g.account_id))
                .or_insert_with(|| SeasonLine {
                    season,
                    tier: *tier,
                    division: division.clone(),
                    class: g.class.clone(),
                    account_id: g.account_id,
                    team: *team,
                    ..Default::default()
                });
            line.add(&adjusted, w);
            if let Some(s) = opp {
                line.opponents.0 += s;
                line.opponents.1 += 1;
            }
        }
        out
    }

    /// Whether a game was played for the player's own team: awards count
    /// only these (Flashy, S33 Low: three of a Sniper's eight officials were
    /// as a merc for other teams). A merc who joined that team later in the
    /// season is one of its players, as for medals. A match whose lineups
    /// were never read keeps every game: nothing says otherwise.
    pub fn own_team_game(&self, g: &hl_db::RatedGame) -> bool {
        let Some(id) = g.etf2l_match_id else { return true };
        let Some(roster) = self.rosters.get(&id).filter(|r| !r.is_empty()) else { return true };
        let Some(m) = self.matches.get(&id) else { return true };
        match roster.iter().find(|(a, _, _)| *a == g.account_id) {
            Some((_, Some(team), _)) if *team == m.clan1 || *team == m.clan2 => {
                !self.mercs.contains(&(id, g.account_id)) || self.joined_after(g.account_id, m.season, *team, m.time.unwrap_or(0))
            }
            _ => false,
        }
    }

    /// Season lines from own-team games only: what the awards are read from.
    pub fn award_lines(&self, games: &[hl_db::RatedGame]) -> HashMap<(i64, i64, String, u32), SeasonLine> {
        let own: Vec<hl_db::RatedGame> = games.iter().filter(|g| self.own_team_game(g)).cloned().collect();
        self.season_lines(&own)
    }

    /// Each team's officials in a season, by how its players rated in them:
    /// `(season, team) -> match -> the team's average official rating`.
    fn team_strength(&self, games: &[hl_db::RatedGame]) -> HashMap<(i64, i64), HashMap<i64, f64>> {
        // (match, account) -> (rating x minutes, minutes): one rating an official.
        let mut per: HashMap<(i64, u32), (f64, f64)> = HashMap::new();
        for g in games.iter().filter(|g| g.official) {
            let Some(id) = g.etf2l_match_id else { continue };
            let m = if g.minutes > 0.0 { g.minutes } else { 1.0 };
            let e = per.entry((id, g.account_id)).or_default();
            e.0 += g.score * m;
            e.1 += m;
        }
        let mut teams: HashMap<(i64, i64, i64), Vec<f64>> = HashMap::new();
        for ((id, a), (sm, m)) in per {
            let (Some(mt), Some(team)) = (self.matches.get(&id), self.rosters.get(&id).and_then(|r| r.iter().find(|(x, _, _)| *x == a)).and_then(|r| r.1)) else { continue };
            teams.entry((mt.season, team, id)).or_default().push(sm / m);
        }
        let mut out: HashMap<(i64, i64), HashMap<i64, f64>> = HashMap::new();
        for ((season, team, id), v) in teams {
            if v.len() >= 6 {
                out.entry((season, team)).or_default().insert(id, v.iter().sum::<f64>() / v.len() as f64);
            }
        }
        out
    }

    /// The thresholds and playoff places [`Catalogue::mvps`] reads.
    pub fn award_rules(&self, lines: &HashMap<(i64, i64, String, u32), SeasonLine>) -> AwardRules {
        let mut rules = AwardRules::default();
        for ((season, division), places) in self.medals() {
            if let Some(tier) = self.tier_of_name(season, &division) {
                for (_, team, _) in places {
                    rules.podium.insert((season, tier, team));
                }
            }
        }
        for m in self.matches.values().filter(|m| Self::played(m) && m.stage != "Cup") {
            if self.stage_weight(Some(m.match_id)) > 1.0 {
                rules.playoff_teams.insert((m.season, m.clan1));
                rules.playoff_teams.insert((m.season, m.clan2));
            }
        }
        // Each player's officials in the season, on any class.
        let mut per_player: HashMap<(i64, i64, u32), std::collections::HashSet<i64>> = HashMap::new();
        for l in lines.values() {
            per_player.entry((l.season, l.tier, l.account_id)).or_default().extend(l.officials.iter().copied());
            if rules.playoff_teams.contains(&(l.season, l.team)) {
                rules.has_playoffs.insert((l.season, l.tier));
            }
        }
        let mut most: HashMap<(i64, i64), usize> = HashMap::new();
        for ((season, tier, _), o) in per_player {
            let e = most.entry((season, tier)).or_default();
            *e = (*e).max(o.len());
        }
        for (key, n) in most {
            rules.need.insert(key, AWARD_MIN_OFFICIALS.max((n as f64 * AWARD_SHARE_OF_SEASON).ceil() as usize));
        }
        rules
    }

    /// Every division's awards, from the rated games (the MVP research,
    /// October 2026; HLTV's MVP and EVPs):
    ///
    /// - **Season MVP**, one per division: the champions' best. Players of
    ///   the team that won the Grand Final, with enough officials in the
    ///   season ([`AwardRules`]), who played more than half of the final's logs; scored
    ///   [`SEASON_MVP_WEIGHTS`] on their season, the final and the playoff
    ///   run, each pulled towards 1.00.
    /// - **EVPs**, [`EVPS`] per division ([`EVPS_SMALL`] below
    ///   [`EVPS_SMALL_BELOW_TEAMS`] teams): the best season ratings of any
    ///   team and class, the Season MVP aside.
    /// - **Best of the class**: the top of each class's season table among
    ///   players with enough officials on the class whose team took one of
    ///   the division's three medals -- by the same number the table ranks by.
    ///
    /// Enough officials is [`AWARD_SHARE_OF_SEASON`] of the most anyone in
    /// the division played that season, and never fewer than
    /// [`AWARD_MIN_OFFICIALS`].
    pub fn mvps(&self, games: &[hl_db::RatedGame]) -> Vec<Mvp> {
        // Own-team games only: a merc appearance is not a season for that team.
        let own: Vec<hl_db::RatedGame> = games.iter().filter(|g| self.own_team_game(g)).cloned().collect();
        let games = &own[..];
        let lines = self.season_lines(games);
        let rules = self.award_rules(&lines);
        let eligible = |l: &&SeasonLine| rules.enough(l);
        let mut out = Vec::new();
        // (season, tier) -> the Season MVPs of its divisions (two divisions
        // can share a tier: Fresh Meat and Freshest, Open A and Open B).
        let mut season_mvp: HashMap<(i64, i64), Vec<u32>> = HashMap::new();

        // The champions: who won each division's final, by division.
        let mut champions: HashMap<(i64, String), (i64, i64, &CatMatch)> = HashMap::new();
        let mut by_match: HashMap<i64, Vec<&hl_db::RatedGame>> = HashMap::new();
        for g in games {
            if let Some(id) = g.etf2l_match_id {
                by_match.entry(id).or_default().push(g);
            }
        }
        for v in by_match.values_mut() {
            if v.iter().any(|g| g.listed) {
                v.retain(|g| g.listed);
            }
        }
        for f in self.matches.values().filter(|m| Self::played(m) && Self::is_final(m)) {
            let winner = match (f.r1, f.r2) {
                (Some(a), Some(b)) if a > b => f.clan1,
                (Some(a), Some(b)) if b > a => f.clan2,
                _ => continue,
            };
            let division = self.medal_division(f);
            let key = (f.season, division.clone());
            let Some(tier) = crate::leagues::canonical_tier(&division).or_else(|| self.tiers.get(&key).copied()).or_else(|| self.tier_by_name.get(&division).copied()) else {
                continue;
            };
            champions.insert((f.season, division), (tier, winner, f));
        }

        // Season MVPs.
        for (&(season, _), &(tier, winner, f)) in &champions {
            let Some(in_final) = by_match.get(&f.match_id) else { continue };
            let mut team_of: HashMap<u32, i64> = self
                .rosters
                .get(&f.match_id)
                .map(|r| r.iter().filter_map(|(a, t, _)| Some((*a, (*t)?))).filter(|(_, t)| *t == f.clan1 || *t == f.clan2).collect())
                .unwrap_or_default();
            for (account, team) in Self::sides_from_logs(self.final_colours.get(&f.match_id), &team_of, f.clan1, f.clan2) {
                team_of.entry(account).or_insert(team);
            }
            // Who played the final, by minutes: a candidate played more than
            // half of it (a sub with one good half is not the Season MVP), on
            // the class they played most in it. Minutes, not logs: one map can
            // be uploaded as one log or three.
            let mut minutes: HashMap<u32, HashMap<&str, f64>> = HashMap::new();
            for g in in_final.iter() {
                *minutes.entry(g.account_id).or_default().entry(g.class.as_str()).or_default() += g.minutes.max(0.0);
            }
            let longest = minutes.values().map(|c| c.values().sum::<f64>()).fold(0.0, f64::max);
            let division = self.medal_division(f);
            let mut best: Option<Mvp> = None;
            for (a, by_class) in &minutes {
                let a = *a;
                if team_of.get(&a) != Some(&winner) {
                    continue;
                }
                if by_class.values().sum::<f64>() * 2.0 <= longest {
                    continue;
                }
                let Some((class, _)) = by_class.iter().max_by(|x, y| x.1.total_cmp(y.1)) else { continue };
                let Some(line) = lines.get(&(season, tier, class.to_string(), a)) else { continue };
                let Some(final_game) = line.games.get(&f.match_id) else { continue };
                // Officials in the season, on any class.
                let officials: std::collections::HashSet<i64> = lines
                    .iter()
                    .filter(|((s, t, _, acc), _)| *s == season && *t == tier && *acc == a)
                    .flat_map(|(_, l)| l.officials.iter().copied())
                    .collect();
                if officials.len() < rules.need(season, tier) {
                    continue;
                }
                // The rest of the playoffs: each official once, by its maps.
                let playoff: Vec<&OfficialGame> = line.games.iter().filter(|(id, o)| **id != f.match_id && (o.stage - PLAYOFF_WEIGHT).abs() < 1e-9).map(|(_, o)| o).collect();
                let maps_of = |o: &OfficialGame| o.maps.len().max(1) as f64;
                let final_maps = maps_of(final_game);
                let playoff_maps: f64 = playoff.iter().map(|o| maps_of(o)).sum();
                let s_rating = line.rating();
                let f_rating = shrunk(final_game.rating() * final_maps, final_maps);
                let p_rating = (!playoff.is_empty()).then(|| shrunk(playoff.iter().map(|o| o.rating() * maps_of(o)).sum(), playoff_maps));
                let (ws, wf, wp) = SEASON_MVP_WEIGHTS;
                let score = match p_rating {
                    Some(p) => ws * s_rating + wf * f_rating + wp * p,
                    None => (ws + wp) * s_rating + wf * f_rating,
                };
                let m = Mvp {
                    season,
                    season_name: self.season_name(season),
                    division: division.clone(),
                    tier: Some(tier),
                    class: class.to_string(),
                    account_id: a,
                    team: self.team(winner),
                    won: true,
                    event: true,
                    kind: "season".into(),
                    score,
                    season_rating: s_rating,
                    officials: officials.len(),
                    season_maps: line.maps(),
                    final_rating: Some(final_game.rating()),
                    final_maps: final_maps as usize,
                    playoffs_rating: (!playoff.is_empty()).then(|| playoff.iter().map(|o| o.rating() * maps_of(o)).sum::<f64>() / playoff_maps),
                    playoffs_maps: playoff_maps as usize,
                };
                // Ties: more officials, then more final maps.
                let better = best
                    .as_ref()
                    .is_none_or(|b| m.score.total_cmp(&b.score).then(m.officials.cmp(&b.officials)).then(m.final_maps.cmp(&b.final_maps)).is_gt());
                if better {
                    best = Some(m);
                }
            }
            if let Some(m) = best {
                season_mvp.entry((season, tier)).or_default().push(m.account_id);
                out.push(m);
            }
        }

        let award = |l: &SeasonLine, kind: &str| {
            let won = champions.values().any(|(t, w, f)| f.season == l.season && *t == l.tier && *w == l.team);
            Mvp {
                season: l.season,
                season_name: self.season_name(l.season),
                division: l.division.clone(),
                tier: Some(l.tier),
                class: l.class.clone(),
                account_id: l.account_id,
                team: self.team(l.team),
                won,
                event: false,
                kind: kind.into(),
                score: l.rating(),
                season_rating: l.rating(),
                officials: l.officials.len(),
                season_maps: l.maps(),
                final_rating: None,
                final_maps: 0,
                playoffs_rating: None,
                playoffs_maps: 0,
            }
        };
        let better = |a: &&SeasonLine, b: &&SeasonLine| a.rating().total_cmp(&b.rating()).then(a.officials.len().cmp(&b.officials.len()));

        // Best of each class. By division, not only tier: two divisions can
        // share a tier (S33's Fresh Meat and Freshest), and each has its own.
        let mut tables: HashMap<(i64, i64, &str, &str), Vec<&SeasonLine>> = HashMap::new();
        for l in lines.values().filter(|l| rules.class_award(l)) {
            tables.entry((l.season, l.tier, l.division.as_str(), l.class.as_str())).or_default().push(l);
        }
        for rows in tables.values() {
            if let Some(top) = rows.iter().max_by(|a, b| better(a, b)) {
                out.push(award(top, "class"));
            }
        }

        // EVPs: each player's best class; the division's best seasons.
        let mut teams: HashMap<(i64, i64, &str), std::collections::HashSet<i64>> = HashMap::new();
        for l in lines.values() {
            teams.entry((l.season, l.tier, l.division.as_str())).or_default().insert(l.team);
        }
        let mut divisions: HashMap<(i64, i64, &str), HashMap<u32, &SeasonLine>> = HashMap::new();
        for l in lines.values().filter(eligible) {
            let e = divisions.entry((l.season, l.tier, l.division.as_str())).or_default();
            let keep = e.get(&l.account_id).is_some_and(|b| better(b, &l).is_ge());
            if !keep {
                e.insert(l.account_id, l);
            }
        }
        for (key, by) in divisions {
            let n = if teams.get(&key).map_or(0, |t| t.len()) < EVPS_SMALL_BELOW_TEAMS { EVPS_SMALL } else { EVPS };
            let mut rows: Vec<&SeasonLine> = by.into_values().filter(|l| !season_mvp.get(&(key.0, key.1)).is_some_and(|v| v.contains(&l.account_id))).collect();
            rows.sort_by(|a, b| better(b, a));
            out.extend(rows.into_iter().take(n).map(|l| award(l, "evp")));
        }

        let order = |k: &str| match k {
            "season" => 0,
            "evp" => 1,
            _ => 2,
        };
        out.sort_by(|a, b| {
            season_order(b.season)
                .cmp(&season_order(a.season))
                .then(a.tier.cmp(&b.tier))
                .then(order(&a.kind).cmp(&order(&b.kind)))
                .then(b.score.total_cmp(&a.score))
        });
        out
    }

    /// The rule before October 2026, kept for `hl mvp-audit` to print old
    /// and new side by side for a release: per Grand Final, the best player
    /// of each class on the two finalists (70% the final, 30% the playoff
    /// run before it), and the event's -- the best of the winning team's.
    pub fn mvps_final_only(&self, games: &[hl_db::RatedGame]) -> Vec<Mvp> {
        let mut by_match: HashMap<i64, Vec<&hl_db::RatedGame>> = HashMap::new();
        for g in games {
            if let Some(id) = g.etf2l_match_id {
                by_match.entry(id).or_default().push(g);
            }
        }
        // A match with its per-map logs held reads those, not also the
        // combined upload of the same games.
        for v in by_match.values_mut() {
            if v.iter().any(|g| g.listed) {
                v.retain(|g| g.listed);
            }
        }
        // (season, division) -> the playoff matches before the final.
        let mut playoffs: HashMap<(i64, String), Vec<i64>> = HashMap::new();
        for m in self.matches.values().filter(|m| Self::played(m) && m.stage != "Cup") {
            let playoff = matches!(m.stage.as_str(), "Playoffs" | "3rd Place") || Self::round_kind(m.round.as_deref()).is_some();
            if playoff && !Self::is_final(m) {
                playoffs.entry((m.season, self.medal_division(m))).or_default().push(m.match_id);
            }
        }
        let mean = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
        let mut out = Vec::new();
        for f in self.matches.values().filter(|m| Self::played(m) && Self::is_final(m)) {
            let Some(in_final) = by_match.get(&f.match_id) else { continue };
            let winner = match (f.r1, f.r2) {
                (Some(a), Some(b)) if a > b => f.clan1,
                (Some(a), Some(b)) if b > a => f.clan2,
                _ => continue,
            };
            let division = self.medal_division(f);
            let key = (f.season, division.clone());
            let mut team_of: HashMap<u32, i64> = self
                .rosters
                .get(&f.match_id)
                .map(|r| r.iter().filter_map(|(a, t, _)| Some((*a, (*t)?))).filter(|(_, t)| *t == f.clan1 || *t == f.clan2).collect())
                .unwrap_or_default();
            // Whoever the roster does not place -- a merc, a ringer ETF2L
            // never listed (S24 Mid: a Sniper who played all five maps of the
            // final at 1.06 lost the MVP to a 0.83) -- is on the side they
            // played: each log's colour is the finalist whose roster players
            // were on it.
            for (account, team) in Self::sides_from_logs(self.final_colours.get(&f.match_id), &team_of, f.clan1, f.clan2) {
                team_of.entry(account).or_insert(team);
            }
            let before: Vec<&hl_db::RatedGame> =
                playoffs.get(&key).into_iter().flatten().filter_map(|id| by_match.get(id)).flatten().copied().collect();
            let tier = crate::leagues::canonical_tier(&division).or_else(|| self.tiers.get(&key).copied()).or_else(|| self.tier_by_name.get(&division).copied());
            // A final is several logs: one per KOTH round, one per stopwatch
            // half. A candidate played more than half of them -- a sub with
            // one good half, or one map of two, is not the final's MVP.
            let mut logs: Vec<i64> = in_final.iter().map(|g| g.log_id).collect();
            logs.sort_unstable();
            logs.dedup();
            let mut played: HashMap<u32, std::collections::HashSet<i64>> = HashMap::new();
            for g in in_final.iter() {
                played.entry(g.account_id).or_default().insert(g.log_id);
            }
            // (account, class) -> final scores
            let mut fin: HashMap<(u32, &str), Vec<f64>> = HashMap::new();
            for g in in_final.iter() {
                fin.entry((g.account_id, g.class.as_str())).or_default().push(g.score);
            }
            let candidates: Vec<Mvp> = fin
                .into_iter()
                .filter_map(|((a, class), scores)| {
                    let team = *team_of.get(&a)?;
                    if team != f.clan1 && team != f.clan2 {
                        return None;
                    }
                    if played.get(&a).map_or(0, |p| p.len()) * 2 <= logs.len() && logs.len() > 1 {
                        return None;
                    }
                    let final_rating = mean(&scores)?;
                    let rest: Vec<f64> = before.iter().filter(|g| g.account_id == a && g.class == class).map(|g| g.score).collect();
                    let playoffs_rating = mean(&rest);
                    let score = mvp_score(final_rating, playoffs_rating, rest.len());
                    Some(Mvp {
                        season: f.season,
                        season_name: self.season_name(f.season),
                        division: division.clone(),
                        tier,
                        class: class.to_string(),
                        account_id: a,
                        team: self.team(team),
                        won: team == winner,
                        event: false,
                        kind: "class".into(),
                        score,
                        season_rating: 0.0,
                        officials: 0,
                        season_maps: 0,
                        final_rating: Some(final_rating),
                        final_maps: scores.len(),
                        playoffs_rating,
                        playoffs_maps: rest.len(),
                    })
                })
                .collect();
            // Best score; a tie goes to the team that won.
            let better = |x: &&Mvp, y: &&Mvp| x.score.total_cmp(&y.score).then(x.won.cmp(&y.won));
            let mut classes: Vec<&str> = candidates.iter().map(|c| c.class.as_str()).collect();
            classes.sort_unstable();
            classes.dedup();
            let mut class_mvps: Vec<Mvp> =
                classes.iter().filter_map(|c| candidates.iter().filter(|m| m.class == *c).max_by(better).cloned()).collect();
            // The event's: the best player of the team that won it, as
            // HLTV's goes to the champions -- whether or not they out-rated
            // the losing side's player on their own class.
            if let Some(top) = candidates.iter().filter(|m| m.won).max_by(better) {
                let mut e = top.clone();
                e.event = true;
                e.kind = "season".into();
                class_mvps.push(e);
            }
            out.extend(class_mvps);
        }
        out.sort_by(|a, b| season_order(b.season).cmp(&season_order(a.season)).then(a.tier.cmp(&b.tier)).then(b.event.cmp(&a.event)));
        out
    }

    /// One player's seasons, medals and officials.
    fn player(&self, account: u32, medals: &BTreeMap<(i64, String), Vec<(u8, i64, String)>>) -> (Vec<PlayerSeason>, Vec<Medal>, Vec<Official>) {
        let mut officials: Vec<Official> = Vec::new();
        // (season, team) -> (division counts, played, won, lost)
        let mut seasons: HashMap<(i64, i64), (HashMap<(String, Option<i64>), u32>, u32, u32, u32)> = HashMap::new();
        // (season, team) they were on the roster for -- not only a merc in.
        // A team's medal is its roster's (twatter: three medals from one
        // sub appearance each for other teams).
        let mut rostered: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
        for (match_id, roster) in &self.rosters {
            let Some(&(_, Some(team), _)) = roster.iter().find(|(a, _, _)| *a == account) else { continue };
            let Some(m) = self.matches.get(match_id) else { continue };
            if !Self::played(m) {
                continue;
            }
            // A merc who joined the team later that season and was on its
            // roster at the end is one of its players (Flashy: one official
            // as a merc in week 1, on DD14's roster from the next day, and
            // DD14 won S36 High): only a merc who never joined is not.
            if !self.mercs.contains(&(*match_id, account)) || self.joined_after(account, m.season, team, m.time.unwrap_or(0)) {
                rostered.insert((m.season, team));
            }
            let (division, tier) = self.division_of(m);
            let (f, a, won) = Self::result_for(m, team);
            // A tournament is their official, not their season.
            if m.stage != "Cup" || m.comp_name.is_empty() || crate::leagues::parse_name(&m.comp_name).is_some() {
                let e = seasons.entry((m.season, team)).or_default();
                *e.0.entry((division.clone(), tier)).or_default() += 1;
                e.1 += 1;
                e.2 += u32::from(won == Some(true));
                e.3 += u32::from(won == Some(false));
            }
            officials.push(Official {
                match_id: m.match_id,
                time: m.time,
                season: m.season,
                competition: m.comp_name.clone(),
                tier,
                division,
                stage: m.stage.clone(),
                round: m.round.clone(),
                team: self.team(team),
                opponent: self.team(if m.clan1 == team { m.clan2 } else { m.clan1 }),
                score_for: f,
                score_against: a,
                won,
            });
        }
        officials.sort_by_key(|o| std::cmp::Reverse(o.time.unwrap_or(0)));

        // A season's own name, not a tournament's kept with it.
        let season_name = |s: i64| {
            let of = |m: &&CatMatch| m.season == s;
            self.matches
                .values()
                .filter(of)
                .find(|m| m.stage != "Cup")
                .or_else(|| self.matches.values().find(of))
                .map(|m| m.season_name.clone())
                .unwrap_or_default()
        };
        let mut won_medals: Vec<Medal> = Vec::new();
        let mut out: Vec<PlayerSeason> = seasons
            .into_iter()
            .map(|((season, team), (divs, played, won, lost))| {
                // The division they played most for this team that season.
                let ((division, tier), _) = divs.into_iter().max_by_key(|(_, n)| *n).unwrap_or(((String::new(), None), 0));
                let left_early = rostered.contains(&(season, team)) && !self.on_roster_at_end(account, season, team);
                let place = medals.iter().filter(|((s, _), _)| *s == season && rostered.contains(&(season, team)) && !left_early).find_map(|((_, d), list)| {
                    list.iter().find(|(_, t, _)| *t == team).map(|(p, _, how)| (*p, d.clone(), how.clone()))
                });
                if let Some((p, d, how)) = &place {
                    won_medals.push(Medal {
                        season,
                        season_name: season_name(season),
                        division: d.clone(),
                        tier: crate::leagues::canonical_tier(d).or_else(|| self.tiers.get(&(season, d.clone())).copied()).or_else(|| self.tier_by_name.get(d).copied()),
                        place: *p,
                        team: self.team(team),
                        how: how.clone(),
                    });
                }
                let merc = !rostered.contains(&(season, team));
                PlayerSeason { season, season_name: season_name(season), division, tier, team: self.team(team), played, won, lost, place: place.map(|p| p.0), merc, left_early }
            })
            .collect();
        out.sort_by(|a, b| season_order(b.season).cmp(&season_order(a.season)).then(b.played.cmp(&a.played)));
        won_medals.sort_by(|a, b| season_order(b.season).cmp(&season_order(a.season)).then(a.place.cmp(&b.place)));
        (out, won_medals, officials)
    }
}

fn main_of(classes: Option<&HashMap<String, i64>>) -> (Vec<(String, i64)>, Option<String>) {
    let mut played: Vec<(String, i64)> = classes.map(|c| c.iter().map(|(k, v)| (k.clone(), *v)).collect()).unwrap_or_default();
    played.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let main = played.first().map(|(c, _)| c.clone());
    (played, main)
}

/// Search the catalogue and the owner's matches by name or Steam ID.
pub async fn search(db: &Db, query: &str, limit: usize) -> Result<Vec<Hit>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let by_id = SteamId::parse(query.trim()).ok().map(|s| s.account_id());
    let cat = Catalogue::load(db).await?;
    let log_names = db.log_names().await?;
    // Every name each account has used, newest first where known.
    let mut names: HashMap<u32, Vec<String>> = log_names;
    let mut officials: HashMap<u32, (u32, i64)> = HashMap::new();
    for (match_id, roster) in &cat.rosters {
        let t = cat.matches.get(match_id).and_then(|m| m.time).unwrap_or(0);
        for (a, _, n) in roster {
            let e = officials.entry(*a).or_default();
            e.0 += 1;
            e.1 = e.1.max(t);
            if !n.is_empty() {
                let v = names.entry(*a).or_default();
                if !v.contains(n) {
                    v.insert(0, n.clone());
                }
            }
        }
    }
    let mut found: Vec<u32> = names
        .iter()
        .filter(|(a, ns)| by_id == Some(**a) || (by_id.is_none() && ns.iter().any(|n| n.to_lowercase().contains(&q))))
        .map(|(a, _)| *a)
        .collect();
    // Exact and prefix matches first, then the most officials played.
    let rank = |a: &u32| {
        let ns = &names[a];
        let exact = ns.iter().any(|n| n.to_lowercase() == q);
        let prefix = ns.iter().any(|n| n.to_lowercase().starts_with(&q));
        (std::cmp::Reverse(exact), std::cmp::Reverse(prefix), std::cmp::Reverse(officials.get(a).map_or(0, |o| o.0)))
    };
    found.sort_by_key(rank);
    found.truncate(limit);

    let medals = cat.medals();
    let divisions = cat.divisions();
    let classes = db.main_class_games().await?;
    Ok(found
        .into_iter()
        .map(|a| {
            let (_, won, _) = cat.player(a, &medals);
            let mut m = [0u32; 3];
            for x in &won {
                m[usize::from(x.place - 1)] += 1;
            }
            Hit {
                account_id: a,
                name: names[&a].first().cloned().unwrap_or_default(),
                highest: Catalogue::highest_of(&divisions, a),
                main_class: main_of(classes.get(&a)).1,
                medals: m,
                officials: officials.get(&a).map_or(0, |o| o.0),
                last_seen: officials.get(&a).map(|o| o.1).filter(|t| *t > 0),
            }
        })
        .collect())
}

/// One player's profile. ETF2L's page for them is fetched if it is not
/// held or is over a week old; a failure there leaves those fields empty.
pub async fn profile(db: &Db, sources: &Sources, account: u32) -> Result<Profile> {
    let cat = Catalogue::load(db).await?;
    let medals = cat.medals();
    let (seasons, won, officials) = cat.player(account, &medals);
    let rated = rated_officials(db, None).await?;
    let mvps: Vec<Mvp> = cat.mvps(&rated).into_iter().filter(|m| m.account_id == account).collect();
    let steamid = SteamId::from_account_id(account);
    let raw_of = |id: Option<i64>, raws: &[(i64, i64, String)]| id.and_then(|id| raws.iter().find(|(i, ..)| *i == id).map(|(.., json)| json.clone()));
    let raws = db.etf2l_raw("player").await?;
    let etf2l = match db.etf2l_player(account).await? {
        // Kept a week; a row from before the raw page was kept is read again.
        Some(p) if now() - p.fetched_at < ETF2L_PLAYER_KEEP_S && raw_of(p.etf2l_id, &raws).is_some() => Some(p),
        held => match fetch_etf2l_player(sources, &steamid.to_steamid64()).await {
            Ok(Some((p, body))) => {
                db.put_etf2l_player(account, &p).await?;
                // The whole answer too, for what the row has no column for:
                // their teams, title and bans.
                if let Some(id) = p.etf2l_id {
                    db.store_etf2l_raw("player", id, &body).await?;
                }
                Some(p)
            }
            Ok(None) => held,
            Err(e) => {
                tracing::warn!(account, error = %format!("{e:#}"), "ETF2L player page");
                held
            }
        },
    };
    let classes = db.main_class_games().await?;
    let (played_classes, main_class) = main_of(classes.get(&account));
    let mut aliases: Vec<String> = Vec::new();
    for roster in cat.rosters.values() {
        for (a, _, n) in roster {
            if *a == account && !n.is_empty() && !aliases.contains(n) {
                aliases.push(n.clone());
            }
        }
    }
    for n in db.log_names().await?.remove(&account).unwrap_or_default() {
        if !aliases.contains(&n) {
            aliases.push(n);
        }
    }
    // And what thirteen years of logs.tf knew them as (the bundled summary):
    // someone the app has never met still gets a name, not a Steam ID.
    if let Some((n, more)) = crate::logstf::names(account) {
        for n in std::iter::once(n).chain(more) {
            if !n.is_empty() && !aliases.contains(&n) {
                aliases.push(n);
            }
        }
    }
    let name = etf2l.as_ref().and_then(|p| p.name.clone()).or_else(|| aliases.first().cloned()).unwrap_or_else(|| steamid.to_steamid64());
    aliases.retain(|a| *a != name);
    let raw = match raw_of(etf2l.as_ref().and_then(|p| p.etf2l_id), &raws) {
        Some(json) => Some(json),
        // Just fetched: read it again, with what was stored since.
        None => raw_of(etf2l.as_ref().and_then(|p| p.etf2l_id), &db.etf2l_raw("player").await?),
    };
    let (etf2l_title, etf2l_teams, bans) = raw.as_deref().map(etf2l_extras).unwrap_or_default();
    Ok(Profile {
        account_id: account,
        steamid64: steamid.to_steamid64(),
        name,
        aliases,
        country: etf2l.as_ref().and_then(|p| p.country.clone()),
        avatar: etf2l.as_ref().and_then(|p| p.avatar.clone()),
        declared_classes: etf2l.as_ref().map(|p| p.classes.clone()).unwrap_or_default(),
        played_classes,
        main_class,
        current: seasons.first().cloned(),
        highest: Catalogue::highest_of(&cat.divisions(), account),
        medals: won,
        mvps,
        seasons,
        officials,
        registered: etf2l.as_ref().and_then(|p| p.registered),
        etf2l_id: etf2l.and_then(|p| p.etf2l_id),
        etf2l_title,
        etf2l_teams,
        bans,
    })
}

/// What a stored ETF2L player page says beyond the profile row.
fn etf2l_extras(json: &str) -> (Option<String>, Vec<Etf2lRoster>, Vec<Etf2lBan>) {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return Default::default() };
    let p = v.get("player").unwrap_or(&v);
    let text = |x: &Value, k: &str| x.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let teams = p
        .get("teams")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| {
            Some(Etf2lRoster {
                id: t.get("id")?.as_i64()?,
                name: text(t, "name")?,
                tag: text(t, "tag"),
                kind: text(t, "type"),
                country: text(t, "country"),
                avatar: t.pointer("/steam/avatar").and_then(Value::as_str).filter(|a| a.starts_with("https://")).map(str::to_string),
            })
        })
        .collect();
    let bans = p
        .get("bans")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|b| Etf2lBan { start: b.get("start").and_then(Value::as_i64), end: b.get("end").and_then(Value::as_i64), reason: text(b, "reason") })
        .collect();
    (text(p, "title"), teams, bans)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

async fn fetch_etf2l_player(sources: &Sources, steamid64: &str) -> Result<Option<(Etf2lPlayer, String)>> {
    let Some(body) = sources.etf2l_get_interactive(&format!("/player/{steamid64}")).await? else { return Ok(None) };
    let v: Value = serde_json::from_str(&body).context("ETF2L player page")?;
    let p = v.get("player").unwrap_or(&Value::Null);
    if p.is_null() {
        return Ok(None);
    }
    Ok(Some((Etf2lPlayer {
        etf2l_id: p.get("id").and_then(Value::as_i64),
        name: p.get("name").and_then(Value::as_str).map(str::to_string),
        country: p.get("country").and_then(Value::as_str).map(str::to_string),
        classes: p.get("classes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect(),
        avatar: p.pointer("/steam/avatar").and_then(Value::as_str).map(str::to_string),
        registered: p.get("registered").and_then(Value::as_i64),
        fetched_at: now(),
    }, body)))
}

/// Index the players of league-sample logs that are not indexed yet: their
/// team and main class, so a player's main class reads from the league too.
/// Returns how many logs were indexed.
pub async fn index_league_players(db: &Db, limit: i64) -> Result<usize> {
    let logs = db.league_logs_unindexed(limit).await?;
    let n = logs.len();
    for (log_id, json) in logs {
        let rows: Vec<(u32, String, Option<&'static str>, i64)> = match serde_json::from_str::<Value>(&json).ok().and_then(|v| crate::normalize::normalize(log_id, &v).ok()) {
            Some(log) => log
                .players
                .iter()
                .map(|p| (p.id.account_id(), p.team.as_str().to_string(), p.main_class().map(|c| c.as_str()), p.total_time()))
                .collect(),
            None => Vec::new(),
        };
        let refs: Vec<(u32, Option<&str>, Option<&str>, i64)> = rows.iter().map(|(a, t, c, s)| (*a, Some(t.as_str()), *c, *s)).collect();
        // A log that cannot be read still gets a (empty) mark by being
        // skipped here; it is retried next time, which costs one parse.
        if !refs.is_empty() {
            db.put_league_players(log_id, &refs).await?;
        }
    }
    Ok(n)
}

/// One season's medals by division: `(place, team name, how)`, for checking
/// against ETF2L (`hl medals <season>`).
pub async fn season_medals(db: &Db, season: i64) -> Result<Vec<(String, Vec<(u8, String, String)>)>> {
    let cat = Catalogue::load(db).await?;
    Ok(cat
        .medals()
        .into_iter()
        .filter(|((s, _), _)| *s == season)
        .map(|((_, d), mut list)| {
            list.sort_by_key(|x| x.0);
            (d, list.into_iter().map(|(p, t, how)| (p, cat.team(t).name, how)).collect())
        })
        .collect())
}

// ---- Ratings, stat bars and ranks (Q36) ----------------------------------

/// Officials in a season, class and division before a player is ranked.
/// Counted as officials, not logs: the owner's matches hold one combined log
/// per official and the league sample one log per map.
/// Officials needed to be ranked (Flashy and function: "3-4 sounds good";
/// one official at 1.50 must not top a division). Everyone with fewer is
/// still listed, greyed out and unranked.
pub const MIN_RANK_GAMES: usize = 3;
/// "Recent form": the games in this long before a player's newest.
const RECENT_S: i64 = 90 * 24 * 3600;
/// Recent games needed before the bars show recent form, not the career.
const MIN_RECENT_GAMES: usize = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassStats {
    pub class: String,
    pub games: usize,
    pub career: f64,
    pub recent: Option<f64>,
    pub recent_games: usize,
    pub best: f64,
    /// Each component group, 0-100 (recent form where there is enough of
    /// it, else the career): the stat bars.
    pub groups: Vec<(String, f64)>,
    pub groups_recent: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rank {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: i64,
    pub class: String,
    pub rank: usize,
    pub of: usize,
    pub avg: f64,
    pub games: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStats {
    /// Most played first.
    pub classes: Vec<ClassStats>,
    /// Newest season first, best rank first within it.
    pub ranks: Vec<Rank>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankRow {
    /// `None` under [`MIN_RANK_GAMES`] officials: listed, not ranked.
    pub rank: Option<usize>,
    pub account_id: u32,
    pub name: String,
    pub team: Option<Team>,
    pub games: usize,
    pub avg: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rankings {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: i64,
    pub class: String,
    pub rows: Vec<RankRow>,
    /// Seasons that can be ranked, newest first: `(season, name)`.
    pub seasons: Vec<(i64, String)>,
    /// Tiers seen in the chosen season: `(tier, name)`.
    pub divisions: Vec<(i64, String)>,
}

impl Catalogue {
    /// Each season's dates, from its officials: `(season, name, from, to)`,
    /// a few days either side for the matches played around them. Newest
    /// first.
    pub fn season_windows(&self) -> Vec<(i64, String, i64, i64)> {
        // keyed by the season's place in time: AFA 2025 between 34 and 35
        let mut w: BTreeMap<i64, (i64, String, i64, i64)> = BTreeMap::new();
        // The league's dates: a tournament kept with a season can be months off.
        for m in self.matches.values().filter(|m| m.stage != "Cup") {
            let Some(t) = m.time else { continue };
            let e = w.entry(season_order(m.season)).or_insert((m.season, m.season_name.clone(), t, t));
            e.2 = e.2.min(t);
            e.3 = e.3.max(t);
        }
        const SLACK: i64 = 3 * 24 * 3600;
        w.into_values().rev().map(|(s, n, a, b)| (s, n, a - SLACK, b + SLACK)).collect()
    }

    /// Every player's division each season, by the rule in the module doc:
    /// `(tier, division, team)`. A season with no division that counts is
    /// left out.
    fn divisions(&self) -> HashMap<(u32, i64), (i64, String, i64)> {
        // (account, season) -> tier -> (officials, team -> officials)
        let mut count: HashMap<(u32, i64), BTreeMap<i64, (u32, HashMap<i64, u32>)>> = HashMap::new();
        // (account, season) -> tiers whose Grand Final they played, and for whom
        let mut finals: HashMap<(u32, i64), Vec<(i64, i64)>> = HashMap::new();
        // (season, tier) -> the division's name that season
        let mut names: HashMap<(i64, i64), String> = HashMap::new();
        for (match_id, roster) in &self.rosters {
            let Some(m) = self.matches.get(match_id) else { continue };
            if !Self::played(m) {
                continue;
            }
            // A preseason cup is an official, not the league.
            if m.stage == "Cup" {
                continue;
            }
            let (division, Some(tier)) = self.division_of(m) else { continue };
            names.entry((m.season, tier)).or_insert_with(|| division.clone());
            // A Grand Final's finalists count one division up -- up that
            // season's own ladder: AFA 2025's Division 2 finalists are
            // Division 1 (High), not Premiership, which AFA ran on its own
            // above Division 1.
            let grand_final = Self::is_final(m);
            let promoted = grand_final.then(|| self.division_above(m)).flatten();
            for (a, team, _) in roster {
                let Some(team) = team else { continue };
                let e = count.entry((*a, m.season)).or_default().entry(tier).or_default();
                e.0 += 1;
                *e.1.entry(*team).or_default() += 1;
                if grand_final {
                    if let Some(up) = promoted {
                        finals.entry((*a, m.season)).or_default().push((up, *team));
                    }
                }
            }
        }
        let name = |season: i64, tier: i64| {
            crate::leagues::TIER_NAMES
                .get(tier as usize)
                .map(|n| n.to_string())
                .or_else(|| names.get(&(season, tier)).cloned())
                .unwrap_or_default()
        };
        let mut out = HashMap::new();
        for (key, tiers) in count {
            // Three officials in a division...
            let by_officials = tiers
                .iter()
                .filter(|(_, (n, _))| *n as usize >= MIN_FOR_DIVISION)
                .map(|(t, (_, teams))| (*t, *teams.iter().max_by_key(|(_, n)| **n).map(|(team, _)| team).unwrap_or(&0)));
            // ...or its Grand Final, one division below.
            let by_final = finals.get(&key).into_iter().flatten().map(|(t, team)| (*t, *team));
            if let Some((tier, team)) = by_officials.chain(by_final).min_by_key(|(t, _)| *t) {
                out.insert(key, (tier, name(key.1, tier), team));
            }
        }
        out
    }

    /// Every player's division as played each season -- the one they played
    /// most league officials in, without the Grand Final's step up -- for
    /// ranks: a season is ranked among the division it was played in
    /// (Flashy: "just list the div of the season").
    fn played_divisions(&self) -> HashMap<(u32, i64), (i64, String, i64)> {
        let mut count: HashMap<(u32, i64), HashMap<(i64, i64), u32>> = HashMap::new();
        for (match_id, roster) in &self.rosters {
            let Some(m) = self.matches.get(match_id) else { continue };
            if !Self::played(m) || m.stage == "Cup" {
                continue;
            }
            let (_, Some(tier)) = self.division_of(m) else { continue };
            for (a, team, _) in roster {
                let Some(team) = team else { continue };
                *count.entry((*a, m.season)).or_default().entry((tier, *team)).or_default() += 1;
            }
        }
        count
            .into_iter()
            .filter_map(|(k, v)| {
                let ((tier, team), _) = v.into_iter().max_by_key(|(_, n)| *n)?;
                let name = crate::leagues::TIER_NAMES.get(tier as usize).map_or_else(String::new, |n| n.to_string());
                Some((k, (tier, name, team)))
            })
            .collect()
    }

    /// The best division that counted for them in any season.
    fn highest_of(divisions: &HashMap<(u32, i64), (i64, String, i64)>, account: u32) -> Option<Division> {
        divisions
            .iter()
            .filter(|((a, _), _)| *a == account)
            .min_by_key(|((_, season), (tier, _, _))| (*tier, -season))
            .map(|(_, (tier, name, _))| Division { name: name.clone(), tier: *tier })
    }

    pub fn name_of(&self, account: u32) -> String {
        let mut newest: Option<(i64, &str)> = None;
        for (match_id, roster) in &self.rosters {
            let t = self.matches.get(match_id).and_then(|m| m.time).unwrap_or(0);
            for (a, _, n) in roster {
                if *a == account && !n.is_empty() && newest.is_none_or(|(nt, _)| t > nt) {
                    newest = Some((t, n));
                }
            }
        }
        newest.map_or_else(|| SteamId::from_account_id(account).to_steamid64(), |(_, n)| n.to_string())
    }
}

/// Every rated official, counted the same way for everyone: the league
/// sample's ratings map by map, and the owner's own officials only where the
/// league holds none of that match for them. A combined log of the owner's
/// used to stand in for the per-map logs it covers, so their own seasons
/// counted one game per official where everyone else's counted each map
/// (Flashy: S31 Low, 9 "maps" over 7 officials). For season tables, ranks
/// and awards.
pub async fn rated_officials(db: &Db, account: Option<u32>) -> Result<Vec<hl_db::RatedGame>> {
    // A combined upload filed beside its own parts, and a second upload of
    // the same game, are left out (`data_audit::classify`).
    let covering = crate::data_audit::excluded_logs(db).await?;
    let mut out: Vec<hl_db::RatedGame> =
        db.league_ratings_all(hl_rating::MODEL_VERSION, account).await?.into_iter().filter(|g| !covering.contains(&g.log_id)).collect();
    let league_logs: std::collections::HashSet<(u32, i64)> = out.iter().map(|g| (g.account_id, g.log_id)).collect();
    let league_matches: std::collections::HashSet<(u32, i64)> = out.iter().filter_map(|g| Some((g.account_id, g.etf2l_match_id?))).collect();
    for g in db.rated_games(hl_rating::MODEL_VERSION, account).await? {
        if !g.official || league_logs.contains(&(g.account_id, g.log_id)) {
            continue;
        }
        if g.etf2l_match_id.is_some_and(|m| league_matches.contains(&(g.account_id, m))) {
            continue;
        }
        out.push(g);
    }
    Ok(out)
}

/// A rated game's season: its ETF2L match's, where known, else by date.
/// Seasons overlap by date (AFA 2025 began before Season 34's playoffs were
/// over), and an official belongs to its competition's season.
pub fn game_season(cat: &Catalogue, windows: &[(i64, String, i64, i64)], g: &hl_db::RatedGame) -> Option<i64> {
    match g.etf2l_match_id.and_then(|id| cat.matches.get(&id)) {
        // A cup is an official but not the season's league: not ranked.
        Some(m) if m.stage == "Cup" => None,
        Some(m) => Some(m.season),
        None => season_of(windows, g.played_at),
    }
}

fn season_of(windows: &[(i64, String, i64, i64)], at: i64) -> Option<i64> {
    windows.iter().find(|(_, _, a, b)| at >= *a && at <= *b).map(|(s, _, _, _)| *s)
}

fn groups_of(g: &hl_db::RatedGame) -> Vec<(String, f64)> {
    if let Some(groups) = &g.groups {
        return serde_json::from_str::<serde_json::Map<String, Value>>(groups)
            .map(|m| m.into_iter().filter_map(|(k, v)| Some((k, v.as_f64()?))).collect())
            .unwrap_or_default();
    }
    let Some(parts) = g.parts.as_deref().and_then(|p| serde_json::from_str::<Vec<hl_rating::model::Part>>(p).ok()) else { return Vec::new() };
    hl_rating::guide::group_scores(&parts).into_iter().map(|(g, v)| (g.key().to_string(), v)).collect()
}

fn mean(xs: impl Iterator<Item = f64>) -> Option<f64> {
    let (s, n) = xs.fold((0.0, 0usize), |(s, n), x| (s + x, n + 1));
    (n > 0).then(|| s / n as f64)
}

/// One season, division and class, everyone with enough officials, best
/// first: `(account, officials, average rating, team)`. The average is over
/// their logs; the threshold is over officials (see [`MIN_RANK_GAMES`]).
fn rank_table(
    cat: &Catalogue,
    games: &[hl_db::RatedGame],
    windows: &[(i64, String, i64, i64)],
    divisions: &HashMap<(u32, i64), (i64, String, i64)>,
    season: i64,
    tier: i64,
    class: &str,
) -> Vec<(u32, usize, f64, i64)> {
    rank_table_from(cat, games, windows, divisions, season, tier, class, MIN_RANK_GAMES)
}

/// [`rank_table`] down to `min` officials, best first: `(account,
/// officials, season rating, team)`. Ranked by the season rating the awards
/// use -- weighted by stage and pulled towards 1.00 by [`SHRINK_MAPS`] -- so
/// a table and the award beside it cannot disagree (the MVP research).
fn rank_table_from(
    cat: &Catalogue,
    games: &[hl_db::RatedGame],
    _windows: &[(i64, String, i64, i64)],
    _divisions: &HashMap<(u32, i64), (i64, String, i64)>,
    season: i64,
    tier: i64,
    class: &str,
    min: usize,
) -> Vec<(u32, usize, f64, i64)> {
    // Officials only: the sample holds nothing else, so a player's pugs and
    // scrims would put them on a different footing from everyone else's.
    let games: Vec<hl_db::RatedGame> = games.iter().filter(|g| g.class == class && g.official).cloned().collect();
    let mut rows: Vec<(u32, usize, f64, i64)> = cat
        .season_lines(&games)
        .into_values()
        .filter(|l| l.season == season && l.tier == tier && l.officials.len() >= min)
        .map(|l| (l.account_id, l.officials.len(), l.rating(), l.team))
        .collect();
    rows.sort_by(|a, b| b.2.total_cmp(&a.2).then(b.1.cmp(&a.1)));
    rows
}

/// Every season table at once, as [`rank_table`] builds one: `(season, tier,
/// class)` -> `(account, officials, season rating)`, best first, officials
/// only, [`MIN_RANK_GAMES`] or more. For MVP audits.
pub fn season_tables(cat: &Catalogue, games: &[hl_db::RatedGame]) -> HashMap<(i64, i64, String), Vec<(u32, usize, f64)>> {
    let mut out: HashMap<(i64, i64, String), Vec<(u32, usize, f64)>> = HashMap::new();
    for l in cat.season_lines(games).into_values().filter(|l| l.officials.len() >= MIN_RANK_GAMES) {
        out.entry((l.season, l.tier, l.class.clone())).or_default().push((l.account_id, l.officials.len(), l.rating()));
    }
    for rows in out.values_mut() {
        rows.sort_by(|a, b| b.2.total_cmp(&a.2));
    }
    out
}

/// One player's ratings per class, stat bars and ranks.
pub async fn player_stats(db: &Db, account: u32) -> Result<PlayerStats> {
    let version = hl_rating::MODEL_VERSION;
    let mine = db.rated_games(version, Some(account)).await?;
    let mut by_class: HashMap<&str, Vec<&hl_db::RatedGame>> = HashMap::new();
    for g in &mine {
        by_class.entry(g.class.as_str()).or_default().push(g);
    }
    let order = |k: &str| hl_rating::guide::Group::ALL.iter().position(|g| g.key() == k).unwrap_or(9);
    let mut classes: Vec<ClassStats> = Vec::new();
    for (class, games) in by_class {
        let newest = games.iter().map(|g| g.played_at).max().unwrap_or(0);
        let recent: Vec<&hl_db::RatedGame> = games.iter().copied().filter(|g| g.played_at >= newest - RECENT_S).collect();
        let use_recent = recent.len() >= MIN_RECENT_GAMES;
        let pool: &[&hl_db::RatedGame] = if use_recent { &recent } else { &games };
        let mut sums: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        for g in pool {
            for (k, v) in groups_of(g) {
                let e = sums.entry(k).or_default();
                e.0 += v;
                e.1 += 1;
            }
        }
        let mut groups: Vec<(String, f64)> = sums.into_iter().map(|(k, (s, n))| (k, s / n as f64)).collect();
        groups.sort_by_key(|(k, _)| order(k));
        classes.push(ClassStats {
            class: class.to_string(),
            games: games.len(),
            career: mean(games.iter().map(|g| g.score)).unwrap_or(0.0),
            recent: mean(recent.iter().map(|g| g.score)).filter(|_| use_recent),
            recent_games: recent.len(),
            best: games.iter().map(|g| g.score).fold(f64::MIN, f64::max),
            groups,
            groups_recent: use_recent,
        });
    }
    classes.sort_by_key(|c| std::cmp::Reverse(c.games));

    // Ranks: every season they played officials in, among the division they
    // played, on every class they played enough that season.
    let cat = Catalogue::load(db).await?;
    let windows = cat.season_windows();
    let divisions = cat.played_divisions();
    let mut theirs: BTreeMap<(i64, String), std::collections::HashSet<i64>> = BTreeMap::new();
    for g in mine.iter().filter(|g| g.official) {
        if let Some(s) = game_season(&cat, &windows, g) {
            theirs.entry((s, g.class.clone())).or_default().insert(g.etf2l_match_id.unwrap_or(-g.log_id));
        }
    }
    let mut ranks = Vec::new();
    if theirs.values().any(|n| n.len() >= MIN_RANK_GAMES) {
        let everyone = rated_officials(db, None).await?;
        for ((season, class), n) in theirs {
            if n.len() < MIN_RANK_GAMES {
                continue;
            }
            let Some((tier, division, _)) = divisions.get(&(account, season)).cloned() else { continue };
            let table = rank_table(&cat, &everyone, &windows, &divisions, season, tier, &class);
            if let Some(i) = table.iter().position(|r| r.0 == account) {
                let name = windows.iter().find(|w| w.0 == season).map(|w| w.1.clone()).unwrap_or_default();
                ranks.push(Rank { season, season_name: name, division, tier, class, rank: i + 1, of: table.len(), avg: table[i].2, games: table[i].1 });
            }
        }
    }
    ranks.sort_by(|a, b| season_order(b.season).cmp(&season_order(a.season)).then(a.rank.cmp(&b.rank)));
    Ok(PlayerStats { classes, ranks })
}

/// A season's ranking for one division and class; the newest season with
/// rated games, and the top tier, when not given.
pub async fn rankings(db: &Db, season: Option<i64>, tier: Option<i64>, class: &str) -> Result<Rankings> {
    let cat = Catalogue::load(db).await?;
    let windows = cat.season_windows();
    let divisions = cat.played_divisions();
    let everyone = rated_officials(db, None).await?;
    let seasons: Vec<(i64, String)> = windows
        .iter()
        .filter(|(_, _, a, b)| everyone.iter().any(|g| g.played_at >= *a && g.played_at <= *b))
        .map(|(s, n, _, _)| (*s, n.clone()))
        .collect();
    let season = season.or_else(|| seasons.first().map(|s| s.0)).unwrap_or(0);
    let mut tiers: BTreeMap<i64, String> = BTreeMap::new();
    for ((_, s), (t, d, _)) in &divisions {
        if *s == season {
            tiers.entry(*t).or_insert_with(|| d.clone());
        }
    }
    let tier = tier.or_else(|| tiers.keys().next().copied()).unwrap_or(0);
    // Everyone who played an official there: the ranked first, then those
    // with too few to be ranked, each best first, for the table to grey out.
    let all = rank_table_from(&cat, &everyone, &windows, &divisions, season, tier, class, 1);
    let (ranked, under): (Vec<_>, Vec<_>) = all.into_iter().partition(|r| r.1 >= MIN_RANK_GAMES);
    let row = |rank: Option<usize>, (a, n, avg, team): (u32, usize, f64, i64)| RankRow { rank, account_id: a, name: cat.name_of(a), team: Some(cat.team(team)), games: n, avg };
    let rows = ranked
        .into_iter()
        .enumerate()
        .map(|(i, r)| row(Some(i + 1), r))
        .chain(under.into_iter().map(|r| row(None, r)))
        .collect();
    Ok(Rankings {
        season,
        season_name: seasons.iter().find(|s| s.0 == season).map(|s| s.1.clone()).unwrap_or_default(),
        division: tiers.get(&tier).cloned().unwrap_or_default(),
        tier,
        class: class.to_string(),
        rows,
        seasons,
        divisions: tiers.into_iter().collect(),
    })
}

// ---- The division of the people you play (Q38) ----------------------------

/// How far from a season a match may be and still take its division.
const NEAREST_SEASON_S: i64 = 365 * 24 * 3600;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerDivision {
    pub tier: i64,
    pub division: String,
    pub season: i64,
    /// "Summer 2025", "AFA 2025": the season as people know it.
    pub season_name: String,
    /// The match was inside that season; false when it was between seasons
    /// or in one the player did not play, and the nearest was used.
    pub exact: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDivisions {
    /// Account -> their division at the time of the match.
    pub players: HashMap<u32, PlayerDivision>,
    /// Tier -> its newest name, to name a side's average.
    pub tier_names: BTreeMap<i64, String>,
}

/// Every player's ETF2L division at the time of a match (Q38): the season
/// the match was in, else the nearest season they played within a year.
/// A player's division at a time: the season it fell in, else their nearest
/// season within [`NEAREST_SEASON_S`]. `(tier, division, season, exact)`.
fn division_at(
    windows: &[(i64, String, i64, i64)],
    divisions: &HashMap<(u32, i64), (i64, String, i64)>,
    account: u32,
    at: i64,
) -> Option<(i64, String, i64, bool)> {
    windows
        .iter()
        .filter_map(|(s, _, from, to)| {
            let (tier, division, _) = divisions.get(&(account, *s))?;
            let gap = if at < *from { from - at } else if at > *to { at - to } else { 0 };
            (gap <= NEAREST_SEASON_S).then(|| (gap, *s, *tier, division.clone()))
        })
        .min_by_key(|(gap, s, ..)| (*gap, -s))
        .map(|(gap, s, tier, division)| (tier, division, s, gap == 0))
}

/// Each game's opposite number's division, for the profile's "Who you
/// played" by division: `(log_id, their account, when)` in, `log_id ->
/// (tier, today's name for it)` out.
pub async fn opponent_divisions(db: &Db, games: &[(i64, u32, i64)]) -> Result<HashMap<i64, (i64, String)>> {
    if games.is_empty() {
        return Ok(HashMap::new());
    }
    let cat = Catalogue::load(db).await?;
    let windows = cat.season_windows();
    let divisions = cat.divisions();
    Ok(games
        .iter()
        .filter_map(|(log_id, account, at)| {
            let (tier, ..) = division_at(&windows, &divisions, *account, *at)?;
            let name = crate::leagues::TIER_NAMES.get(tier as usize)?.to_string();
            Some((*log_id, (tier, name)))
        })
        .collect())
}

pub async fn match_divisions(db: &Db, log_id: i64) -> Result<MatchDivisions> {
    let Some((played_at, accounts)) = db.match_accounts(log_id).await? else {
        return Ok(MatchDivisions { players: HashMap::new(), tier_names: BTreeMap::new() });
    };
    let cat = Catalogue::load(db).await?;
    let windows = cat.season_windows();
    let divisions = cat.divisions();
    let mut players = HashMap::new();
    for a in accounts {
        if let Some((tier, division, season, exact)) = division_at(&windows, &divisions, a, played_at) {
            let season_name = windows.iter().find(|w| w.0 == season).map(|w| w.1.clone()).unwrap_or_default();
            players.insert(a, PlayerDivision { tier, division, season, season_name, exact });
        }
    }
    // Every tier by today's name: "High", not the older "Division 1".
    let tier_names: BTreeMap<i64, String> = crate::leagues::TIER_NAMES.iter().enumerate().map(|(t, n)| (t as i64, n.to_string())).collect();
    Ok(MatchDivisions { players, tier_names })
}

// ---- Seasons and teams, for the Teams tab (Flashy's UX pass) ----------

/// One season's tile: what it was, when, who won it, and how the owner did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonTile {
    pub season: i64,
    pub season_name: String,
    /// Unix seconds: its first and last official.
    pub from: i64,
    pub to: i64,
    /// Top tier first.
    pub divisions: Vec<Division>,
    pub teams: usize,
    pub matches: usize,
    /// The top division's winner, once there is one.
    pub champion: Option<Team>,
    pub champion_division: Option<String>,
    /// The owner's team that season, its division and its medal.
    pub you: Option<PlayerSeason>,
}

/// One division's podium in a season, and its MVP.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Podium {
    pub division: String,
    pub tier: Option<i64>,
    pub medals: Vec<PodiumPlace>,
    /// The Season MVP.
    pub mvp: Option<Mvp>,
    pub mvp_name: Option<String>,
    /// The EVPs and the best of each class, with their names.
    pub awards: Vec<(Mvp, String)>,
    /// Another playoff bracket of the same regular-season division holds
    /// them (S33: Fresh Meat and Freshest were one division until the
    /// playoffs): its name, and `awards` is empty here.
    pub awards_with: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PodiumPlace {
    pub place: u8,
    pub team: Team,
    pub how: String,
}

/// A team's honours: its medals, and each season's division and record.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamHonours {
    pub medals: Vec<Medal>,
    /// Newest first.
    pub seasons: Vec<TeamSeason>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSeason {
    pub season: i64,
    pub season_name: String,
    pub division: String,
    pub tier: Option<i64>,
    pub played: u32,
    pub won: u32,
    pub lost: u32,
    pub place: Option<u8>,
}

impl Catalogue {
    /// A tier for a medal division's name.
    fn tier_of_name(&self, season: i64, name: &str) -> Option<i64> {
        crate::leagues::canonical_tier(name).or_else(|| self.tiers.get(&(season, name.to_string())).copied()).or_else(|| self.tier_by_name.get(name).copied())
    }
}

/// Every league season, newest first, as tiles.
pub async fn seasons_overview(db: &Db) -> Result<Vec<SeasonTile>> {
    let cat = Catalogue::load(db).await?;
    let medals = cat.medals();
    let windows = cat.season_windows();
    let me = db.get_me().await?.map(|s| s.account_id());
    let mine: Vec<PlayerSeason> = match me {
        Some(a) => cat.player(a, &medals).0,
        None => Vec::new(),
    };
    let mut out = Vec::new();
    for (season, name, from, to) in windows {
        let mut teams: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut divisions: BTreeMap<(i64, String), ()> = BTreeMap::new();
        let mut matches = 0;
        for m in cat.matches.values().filter(|m| m.season == season && m.stage != "Cup" && Catalogue::played(m)) {
            matches += 1;
            teams.insert(m.clan1);
            teams.insert(m.clan2);
            let d = cat.medal_division(m);
            if let Some(t) = cat.tier_of_name(season, &d) {
                if !d.is_empty() {
                    divisions.insert((t, d), ());
                }
            }
        }
        let champion = medals
            .iter()
            .filter(|((s, _), _)| *s == season)
            .filter_map(|((_, d), list)| Some((cat.tier_of_name(season, d)?, d, list.iter().find(|(p, _, _)| *p == 1)?.1)))
            .min_by_key(|(t, _, _)| *t);
        // Their own team; a merc's where they were on no roster that season.
        let here: Vec<&PlayerSeason> = mine.iter().filter(|p| p.season == season).collect();
        let you = here.iter().filter(|p| !p.merc).max_by_key(|p| p.played).or_else(|| here.iter().max_by_key(|p| p.played)).map(|p| (*p).clone());
        out.push(SeasonTile {
            season,
            season_name: name,
            from: from + 3 * 24 * 3600,
            to: to - 3 * 24 * 3600,
            divisions: divisions.into_keys().map(|(tier, name)| Division { name, tier }).collect(),
            teams: teams.len(),
            matches,
            champion: champion.map(|(_, _, t)| cat.team(t)),
            champion_division: champion.map(|(_, d, _)| d.clone()),
            you,
        });
    }
    Ok(out)
}


/// One season's podiums, top tier first, each with its event MVP.
pub async fn season_podiums(db: &Db, season: i64) -> Result<Vec<Podium>> {
    let cat = Catalogue::load(db).await?;
    let rated = rated_officials(db, None).await?;
    let mvps = cat.mvps(&rated);
    let mut out: Vec<Podium> = cat
        .medals()
        .into_iter()
        .filter(|((s, _), _)| *s == season)
        .map(|((_, division), list)| {
            let mut medals: Vec<PodiumPlace> = list.into_iter().map(|(place, team, how)| PodiumPlace { place, team: cat.team(team), how }).collect();
            medals.sort_by_key(|p| p.place);
            let tier = cat.tier_of_name(season, &division);
            let mvp = mvps.iter().find(|m| m.event && m.season == season && m.division == division).cloned();
            // The division's own, by name; by tier where the names differ.
            let awards: Vec<(Mvp, String)> =
                mvps.iter().filter(|m| !m.event && m.season == season && m.division == division).map(|m| (m.clone(), cat.name_of(m.account_id))).collect();
            Podium { tier, mvp_name: mvp.as_ref().map(|m| cat.name_of(m.account_id)), division, medals, mvp, awards, awards_with: None }
        })
        .collect();
    // A podium whose awards go by another name (a playoff bracket): the
    // tier's awards, once -- on the bracket that has the division's name,
    // and pointed to from the others.
    let tiers: Vec<Option<i64>> = out.iter().map(|p| p.tier).collect();
    for (i, tier) in tiers.into_iter().enumerate() {
        if !out[i].awards.is_empty() || tier.is_none() {
            continue;
        }
        if let Some(holder) = out.iter().find(|p| p.tier == tier && !p.awards.is_empty()).map(|p| p.division.clone()) {
            out[i].awards_with = Some(holder);
            continue;
        }
        let by_tier: Vec<(Mvp, String)> =
            mvps.iter().filter(|m| !m.event && m.season == season && m.tier == tier).map(|m| (m.clone(), cat.name_of(m.account_id))).collect();
        if !by_tier.is_empty() {
            for j in 0..out.len() {
                if j != i && out[j].tier == tier && out[j].awards.is_empty() {
                    out[j].awards_with = Some(out[i].division.clone());
                }
            }
            out[i].awards = by_tier;
        }
    }
    out.sort_by_key(|p| (p.tier.unwrap_or(99), p.division.clone()));
    Ok(out)
}

/// A team's medals and its seasons, newest first.
pub async fn team_honours(db: &Db, team: i64) -> Result<TeamHonours> {
    let cat = Catalogue::load(db).await?;
    let all = cat.medals();
    let mut medals: Vec<Medal> = Vec::new();
    for ((season, division), list) in &all {
        for (place, t, how) in list {
            if *t == team {
                medals.push(Medal {
                    season: *season,
                    season_name: cat.season_name(*season),
                    division: division.clone(),
                    tier: cat.tier_of_name(*season, division),
                    place: *place,
                    team: cat.team(team),
                    how: how.clone(),
                });
            }
        }
    }
    medals.sort_by(|a, b| season_order(b.season).cmp(&season_order(a.season)).then(a.place.cmp(&b.place)));
    // Each season: the division it played most, and its record.
    let mut by: BTreeMap<i64, (HashMap<(String, Option<i64>), u32>, u32, u32, u32)> = BTreeMap::new();
    for m in cat.matches.values().filter(|m| Catalogue::played(m) && m.stage != "Cup" && (m.clan1 == team || m.clan2 == team)) {
        let e = by.entry(m.season).or_default();
        let d = cat.medal_division(m);
        let t = cat.tier_of_name(m.season, &d);
        *e.0.entry((d, t)).or_default() += 1;
        let (_, _, won) = Catalogue::result_for(m, team);
        e.1 += 1;
        e.2 += u32::from(won == Some(true));
        e.3 += u32::from(won == Some(false));
    }
    let mut seasons: Vec<TeamSeason> = by
        .into_iter()
        .map(|(season, (divs, played, won, lost))| {
            let ((division, tier), _) = divs.into_iter().max_by_key(|(_, n)| *n).unwrap_or(((String::new(), None), 0));
            let place = medals.iter().filter(|m| m.season == season).map(|m| m.place).min();
            TeamSeason { season, season_name: cat.season_name(season), division, tier, played, won, lost, place }
        })
        .collect();
    seasons.sort_by_key(|s| std::cmp::Reverse(season_order(s.season)));
    Ok(TeamHonours { medals, seasons })
}

/// The season's banner from ETF2L's news, where one of its posts has one:
/// looked for once and kept ("-" when there is none, asked again after a
/// week). Season tiles show it.
pub async fn season_banner(db: &Db, sources: &Sources, season: i64, season_name: &str) -> Result<Option<String>> {
    let key = format!("season_banner2:{season}");
    if let Some(v) = db.get_setting(&key).await? {
        let (url, at) = v.split_once('|').unwrap_or((v.as_str(), "0"));
        let fresh = now() - at.parse::<i64>().unwrap_or(0) < 7 * 24 * 3600;
        if url != "-" || fresh {
            return Ok((url != "-").then(|| url.to_string()));
        }
    }
    // By number first ("Highlander Season 34"), then by name ("Winter
    // 2024", "AFA 2025") for the seasons without one.
    let numbered = season < crate::leagues::OFF_SEASON && !(28..=31).contains(&season);
    let mut queries: Vec<String> = Vec::new();
    if numbered {
        queries.push(format!("Highlander Season {season}"));
    }
    if !season_name.is_empty() && !season_name.starts_with("Season") {
        queries.push(format!("Highlander {season_name}"));
    }
    let mut found = None;
    for q in queries {
        let url = crate::http::url(
            "https://etf2l.org/wp-json/wp/v2/posts",
            [("search", q.as_str()), ("per_page", "50"), ("_fields", "title,content")],
        );
        if let Ok(Some(body)) = sources.fetch_text_interactive(&url).await {
            found = banner_in(&body, numbered.then_some(season), season_name);
        }
        if found.is_some() {
            break;
        }
    }
    db.set_setting(&key, &format!("{}|{}", found.as_deref().unwrap_or("-"), now())).await?;
    Ok(found)
}

/// A season's banner as a small JPEG, made once from ETF2L's picture and
/// kept: 640 px wide for a tile, 1280 for the season's own header. ETF2L's
/// banners are up to 1.9 MB of PNG each; a grid of them made the page slow.
/// When the picture cannot be read, its link is returned as it is.
pub async fn season_banner_image(db: &Db, sources: &Sources, season: i64, season_name: &str, large: bool) -> Result<Option<String>> {
    let width = if large { 1280 } else { 640 };
    let key = format!("season_banner_img:{season}:{width}");
    if let Some(v) = db.get_setting(&key).await? {
        return Ok(Some(v));
    }
    let Some(url) = season_banner(db, sources, season, season_name).await? else { return Ok(None) };
    let Some(bytes) = sources.fetch_bytes(&url).await? else { return Ok(Some(url)) };
    match tokio::task::spawn_blocking(move || shrink_to_jpeg(&bytes, width)).await? {
        Ok(jpeg) => {
            use base64::Engine;
            let data = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpeg));
            db.set_setting(&key, &data).await?;
            Ok(Some(data))
        }
        Err(e) => {
            tracing::warn!("season {season} banner kept at full size: {e:#}");
            Ok(Some(url))
        }
    }
}

/// A picture no wider than `width`, as a JPEG at quality 80.
fn shrink_to_jpeg(bytes: &[u8], width: u32) -> Result<Vec<u8>> {
    let img = image::load_from_memory(bytes).context("reading the picture")?;
    let img = if img.width() > width { img.resize(width, u32::MAX, image::imageops::FilterType::Triangle) } else { img };
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80).encode_image(&img.to_rgb8())?;
    Ok(out)
}

/// The first picture in a post about this Highlander season: its title says
/// "Highlander" and "Season 34" (not 340) or the season's name.
fn banner_in(body: &str, number: Option<i64>, name: &str) -> Option<String> {
    let posts: Vec<Value> = serde_json::from_str(body).ok()?;
    let name = name.to_ascii_lowercase();
    for p in posts {
        let title = p.pointer("/title/rendered").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
        if !title.contains("highlander") {
            continue;
        }
        let by_number = number.is_some_and(|n| {
            let w = format!("season {n}");
            title.match_indices(&w).any(|(i, _)| !title[i + w.len()..].starts_with(|c: char| c.is_ascii_digit()))
        });
        let by_name = !name.is_empty() && !name.starts_with("season") && title.contains(&name);
        if !by_number && !by_name {
            continue;
        }
        let content = p.pointer("/content/rendered").and_then(Value::as_str).unwrap_or("");
        for part in content.split("<img").skip(1) {
            let Some(src) = part.split("src=\"").nth(1).and_then(|s| s.split('"').next()) else { continue };
            let lower = src.to_ascii_lowercase();
            let path = lower.split('?').next().unwrap_or(&lower);
            let picture = path.ends_with(".jpg") || path.ends_with(".jpeg") || path.ends_with(".png") || path.ends_with(".webp");
            let chrome = ["/flags/", "/achs/", "emoji", "placeholder", "avatar"].iter().any(|x| lower.contains(x));
            if picture && !chrome && src.starts_with("http") {
                return Some(src.replace("&amp;", "&"));
            }
        }
    }
    None
}

/// What ETF2L's own page says about a team: its description, and its
/// awards as ETF2L lists them.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamEtf2l {
    pub description: Option<String>,
    pub awards: Vec<TeamAward>,
    pub url: String,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamAward {
    /// "1st", "2nd", "3rd".
    pub place: String,
    pub competition: String,
}

/// A team's ETF2L page, read when the team is opened and kept a week. The
/// API has neither the description nor the awards; the page has both.
pub async fn team_etf2l(db: &Db, sources: &Sources, team: i64) -> Result<TeamEtf2l> {
    let key = format!("etf2l_team_page:{team}");
    let url = format!("https://etf2l.org/teams/{team}/");
    if let Some(held) = db.get_setting(&key).await?.and_then(|v| serde_json::from_str::<TeamEtf2l>(&v).ok()) {
        if now() - held.fetched_at < 7 * 24 * 3600 {
            return Ok(held);
        }
    }
    let Ok(Some(html)) = sources.fetch_text(&url).await else {
        return Ok(TeamEtf2l { url, ..Default::default() });
    };
    let page = TeamEtf2l { description: team_description(&html), awards: team_awards(&html), url, fetched_at: now() };
    db.set_setting(&key, &serde_json::to_string(&page)?).await?;
    Ok(page)
}

/// The free text under the team's info table, as plain lines.
fn team_description(html: &str) -> Option<String> {
    let start = html.find("teamplaceholder")?;
    let end = html[start..].find("<h2>Warnings").map_or(html.len(), |e| start + e);
    let block = &html[start..end];
    let div = block.find("<div class=\"fix\">")?;
    let text = html_text(&block[div + "<div class=\"fix\">".len()..]);
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The Awards list: `1st: Highlander Season 33 (Spring 2025): Low Playoffs`.
fn team_awards(html: &str) -> Vec<TeamAward> {
    let Some(start) = html.find(">Awards</h2>") else { return Vec::new() };
    let end = html[start..].find("</ul>").map_or(html.len(), |e| start + e);
    html[start..end]
        .split("<li>")
        .skip(1)
        .filter_map(|li| {
            let line = html_text(li);
            let (place, competition) = line.split_once(':')?;
            Some(TeamAward { place: place.trim().to_string(), competition: competition.trim().to_string() })
        })
        .collect()
}

/// HTML to plain text: line breaks kept, tags dropped, entities read.
pub(crate) fn html_text(fragment: &str) -> String {
    // A newline in the source is a space in HTML; <br> and </p> are breaks.
    let mut s = fragment.replace(['\r', '\n'], " ").replace("<br />", "\n").replace("<br/>", "\n").replace("<br>", "\n").replace("</p>", "\n\n").replace("</li>", "\n");
    // Tags out.
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.drain(..) {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // Entities: numeric and the common named ones.
    let mut text = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(i) = rest.find('&') {
        text.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(semi) = tail[..tail.len().min(10)].find(';') else {
            text.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..semi];
        let ch = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#039" => Some('\''),
            "nbsp" => Some(' '),
            "hellip" => Some('…'),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                text.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                text.push('&');
                rest = &tail[1..];
            }
        }
    }
    text.push_str(rest);
    // Tidy the whitespace: trimmed lines, at most one blank line in a row.
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() && lines.last().is_some_and(|l| l.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_banner_is_shrunk_to_a_small_jpeg() {
        // A 2048 x 1152 PNG, as big as ETF2L's largest.
        let img = image::RgbImage::from_fn(2048, 1152, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let jpeg = super::shrink_to_jpeg(&png, 640).unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8, 0xFF]), "a JPEG");
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((back.width(), back.height()), (640, 360), "aspect kept");
        // A small picture is not blown up.
        let small = super::shrink_to_jpeg(&jpeg, 1280).unwrap();
        assert_eq!(image::load_from_memory(&small).unwrap().width(), 640);
    }

    use super::*;

    #[test]
    fn mvps_come_from_the_final_and_the_event_goes_to_the_winners() {
        // Semi-final 1 (team 11 beat 10), Grand Final 3 (11 beat 12, 6-3).
        let mut c = cat(vec![
            m(1, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 10, 11, 3, 6),
            m(3, 34, "Playoffs", None, "Mid", Some("Grand Final"), 12, 11, 3, 6),
        ]);
        c.rosters.get_mut(&3).unwrap().extend([(211, Some(11), "medic".to_string()), (311, Some(11), "sub".to_string())]);
        let g = |account: u32, log_id: i64, class: &str, score: f64, match_id: i64| hl_db::RatedGame {
            account_id: account,
            log_id,
            played_at: 0,
            class: class.into(),
            score,
            parts: None,
            groups: None,
            official: true,
            etf2l_match_id: Some(match_id),
            listed: true,
            minutes: 15.0,
            map: None,
        };
        let games = vec![
            // The losing Sniper had the better final; the winners' Medic the
            // best of the team that won.
            g(12, 31, "sniper", 1.5, 3),
            g(12, 32, "sniper", 1.5, 3),
            g(11, 31, "sniper", 1.2, 3),
            g(11, 32, "sniper", 1.2, 3),
            g(11, 10, "sniper", 0.9, 1),
            g(211, 31, "medic", 1.3, 3),
            g(211, 32, "medic", 1.3, 3),
            // A sub with one great half of two: not a candidate.
            g(311, 31, "scout", 2.0, 3),
        ];
        let v = c.mvps_final_only(&games);
        let sniper = v.iter().find(|x| x.class == "sniper" && !x.event).unwrap();
        assert_eq!((sniper.account_id, sniper.won), (12, false), "the class MVP can come from the losing finalist");
        let won_sniper = mvp_score(1.2, Some(0.9), 1);
        assert!(won_sniper < 1.3);
        let event = v.iter().find(|x| x.event).unwrap();
        assert_eq!((event.account_id, event.class.as_str(), event.won), (211, "medic", true));
        assert!(!v.iter().any(|x| x.account_id == 311), "more than half the final to be a candidate");
        assert_eq!(event.final_maps, 2);
    }

    #[test]
    fn the_season_mvp_goes_to_the_champions_and_the_class_awards_follow_the_season() {
        // S34 Mid: four regular weeks between 11 and 12, a semi-final, and a
        // Grand Final 11 won.
        let mut ms: Vec<CatMatch> = (101..=104).map(|id| m(id, 34, "regular", Some("Mid"), "", None, 11, 12, 6, 3)).collect();
        ms.push(m(1, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 10, 11, 3, 6));
        ms.push(m(3, 34, "Playoffs", None, "Mid", Some("Grand Final"), 12, 11, 3, 6));
        let mut c = cat(ms);
        for (id, roster) in c.rosters.iter_mut() {
            roster.push((211, Some(11), "medic".into()));
            if *id != 1 {
                roster.push((212, Some(12), "medic".into()));
            }
        }
        c.rosters.get_mut(&3).unwrap().push((311, Some(11), "sub".into()));
        let g = |account: u32, log_id: i64, class: &str, score: f64, match_id: i64| hl_db::RatedGame {
            account_id: account,
            log_id,
            played_at: 0,
            class: class.into(),
            score,
            parts: None,
            groups: None,
            official: true,
            etf2l_match_id: Some(match_id),
            listed: true,
            minutes: 15.0,
            map: None,
        };
        let mut games = Vec::new();
        for week in 101..=104 {
            games.push(g(12, week * 10, "sniper", 1.4, week));
            games.push(g(11, week * 10, "sniper", 1.0, week));
            games.push(g(211, week * 10, "medic", 1.2, week));
            games.push(g(212, week * 10, "medic", 1.1, week));
        }
        for log in [31, 32] {
            games.push(g(12, log, "sniper", 1.5, 3));
            games.push(g(11, log, "sniper", 1.2, 3));
            games.push(g(211, log, "medic", 1.3, 3));
        }
        // A sub with one great half of the final and no season: no award.
        games.push(g(311, 31, "scout", 2.0, 3));
        // The final was two maps, a log each.
        for x in games.iter_mut().filter(|x| x.etf2l_match_id == Some(3)) {
            x.map = Some(if x.log_id == 31 { "koth_product_final".into() } else { "koth_proot_b5b".into() });
        }
        let v = c.mvps(&games);
        let season = v.iter().find(|x| x.kind == "season").unwrap();
        assert_eq!((season.account_id, season.won, season.final_maps), (211, true, 2), "the champions' best");
        let sniper = v.iter().find(|x| x.kind == "class" && x.class == "sniper").unwrap();
        assert_eq!((sniper.account_id, sniper.won), (12, false), "the class award goes to the season's best, finalist or not");
        assert!(sniper.season_rating < 1.45 && sniper.season_rating > 1.2, "pulled towards 1.00, the final counting double");
        let evps: Vec<u32> = v.iter().filter(|x| x.kind == "evp").map(|x| x.account_id).collect();
        assert_eq!(evps.len(), 3, "three in a division of fewer than eight teams");
        assert_eq!(evps[0], 12);
        assert!(!evps.contains(&211), "the Season MVP is not also an EVP");
        assert!(!v.iter().any(|x| x.account_id == 311));
        // The season table ranks by the same number.
        let lines = c.season_lines(&games);
        let s12 = &lines[&(34, sniper.tier.unwrap(), "sniper".to_string(), 12)];
        assert!((s12.rating() - sniper.season_rating).abs() < 1e-9);
        assert_eq!(s12.officials.len(), 5);
    }

    #[test]
    fn the_class_award_asks_for_most_of_the_season_and_a_playoff_place() {
        // S31 Low as Flashy saw it: the table's top Sniper played 4 of a
        // season whose longest was 10; the champions' Sniper played 7.
        let line = |account: u32, team: i64, officials: i64| SeasonLine {
            season: 31,
            tier: 3,
            class: "sniper".into(),
            account_id: account,
            team,
            officials: (0..officials).collect(),
            ..Default::default()
        };
        let lines: HashMap<(i64, i64, String, u32), SeasonLine> =
            [line(1, 10, 4), line(2, 11, 7), line(3, 12, 10), line(4, 13, 7)].into_iter().map(|l| ((31, 3, "sniper".to_string(), l.account_id), l)).collect();
        let c = cat(vec![m(1, 31, "Playoffs", None, "Low", Some("Grand Final"), 11, 12, 6, 3)]);
        let rules = c.award_rules(&lines);
        assert_eq!(rules.need(31, 3), 6, "60% of the longest season, 10");
        assert!(!rules.class_award(&lines[&(31, 3, "sniper".to_string(), 1)]), "4 officials is not enough");
        assert!(rules.class_award(&lines[&(31, 3, "sniper".to_string(), 2)]), "the champions' Sniper");
        assert!(rules.class_award(&lines[&(31, 3, "sniper".to_string(), 3)]), "the runners-up's");
        assert!(!rules.class_award(&lines[&(31, 3, "sniper".to_string(), 4)]), "7 officials, but no medal");
        assert!(rules.enough(&lines[&(31, 3, "sniper".to_string(), 4)]), "still eligible as an EVP");
        // A short season asks for no more than four.
        let short: HashMap<(i64, i64, String, u32), SeasonLine> = [line(5, 11, 5)].into_iter().map(|l| ((31, 3, "sniper".to_string(), 5), l)).collect();
        assert_eq!(c.award_rules(&short).need(31, 3), 4);
    }

    #[test]
    fn merc_games_do_not_count_for_awards() {
        let mut c = cat(vec![m(1, 33, "regular", Some("Low"), "", None, 10, 11, 6, 0)]);
        // 10 and 11 are on their teams' rosters (cat()); 500 played as a
        // merc; 600 played and is on no lineup at all.
        c.rosters.get_mut(&1).unwrap().push((500, Some(10), "merc".into()));
        c.mercs.insert((1, 500));
        let g = |account: u32| hl_db::RatedGame { account_id: account, log_id: 1, etf2l_match_id: Some(1), official: true, minutes: 30.0, ..Default::default() };
        assert!(c.own_team_game(&g(10)));
        assert!(!c.own_team_game(&g(500)), "a merc for that team");
        assert!(!c.own_team_game(&g(600)), "not on the lineup ETF2L has");
        // A match whose lineups were never read keeps everyone.
        let mut unread = g(600);
        unread.etf2l_match_id = Some(99);
        assert!(c.own_team_game(&unread));
    }

    #[test]
    fn a_short_playoff_run_counts_for_less_than_a_long_one() {
        // Two playoff maps at 1.57 no longer carry a 0.85 final past a 1.05.
        assert!(mvp_score(0.85, Some(1.57), 2) < mvp_score(1.05, Some(1.03), 2));
        assert_eq!(mvp_score(1.0, None, 0), 1.0);
        // From four maps on, the full 30%.
        assert!((mvp_score(1.0, Some(2.0), 4) - 1.3).abs() < 1e-9);
        assert!((mvp_score(1.0, Some(2.0), 9) - 1.3).abs() < 1e-9);
        assert!((mvp_score(1.0, Some(2.0), 1) - 1.075).abs() < 1e-9);
    }

    #[test]
    fn a_player_the_roster_does_not_place_is_on_the_side_the_logs_put_them() {
        // Team 10 in Red, team 11 in Blue on both logs; 900 is on no roster
        // and played Blue in both.
        let roster: HashMap<u32, i64> = [(1, 10), (2, 10), (3, 11), (4, 11)].into_iter().collect();
        let log = |swap: bool| {
            let (r, b) = if swap { ("Blue", "Red") } else { ("Red", "Blue") };
            vec![(1, r.to_string()), (2, r.to_string()), (3, b.to_string()), (4, b.to_string()), (900, b.to_string())]
        };
        // Sides swap between the halves: the colour follows the roster.
        let logs: HashMap<i64, Vec<(u32, String)>> = [(1, log(false)), (2, log(true))].into_iter().collect();
        let sides = Catalogue::sides_from_logs(Some(&logs), &roster, 10, 11);
        assert_eq!(sides.get(&900), Some(&11), "with team 11's players both halves, whichever colour that was");
        assert_eq!(sides.get(&1), Some(&10));
        // A log where the rosters do not settle the colours says nothing.
        let muddled: HashMap<i64, Vec<(u32, String)>> = [(1, vec![(1, "Red".to_string()), (3, "Red".to_string()), (900, "Blue".to_string())])].into_iter().collect();
        assert!(Catalogue::sides_from_logs(Some(&muddled), &roster, 10, 11).is_empty());
    }

    #[test]
    fn a_finalist_steps_up_their_own_seasons_ladder() {
        // AFA 2025: Premiership, then Division 1 to 4. A Division 2 finalist
        // is Division 1 -- High -- not Premiership.
        let c = cat(vec![
            m(1, 134, "regular", Some("Division 2"), "Top Tiers", Some("Grand Final"), 10, 11, 6, 0),
            m(2, 134, "regular", Some("Division 1"), "Top Tiers", Some("Grand Final"), 12, 13, 6, 0),
            m(3, 34, "Playoffs", None, "Mid", Some("Grand Final"), 14, 15, 6, 0),
            m(4, 34, "Playoffs", None, "Premiership", Some("Grand Final"), 16, 17, 6, 0),
        ]);
        let up = |id: i64| c.division_above(&c.matches[&id]);
        assert_eq!(up(1), Some(1), "Division 2 -> Division 1, High");
        assert_eq!(up(2), Some(0), "Division 1 -> Premiership");
        assert_eq!(up(3), Some(1), "Mid -> High");
        assert_eq!(up(4), None, "nothing above Premiership");
    }

    #[test]
    fn a_merc_does_not_take_home_the_teams_medal() {
        // Team 11 wins the final; account 500 played one official for it as
        // a merc, account 11 is on its roster.
        let mut c = cat(vec![
            m(1, 33, "regular", Some("Open"), "Open", Some("Week 1"), 11, 12, 6, 0),
            m(3, 33, "Playoffs", None, "Open", Some("Grand Final"), 11, 13, 6, 0),
        ]);
        c.rosters.get_mut(&1).unwrap().push((500, Some(11), "merc".into()));
        c.mercs.insert((1, 500));
        let medals = c.medals();
        let (_, won, _) = c.player(11, &medals);
        assert_eq!(won.len(), 1, "the roster's medal");
        let (_, won, officials) = c.player(500, &medals);
        assert!(won.is_empty(), "no medal for a merc appearance");
        assert_eq!(officials.len(), 1, "but the official is theirs");
    }

    #[test]
    fn a_merc_who_joins_later_that_season_takes_the_medal() {
        // Flashy's S36 High: one official as a merc in week 1 (time 1), on
        // the roster from time 2, and the team wins the final (time 3).
        // Account 501 mercs once too and never joins.
        let mut c = cat(vec![
            m(1, 33, "regular", Some("Open"), "Open", Some("Week 1"), 11, 12, 6, 0),
            m(3, 33, "Playoffs", None, "Open", Some("Grand Final"), 11, 13, 6, 0),
        ]);
        c.rosters.get_mut(&1).unwrap().push((500, Some(11), "joined later".into()));
        c.rosters.get_mut(&1).unwrap().push((501, Some(11), "never joined".into()));
        c.mercs.insert((1, 500));
        c.mercs.insert((1, 501));
        c.season_end = Catalogue::season_ends(&c.matches);
        c.transfers.insert((11, 500), vec![(2, true)]);
        let medals = c.medals();
        let (seasons, won, _) = c.player(500, &medals);
        assert_eq!(won.len(), 1, "on the roster at the end, and played for it");
        assert!(!seasons[0].merc, "a player of the team, not a merc");
        let (_, won, _) = c.player(501, &medals);
        assert!(won.is_empty(), "a merc who never joined");
    }

    #[test]
    fn an_etf2l_page_gives_teams_title_and_bans() {
        // The shape of api-v2's /player answer (Flashy's, October 2026).
        let json = r#"{"player":{"id":97913,"name":"Flashy","title":"Player","registered":1402247571,"bans":null,
            "teams":[{"id":35849,"name":"The 9 Stooges","tag":"9S","type":"Highlander Fun Team","country":"Croatia",
                      "steam":{"avatar":"https://etf2l.org/wp-content/uploads/avatars/657792a02e49d.png"}},
                     {"id":1,"name":"","tag":"x"}]},"status":{"code":200}}"#;
        let (title, teams, bans) = etf2l_extras(json);
        assert_eq!(title.as_deref(), Some("Player"));
        assert_eq!(teams.len(), 1, "a team with no name is skipped");
        assert_eq!(teams[0].tag.as_deref(), Some("9S"));
        assert_eq!(teams[0].kind.as_deref(), Some("Highlander Fun Team"));
        assert!(teams[0].avatar.is_some());
        assert!(bans.is_empty());

        let banned = r#"{"player":{"bans":[{"start":1600000000,"end":1600600000,"reason":"VAC"}]}}"#;
        assert_eq!(etf2l_extras(banned).2, vec![Etf2lBan { start: Some(1_600_000_000), end: Some(1_600_600_000), reason: Some("VAC".into()) }]);
        assert_eq!(etf2l_extras("not json"), (None, vec![], vec![]));
    }

    #[test]
    fn a_player_who_left_before_the_final_does_not_take_its_medal() {
        // Team 11 wins the final (match 3, at time 3). Account 11 played week
        // 1 on the roster and left at time 2; account 600 played week 1 too
        // and stayed.
        let mut c = cat(vec![
            m(1, 33, "regular", Some("Open"), "Open", Some("Week 1"), 11, 12, 6, 0),
            m(3, 33, "Playoffs", None, "Open", Some("Grand Final"), 11, 13, 6, 0),
        ]);
        c.rosters.get_mut(&1).unwrap().push((600, Some(11), "stayed".into()));
        c.season_end = Catalogue::season_ends(&c.matches);
        c.transfers.insert((11, 11), vec![(0, true), (2, false)]);
        c.transfers.insert((11, 600), vec![(0, true), (5, false)]);
        let medals = c.medals();
        let (seasons, won, _) = c.player(11, &medals);
        assert!(won.is_empty(), "left before the final");
        assert!(seasons[0].left_early && seasons[0].place.is_none());
        let (seasons, won, _) = c.player(600, &medals);
        assert_eq!(won.len(), 1, "left after it: still theirs");
        assert!(!seasons[0].left_early);
        assert_eq!(c.withheld_medals(), vec![(33, 11, 11, "p11".to_string())]);
    }

    #[test]
    fn a_season_banner_is_taken_from_a_post_naming_that_season_only() {
        let body = r#"[
            {"title":{"rendered":"Highlander Season 34 (Summer 2025): Wrap-Up"},"content":{"rendered":"<p><img src=\"https://etf2l.org/wp-content/uploads/2025/HL34.png\"></p>"}},
            {"title":{"rendered":"Highlander Season 3 recap"},"content":{"rendered":"<img src=\"https://etf2l.org/wp-content/uploads/placeholderbanner.jpg\"><img src=\"/images/flags/European.gif\"><img src=\"https://i.imgur.com/HL3.jpg?x=1\">"}},
            {"title":{"rendered":"6v6 Season 34"},"content":{"rendered":"<img src=\"https://etf2l.org/wp-content/uploads/6v6.jpg\">"}},
            {"title":{"rendered":"Highlander Winter 2024: Playoffs"},"content":{"rendered":"<img src=\"https://etf2l.org/wp-content/uploads/w24.png\">"}}
        ]"#;
        assert_eq!(banner_in(body, Some(3), "Season 3").as_deref(), Some("https://i.imgur.com/HL3.jpg?x=1"), "not Season 34's, not the placeholder, not a flag");
        assert_eq!(banner_in(body, Some(34), "Summer 2025").as_deref(), Some("https://etf2l.org/wp-content/uploads/2025/HL34.png"), "not the 6v6 post");
        assert_eq!(banner_in(body, None, "Winter 2024").as_deref(), Some("https://etf2l.org/wp-content/uploads/w24.png"), "by name");
        assert_eq!(banner_in(body, Some(35), "Spring 2026"), None);
    }

    #[test]
    fn a_teams_etf2l_page_gives_its_description_and_awards() {
        let html = r#"<h1>SBQRRA</h1> <table class="teaminfo"></table>
            <div class="teamplaceholder"><img src="x.png"/></div>
            <div class="fix"> <p><p>&#8220;Se ni&#8217; mondo&#8221;<br />
            P.P.</p>
            <p>&#8211; 1st Place S30: tiad &#8211; bad</p>
            </p></div>
            <h2>Warnings</h2><ul><li>None</li></ul>
            <h2>Awards</h2>
            <ul><li><img src="1st.gif" alt="1st">1st: <a href="/archives/885">Highlander Autumn 2023 (Open B)</a></li><li><img alt="3rd">3rd: <a href="/archives/966">Highlander Season 34 (Summer 2025) (Mid)</a></li>
            </ul><h2>Upcoming Fixtures</h2>"#;
        assert_eq!(team_description(html).as_deref(), Some("\u{201c}Se ni\u{2019} mondo\u{201d}\nP.P.\n\n\u{2013} 1st Place S30: tiad \u{2013} bad"));
        let awards = team_awards(html);
        assert_eq!(awards.len(), 2);
        assert_eq!((awards[0].place.as_str(), awards[0].competition.as_str()), ("1st", "Highlander Autumn 2023 (Open B)"));
        assert_eq!(awards[1].place, "3rd");
    }

    #[test]
    fn every_way_etf2l_named_a_playoff_round_is_read() {
        use RoundKind::*;
        for (r, k) in [
            ("Grand Final", Some(Final)),
            ("Grand-Final", Some(Final)),
            ("Division 1 Grand Final", Some(Final)),
            ("Premiership Grand Final", Some(Final)),
            ("Final", Some(Final)),
            ("3rd Place ", Some(Third)),
            ("3rd Place Match", Some(Third)),
            ("Semi-Finals", Some(Semi)),
            ("Division 2 Semi-Finals", Some(Semi)),
            ("Lower Bracket Final", Some(LowerFinal)),
            ("Upper Bracket Final", None),
            ("Week 3", None),
            ("Round 4", None),
        ] {
            assert_eq!(Catalogue::round_kind(Some(r)), k, "{r}");
        }
    }

    #[test]
    fn a_numbered_division_keeps_its_number_and_groups_fold() {
        assert_eq!(Catalogue::ungrouped("Division 2A"), "Division 2");
        assert_eq!(Catalogue::ungrouped("Division 2"), "Division 2");
        assert_eq!(Catalogue::ungrouped("Open B"), "Open");
        assert_eq!(Catalogue::ungrouped("Premiership Division"), "Premiership");
        assert_eq!(Catalogue::ungrouped("Mid"), "Mid");
        assert_eq!(Catalogue::numbered("Division 3"), Some(3));
        assert_eq!(Catalogue::numbered("Mid"), None);
    }

    fn m(id: i64, season: i64, stage: &str, div: Option<&str>, comp_div: &str, round: Option<&str>, c1: i64, c2: i64, r1: i64, r2: i64) -> CatMatch {
        CatMatch {
            match_id: id,
            competition_id: 1,
            season,
            season_name: format!("S{season}"),
            comp_name: String::new(),
            comp_division: comp_div.into(),
            stage: stage.into(),
            division: div.map(str::to_string),
            tier: div.map(|_| 2),
            round: round.map(str::to_string),
            time: Some(id),
            clan1: c1,
            clan2: c2,
            r1: Some(r1),
            r2: Some(r2),
            default_win: false,
        }
    }

    fn cat(matches: Vec<CatMatch>) -> Catalogue {
        let mut rosters: HashMap<i64, Vec<(u32, Option<i64>, String)>> = HashMap::new();
        for x in &matches {
            // Player 1 always plays for team 10; others for their team.
            for team in [x.clan1, x.clan2] {
                rosters.entry(x.match_id).or_default().push((team as u32, Some(team), format!("p{team}")));
            }
        }
        let tiers = matches.iter().filter_map(|x| Some(((x.season, x.division.clone()?), x.tier?))).collect();
        Catalogue { newest_season: matches.iter().map(|x| x.season).max().unwrap_or(0), matches: matches.into_iter().map(|x| (x.match_id, x)).collect(), rosters, teams: HashMap::new(), tiers, tier_by_name: HashMap::new(), mercs: Default::default(), transfers: HashMap::new(), season_end: HashMap::new(), final_colours: HashMap::new(), official_places: HashMap::new() }
    }

    #[test]
    fn a_grand_final_gives_gold_and_silver_and_a_third_place_match_bronze() {
        let c = cat(vec![
            m(1, 35, "regular", Some("Low"), "", None, 10, 11, 6, 0),
            m(2, 35, "Playoffs", None, "Low", Some("Semi-Finals"), 10, 12, 5, 1),
            m(3, 35, "Playoffs", None, "Low", Some("Grand Final"), 10, 11, 4, 5),
            m(4, 35, "3rd Place", None, "Low", None, 12, 13, 6, 3),
            m(5, 36, "regular", Some("Low"), "", None, 10, 11, 6, 0),
        ]);
        let medals = c.medals();
        let low = &medals[&(35, "Low".to_string())];
        assert!(low.contains(&(1, 11, "Grand Final 5-4".into())));
        assert!(low.contains(&(2, 10, "Grand Final 4-5".into())));
        assert!(low.contains(&(3, 12, "3rd place match 6-3".into())));
        // Team 10's player: silver in S35, and their season is Low with its tier.
        let (seasons, won, officials) = c.player(10, &medals);
        assert_eq!(won.len(), 1);
        assert_eq!(won[0].place, 2);
        assert_eq!(seasons.iter().find(|s| s.season == 35).unwrap().tier, Some(3), "the playoffs are Low, tier 3 on the ladder");
        assert_eq!(officials.len(), 4);
    }

    #[test]
    fn a_division_without_a_final_is_settled_by_its_table_once_the_season_is_over() {
        let mut ms = Vec::new();
        let mut id = 0;
        // Four teams, 20 beats all, 21 beats 22 and 23, 22 beats 23.
        for (a, b) in [(20, 21), (20, 22), (20, 23), (21, 22), (21, 23), (22, 23)] {
            id += 1;
            ms.push(m(id, 34, "regular", Some("High"), "", None, a, b, 6, 0));
        }
        ms.push(m(99, 35, "regular", Some("High"), "", None, 20, 21, 6, 0));
        let c = cat(ms);
        let medals = c.medals();
        assert_eq!(medals[&(34, "High".to_string())].iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(), vec![(1, 20), (2, 21), (3, 22)]);
        assert!(!medals.contains_key(&(35, "High".to_string())), "the newest season is not over");
    }

    #[test]
    fn without_a_third_place_match_both_semi_final_losers_share_bronze() {
        // S34 Mid: two semi-finals, a Grand Final, no match for third.
        let c = cat(vec![
            m(1, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 10, 11, 3, 6),
            m(2, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 12, 13, 6, 0),
            m(3, 34, "Playoffs", None, "Mid", Some("Grand Final"), 12, 11, 3, 6),
        ]);
        let mid = &c.medals()[&(34, "Mid".to_string())];
        let bronze: Vec<i64> = mid.iter().filter(|x| x.0 == 3).map(|x| x.1).collect();
        assert_eq!(bronze.len(), 2);
        assert!(bronze.contains(&10) && bronze.contains(&13), "both losing semi-finalists");
        assert!(mid.contains(&(3, 10, "joint 3rd: semi-final 3-6".into())));
        // With a 3rd-place match, only its winner.
        let c = cat(vec![
            m(1, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 10, 11, 3, 6),
            m(2, 34, "Playoffs", None, "Mid", Some("Semi-finals"), 12, 13, 6, 0),
            m(3, 34, "Playoffs", None, "Mid", Some("Grand Final"), 12, 11, 3, 6),
            m(4, 34, "3rd Place", None, "Mid", None, 10, 13, 6, 3),
        ]);
        let bronze: Vec<i64> = c.medals()[&(34, "Mid".to_string())].iter().filter(|x| x.0 == 3).map(|x| x.1).collect();
        assert_eq!(bronze, vec![10]);
    }

    #[test]
    fn a_final_with_no_division_takes_its_teams_division() {
        let c = cat(vec![
            m(1, 30, "regular", Some("Open B"), "", Some("Week 1"), 10, 11, 6, 0),
            m(2, 30, "regular", None, "", Some("Grand Final"), 10, 12, 5, 1),
        ]);
        assert!(c.medals()[&(30, "Open".to_string())].contains(&(1, 10, "Grand Final 5-1".into())));
    }

    #[test]
    fn a_grand_final_inside_the_season_counts_but_a_cups_does_not() {
        // Autumn 2023: the Grand Final is a round of the season itself.
        let c = cat(vec![
            m(1, 30, "regular", Some("Open B"), "", Some("Week 1"), 10, 11, 6, 0),
            m(2, 30, "regular", Some("Open B"), "", Some("Grand Final"), 10, 12, 5, 1),
            m(3, 31, "Cup", Some("Low A"), "", Some("Grand Final"), 10, 13, 6, 0),
        ]);
        let medals = c.medals();
        assert!(medals[&(30, "Open".to_string())].contains(&(1, 10, "Grand Final 5-1".into())), "Open B is Open on the ladder");
        assert!(!medals.contains_key(&(31, "Low A".to_string())), "a preseason cup is not the league");
    }

    /// Matches with their own tier, and one roster: player 1 for team 10.
    fn tiered(ms: Vec<(i64, i64, &str, Option<&str>, i64)>) -> Catalogue {
        let matches: Vec<CatMatch> = ms
            .into_iter()
            .map(|(id, season, stage, round, tier)| CatMatch {
                match_id: id,
                competition_id: 1,
                season,
                season_name: format!("S{season}"),
                comp_name: String::new(),
                comp_division: format!("t{tier}"),
                stage: stage.into(),
                division: (stage == "regular").then(|| format!("t{tier}")),
                tier: (stage == "regular").then_some(tier),
                round: round.map(str::to_string),
                time: Some(id),
                clan1: 10,
                clan2: 11,
                r1: Some(6),
                r2: Some(0),
                default_win: false,
            })
            .collect();
        let mut c = cat(matches);
        for r in c.rosters.values_mut() {
            r.retain(|(_, t, _)| *t == Some(10));
            r[0].0 = 1;
        }
        for m in c.matches.values() {
            if let Some(d) = &m.division {
                c.tier_by_name.insert(d.clone(), m.tier.unwrap());
            }
        }
        c
    }

    #[test]
    fn a_division_counts_from_three_officials() {
        // Two Premiership mercs and nine High games: High, not Premiership.
        let mut ms: Vec<(i64, i64, &str, Option<&str>, i64)> = vec![(1, 30, "regular", None, 0), (2, 30, "regular", None, 0)];
        ms.extend((3..12).map(|id| (id, 30, "regular", None, 1)));
        let c = tiered(ms);
        let d = c.divisions();
        assert_eq!(d[&(1, 30)].0, 1);
        assert_eq!(Catalogue::highest_of(&d, 1).map(|x| x.tier), Some(1));
        // Two officials anywhere: no division at all.
        let c = tiered(vec![(1, 31, "regular", None, 2), (2, 31, "regular", None, 2)]);
        assert!(c.divisions().is_empty());
    }

    #[test]
    fn a_grand_final_counts_for_the_division_above() {
        // Four Mid games and the Mid Grand Final: High that season.
        let mut ms: Vec<(i64, i64, &str, Option<&str>, i64)> = (1..5).map(|id| (id, 32, "regular", None, 2)).collect();
        ms.push((5, 32, "Playoffs", Some("Grand Final"), 2));
        let c = tiered(ms);
        assert_eq!(c.divisions()[&(1, 32)].0, 1, "a Mid finalist counts as High");
    }
}
