//! Season rankings and the deeper player profile (the MVP research,
//! October 2026): every logs.tf number per season, division and class,
//! the season rating the awards use, impact, and HLTV-style attribute cards.
//!
//! Everything here reads `log_stat` (see `logstat.rs`) beside the ratings;
//! nothing is fetched.

use crate::catalogue::{Catalogue, Mvp, Team, MIN_RANK_GAMES};
use anyhow::Result;
use hl_db::{Db, LogStat};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Minutes on a class in a log for it to count towards averages and
/// percentiles: a spawn swap is not a game on the class.
const MIN_SECS: i64 = 300;
/// Maps on a class for a player to be placed among everyone else on it.
const MIN_POPULATION_MAPS: usize = 6;
/// Pseudo-maps at the class average a season's impact is pulled towards,
/// as the season rating is towards 1.00.
const IMPACT_SHRINK_MAPS: f64 = 4.0;

/// Everything summed over a player's logs on one class.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub maps: usize,
    pub time_s: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub dmg: i64,
    pub dt: i64,
    pub heal: i64,
    pub ubers: i64,
    pub drops: i64,
    pub airshots: i64,
    pub headshots: i64,
    pub backstabs: i64,
    pub caps: i64,
    pub medkits: i64,
    pub medkits_hp: i64,
    pub longest_streak: i64,
    pub adv_lost: i64,
    /// Average seconds to build and to use a charge, over the maps that have one.
    pub build_s: Option<f64>,
    pub use_s: Option<f64>,
    /// From the server logs: seconds and maps that had one.
    pub raw_time_s: i64,
    pub raw_maps: usize,
    pub opening_kills: i64,
    pub opening_deaths: i64,
    pub first_picks: i64,
    pub trade_kills: i64,
    pub traded_deaths: i64,
    pub cleanup_kills: i64,
    pub forces: i64,
    pub fights_present: i64,
    pub fights_kast: i64,
    pub impact: f64,
    pub impact_attack: f64,
    pub impact_defence: f64,
    pub kills_attack: i64,
    pub deaths_attack: i64,
    pub kills_defence: i64,
    pub deaths_defence: i64,
    pub secs_attack: i64,
    pub secs_defence: i64,
    pub down_kills: i64,
    pub multi2: i64,
    pub multi3: i64,
    #[serde(skip)]
    build_n: usize,
    #[serde(skip)]
    use_n: usize,
}

impl Totals {
    pub fn add(&mut self, s: &LogStat) {
        self.maps += 1;
        self.time_s += s.time_s;
        self.kills += s.kills;
        self.deaths += s.deaths;
        self.assists += s.assists;
        self.dmg += s.dmg;
        self.dt += s.dt;
        self.heal += s.heal;
        self.ubers += s.ubers;
        self.drops += s.drops;
        self.airshots += s.airshots;
        self.headshots += s.headshots;
        self.backstabs += s.backstabs;
        self.caps += s.caps;
        self.medkits += s.medkits;
        self.medkits_hp += s.medkits_hp;
        self.longest_streak = self.longest_streak.max(s.longest_streak);
        self.adv_lost += s.adv_lost;
        if let Some(b) = s.build_s.filter(|b| *b > 0.0) {
            self.build_s = Some((self.build_s.unwrap_or(0.0) * self.build_n as f64 + b) / (self.build_n + 1) as f64);
            self.build_n += 1;
        }
        if let Some(u) = s.use_s.filter(|u| *u > 0.0) {
            self.use_s = Some((self.use_s.unwrap_or(0.0) * self.use_n as f64 + u) / (self.use_n + 1) as f64);
            self.use_n += 1;
        }
        if let Some(i) = s.impact {
            self.raw_time_s += s.time_s;
            self.raw_maps += 1;
            self.impact += i;
            let o = |v: Option<i64>| v.unwrap_or(0);
            self.opening_kills += o(s.opening_kills);
            self.opening_deaths += o(s.opening_deaths);
            self.first_picks += o(s.first_picks);
            self.trade_kills += o(s.trade_kills);
            self.traded_deaths += o(s.traded_deaths);
            self.cleanup_kills += o(s.cleanup_kills);
            self.forces += o(s.forces);
            self.fights_present += o(s.fights_present);
            self.fights_kast += o(s.fights_kast);
            self.impact_attack += s.impact_attack.unwrap_or(0.0);
            self.impact_defence += s.impact_defence.unwrap_or(0.0);
            self.kills_attack += o(s.kills_attack);
            self.deaths_attack += o(s.deaths_attack);
            self.kills_defence += o(s.kills_defence);
            self.deaths_defence += o(s.deaths_defence);
            self.secs_attack += o(s.secs_attack);
            self.secs_defence += o(s.secs_defence);
            self.down_kills += o(s.down_kills);
            self.multi2 += o(s.multi2);
            self.multi3 += o(s.multi3);
        }
    }

