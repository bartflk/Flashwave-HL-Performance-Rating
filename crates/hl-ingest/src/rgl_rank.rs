//! An RGL Highlander power ranking, after tf2esports.com's Rankings page
//! (Flashy: "keep working and iterating on the rgl page ... i want a lot of
//! the site features [of tf2esports.com] copied").
//!
//! RGL's API gives no ranking, so it is worked out here, and only from
//! matches the app already has: the live season's (read for Home) and the
//! earlier seasons whose matches are on disk (`rgl_match`). Opening the page
//! asks RGL for nothing. An older season is read only when the person asks
//! for it ("Include Season N"), through `season_matches`, which reads at
//! RGL's pace and keeps what it reads for good.
//!
//! The model is Elo. Every team starts from a seed by division, the app's
//! ETF2L ladder (Invite 1600, Advanced 1500, Main 1400, Intermediate and
//! Amateur 1300, Newcomer 1200). Each decided match moves both teams by
//! K * (result - expected), where expected is the usual logistic of the
//! rating gap; a win by more maps moves it a little more (up to +30%). A
//! forfeit says little about strength, so it moves a quarter as much and is
//! not counted as a match played; a double forfeit moves nothing. Each new
//! season pulls a team's rating 30% of the way back to its division's seed,
//! once for every season since it last played (so a team that moves up a
//! division is not carried by last season's field, and a team coming back
//! from a long gap is nearly a fresh one). Matches of the same day are all
//! rated from the ratings at the start of that day, so their order does not
//! matter. Nothing here is random or reads a clock: `now` is passed in.
//!
//! One team is one identity across seasons: RGL gives a new team id every
//! season, so ids are joined by RGL's own `linked_teams` where a team's page
//! was kept (a team that was opened), and by name otherwise.

use crate::rgl::{self, RglMatch, RglSeasonItem};
use crate::sources::Sources;
use anyhow::Result;
use hl_db::Db;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The most a match moves a team that is evenly matched with its opponent
/// (half of it, 16 points, for a win over an equal).
const K: f64 = 32.0;
/// How much of its distance from the division's seed a team keeps into the
/// next season.
const RETAIN: f64 = 0.7;
/// A forfeit's share of an ordinary result.
const FORFEIT_WEIGHT: f64 = 0.25;
/// A win by `d` more maps is worth 1 + 0.1 * min(d, 3) of a bare win.
const MARGIN_STEP: f64 = 0.1;
/// "+/-" compares with the table this long ago.
const WEEK: i64 = 7 * 24 * 3600;
/// Roster names shown under a team.
const ROSTER_SHOWN: usize = 8;
/// A season counts as on disk when this share of its matches is.
const ON_DISK: f64 = 0.8;

/// A team's starting rating by RGL division, through the ETF2L ladder.
pub fn seed(division: Option<&str>) -> f64 {
    match division.and_then(rgl::tier_of) {
        Some("Premiership") => 1600.0,
        Some("High") => 1500.0,
        Some("Mid") => 1400.0,
        Some("Low") => 1300.0,
        Some("Open") => 1200.0,
        _ => 1300.0,
    }
}

/// How a match ended, for the ratings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// Not played, not decided, or forfeited by both: it moves nothing, but
    /// its teams still count as being in the season.
    Pending,
    /// The home team's result (1, 0.5 for a tie, 0), by how many more maps
    /// the winner took, and whether it was a forfeit.
    Decided { home: f64, margin: i64, forfeit: bool },
}

/// One match, with its teams as identities.
#[derive(Debug, Clone, PartialEq)]
pub struct Game {
    pub id: i64,
    /// Unix seconds.
    pub at: i64,
    pub season: i64,
    pub division: Option<String>,
    pub home: i64,
    pub away: i64,
    pub outcome: Outcome,
}

/// A team's rating after the matches counted.
#[derive(Debug, Clone, PartialEq)]
pub struct Rating {
    pub points: f64,
    /// Matches played (forfeits apart).
    pub matches: usize,
    /// Index of the newest season it was in, among the seasons given.
    pub season: usize,
    pub division: Option<String>,
    /// The newest decided match counted.
    pub last: Option<i64>,
}

fn expected(a: f64, b: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf((b - a) / 400.0))
}

