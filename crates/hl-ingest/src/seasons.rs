//! Seasons, and how you played in each.
//!
//! ETF2L's API gives a competition's matches but no season dates, so seasons
//! come from the officials already stored for the owner: every competition
//! of one season (its divisions, group stages, playoffs) groups under the
//! season's number, and anything else under its name before the colon
//! ("Highlander Winter 2024: Low Playoffs" is "Winter 2024"). A season runs
//! from six days before its first official, which takes in the week of
//! scrims leading up to it, to the day after its last. Every game in that
//! window counts: officials, scrims and pugs.
//!
//! The newest season stays open up to today while its last official is
//! under [`ONGOING_DAYS`] old: the rest of it has not been played yet.
//!
//! Seasons without an official of yours are not listed; a custom date range
//! covers them.

use anyhow::Result;
use hl_core::{SteamId, TfClass};
use hl_db::{ClassGame, Db, FightFilter, FightTotals, SeasonOfficial, FIGHT_COLUMNS};
use hl_rating::MODEL_VERSION;
use serde::Serialize;

const DAY: i64 = 86_400;
/// Scrims in the week before week one belong to the season.
const LEAD_IN_DAYS: i64 = 6;
/// A season whose last official is this recent may still be running.
const ONGOING_DAYS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Season {
    /// Stable id: `s33`, or a slug of the name.
    pub key: String,
    /// "Season 33 (Spring 2025)", "Winter 2024", "AFA 2025".
    pub name: String,
    /// Unix seconds, inclusive.
    pub from: i64,
    pub to: i64,
    pub officials: usize,
    pub divisions: Vec<String>,
    /// Still being played: `to` is now.
    pub ongoing: bool,
}

/// Group officials into seasons, newest first. `now` is unix seconds.
pub fn group(officials: &[SeasonOfficial], now: i64) -> Vec<Season> {
    let mut out: Vec<(Season, i64, i64)> = Vec::new();
    for o in officials {
        let (key, name) = season_of(&o.competition);
        let i = match out.iter().position(|(s, _, _)| s.key == key) {
            Some(i) => i,
            None => {
                out.push((Season { key, name: name.clone(), from: 0, to: 0, officials: 0, divisions: vec![], ongoing: false }, i64::MAX, i64::MIN));
                out.len() - 1
            }
        };
        let (s, first, last) = &mut out[i];
        // The fullest name wins: "Season 33 (Spring 2025)" over "Season 33".
        if name.len() > s.name.len() {
            s.name = name;
        }
        s.officials += 1;
        *first = (*first).min(o.time);
        *last = (*last).max(o.time);
        if let Some(d) = o.division.as_deref().filter(|d| !d.is_empty()) {
            if !s.divisions.iter().any(|x| x == d) {
                s.divisions.push(d.to_string());
            }
        }
    }
    let mut seasons: Vec<Season> = out
        .into_iter()
        .map(|(mut s, first, last)| {
            s.from = first / DAY * DAY - LEAD_IN_DAYS * DAY;
            s.to = last / DAY * DAY + 2 * DAY - 1;
            s
        })
        .collect();
    seasons.sort_by_key(|s| std::cmp::Reverse(s.from));
    if let Some(newest) = seasons.first_mut() {
        if now - newest.to < ONGOING_DAYS * DAY && now > newest.to {
            newest.to = now;
            newest.ongoing = true;
        }
    }
    seasons
}

/// `(key, display name)` for a competition name.
fn season_of(competition: &str) -> (String, String) {
    let name = competition.trim().trim_start_matches("Highlander").trim();
    if let Some(rest) = name.strip_prefix("Season ") {
        let num: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !num.is_empty() {
            // Keep "(Spring 2025)" when the name has it.
            let head = name.split(':').next().unwrap_or(name).trim();
            let label = if head.contains('(') { head.to_string() } else { format!("Season {num}") };
            return (format!("s{num}"), label);
        }
    }
    let head = name.split(':').next().unwrap_or(name).trim().to_string();
    let key = head.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    (key, head)
}