    fn per30(&self, v: i64) -> Option<f64> {
        (self.time_s > 0).then(|| v as f64 * 1800.0 / self.time_s as f64)
    }
    fn per30_raw(&self, v: f64) -> Option<f64> {
        (self.raw_time_s > 0).then(|| v * 1800.0 / self.raw_time_s as f64)
    }
    pub fn impact_per30(&self) -> Option<f64> {
        self.per30_raw(self.impact)
    }
}

/// A class's impact per 30 minutes over single maps: its average and spread,
/// for the impact rating's scale (1.00 average, 0.25 a standard deviation).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactScale {
    pub mean: f64,
    pub sd: f64,
}

impl ImpactScale {
    fn of(rows: &[LogStat]) -> ImpactScale {
        let v: Vec<f64> =
            rows.iter().filter(|r| r.main && r.time_s >= MIN_SECS).filter_map(|r| r.impact.map(|i| i * 1800.0 / r.time_s as f64)).collect();
        if v.len() < 30 {
            return ImpactScale { mean: 0.0, sd: 0.5 };
        }
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt().max(0.05);
        ImpactScale { mean, sd }
    }

    /// One map's or a season's impact per 30, on the rating's scale. A
    /// season is pulled towards the class average by [`IMPACT_SHRINK_MAPS`].
    pub fn rating(&self, per30: f64, maps: usize) -> f64 {
        let shrunk = (per30 * maps as f64 + self.mean * IMPACT_SHRINK_MAPS) / (maps as f64 + IMPACT_SHRINK_MAPS);
        1.0 + (shrunk - self.mean) / self.sd * 0.25
    }

    pub fn map_rating(&self, impact: f64, time_s: i64) -> Option<f64> {
        (time_s > 0).then(|| 1.0 + (impact * 1800.0 / time_s as f64 - self.mean) / self.sd * 0.25)
    }
}

/// One impact rating per player in a log: a match page's column.
pub async fn match_impact(db: &Db, log_id: i64) -> Result<HashMap<u32, f64>> {
    let rows = db.log_stats_of(log_id).await?;
    let mut out = HashMap::new();
    let mut scales: HashMap<String, ImpactScale> = HashMap::new();
    for r in rows.iter().filter(|r| r.main) {
        let Some(i) = r.impact else { continue };
        if !scales.contains_key(&r.class) {
            scales.insert(r.class.clone(), ImpactScale::of(&db.log_stats_of_class(&r.class).await?));
        }
        if let Some(v) = scales[&r.class].map_rating(i, r.time_s) {
            out.insert(r.account_id as u32, v);
        }
    }
    Ok(out)
}

// ---- The season rankings page ----------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonRow {
    /// Ranked from [`MIN_RANK_GAMES`] officials; `None` below, listed after.
    pub rank: Option<usize>,
    pub account_id: u32,
    pub name: String,
    pub team: Option<Team>,
    pub officials: usize,
    /// Rated maps (logs) in the stage shown.
    pub maps: usize,
    /// Enough officials, and a playoff place where there were playoffs: may
    /// take the class award ([`crate::catalogue::AwardRules`]).
    pub eligible: bool,
    /// The season rating (what the awards and the rank use) and the plain
    /// average beside it.
    pub rating: f64,
    pub average: f64,
    /// The average strength of the teams faced (their season's average
    /// official rating), which the season rating is adjusted for.
    pub opponents: Option<f64>,
    /// The season's impact on the rating's scale, and per 30 minutes.
    pub impact: Option<f64>,
    pub impact_per30: Option<f64>,
    /// "season", "evp", "class": the awards this row holds.
    pub awards: Vec<String>,
    pub totals: Totals,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonTable {
    pub season: i64,
    pub season_name: String,
    pub tier: i64,
    pub division: String,
    pub class: String,
    pub stage: String,
    pub seasons: Vec<(i64, String)>,
    pub divisions: Vec<(i64, String)>,
    pub rows: Vec<SeasonRow>,
    pub impact_scale: ImpactScale,
    /// What the class award asks here: officials on the class, and a team
    /// among the medals (`playoffs_required` names that: a medal team).
    pub award_officials: usize,
    pub playoffs_required: bool,
}