fn touch(ratings: &mut BTreeMap<i64, Rating>, key: i64, season: usize, division: &Option<String>) {
    match ratings.get_mut(&key) {
        None => {
            ratings.insert(key, Rating { points: seed(division.as_deref()), matches: 0, season, division: division.clone(), last: None });
        }
        Some(r) if r.season < season => {
            let to = seed(division.as_deref());
            r.points = to + RETAIN.powi((season - r.season) as i32) * (r.points - to);
            r.season = season;
            r.division = division.clone();
        }
        Some(r) if r.season == season && division.is_some() => r.division = division.clone(),
        Some(_) => {}
    }
}

/// Every team's rating once the matches before `until` (unix s) are counted.
/// Later matches only register their teams (and apply the seasons' pull to
/// the seed), so a team that has yet to play still has a rating to compare.
pub fn rate(games: &[Game], until: i64) -> BTreeMap<i64, Rating> {
    let seasons: BTreeMap<i64, usize> = games.iter().map(|g| g.season).collect::<BTreeSet<_>>().into_iter().enumerate().map(|(i, s)| (s, i)).collect();
    let mut order: Vec<&Game> = games.iter().collect();
    order.sort_by_key(|g| (g.at.div_euclid(86_400), seasons[&g.season], g.id));
    let mut ratings: BTreeMap<i64, Rating> = BTreeMap::new();
    let mut i = 0;
    while i < order.len() {
        let day = order[i].at.div_euclid(86_400);
        let end = i + order[i..].iter().take_while(|g| g.at.div_euclid(86_400) == day).count();
        let today = &order[i..end];
        i = end;
        for g in today {
            for key in [g.home, g.away] {
                touch(&mut ratings, key, seasons[&g.season], &g.division);
            }
        }
        // Everything today is rated from the day's start, then applied.
        let start: BTreeMap<i64, f64> = ratings.iter().map(|(k, r)| (*k, r.points)).collect();
        let mut moves: BTreeMap<i64, f64> = BTreeMap::new();
        for g in today.iter().filter(|g| g.at < until) {
            let Outcome::Decided { home, margin, forfeit } = g.outcome else { continue };
            let weight = if forfeit { FORFEIT_WEIGHT } else { 1.0 + MARGIN_STEP * margin.clamp(0, 3) as f64 };
            let delta = K * weight * (home - expected(start[&g.home], start[&g.away]));
            *moves.entry(g.home).or_default() += delta;
            *moves.entry(g.away).or_default() -= delta;
            for key in [g.home, g.away] {
                let r = ratings.get_mut(&key).expect("registered above");
                if !forfeit {
                    r.matches += 1;
                }
                r.last = r.last.max(Some(g.at));
            }
        }
        for (key, delta) in moves {
            ratings.get_mut(&key).expect("registered above").points += delta;
        }
    }
    ratings
}

/// A team's place in the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub key: i64,
    pub rank: usize,
    pub points: f64,
    /// Places gained since a week ago (negative: lost); `None` for a team
    /// that had played nothing a week ago.
    pub change: Option<i64>,
    pub points_change: f64,
    pub matches: usize,
    pub division: Option<String>,
    pub last: Option<i64>,
}

/// Best first; evenly rated teams by matches played, then by identity.
fn standing(ratings: &BTreeMap<i64, Rating>, keys: &[i64]) -> Vec<i64> {
    let mut out = keys.to_vec();
    out.sort_by(|a, b| {
        let (x, y) = (&ratings[a], &ratings[b]);
        y.points.total_cmp(&x.points).then(y.matches.cmp(&x.matches)).then(a.cmp(b))
    });
    out
}

/// The table of the teams in the newest season among `games`, with each
/// one's change since `now` - 7 days.
pub fn rank(games: &[Game], now: i64) -> Vec<Ranked> {
    let after = rate(games, i64::MAX);
    let before = rate(games, now - WEEK);
    let Some(newest) = after.values().map(|r| r.season).max() else { return Vec::new() };
    let active: Vec<i64> = after.iter().filter(|(_, r)| r.season == newest).map(|(k, _)| *k).collect();
    let place_before: HashMap<i64, usize> = standing(&before, &active).into_iter().enumerate().map(|(i, k)| (k, i + 1)).collect();
    standing(&after, &active)
        .into_iter()
        .enumerate()
        .map(|(i, key)| {
            let (now_r, was) = (&after[&key], &before[&key]);
            Ranked {
                key,
                rank: i + 1,
                points: now_r.points,
                change: (was.matches > 0).then(|| place_before[&key] as i64 - (i as i64 + 1)),
                points_change: now_r.points - was.points,
                matches: now_r.matches,
                division: now_r.division.clone(),
                last: now_r.last,
            }
        })
        .collect()
}

