//! What kind of game each match was: an ETF2L official, a scrim, or a pug.
//!
//! Pure: every input is already in memory, so the rules are testable and the
//! whole pass reruns in milliseconds after any sync.
//!
//! **Officials.** trends.tf tags most of them. The rest are found through the
//! ETF2L rosters: an official's log is one where the owner's side is mostly
//! one team's roster and the other side mostly the opponent's, close to the
//! scheduled time. On this account that rule agrees with trends.tf on every
//! official trends.tf tagged, and finds seven more (2018 and 2023 seasons).
//!
//! **Scrims.** A team game is one where most of the owner's side are
//! *regulars* — people who were on the owner's team often around that date.
//! Pugs and lobbies draw a new team every game, so the split is sharp: on this
//! account 560 games have five or more regulars and 150 have two or fewer.
//! The team is named from the nearest official whose roster holds most of the
//! owner's side; the opponent likewise, from any official roster. A roster
//! also catches what the regulars rule cannot: the first scrims with a newly
//! joined team, before anyone has played enough games to count as a regular.

use serde::Serialize;
use std::collections::HashMap;

const HOUR: i64 = 3600;
const DAY: i64 = 24 * HOUR;

/// An official's log starts at most this long before the scheduled time...
const OFFICIAL_BEFORE: i64 = 4 * HOUR;
/// ...and at most this long after (reschedules within the evening, long BO3s).
const OFFICIAL_AFTER: i64 = 10 * HOUR;
/// Players of the right roster needed on each side to call a log an official.
const ROSTER_SIDE_MIN: usize = 5;

/// A teammate is a regular around a date when they were on the owner's team
/// at least `REGULAR_MIN_GAMES` times within `REGULAR_WINDOW` either side.
const REGULAR_WINDOW: i64 = 45 * DAY;
const REGULAR_MIN_GAMES: usize = 6;
/// Regulars needed for a team game. Highlander has eight teammates.
const SCRIM_MIN_REGULARS: usize = 5;

/// Rosters are only used to name a team this close to the official they came from.
const TEAM_WINDOW: i64 = 150 * DAY;
/// Players of a roster needed on a side to put that team's name on it.
const TEAM_NAME_MIN: usize = 4;
/// Players of the owner's own roster that make a game a scrim even without
/// regulars: a newly joined team has no history yet, but it has a roster.
const TEAM_GAME_ROSTER_MIN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Official,
    Scrim,
    Pug,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Official => "official",
            Kind::Scrim => "scrim",
            Kind::Pug => "pug",
        }
    }
}