/// One season, division and class: every player with every number, ranked
/// by the season rating. `stage` is "all", "regular" or "playoffs".
pub async fn season_table(db: &Db, season: Option<i64>, tier: Option<i64>, class: &str, stage: &str) -> Result<SeasonTable> {
    let cat = Catalogue::load(db).await?;
    let rated = crate::catalogue::rated_officials(db, None).await?;
    let lines = cat.season_lines(&rated);
    // Seasons with rated officials, newest first; their divisions.
    let mut seasons: Vec<(i64, String)> = lines.values().map(|l| l.season).collect::<HashSet<_>>().into_iter().map(|s| (s, cat.season_title(s))).collect();
    seasons.sort_by_key(|(s, _)| std::cmp::Reverse(crate::leagues::season_order(*s)));
    let season = season.or_else(|| seasons.first().map(|s| s.0)).unwrap_or(0);
    let mut divisions: BTreeMap<i64, String> = BTreeMap::new();
    for l in lines.values().filter(|l| l.season == season) {
        divisions.entry(l.tier).or_insert_with(|| l.division.clone());
    }
    let tier = tier.filter(|t| divisions.contains_key(t)).or_else(|| divisions.keys().next().copied()).unwrap_or(0);

    // The games of this season, division and class, in the chosen stage.
    let in_stage = |w: f64| match stage {
        "regular" => w == 1.0,
        "playoffs" => w > 1.0,
        _ => true,
    };
    let members: HashMap<u32, &crate::catalogue::SeasonLine> =
        lines.values().filter(|l| l.season == season && l.tier == tier && l.class == class).map(|l| (l.account_id, l)).collect();
    let windows = cat.season_windows();
    let games: Vec<&hl_db::RatedGame> = rated
        .iter()
        .filter(|g| g.official && g.class == class && members.contains_key(&g.account_id))
        .filter(|g| crate::catalogue::game_season(&cat, &windows, g) == Some(season))
        .filter(|g| in_stage(cat.stage_weight(g.etf2l_match_id)))
        .collect();
    let logs: Vec<i64> = games.iter().map(|g| g.log_id).collect::<HashSet<_>>().into_iter().collect();
    let stats: HashMap<(i64, i64, String), LogStat> =
        db.log_stats_for_logs(&logs).await?.into_iter().map(|s| ((s.log_id, s.account_id, s.class.clone()), s)).collect();
    let scale = ImpactScale::of(&db.log_stats_of_class(class).await?);
    let awards: Vec<Mvp> = cat.mvps(&rated).into_iter().filter(|m| m.season == season && m.tier == Some(tier)).collect();
    // Eligibility as the awards read it: own-team games only.
    let award_lines = cat.award_lines(&rated);
    let rules = cat.award_rules(&award_lines);

    // Per player: the stage's season line (opponent-adjusted, as the awards
    // read it), and the summed numbers.
    let stage_games: Vec<hl_db::RatedGame> = games.iter().map(|g| (*g).clone()).collect();
    let stage_lines = cat.season_lines_with(&stage_games, &rated);
    let mut per: HashMap<u32, (crate::catalogue::SeasonLine, Totals)> = HashMap::new();
    for g in &games {
        let e = per.entry(g.account_id).or_insert_with(|| {
            let l = stage_lines.get(&(season, tier, class.to_string(), g.account_id)).cloned().unwrap_or_else(|| members[&g.account_id].clone());
            (l, Totals::default())
        });
        if let Some(s) = stats.get(&(g.log_id, i64::from(g.account_id), class.to_string())) {
            e.1.add(s);
        }
    }
    let mut rows: Vec<SeasonRow> = per
        .into_iter()
        .map(|(a, (line, totals))| {
            let impact_per30 = totals.impact_per30();
            SeasonRow {
                rank: None,
                account_id: a,
                name: cat.name_of(a),
                team: Some(cat.team(line.team)),
                officials: line.officials.len(),
                maps: line.maps(),
                opponents: line.opponent_strength(),
                eligible: award_lines.get(&(season, tier, class.to_string(), a)).is_some_and(|l| rules.class_award(l)),
                rating: line.rating(),
                average: line.average(),
                impact: impact_per30.map(|p| scale.rating(p, totals.raw_maps)),
                impact_per30,
                awards: awards
                    .iter()
                    .filter(|m| m.account_id == a && (m.kind != "class" || m.class == class))
                    .map(|m| m.kind.clone())
                    .collect(),
                totals,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        (b.officials >= MIN_RANK_GAMES).cmp(&(a.officials >= MIN_RANK_GAMES)).then(b.rating.total_cmp(&a.rating)).then(b.officials.cmp(&a.officials))
    });
    for (i, r) in rows.iter_mut().enumerate() {
        if r.officials >= MIN_RANK_GAMES {
            r.rank = Some(i + 1);
        }
    }
    Ok(SeasonTable {
        season,
        season_name: seasons.iter().find(|s| s.0 == season).map(|s| s.1.clone()).unwrap_or_default(),
        tier,
        division: divisions.get(&tier).cloned().unwrap_or_default(),
        class: class.to_string(),
        stage: stage.to_string(),
        seasons,
        divisions: divisions.into_iter().collect(),
        rows,
        impact_scale: scale,
        award_officials: rules.need(season, tier),
        playoffs_required: rules.has_playoffs.contains(&(season, tier)),
    })
}

// ---- The deeper profile ----------------------------------------------------

/// One number on a profile: the player's value, the class average, where
/// they stand among everyone on the class, and a verdict.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    pub key: String,
    pub value: f64,
    pub average: f64,
    /// 0-100 among players with [`MIN_POPULATION_MAPS`] maps on the class.
    pub percentile: f64,
    /// "good" (70th percentile and up), "okay", "poor" (30th and below).
    pub verdict: String,
    /// Lower is better (deaths, damage taken).
    pub lower_better: bool,
}