// ---- Identities and games ----------------------------------------------------

fn name_key(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Which identity each team id is: ids joined by RGL's links and by equal
/// names, the identity being the lowest id of the set.
pub fn identities(names: &[(i64, String)], links: &[(i64, i64)]) -> HashMap<i64, i64> {
    let mut parent: HashMap<i64, i64> = HashMap::new();
    fn find(parent: &mut HashMap<i64, i64>, x: i64) -> i64 {
        let p = *parent.entry(x).or_insert(x);
        if p == x {
            return x;
        }
        let root = find(parent, p);
        parent.insert(x, root);
        root
    }
    fn join(parent: &mut HashMap<i64, i64>, a: i64, b: i64) {
        let (ra, rb) = (find(parent, a), find(parent, b));
        // The lower id stays the root: the identity does not depend on the order of joining.
        parent.insert(ra.max(rb), ra.min(rb));
    }
    for (id, _) in names {
        find(&mut parent, *id);
    }
    for &(a, b) in links {
        join(&mut parent, a, b);
    }
    let mut by_name: HashMap<String, i64> = HashMap::new();
    for (id, name) in names {
        match by_name.get(&name_key(name)) {
            Some(&first) => join(&mut parent, first, *id),
            None => {
                by_name.insert(name_key(name), *id);
            }
        }
    }
    let ids: Vec<i64> = parent.keys().copied().collect();
    ids.into_iter().map(|id| (id, find(&mut parent, id))).collect()
}

/// A match as a game, its teams through `key`; `None` for a match that
/// cannot be placed (no date, not two teams, a "Bye Week" placeholder).
pub fn game_from(m: &RglMatch, season: i64, key: &dyn Fn(i64) -> i64) -> Option<Game> {
    let at = rgl::rfc3339(m.match_date.as_deref())?;
    let home = m.teams.iter().find(|t| t.is_home).or(m.teams.first())?;
    let away = m.teams.iter().find(|t| t.team_id != home.team_id)?;
    if m.teams.len() != 2 || home.team_name.starts_with("Bye Week") || away.team_name.starts_with("Bye Week") {
        return None;
    }
    let (h, a) = (key(home.team_id), key(away.team_id));
    if h == a {
        return None;
    }
    let wins = |home_side: bool| m.maps.iter().filter(|x| matches!((x.home_score, x.away_score), (Some(p), Some(q)) if if home_side { p > q } else { q > p })).count() as i64;
    let (hw, aw) = (wins(true), wins(false));
    let winner_score = |w: i64| if w == home.team_id { Some(1.0) } else if w == away.team_id { Some(0.0) } else { None };
    let outcome = match (m.winner.and_then(winner_score), m.is_forfeit) {
        (Some(home), true) => Outcome::Decided { home, margin: 0, forfeit: true },
        (Some(home), false) => Outcome::Decided { home, margin: (hw - aw).abs(), forfeit: false },
        // No winner named: a tie only when every map was played and they split.
        (None, false) if m.winner.is_none() && !m.maps.is_empty() && hw == aw && m.maps.iter().all(|x| matches!((x.home_score, x.away_score), (Some(p), Some(q)) if p + q > 0)) => {
            Outcome::Decided { home: 0.5, margin: 0, forfeit: false }
        }
        _ => Outcome::Pending,
    };
    Some(Game { id: m.match_id, at, season, division: m.division_name.clone(), home: h, away: a, outcome })
}

// ---- The page's data -----------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglRankRow {
    pub rank: usize,
    /// The team's id in the live season.
    pub id: i64,
    /// Every id RGL gave the team, for finding it among the seasons.
    pub ids: Vec<i64>,
    pub name: String,
    pub tag: Option<String>,
    pub division: Option<String>,
    pub tier: Option<&'static str>,
    pub points: i64,
    /// Places gained since a week ago.
    pub change: Option<i64>,
    pub points_change: i64,
    pub matches: usize,
    /// Current roster, only where the team's page is already kept.
    pub players: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RglRanking {
    /// The live season, whose teams are ranked.
    pub season: Option<RglSeasonItem>,
    /// The seasons counted, newest first (the live one among them).
    pub used: Vec<RglSeasonItem>,
    /// The next season that could be included, if one is left.
    pub offer: Option<RglSeasonItem>,
    /// Unix seconds of the newest match counted.
    pub updated: Option<i64>,
    pub rows: Vec<RglRankRow>,
}

/// An earlier season's matches when they are on disk, else `None`: its page
/// is kept (so it is over) and most of its matches are. Asks nobody.
async fn on_disk(db: &Db, id: i64) -> Result<Option<Vec<RglMatch>>> {
    let path = format!("/seasons/{id}");
    let Some((body, _)) = db.rgl_answers(std::slice::from_ref(&path)).await?.remove(&path) else { return Ok(None) };
    let Ok(head) = serde_json::from_str::<rgl::SeasonHead>(&body) else { return Ok(None) };
    let ids = head.matches_played_during_season;
    if ids.is_empty() {
        return Ok(None);
    }
    let kept = db.rgl_matches(&ids).await?;
    if (kept.len() as f64) < ids.len() as f64 * ON_DISK {
        return Ok(None);
    }
    let mut out: Vec<RglMatch> = kept.values().filter_map(|b| serde_json::from_str(b).ok()).collect();
    out.sort_by_key(|m| m.match_id);
    Ok(Some(out))
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The ranking. Asks RGL for nothing the Home page has not asked (the live
/// season's matches come from memory when Home read them, and are otherwise
/// read as Home reads them), nor for any older season, unless `read` names
/// one: that one is read now, at RGL's pace, and kept.
pub async fn ranking(db: &Db, sources: &Sources, read: Option<i64>) -> Result<RglRanking> {
    let seasons = rgl::seasons(sources).await?;
    let live_matches = |id: i64| async move {
        match rgl::held_season_matches(id) {
            Some(m) => Ok(m),
            None => rgl::season_matches(db, sources, id).await,
        }
    };
    // As Home picks it: the newest season, or the one before while that has no match yet.
    let Some(mut live) = seasons.first().cloned() else { return Ok(RglRanking { season: None, used: Vec::new(), offer: None, updated: None, rows: Vec::new() }) };
    let mut live_ms = live_matches(live.id).await?;
    if live_ms.is_empty() && seasons.len() > 1 {
        live = seasons[1].clone();
        live_ms = live_matches(live.id).await?;
    }
    let mut counted: Vec<(RglSeasonItem, Vec<RglMatch>)> = Vec::new();
    let mut offer = None;
    for s in seasons.iter().filter(|s| s.id < live.id) {
        let ms = if read == Some(s.id) {
            // The one the person asked for.
            Some(rgl::season_matches(db, sources, s.id).await?)
        } else {
            on_disk(db, s.id).await?
        };
        match ms {
            Some(ms) => counted.push((s.clone(), ms)),
            None if offer.is_none() => offer = Some(s.clone()),
            None => {}
        }
    }
    counted.push((live.clone(), live_ms));

    // Teams RGL's pages are kept for (or held) give links and rosters, without a request.
    let ids: BTreeSet<i64> = counted.iter().flat_map(|(_, ms)| ms.iter().flat_map(|m| m.teams.iter().map(|t| t.team_id))).collect();
    let paths: Vec<String> = ids.iter().map(|id| format!("/teams/{id}")).collect();
    let stored = db.rgl_answers(&paths).await?;
    let mut raw: HashMap<i64, rgl::TeamRaw> = HashMap::new();
    for id in &ids {
        let path = format!("/teams/{id}");
        let body = stored.get(&path).map(|(b, _)| b.clone()).or_else(|| rgl::held(&path));
        if let Some(t) = body.and_then(|b| serde_json::from_str::<rgl::TeamRaw>(&b).ok()) {
            raw.insert(*id, t);
        }
    }
    let mut names: Vec<(i64, String)> = Vec::new();
    let mut latest: HashMap<i64, (i64, i64, String, Option<String>)> = HashMap::new();
    for m in counted.iter().flat_map(|(_, ms)| ms) {
        for t in &m.teams {
            names.push((t.team_id, t.team_name.clone()));
        }
    }
    let links: Vec<(i64, i64)> = raw.values().flat_map(|t| t.linked_teams.iter().map(|l| (t.team_id, *l))).collect();
    let who = identities(&names, &links);
    let key = |id: i64| who.get(&id).copied().unwrap_or(id);

    let mut games = Vec::new();
    for (s, ms) in &counted {
        for m in ms {
            let Some(g) = game_from(m, s.id, &key) else { continue };
            for t in &m.teams {
                let seen = latest.get(&key(t.team_id)).map(|(at, mid, ..)| (*at, *mid));
                if seen.is_none_or(|x| x < (g.at, g.id)) {
                    latest.insert(key(t.team_id), (g.at, g.id, t.team_name.clone(), t.team_tag.clone()));
                }
            }
            games.push(g);
        }
    }
    let table = rank(&games, unix_now());
    // Current id: the team's id in its newest match.
    let mut current: HashMap<i64, i64> = HashMap::new();
    let mut newest: HashMap<i64, (i64, i64)> = HashMap::new();
    for (_, ms) in &counted {
        for m in ms {
            let Some(at) = rgl::rfc3339(m.match_date.as_deref()) else { continue };
            for t in &m.teams {
                let k = key(t.team_id);
                if newest.get(&k).is_none_or(|x| *x < (at, m.match_id)) {
                    newest.insert(k, (at, m.match_id));
                    current.insert(k, t.team_id);
                }
            }
        }
    }
    let rows: Vec<RglRankRow> = table
        .iter()
        .map(|r| {
            let id = current.get(&r.key).copied().unwrap_or(r.key);
            let mut all: Vec<i64> = who.iter().filter(|(_, k)| **k == r.key).map(|(i, _)| *i).collect();
            all.sort_unstable();
            let (name, tag) = latest.get(&r.key).map(|(_, _, n, t)| (n.clone(), t.clone())).unwrap_or_default();
            let players = raw.get(&id).map(|t| rgl::roster_at(&t.players, None).into_iter().take(ROSTER_SHOWN).map(|p| p.name).collect()).unwrap_or_default();
            RglRankRow {
                rank: r.rank,
                id,
                ids: all,
                name,
                tag,
                tier: r.division.as_deref().and_then(rgl::tier_of),
                division: r.division.clone(),
                points: r.points.round() as i64,
                change: r.change,
                points_change: r.points_change.round() as i64,
                matches: r.matches,
                players,
            }
        })
        .collect();
    let updated = table.iter().filter_map(|r| r.last).max();
    let mut used: Vec<RglSeasonItem> = counted.into_iter().map(|(s, _)| s).collect();
    used.reverse();
    Ok(RglRanking { season: Some(live), used, offer, updated, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rgl::{RglMatchMap, RglMatchTeam};

    const DAY: i64 = 86_400;
    /// An hour into a day, so a few hours on stay in the same day.
    const T0: i64 = 1_699_920_000 + 3600;

    fn game(id: i64, at: i64, season: i64, home: i64, away: i64, outcome: Outcome) -> Game {
        Game { id, at, season, division: Some("Advanced".into()), home, away, outcome }
    }
    fn win(home: f64) -> Outcome {
        Outcome::Decided { home, margin: 0, forfeit: false }
    }
    fn points(r: &BTreeMap<i64, Rating>, k: i64) -> f64 {
        r[&k].points
    }

    #[test]
    fn a_win_over_an_equal_moves_sixteen_points_and_conserves_them() {
        let r = rate(&[game(1, T0, 1, 10, 20, win(1.0))], i64::MAX);
        assert!((points(&r, 10) - 1516.0).abs() < 1e-9);
        assert!((points(&r, 20) - 1484.0).abs() < 1e-9);
        assert_eq!((r[&10].matches, r[&20].matches), (1, 1));
    }

    #[test]
    fn a_tie_between_equals_moves_nothing_and_a_tie_against_a_stronger_team_helps() {
        let r = rate(&[game(1, T0, 1, 10, 20, win(0.5))], i64::MAX);
        assert_eq!(points(&r, 10), 1500.0);
        let r = rate(&[game(0, T0 - 10 * DAY, 1, 20, 30, win(1.0)), game(1, T0, 1, 10, 20, win(0.5))], i64::MAX);
        assert!(points(&r, 10) > 1500.0, "a draw with the team that has been winning gains");
    }

    #[test]
    fn a_bigger_win_moves_more() {
        let one = |margin| rate(&[game(1, T0, 1, 10, 20, Outcome::Decided { home: 1.0, margin, forfeit: false })], i64::MAX);
        assert!(points(&one(2), 10) > points(&one(0), 10));
        assert_eq!(points(&one(3), 10), points(&one(9), 10), "capped");
    }

    #[test]
    fn a_forfeit_moves_a_quarter_and_is_no_match_played() {
        let r = rate(&[game(1, T0, 1, 10, 20, Outcome::Decided { home: 1.0, margin: 0, forfeit: true })], i64::MAX);
        assert!((points(&r, 10) - 1504.0).abs() < 1e-9);
        assert_eq!(r[&10].matches, 0);
        assert_eq!(r[&10].last, Some(T0));
        let r = rate(&[game(1, T0, 1, 10, 20, Outcome::Pending)], i64::MAX);
        assert_eq!((points(&r, 10), r[&10].matches), (1500.0, 0));
    }

    #[test]
    fn matches_of_one_day_do_not_depend_on_their_order() {
        let a = game(1, T0, 1, 10, 20, win(1.0));
        let b = game(2, T0 + 3600, 1, 10, 30, win(1.0));
        let c = game(3, T0 + 7200, 1, 20, 30, win(0.0));
        let forward = rate(&[a.clone(), b.clone(), c.clone()], i64::MAX);
        let back = rate(&[c.clone(), b.clone(), a.clone()], i64::MAX);
        assert_eq!(forward, back);
        // Team 10's two wins are both against teams at the day's start rating.
        assert!((points(&forward, 10) - 1532.0).abs() < 1e-9);
        // Two wins over the same team: rated together on one day, the second is against the
        // team as it was that morning; a day apart, against the team the first win changed.
        let together = rate(&[game(1, T0, 1, 10, 20, win(1.0)), game(2, T0 + 3600, 1, 10, 20, win(1.0))], i64::MAX);
        let apart = rate(&[game(1, T0, 1, 10, 20, win(1.0)), game(2, T0 + DAY, 1, 10, 20, win(1.0))], i64::MAX);
        assert!((points(&together, 10) - 1532.0).abs() < 1e-9);
        assert!(points(&apart, 10) < points(&together, 10) - 1.0);
    }

    #[test]
    fn a_new_season_pulls_a_team_toward_its_divisions_seed() {
        let mut g2 = game(2, T0 + 100 * DAY, 2, 10, 30, win(1.0));
        g2.division = Some("Invite".into());
        let r = rate(&[game(1, T0, 1, 10, 20, win(1.0)), g2], i64::MAX);
        // 1516 pulled 30% of the way to 1600 is 1541.2; then a win over a 1600 team.
        let pulled = 1600.0 + 0.7 * (1516.0 - 1600.0);
        assert!(points(&r, 10) > pulled);
        assert_eq!(r[&10].season, 1);
        assert_eq!(r[&10].division.as_deref(), Some("Invite"));
        // A team out of the second season keeps what it had.
        assert_eq!(r[&20].season, 0);
        assert!((points(&r, 20) - 1484.0).abs() < 1e-9);
        // A gap of two seasons pulls twice.
        let gap = rate(&[game(1, T0, 1, 10, 20, win(1.0)), game(2, T0 + 100 * DAY, 2, 40, 50, Outcome::Pending), game(3, T0 + 200 * DAY, 3, 10, 60, Outcome::Pending)], i64::MAX);
        assert!((points(&gap, 10) - (1500.0 + 0.49 * 16.0)).abs() < 1e-9);
    }

    #[test]
    fn rank_orders_the_newest_seasons_teams_and_reports_the_weeks_change() {
        let now = T0 + 20 * DAY;
        let games = vec![
            game(1, T0, 5, 10, 20, win(1.0)),
            game(2, T0 + 10 * DAY, 5, 20, 30, win(1.0)),
            // This week: 20 beats 10, and 30 beats 10.
            game(3, now - 2 * DAY, 5, 20, 10, win(1.0)),
            game(4, now - DAY, 5, 30, 10, win(1.0)),
            // A team of an earlier season only is not in the table.
            game(5, T0 - 100 * DAY, 4, 90, 91, win(1.0)),
        ];
        let table = rank(&games, now);
        assert_eq!(table.iter().map(|r| r.key).collect::<Vec<_>>().len(), 3);
        assert!(table.iter().all(|r| r.key != 90));
        assert!(table.windows(2).all(|w| w[0].points >= w[1].points));
        let ten = table.iter().find(|r| r.key == 10).unwrap();
        assert_eq!(ten.rank, 3);
        assert!(ten.points_change < 0.0);
        // Before the week, 10 stood first.
        assert_eq!(ten.change, Some(-2));
    }

    #[test]
    fn a_team_with_nothing_played_a_week_ago_has_no_change() {
        let now = T0 + 20 * DAY;
        let table = rank(&[game(1, now - DAY, 5, 10, 20, win(1.0))], now);
        assert!(table.iter().all(|r| r.change.is_none()));
        assert_eq!(table[0].key, 10);
    }

    #[test]
    fn identities_follow_links_and_names_whatever_the_order() {
        let names = vec![(5, "The Duds".into()), (9, "the  duds".into()), (7, "Other".into()), (3, "Renamed".into())];
        let one = identities(&names, &[(3, 5)]);
        assert_eq!((one[&3], one[&5], one[&9]), (3, 3, 3));
        assert_eq!(one[&7], 7);
        let mut back = names.clone();
        back.reverse();
        assert_eq!(identities(&back, &[(5, 3)]), one);
    }

    fn rgl_match(winner: Option<i64>, forfeit: bool, maps: &[(i64, i64)]) -> RglMatch {
        RglMatch {
            match_id: 1,
            match_date: Some("2026-10-01T01:30:00.000Z".into()),
            match_name: None,
            division_name: Some("Invite".into()),
            season_id: None,
            season_name: None,
            is_forfeit: forfeit,
            winner,
            teams: vec![
                RglMatchTeam { team_id: 10, team_name: "A".into(), team_tag: None, is_home: true },
                RglMatchTeam { team_id: 20, team_name: "B".into(), team_tag: None, is_home: false },
            ],
            maps: maps.iter().map(|(h, a)| RglMatchMap { map_name: "m".into(), home_score: Some(*h), away_score: Some(*a) }).collect(),
        }
    }

    #[test]
    fn matches_become_games_with_their_outcome() {
        let key = |id: i64| id;
        let g = |m: RglMatch| game_from(&m, 1, &key).unwrap().outcome;
        assert_eq!(g(rgl_match(Some(10), false, &[(3, 1), (4, 0)])), Outcome::Decided { home: 1.0, margin: 2, forfeit: false });
        assert_eq!(g(rgl_match(Some(20), false, &[(3, 1), (1, 3)])), Outcome::Decided { home: 0.0, margin: 0, forfeit: false });
        assert_eq!(g(rgl_match(None, false, &[(3, 1), (1, 3)])), Outcome::Decided { home: 0.5, margin: 0, forfeit: false });
        assert_eq!(g(rgl_match(Some(20), true, &[])), Outcome::Decided { home: 0.0, margin: 0, forfeit: true });
        // Not played: no winner and no scores, or a double forfeit.
        assert_eq!(g(rgl_match(None, false, &[])), Outcome::Pending);
        assert_eq!(g(rgl_match(None, false, &[(0, 0), (0, 0)])), Outcome::Pending);
        assert_eq!(g(rgl_match(None, true, &[])), Outcome::Pending);
        let mut bye = rgl_match(Some(10), false, &[]);
        bye.teams[1].team_name = "Bye Week - Invite".into();
        assert!(game_from(&bye, 1, &key).is_none());
    }

    #[tokio::test]
    async fn a_season_is_on_disk_only_when_its_page_and_most_matches_are() {
        let db = Db::connect_in_memory().await.unwrap();
        assert!(on_disk(&db, 193).await.unwrap().is_none());
        db.store_rgl_answer("/seasons/193", r#"{"name":"HL Season 24","matchesPlayedDuringSeason":[1,2,3,4,5]}"#).await.unwrap();
        for id in 1..=3 {
            db.store_rgl_match(id, &serde_json::to_string(&rgl_match(Some(10), false, &[(3, 1)])).unwrap()).await.unwrap();
        }
        assert!(on_disk(&db, 193).await.unwrap().is_none(), "three of five is not enough");
        db.store_rgl_match(4, &serde_json::to_string(&rgl_match(Some(10), false, &[(3, 1)])).unwrap()).await.unwrap();
        assert_eq!(on_disk(&db, 193).await.unwrap().unwrap().len(), 4);
        // Matches without their season's page are not counted either.
        db.store_rgl_match(9, &serde_json::to_string(&rgl_match(Some(10), false, &[])).unwrap()).await.unwrap();
        assert!(on_disk(&db, 198).await.unwrap().is_none());
    }
}