/// How one class went over a stretch of time.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodStats {
    pub games: usize,
    pub officials: usize,
    pub scrims: usize,
    pub pugs: usize,
    pub wins: usize,
    pub losses: usize,
    pub ties: usize,
    /// Mean rating.
    pub rating: Option<f64>,
    pub minutes: f64,
    /// Damage per minute on the class.
    pub dpm: Option<f64>,
    pub kd: Option<f64>,
    pub kills_per10: Option<f64>,
    pub deaths_per10: Option<f64>,
    /// Opening duels won, 0-1 (fights pass).
    pub opening_won: Option<f64>,
    /// Kills traded back within 3 s, 0-1.
    pub traded: Option<f64>,
}

/// Stats over the games played in `[from, to]`; `None` bounds are open.
pub fn period(games: &[ClassGame], from: Option<i64>, to: Option<i64>) -> PeriodStats {
    let inside = |g: &&ClassGame| {
        let t = g.played_at.unwrap_or(0);
        from.is_none_or(|f| t >= f) && to.is_none_or(|x| t <= x)
    };
    let gs: Vec<&ClassGame> = games.iter().filter(inside).collect();
    let mut p = PeriodStats { games: gs.len(), ..Default::default() };
    if gs.is_empty() {
        return p;
    }
    let (mut kills, mut deaths, mut dmg, mut secs, mut score) = (0i64, 0i64, 0i64, 0i64, 0.0);
    for g in &gs {
        match g.kind.as_deref() {
            Some("official") => p.officials += 1,
            Some("scrim") => p.scrims += 1,
            Some("pug") => p.pugs += 1,
            _ => {}
        }
        match g.result {
            Some("W") => p.wins += 1,
            Some("L") => p.losses += 1,
            Some("T") => p.ties += 1,
            _ => {}
        }
        kills += g.kills;
        deaths += g.deaths;
        dmg += g.dmg;
        secs += g.time_s;
        score += g.score;
        p.minutes += g.minutes;
    }
    let mins = secs as f64 / 60.0;
    p.rating = Some(score / gs.len() as f64);
    if mins > 0.0 {
        p.dpm = Some(dmg as f64 / mins);
        p.kills_per10 = Some(kills as f64 / mins * 10.0);
        p.deaths_per10 = Some(deaths as f64 / mins * 10.0);
    }
    p.kd = (deaths > 0).then(|| kills as f64 / deaths as f64);
    p
}

fn col(t: &FightTotals, name: &str) -> f64 {
    FIGHT_COLUMNS.iter().position(|c| *c == name).map_or(0.0, |i| t.values[i] as f64)
}

fn ratio(n: f64, d: f64) -> Option<f64> {
    (d > 0.0).then(|| n / d)
}