/// An attribute card: a 0-100 score and the stats behind it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribute {
    pub key: String,
    pub score: f64,
    pub stats: Vec<Stat>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VsDivision {
    pub tier: i64,
    pub division: String,
    pub rating: f64,
    pub maps: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Side {
    pub kills_per30: Option<f64>,
    pub deaths_per30: Option<f64>,
    pub impact_per30: Option<f64>,
    pub minutes: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Weapon {
    pub weapon: String,
    pub kills: i64,
    pub dmg: i64,
    /// Hits over shots, where logs.tf recorded shots.
    pub accuracy: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassDeep {
    pub class: String,
    pub maps: usize,
    pub minutes: f64,
    /// The headline row: rating, impact and the usual per-30s, with verdicts.
    pub headline: Vec<Stat>,
    pub attributes: Vec<Attribute>,
    pub vs_divisions: Vec<VsDivision>,
    pub attack: Option<Side>,
    pub defence: Option<Side>,
    /// Kills with the side a player or more down; bursts of 2 and 3+ kills.
    pub down_kills: i64,
    pub multi2: i64,
    pub multi3: i64,
    /// Fights the player was in where they killed, assisted, survived or
    /// were traded (KAST), in percent.
    pub kast: Option<f64>,
    pub weapons: Vec<Weapon>,
    pub totals: Totals,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerDeep {
    pub account_id: u32,
    pub classes: Vec<ClassDeep>,
}

/// What a stat is, how it is worked out from the totals, and which way is good.
struct Def {
    key: &'static str,
    lower_better: bool,
    raw: bool,
    f: fn(&Totals) -> Option<f64>,
}

const fn d(key: &'static str, lower_better: bool, raw: bool, f: fn(&Totals) -> Option<f64>) -> Def {
    Def { key, lower_better, raw, f }
}

fn ratio(a: i64, b: i64) -> Option<f64> {
    (b > 0).then(|| a as f64 / b as f64)
}

fn per_min(t: &Totals, v: i64) -> Option<f64> {
    (t.time_s > 0).then(|| v as f64 * 60.0 / t.time_s as f64)
}

const K30: Def = d("kills_per30", false, false, |t| t.per30(t.kills));
const D30: Def = d("deaths_per30", true, false, |t| t.per30(t.deaths));
const A30: Def = d("assists_per30", false, false, |t| t.per30(t.assists));
const DPM: Def = d("dpm", false, false, |t| per_min(t, t.dmg));
const DTM: Def = d("dtm", true, false, |t| per_min(t, t.dt));
const KD: Def = d("kd", false, false, |t| ratio(t.kills, t.deaths.max(1)));
const IMP: Def = d("impact_per30", false, true, |t| t.per30_raw(t.impact));
const OPEN: Def = d("opening_kills_per30", false, true, |t| t.per30_raw(t.opening_kills as f64));
const FIRST: Def = d("first_picks_per30", false, true, |t| t.per30_raw(t.first_picks as f64));
const OPEN_WIN: Def = d("opening_success", false, true, |t| ratio(t.opening_kills, t.opening_kills + t.opening_deaths));
const TRADE: Def = d("trade_kills_per30", false, true, |t| t.per30_raw(t.trade_kills as f64));
const TRADED: Def = d("traded_death_share", false, true, |t| ratio(t.traded_deaths, t.deaths));
const CLEANUP: Def = d("cleanup_kills_per30", false, true, |t| t.per30_raw(t.cleanup_kills as f64));
const DOWN: Def = d("down_kills_per30", false, true, |t| t.per30_raw(t.down_kills as f64));
const MULTI: Def = d("multi_kills_per30", false, true, |t| t.per30_raw((t.multi2 + t.multi3) as f64));
const KAST: Def = d("kast", false, true, |t| ratio(t.fights_kast, t.fights_present).map(|v| v * 100.0));
const HEAL: Def = d("heals_per_minute", false, false, |t| per_min(t, t.heal));
const UBERS: Def = d("ubers_per30", false, false, |t| t.per30(t.ubers));
const DROPS: Def = d("drops_per30", true, false, |t| t.per30(t.drops));
const BUILD: Def = d("build_s", true, false, |t| t.build_s);
const HS: Def = d("headshots_per30", false, false, |t| t.per30(t.headshots));
const BS: Def = d("backstabs_per30", false, false, |t| t.per30(t.backstabs));
const AIR: Def = d("airshots_per30", false, false, |t| t.per30(t.airshots));
const FORCES: Def = d("forces_per30", false, true, |t| t.per30_raw(t.forces as f64));
const CAPS: Def = d("caps_per30", false, false, |t| t.per30(t.caps));

/// The attribute cards for a class: name and the stats it averages.
fn attributes_of(class: &str) -> Vec<(&'static str, Vec<&'static Def>)> {
    let mut v: Vec<(&'static str, Vec<&'static Def>)> = vec![
        ("firepower", vec![&K30, &DPM, &KD]),
        ("opening", vec![&OPEN, &FIRST, &OPEN_WIN]),
        ("trading", vec![&TRADE, &TRADED]),
        ("closing", vec![&CLEANUP, &DOWN, &MULTI]),
        ("survival", vec![&D30, &DTM, &KAST]),
        ("impact", vec![&IMP, &FORCES]),
    ];
    match class {
        "medic" => {
            v[0] = ("healing", vec![&HEAL, &UBERS, &BUILD]);
            v.push(("keeping_the_charge", vec![&DROPS, &D30]));
        }
        "sniper" => v.push(("sniping", vec![&HS])),
        "spy" => v.push(("stabbing", vec![&BS])),
        "soldier" | "demoman" => v.push(("airshots", vec![&AIR])),
        "scout" => v.push(("objective", vec![&CAPS])),
        _ => v.push(("support", vec![&A30])),
    }
    v
}

fn headline_of(class: &str) -> Vec<&'static Def> {
    let mut v: Vec<&'static Def> = vec![&IMP, &K30, &D30, &DPM, &KAST];
    if class == "medic" {
        v = vec![&IMP, &HEAL, &UBERS, &DROPS, &D30];
    }
    v
}

/// Where `x` falls among `pop` (sorted), 0-100; flipped for lower-is-better.
fn percentile(pop: &[f64], x: f64, lower_better: bool) -> f64 {
    if pop.is_empty() {
        return 50.0;
    }
    let below = pop.partition_point(|v| *v < x);
    let equal = pop[below..].partition_point(|v| *v <= x);
    let p = (below as f64 + equal as f64 / 2.0) / pop.len() as f64 * 100.0;
    if lower_better { 100.0 - p } else { p }
}

fn verdict(p: f64) -> &'static str {
    if p >= 70.0 {
        "good"
    } else if p <= 30.0 {
        "poor"
    } else {
        "okay"
    }
}

/// A player's numbers on every class they have [`MIN_POPULATION_MAPS`] on,
/// each placed among everyone else's on that class.
pub async fn player_deep(db: &Db, account: u32) -> Result<PlayerDeep> {
    let mine = db.log_stats(Some(account)).await?;
    let mut by_class: BTreeMap<String, Vec<&LogStat>> = BTreeMap::new();
    for s in mine.iter().filter(|s| s.time_s >= MIN_SECS) {
        by_class.entry(s.class.clone()).or_default().push(s);
    }
    let rated = db.rated_games(hl_rating::MODEL_VERSION, Some(account)).await?;
    let opp = crate::catalogue::opponent_divisions(db, &rated.iter().map(|g| (g.log_id, g.account_id, g.played_at)).collect::<Vec<_>>()).await?;
    let weapons = db.log_weapons(account).await?;

    let mut classes = Vec::new();
    for (class, rows) in by_class {
        if rows.len() < MIN_POPULATION_MAPS {
            continue;
        }
        let mut t = Totals::default();
        for r in &rows {
            t.add(r);
        }
        // Everyone on the class: one total per player.
        let all = db.log_stats_of_class(&class).await?;
        let scale = ImpactScale::of(&all);
        let mut players: HashMap<i64, Totals> = HashMap::new();
        for r in all.iter().filter(|r| r.time_s >= MIN_SECS) {
            players.entry(r.account_id).or_default().add(r);
        }
        let pop: Vec<&Totals> = players.values().filter(|p| p.maps >= MIN_POPULATION_MAPS).collect();
        let stat = |def: &Def| -> Option<Stat> {
            let value = (def.f)(&t)?;
            let mut vals: Vec<f64> = pop.iter().filter(|p| !def.raw || p.raw_time_s > 0).filter_map(|p| (def.f)(p)).collect();
            vals.sort_by(|a, b| a.total_cmp(b));
            let average = if vals.is_empty() { value } else { vals.iter().sum::<f64>() / vals.len() as f64 };
            let p = percentile(&vals, value, def.lower_better);
            Some(Stat { key: def.key.to_string(), value, average, percentile: p, verdict: verdict(p).to_string(), lower_better: def.lower_better })
        };
        let mut headline: Vec<Stat> = Vec::new();
        // The rating first, against everyone's season-long average on the class.
        let my_rated: Vec<f64> = rated.iter().filter(|g| g.class == class).map(|g| g.score).collect();
        if !my_rated.is_empty() {
            let avg = my_rated.iter().sum::<f64>() / my_rated.len() as f64;
            headline.push(Stat {
                key: "rating".into(),
                value: avg,
                average: 1.0,
                percentile: (50.0 + (avg - 1.0) / 0.25 * 34.0).clamp(0.0, 100.0),
                verdict: if avg >= 1.10 { "good" } else if avg <= 0.90 { "poor" } else { "okay" }.into(),
                lower_better: false,
            });
        }
        headline.extend(headline_of(&class).into_iter().filter_map(stat));
        // The impact headline on the rating's scale.
        if let Some(h) = headline.iter_mut().find(|s| s.key == "impact_per30") {
            h.key = "impact".into();
            h.value = scale.rating(h.value, t.raw_maps);
            h.average = 1.0;
        }
        let attributes = attributes_of(&class)
            .into_iter()
            .filter_map(|(key, defs)| {
                let stats: Vec<Stat> = defs.into_iter().filter_map(stat).collect();
                (!stats.is_empty()).then(|| Attribute {
                    key: key.to_string(),
                    score: stats.iter().map(|s| s.percentile).sum::<f64>() / stats.len() as f64,
                    stats,
                })
            })
            .collect();

        // Rating by the opposite number's division.
        let mut vs: BTreeMap<i64, (String, Vec<f64>)> = BTreeMap::new();
        for g in rated.iter().filter(|g| g.class == class) {
            if let Some((tier, name)) = opp.get(&g.log_id) {
                vs.entry(*tier).or_insert_with(|| (name.clone(), Vec::new())).1.push(g.score);
            }
        }
        let vs_divisions = vs
            .into_iter()
            .filter(|(_, (_, v))| !v.is_empty())
            .map(|(tier, (division, v))| VsDivision { tier, division, rating: v.iter().sum::<f64>() / v.len() as f64, maps: v.len() })
            .collect();

        let side = |k: i64, d: i64, imp: f64, secs: i64| {
            (secs >= 600).then(|| Side {
                kills_per30: Some(k as f64 * 1800.0 / secs as f64),
                deaths_per30: Some(d as f64 * 1800.0 / secs as f64),
                impact_per30: Some(imp * 1800.0 / secs as f64),
                minutes: secs as f64 / 60.0,
            })
        };
        let mut w: HashMap<&str, (i64, i64, i64, i64)> = HashMap::new();
        for x in weapons.iter().filter(|x| x.class == class) {
            let e = w.entry(x.weapon.as_str()).or_default();
            e.0 += x.kills;
            e.1 += x.dmg;
            e.2 += x.shots;
            e.3 += x.hits;
        }
        let mut weapons_out: Vec<Weapon> = w
            .into_iter()
            .map(|(weapon, (kills, dmg, shots, hits))| Weapon { weapon: weapon.to_string(), kills, dmg, accuracy: (shots > 0).then(|| hits as f64 / shots as f64) })
            .collect();
        weapons_out.sort_by(|a, b| b.kills.cmp(&a.kills).then(b.dmg.cmp(&a.dmg)));

        classes.push(ClassDeep {
            class: class.clone(),
            maps: t.maps,
            minutes: t.time_s as f64 / 60.0,
            headline,
            attributes,
            vs_divisions,
            attack: side(t.kills_attack, t.deaths_attack, t.impact_attack, t.secs_attack),
            defence: side(t.kills_defence, t.deaths_defence, t.impact_defence, t.secs_defence),
            down_kills: t.down_kills,
            multi2: t.multi2,
            multi3: t.multi3,
            kast: ratio(t.fights_kast, t.fights_present).map(|v| v * 100.0),
            weapons: weapons_out,
            totals: t,
        });
    }
    classes.sort_by_key(|c| std::cmp::Reverse(c.maps));
    Ok(PlayerDeep { account_id: account, classes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_and_verdicts() {
        let pop = vec![1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile(&pop, 4.5, false), 100.0);
        assert_eq!(percentile(&pop, 0.5, true), 100.0);
        assert_eq!(verdict(75.0), "good");
        assert_eq!(verdict(50.0), "okay");
        assert_eq!(verdict(10.0), "poor");
    }

    #[test]
    fn impact_on_the_rating_scale() {
        let s = ImpactScale { mean: 0.2, sd: 0.6 };
        assert!((s.map_rating(0.8 / 1800.0 * 1800.0, 1800).unwrap() - 1.25).abs() < 1e-9);
        // A season of few maps is pulled towards the average.
        assert!(s.rating(0.8, 2) < s.rating(0.8, 20));
    }
}

// ---- One player's season, worked out ---------------------------------------

/// One official in a season breakdown.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialRow {
    pub match_id: i64,
    pub time: Option<i64>,
    /// "Week 3", "Quarter Finals".
    pub round: String,
    pub opponent: Option<Team>,
    /// From the player's side: "won", "lost", "draw".
    pub result: Option<String>,
    pub stage_weight: f64,
    pub maps: Vec<String>,
    pub minutes: f64,
    /// The official's rating, its logs by minutes, before and after the
    /// opponents' strength.
    pub rating: f64,
    pub opponent_strength: Option<f64>,
    pub adjustment: f64,
    pub adjusted: f64,
    /// Played for their own team: counts for awards.
    pub own_team: bool,
}

/// One part of the rating, averaged over the season's logs by minutes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartAverage {
    pub component: String,
    pub label: String,
    pub unit: String,
    pub raw: f64,
    pub percentile: f64,
    pub weight: f64,
    pub logs: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonBreakdown {
    pub officials: Vec<OfficialRow>,
    /// The season rating: (sum of weight x adjusted + SHRINK x 1.00) /
    /// (sum of weights + SHRINK).
    pub sum_weighted: f64,
    pub sum_weights: f64,
    pub shrink: f64,
    pub rating: f64,
    pub opponent_slope: f64,
    pub parts: Vec<PartAverage>,
}

/// Why a player's season rating is what it is: every official with its
/// rating, opponents and weight, the sum that makes the season, and the
/// rating's parts over the season's logs.
pub async fn season_breakdown(db: &Db, w: &hl_rating::Weights, season: i64, tier: i64, class: &str, account: u32) -> Result<SeasonBreakdown> {
    let cat = Catalogue::load(db).await?;
    let rated = crate::catalogue::rated_officials(db, None).await?;
    let lines = cat.season_lines(&rated);
    let key = (season, tier, class.to_string(), account);
    let Some(line) = lines.get(&key) else { anyhow::bail!("no season on {class} for that player there") };
    let own: HashSet<i64> = rated
        .iter()
        .filter(|g| g.account_id == account && g.class == class && g.etf2l_match_id.is_some_and(|m| line.officials.contains(&m)))
        .filter(|g| cat.own_team_game(g))
        .filter_map(|g| g.etf2l_match_id)
        .collect();
    let mut officials: Vec<OfficialRow> = line
        .games
        .iter()
        .map(|(&id, o)| {
            let info = cat.official_info(id, line.team);
            let adjusted = o.rating();
            let adjustment = o.opponent.map_or(0.0, |s| crate::catalogue::OPPONENT_SLOPE * (s - 1.0));
            let mut maps: Vec<String> = o.maps.iter().cloned().collect();
            maps.sort();
            OfficialRow {
                match_id: id,
                time: info.as_ref().and_then(|i| i.0),
                round: info.as_ref().map(|i| i.1.clone()).unwrap_or_default(),
                opponent: info.as_ref().and_then(|i| i.2.clone()),
                result: info.as_ref().and_then(|i| i.3.clone()),
                stage_weight: o.stage,
                maps,
                minutes: o.minutes,
                rating: adjusted - adjustment,
                opponent_strength: o.opponent,
                adjustment,
                adjusted,
                own_team: own.contains(&id),
            }
        })
        .collect();
    officials.sort_by_key(|o| o.time.unwrap_or(0));
    let sum_weighted: f64 = officials.iter().map(|o| o.stage_weight * o.adjusted).sum();
    let sum_weights: f64 = officials.iter().map(|o| o.stage_weight).sum();

    // The parts, from each log rated again as the rating pass rates it.
    let baseline = crate::rating::load_baseline(db).await?;
    /// A part's sums over the logs, each by minutes.
    #[derive(Default)]
    struct Sums {
        label: String,
        unit: String,
        raw: f64,
        pct: f64,
        weight: f64,
        minutes: f64,
        logs: usize,
    }
    let mut acc: BTreeMap<String, Sums> = BTreeMap::new();
    for id in line.games.values().flat_map(|o| o.log_ids.iter().copied()) {
        let (json, zip) = match db.league_log_files(id).await? {
            (Some(j), z) => (j, z),
            _ => match db.raw_log(id).await? {
                Some(j) => (j, db.rawlog(id).await?),
                None => continue,
            },
        };
        let w2 = w.clone();
        let Ok(Ok(perfs)) = tokio::task::spawn_blocking(move || crate::league_rating::rate_one(id, &json, zip.as_deref(), &w2)).await else { continue };
        let Some(perf) = perfs.iter().find(|p| p.account_id == account && p.class.as_str() == class) else { continue };
        let Some(r) = hl_rating::model::rate(perf, &baseline, w) else { continue };
        for p in r.parts {
            let e = acc.entry(p.component.key().to_string()).or_insert_with(|| Sums { label: p.label.clone(), unit: p.unit.clone(), ..Default::default() });
            e.raw += p.raw * r.minutes;
            e.pct += p.percentile * r.minutes;
            e.weight += p.weight * r.minutes;
            e.minutes += r.minutes;
            e.logs += 1;
        }
    }
    let mut parts: Vec<PartAverage> = acc
        .into_iter()
        .map(|(k, a)| PartAverage { component: k, label: a.label, unit: a.unit, raw: a.raw / a.minutes, percentile: a.pct / a.minutes, weight: a.weight / a.minutes, logs: a.logs })
        .collect();
    parts.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    Ok(SeasonBreakdown {
        officials,
        sum_weighted,
        sum_weights,
        shrink: crate::catalogue::SHRINK_OFFICIALS,
        rating: line.rating(),
        opponent_slope: crate::catalogue::OPPONENT_SLOPE,
        parts,
    })
}