/// One Highlander match the owner played, reduced to who was on which side.
#[derive(Debug, Clone)]
pub struct Game {
    pub log_id: i64,
    pub played_at: i64,
    /// The ETF2L match trends.tf linked, if any.
    pub trends_match: Option<i64>,
    /// The owner's teammates, owner excluded.
    pub mates: Vec<u32>,
    pub opponents: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Official {
    pub match_id: i64,
    pub time: i64,
    pub clan1: (i64, String),
    pub clan2: (i64, String),
    /// Registered players and their team. Mercs (no team) are left out.
    pub roster: HashMap<u32, i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Context {
    pub log_id: i64,
    pub kind: Kind,
    pub etf2l_match_id: Option<i64>,
    /// `trends` or `roster`, for officials.
    pub link_method: Option<&'static str>,
    pub team: Option<(i64, String)>,
    pub opponent: Option<(i64, String)>,
    pub regulars: usize,
}

pub fn classify(games: &[Game], officials: &[Official]) -> Vec<Context> {
    let by_id: HashMap<i64, &Official> = officials.iter().map(|o| (o.match_id, o)).collect();
    let regulars = regular_counts(games);

    // Pass 1: officials, and which side the owner played for in each. The
    // owner's clan in an official is what names their scrims in pass 2.
    let mut out: Vec<Context> = games
        .iter()
        .zip(&regulars)
        .map(|(g, &regulars)| {
            let linked = g
                .trends_match
                .map(|id| (id, "trends"))
                .or_else(|| roster_match(g, officials).map(|id| (id, "roster")));
            match linked {
                Some((id, method)) => {
                    let (team, opponent) = match by_id.get(&id) {
                        Some(o) => sides(g, o),
                        None => (None, None),
                    };
                    Context {
                        log_id: g.log_id,
                        kind: Kind::Official,
                        etf2l_match_id: Some(id),
                        link_method: Some(method),
                        team,
                        opponent,
                        regulars,
                    }
                }
                None => Context {
                    log_id: g.log_id,
                    kind: if regulars >= SCRIM_MIN_REGULARS { Kind::Scrim } else { Kind::Pug },
                    etf2l_match_id: None,
                    link_method: None,
                    team: None,
                    opponent: None,
                    regulars,
                },
            }
        })
        .collect();

    // The owner's teams, as seen in their officials: (time, clan, roster).
    let own: Vec<(i64, (i64, String), &Official)> = out
        .iter()
        .filter_map(|c| {
            let o = by_id.get(&c.etf2l_match_id?)?;
            Some((o.time, c.team.clone()?, *o))
        })
        .collect();

    // Pass 2: name the teams in scrims, and promote pugs played with a roster.
    for (c, g) in out.iter_mut().zip(games) {
        if c.kind == Kind::Official {
            continue;
        }
        let team = best_team(&g.mates, g.played_at, own.iter().map(|(t, clan, o)| (*t, clan, *o)));
        if c.kind == Kind::Pug && team.as_ref().is_some_and(|(n, _)| *n >= TEAM_GAME_ROSTER_MIN) {
            c.kind = Kind::Scrim;
        }
        if c.kind != Kind::Scrim {
            continue;
        }
        c.team = team.map(|(_, clan)| clan);
        let mine = c.team.as_ref().map(|t| t.0);
        let all = officials.iter().flat_map(|o| [(o.time, &o.clan1, o), (o.time, &o.clan2, o)]);
        c.opponent = best_team(&g.opponents, g.played_at, all.filter(|(_, clan, _)| Some(clan.0) != mine))
            .map(|(_, clan)| clan);
    }
    out
}

/// For each game, how many of the owner's teammates were regulars at the time.
fn regular_counts(games: &[Game]) -> Vec<usize> {
    let mut seen: HashMap<u32, Vec<i64>> = HashMap::new();
    for g in games {
        for &m in &g.mates {
            seen.entry(m).or_default().push(g.played_at);
        }
    }
    for times in seen.values_mut() {
        times.sort_unstable();
    }
    games
        .iter()
        .map(|g| {
            g.mates
                .iter()
                .filter(|m| {
                    let ts = &seen[m];
                    let lo = ts.partition_point(|&t| t < g.played_at - REGULAR_WINDOW);
                    let hi = ts.partition_point(|&t| t <= g.played_at + REGULAR_WINDOW);
                    hi - lo >= REGULAR_MIN_GAMES
                })
                .count()
        })
        .collect()
}

/// The official this game was, judged by who was on each side. Closest to the
/// scheduled time when more than one fits.
fn roster_match(g: &Game, officials: &[Official]) -> Option<i64> {
    officials
        .iter()
        .filter(|o| g.played_at >= o.time - OFFICIAL_BEFORE && g.played_at <= o.time + OFFICIAL_AFTER)
        .filter(|o| {
            let (a, b) = (o.clan1.0, o.clan2.0);
            let fits = |ours: i64, theirs: i64| {
                on_roster(&g.mates, o, ours) >= ROSTER_SIDE_MIN
                    && on_roster(&g.opponents, o, theirs) >= ROSTER_SIDE_MIN
            };
            fits(a, b) || fits(b, a)
        })
        .min_by_key(|o| (g.played_at - o.time).abs())
        .map(|o| o.match_id)
}

/// Which clan the owner played for, and which they played against. Decided by
/// whose roster the owner's side overlaps more; unknown when neither does.
#[allow(clippy::type_complexity)]
fn sides(g: &Game, o: &Official) -> (Option<(i64, String)>, Option<(i64, String)>) {
    let one = on_roster(&g.mates, o, o.clan1.0) + on_roster(&g.opponents, o, o.clan2.0);
    let two = on_roster(&g.mates, o, o.clan2.0) + on_roster(&g.opponents, o, o.clan1.0);
    match one.cmp(&two) {
        std::cmp::Ordering::Greater => (Some(o.clan1.clone()), Some(o.clan2.clone())),
        std::cmp::Ordering::Less => (Some(o.clan2.clone()), Some(o.clan1.clone())),
        std::cmp::Ordering::Equal => (None, None),
    }
}

fn on_roster(players: &[u32], o: &Official, team: i64) -> usize {
    players.iter().filter(|p| o.roster.get(p) == Some(&team)).count()
}

/// The team whose nearby roster holds most of `players`, and how many of them
/// it holds. Ties go to the roster closest in time, so a renamed or re-formed
/// team takes its name from the right era.
fn best_team<'a>(
    players: &[u32],
    at: i64,
    candidates: impl Iterator<Item = (i64, &'a (i64, String), &'a Official)>,
) -> Option<(usize, (i64, String))> {
    candidates
        .filter(|(t, _, _)| (at - t).abs() <= TEAM_WINDOW)
        .map(|(t, clan, o)| (on_roster(players, o, clan.0), -(at - t).abs(), clan))
        .filter(|(n, _, _)| *n >= TEAM_NAME_MIN)
        .max_by_key(|(n, closeness, _)| (*n, *closeness))
        .map(|(n, _, clan)| (n, clan.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000;

    fn official(id: i64, time: i64, a: &[u32], b: &[u32]) -> Official {
        let mut roster = HashMap::new();
        roster.extend(a.iter().map(|&p| (p, 1)));
        roster.extend(b.iter().map(|&p| (p, 2)));
        Official { match_id: id, time, clan1: (1, "Ours".into()), clan2: (2, "Theirs".into()), roster }
    }

    fn game(id: i64, at: i64, mates: &[u32], opps: &[u32]) -> Game {
        Game { log_id: id, played_at: at, trends_match: None, mates: mates.to_vec(), opponents: opps.to_vec() }
    }

    const US: [u32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    const THEM: [u32; 9] = [21, 22, 23, 24, 25, 26, 27, 28, 29];

    /// A season of team games: the same eight teammates every few days.
    fn season() -> Vec<Game> {
        (0..10).map(|i| game(100 + i, T0 + i * 3 * DAY, &US, &[40, 41, 42, 43, 44, 45, 46, 47, 48])).collect()
    }

    #[test]
    fn untagged_official_is_found_by_roster() {
        let o = official(9, T0 + 5 * DAY, &US, &THEM);
        let mut games = season();
        games.push(game(1, T0 + 5 * DAY + HOUR, &US, &THEM));
        let ctx = classify(&games, &[o]);
        let off = ctx.iter().find(|c| c.log_id == 1).unwrap();
        assert_eq!(off.kind, Kind::Official);
        assert_eq!(off.etf2l_match_id, Some(9));
        assert_eq!(off.link_method, Some("roster"));
        assert_eq!(off.team.as_ref().unwrap().1, "Ours");
        assert_eq!(off.opponent.as_ref().unwrap().1, "Theirs");
    }

    #[test]
    fn the_warmup_before_an_official_is_not_the_official() {
        // Same teammates, same evening, but the other side is not the opponent's roster.
        let o = official(9, T0 + 5 * DAY, &US, &THEM);
        let mut games = season();
        games.push(game(2, T0 + 5 * DAY - 30 * 60, &US, &[60, 61, 62, 63, 64, 65, 66, 67, 68]));
        games.push(game(1, T0 + 5 * DAY + HOUR, &US, &THEM));
        let ctx = classify(&games, &[o]);
        let warm = ctx.iter().find(|c| c.log_id == 2).unwrap();
        assert_eq!(warm.kind, Kind::Scrim);
        assert_eq!(warm.team.as_ref().unwrap().1, "Ours", "named from the official's roster");
    }

    #[test]
    fn a_trends_tag_wins_even_without_rosters() {
        let mut g = game(3, T0, &US, &THEM);
        g.trends_match = Some(77);
        let ctx = classify(&[g], &[]);
        assert_eq!(ctx[0].kind, Kind::Official);
        assert_eq!(ctx[0].link_method, Some("trends"));
        assert_eq!(ctx[0].team, None);
    }

    #[test]
    fn owner_side_follows_the_roster_not_the_clan_order() {
        // The owner's team is clan2 on ETF2L.
        let o = official(9, T0, &THEM, &US);
        let mut g = game(4, T0 + HOUR, &US, &THEM);
        g.trends_match = Some(9);
        let ctx = classify(&[g], &[o]);
        assert_eq!(ctx[0].team.as_ref().unwrap().0, 2);
        assert_eq!(ctx[0].opponent.as_ref().unwrap().0, 1);
    }

    #[test]
    fn a_fresh_team_every_game_is_a_pug() {
        let games: Vec<Game> = (0..10)
            .map(|i| {
                let base = 1000 + i as u32 * 10;
                game(i, T0 + i * DAY, &(base..base + 8).collect::<Vec<_>>(), &THEM)
            })
            .collect();
        assert!(classify(&games, &[]).iter().all(|c| c.kind == Kind::Pug && c.regulars == 0));
    }

    #[test]
    fn regulars_need_enough_games_nearby() {
        // Five games together, then one a year later: nobody is a regular then.
        let mut games: Vec<Game> = (0..5).map(|i| game(i, T0 + i * DAY, &US, &THEM)).collect();
        games.push(game(9, T0 + 365 * DAY, &US, &THEM));
        let ctx = classify(&games, &[]);
        assert_eq!(ctx[0].regulars, 0, "five games is one short of a regular");
        assert_eq!(ctx[5].kind, Kind::Pug);
    }

    #[test]
    fn scrim_opponent_is_named_from_any_roster() {
        // We once played Theirs officially; a later scrim against most of them is named.
        let o = official(9, T0, &US, &THEM);
        let mut games = season();
        games.push(game(5, T0 + 20 * DAY, &US, &THEM[..6]));
        let ctx = classify(&games, &[o]);
        let s = ctx.iter().find(|c| c.log_id == 5).unwrap();
        assert_eq!(s.kind, Kind::Scrim);
        assert_eq!(s.opponent.as_ref().unwrap().1, "Theirs");
    }

    #[test]
    fn a_new_team_is_recognised_by_its_roster_before_anyone_is_a_regular() {
        // First week on a new team: one official, two scrims, nobody regular yet.
        let o = official(9, T0, &US, &THEM);
        let mut off = game(1, T0 + HOUR, &US, &THEM);
        off.trends_match = Some(9);
        let games = vec![off, game(2, T0 + DAY, &US, &[50, 51]), game(3, T0 + 2 * DAY, &US[..5], &[50, 51])];
        let ctx = classify(&games, &[o]);
        assert_eq!(ctx[1].regulars, 0);
        assert_eq!(ctx[1].kind, Kind::Scrim);
        assert_eq!(ctx[1].team.as_ref().unwrap().1, "Ours");
        assert_eq!(ctx[2].kind, Kind::Scrim, "five of the roster is enough");
    }

    #[test]
    fn four_of_a_roster_names_a_scrim_but_does_not_make_a_pug_one() {
        let o = official(9, T0, &US, &THEM);
        let mut off = game(1, T0 + HOUR, &US, &THEM);
        off.trends_match = Some(9);
        let games = vec![off, game(2, T0 + DAY, &[1, 2, 3, 4, 90, 91, 92, 93], &[50])];
        assert_eq!(classify(&games, &[o])[1].kind, Kind::Pug);
    }

    #[test]
    fn team_names_expire() {
        let o = official(9, T0, &US, &THEM);
        let mut games = season();
        let late: Vec<Game> =
            (0..8).map(|i| game(200 + i, T0 + 400 * DAY + i * DAY, &US, &[50, 51, 52])).collect();
        games.extend(late);
        let ctx = classify(&games, &[o]);
        let last = ctx.iter().find(|c| c.log_id == 207).unwrap();
        assert_eq!(last.kind, Kind::Scrim);
        assert_eq!(last.team, None, "a roster from over a year ago names nothing");
    }
}