fn add_fights(p: &mut PeriodStats, t: &FightTotals) {
    p.opening_won = ratio(col(t, "opening_kills"), col(t, "opening_kills") + col(t, "opening_deaths"));
    p.traded = ratio(col(t, "traded_kills"), col(t, "kills"));
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonRow {
    pub season: Season,
    pub stats: PeriodStats,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonsView {
    pub class: TfClass,
    pub seasons: Vec<SeasonRow>,
    pub all_time: PeriodStats,
}

/// Every season, newest first.
pub async fn list(db: &Db) -> Result<Vec<Season>> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    Ok(group(&db.officials_played().await?, now))
}

/// One class, season by season, and all time.
pub async fn by_season(db: &Db, me: SteamId, class: TfClass) -> Result<SeasonsView> {
    let games = db.class_games(me.account_id(), class.as_str(), MODEL_VERSION).await?;
    let fights = |from: Option<i64>, to: Option<i64>| FightFilter {
        class: class.as_str(),
        model_version: MODEL_VERSION,
        kind: None,
        from,
        to,
    };
    let mut seasons = Vec::new();
    for s in list(db).await? {
        let mut stats = period(&games, Some(s.from), Some(s.to));
        let (mine, _) = db.fight_totals(me.account_id(), &fights(Some(s.from), Some(s.to))).await?;
        add_fights(&mut stats, &mine);
        seasons.push(SeasonRow { season: s, stats });
    }
    let mut all_time = period(&games, None, None);
    let (mine, _) = db.fight_totals(me.account_id(), &fights(None, None)).await?;
    add_fights(&mut all_time, &mine);
    Ok(SeasonsView { class, seasons, all_time })
}

/// One line of the profile's fights card: you against the players you face.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FightLine {
    pub label: String,
    /// How it is measured, e.g. "% of your kills".
    pub unit: String,
    pub you: Option<f64>,
    pub pool: Option<f64>,
    /// 1 when higher is better, -1 when lower is, 0 when neither.
    pub better: i8,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FightsCard {
    pub games: i64,
    pub pool_games: i64,
    pub lines: Vec<FightLine>,
}

/// The fights card for the profile, under the profile's filters.
pub async fn fights_card(
    db: &Db,
    me: SteamId,
    class: TfClass,
    kind: Option<&str>,
    from: Option<i64>,
    to: Option<i64>,
) -> Result<Option<FightsCard>> {
    let f = FightFilter { class: class.as_str(), model_version: MODEL_VERSION, kind, from, to };
    let (mine, pool) = db.fight_totals(me.account_id(), &f).await?;
    if mine.games == 0 {
        return Ok(None);
    }
    let per10 = |t: &FightTotals, c: &str| ratio(col(t, c) * 10.0, t.minutes);
    let share = |t: &FightTotals, n: &str, d: &str| ratio(col(t, n) * 100.0, col(t, d));
    let duel = |t: &FightTotals| ratio(col(t, "opening_kills") * 100.0, col(t, "opening_kills") + col(t, "opening_deaths"));
    let line = |label: &str, unit: &str, you: Option<f64>, pool: Option<f64>, better: i8, hint: &str| FightLine {
        label: label.into(),
        unit: unit.into(),
        you,
        pool,
        better,
        hint: hint.into(),
    };
    let lines = vec![
        line(
            "Opening duels won",
            "% of fights you opened or died opening",
            duel(&mine),
            duel(&pool),
            1,
            "The first kill of a fight (more than 10 s after the last one): how often it was yours rather than you dying.",
        ),
        line("Opening kills", "per 10 min", per10(&mine, "opening_kills"), per10(&pool, "opening_kills"), 1, "First kills of fights you got."),
        line(
            "First pick of the round",
            "% of rounds",
            share(&mine, "first_picks", "rounds"),
            share(&pool, "first_picks", "rounds"),
            1,
            "Rounds where the first kill was yours.",
        ),
        line(
            "First death of the round",
            "% of rounds",
            share(&mine, "first_deaths", "rounds"),
            share(&pool, "first_deaths", "rounds"),
            -1,
            "Rounds where you were the first to die.",
        ),
        line(
            "Kills traded back",
            "% of your kills",
            share(&mine, "traded_kills", "kills"),
            share(&pool, "traded_kills", "kills"),
            -1,
            "Your team lost someone within 3 s of your kill, so it opened nothing.",
        ),
        line(
            "Died right after your kill",
            "% of your kills",
            share(&mine, "died_after_kill", "kills"),
            share(&pool, "died_after_kill", "kills"),
            -1,
            "You died within 3 s of your own kill: often a sign of not repositioning.",
        ),
        line("Trades", "per 10 min", per10(&mine, "trade_kills"), per10(&pool, "trade_kills"), 1, "You killed back within 3 s of losing a teammate."),
        line(
            "Clean-ups",
            "% of your kills",
            share(&mine, "cleanup_kills", "kills"),
            share(&pool, "cleanup_kills", "kills"),
            0,
            "Kills while your team was already up a player. Not bad, but they rarely decide a fight.",
        ),
        line(
            "Picks into a ready charge",
            "per 10 min",
            per10(&mine, "charged_picks"),
            per10(&pool, "charged_picks"),
            1,
            "Medic, Demoman, Heavy or Pyro killed while their team held a ready uber.",
        ),
        line("Drops", "per 10 min", per10(&mine, "drops"), per10(&pool, "drops"), 1, "Medics killed holding a ready uber."),
        line("Forced ubers", "per 10 min", per10(&mine, "forces"), per10(&pool, "forces"), 1, "Enemy ubers popped right after your damage on the Medic."),
        line(
            "Deaths during your uber",
            "per 10 min",
            per10(&mine, "deaths_during_uber"),
            per10(&pool, "deaths_during_uber"),
            -1,
            "Dying while your team's uber is in use.",
        ),
    ];
    Ok(Some(FightsCard { games: mine.games, pool_games: pool.games, lines }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(c: &str, d: &str, t: i64) -> SeasonOfficial {
        SeasonOfficial { competition: c.into(), division: Some(d.into()), time: t }
    }

    #[test]
    fn competitions_group_into_seasons() {
        let day = DAY;
        let s = group(&[
            o("Highlander Season 33 (Spring 2025): Group Stage #1", "Low", 100 * day),
            o("Highlander Season 33 (Spring 2025): Low Playoffs", "Low", 150 * day),
            o("Highlander Season 34: Mid Playoffs", "Mid", 300 * day),
            o("Highlander Season 34 (Summer 2025)", "Mid", 280 * day),
            o("Highlander Winter 2024: Low", "Low", 40 * day),
            o("Highlander Winter 2024: Low Playoffs", "Low", 60 * day),
            o("Highlander AFA 2025: Top Tiers", "Top", 330 * day),
        ], 1000 * day);
        let names: Vec<&str> = s.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["AFA 2025", "Season 34 (Summer 2025)", "Season 33 (Spring 2025)", "Winter 2024"]);
        let s33 = &s[2];
        assert_eq!((s33.key.as_str(), s33.officials), ("s33", 2));
        assert_eq!(s33.from, 94 * day, "six days of scrims before week one");
        assert_eq!(s33.to, 152 * day - 1, "through the day after the last official");
        assert_eq!(s[3].key, "winter-2024");
        assert!(!s[0].ongoing, "months ago");
    }

    #[test]
    fn the_newest_season_runs_to_today_while_recent() {
        let s = group(&[o("Highlander Season 36 (Autumn 2026): High", "High", 100 * DAY)], 120 * DAY);
        assert!(s[0].ongoing);
        assert_eq!(s[0].to, 120 * DAY);
    }

    fn g(t: i64, kind: &str, result: &'static str, score: f64, kills: i64, deaths: i64, dmg: i64) -> ClassGame {
        ClassGame {
            log_id: t,
            played_at: Some(t),
            kind: Some(kind.into()),
            result: Some(result),
            score,
            minutes: 30.0,
            kills,
            deaths,
            assists: 0,
            dmg,
            time_s: 1800,
        }
    }

    #[test]
    fn a_period_sums_only_its_games() {
        let games = [g(10, "official", "W", 60.0, 20, 10, 6000), g(20, "scrim", "L", 40.0, 10, 10, 3000), g(99, "pug", "W", 90.0, 50, 1, 9000)];
        let p = period(&games, Some(0), Some(50));
        assert_eq!((p.games, p.officials, p.scrims, p.pugs, p.wins, p.losses), (2, 1, 1, 0, 1, 1));
        assert_eq!(p.rating, Some(50.0));
        assert_eq!(p.dpm, Some(150.0), "9,000 damage over 60 minutes");
        assert_eq!(p.kd, Some(1.5));
        assert_eq!(period(&games, None, None).games, 3);
        assert_eq!(period(&games, Some(1000), None).games, 0);
    }
}
