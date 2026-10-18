//! The logs.tf summary shipped in the app (Flashy: "build the bundled logs.tf
//! summary"): every Highlander player in thirteen years of logs.tf -- the
//! names they went by, their careers by class in the competitive pool
//! (officials and other team games) and the pickup pool (TF2Center lobbies
//! and pugs), and a rating a year for the regulars.
//!
//! Made by the maintainer from a whole logs.tf database (`hl logsdump`, then
//! `hl logsdump-summary`); about 6 MB built in. Read once, on first use, into
//! memory -- nothing of it goes into the database, and it is never sent
//! anywhere. Rated on logs.tf's JSON alone: the components a server log
//! gives are absent and the others carry their weight, so these numbers sit
//! a little apart from the app's own ratings of the games it holds.

use flate2::read::GzDecoder;
use hl_core::{SteamId, TfClass};
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

/// The summary this build ships (`hl logsdump-summary` writes it).
const BUILT_IN: &[u8] = include_bytes!("../../../league/logstf-summary.tsv.gz");

/// A class's regulars, whom ranks count: this many competitive games on it.
pub const RANK_MIN_GAMES: u32 = 100;

/// One player, as the search lists them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsTfPlayer {
    pub account_id: u32,
    pub steamid64: String,
    pub name: String,
    /// The other names they used most.
    pub aliases: Vec<String>,
    /// Highlander logs, and of them the competitive and the pickup ones.
    pub logs: u32,
    pub competitive: u32,
    pub pickup: u32,
    pub first_year: i32,
    pub last_year: i32,
    pub main_class: Option<&'static str>,
}

/// A career on one class in one pool.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassCareer {
    /// "competitive" or "pickup".
    pub pool: &'static str,
    pub class: &'static str,
    pub games: u32,
    pub rating: f64,
    /// The last 20 games.
    pub form: f64,
    pub best: f64,
    pub won: u32,
    pub lost: u32,
    pub first_year: i32,
    pub last_year: i32,
    /// Among the class's regulars in the pool; `None` below [`RANK_MIN_GAMES`].
    pub rank: Option<u32>,
    pub of: Option<u32>,
}

/// A competitive year on one class.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YearPoint {
    pub class: &'static str,
    pub year: i32,
    pub games: u32,
    pub rating: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsTfCareer {
    pub player: LogsTfPlayer,
    /// Most played first.
    pub classes: Vec<ClassCareer>,
    pub years: Vec<YearPoint>,
    /// When the summary was made (unix seconds), and on which model.
    pub built_at: i64,
    pub model: String,
}

struct Index {
    players: Vec<LogsTfPlayer>,
    by_account: HashMap<u32, usize>,
    /// Lower-cased name and aliases, per player, for the search.
    keys: Vec<Vec<String>>,
    careers: HashMap<u32, Vec<ClassCareer>>,
    years: HashMap<u32, Vec<YearPoint>>,
    meta: HashMap<String, String>,
}

fn class_of(code: &str) -> Option<&'static str> {
    code.parse::<usize>().ok().and_then(|i| TfClass::ALL.get(i)).map(|c| c.as_str())
}

fn parse(gz: &[u8]) -> Index {
    let mut text = String::new();
    // A build without a usable summary has an empty one, not a crash.
    if GzDecoder::new(gz).read_to_string(&mut text).is_err() {
        text.clear();
    }
    let mut ix = Index { players: Vec::new(), by_account: HashMap::new(), keys: Vec::new(), careers: HashMap::new(), years: HashMap::new(), meta: HashMap::new() };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let n = |i: usize| f.get(i).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
        let y = |i: usize| f.get(i).and_then(|x| x.parse::<i32>().ok()).unwrap_or(0);
        let r = |i: usize| f.get(i).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
        match f.first().copied() {
            Some("M") if f.len() >= 3 => {
                ix.meta.insert(f[1].to_string(), f[2].to_string());
            }
            Some("P") if f.len() >= 10 => {
                let account = n(1);
                let aliases: Vec<String> = f[3].split('\u{1f}').filter(|a| !a.is_empty()).map(str::to_string).collect();
                let mut keys = vec![f[2].to_lowercase()];
                keys.extend(aliases.iter().map(|a| a.to_lowercase()));
                ix.by_account.insert(account, ix.players.len());
                ix.keys.push(keys);
                ix.players.push(LogsTfPlayer {
                    account_id: account,
                    steamid64: (76561197960265728u64 + u64::from(account)).to_string(),
                    name: f[2].to_string(),
                    aliases,
                    logs: n(4),
                    competitive: n(5),
                    pickup: n(6),
                    first_year: y(7),
                    last_year: y(8),
                    main_class: class_of(f[9]),
                });
            }
            Some("C") if f.len() >= 12 => {
                let Some(class) = class_of(f[3]) else { continue };
                ix.careers.entry(n(1)).or_default().push(ClassCareer {
                    pool: if f[2] == "p" { "pickup" } else { "competitive" },
                    class,
                    games: n(4),
                    rating: r(5),
                    form: r(6),
                    best: r(7),
                    won: n(8),
                    lost: n(9),
                    first_year: y(10),
                    last_year: y(11),
                    rank: None,
                    of: None,
                });
            }
            Some("Y") if f.len() >= 7 => {
                let Some(class) = class_of(f[3]) else { continue };
                ix.years.entry(n(1)).or_default().push(YearPoint { class, year: y(4), games: n(5), rating: r(6) });
            }
            _ => {}
        }
    }
    // Ranks among each class's regulars, per pool.
    let mut pools: HashMap<(&'static str, &'static str), Vec<f64>> = HashMap::new();
    for c in ix.careers.values().flatten().filter(|c| c.games >= RANK_MIN_GAMES) {
        pools.entry((c.pool, c.class)).or_default().push(c.rating);
    }
    for v in pools.values_mut() {
        v.sort_by(|a, b| b.total_cmp(a));
    }
    for c in ix.careers.values_mut().flatten().filter(|c| c.games >= RANK_MIN_GAMES) {
        if let Some(v) = pools.get(&(c.pool, c.class)) {
            c.rank = Some(v.partition_point(|x| *x > c.rating) as u32 + 1);
            c.of = Some(v.len() as u32);
        }
    }
    for v in ix.careers.values_mut() {
        v.sort_by(|a, b| (a.pool != "competitive").cmp(&(b.pool != "competitive")).then(b.games.cmp(&a.games)));
    }
    ix
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| parse(BUILT_IN))
}

/// Players by name, any of their names, or Steam ID: exact names first, then
/// names that start so, then the rest -- each by how much they played.
pub fn search(query: &str, limit: usize) -> Vec<LogsTfPlayer> {
    let ix = index();
    let q = query.trim().to_lowercase();
    if q.chars().count() < 2 {
        return Vec::new();
    }
    if let Ok(id) = SteamId::parse(query.trim()) {
        return ix.by_account.get(&id.account_id()).map(|i| vec![ix.players[*i].clone()]).unwrap_or_default();
    }
    let mut hits: Vec<(u8, u32, usize)> = Vec::new();
    for (i, keys) in ix.keys.iter().enumerate() {
        let rank = keys
            .iter()
            .enumerate()
            .filter_map(|(k, name)| {
                let main = k == 0;
                if *name == q {
                    Some(if main { 0 } else { 1 })
                } else if name.starts_with(&q) {
                    Some(if main { 2 } else { 3 })
                } else if name.contains(&q) {
                    Some(4)
                } else {
                    None
                }
            })
            .min();
        if let Some(rank) = rank {
            hits.push((rank, ix.players[i].logs, i));
        }
    }
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    hits.into_iter().take(limit).map(|(_, _, i)| ix.players[i].clone()).collect()
}

/// One player's careers, or `None` for someone not in the summary.
pub fn career(account: u32) -> Option<LogsTfCareer> {
    let ix = index();
    let player = ix.players.get(*ix.by_account.get(&account)?)?.clone();
    Some(LogsTfCareer {
        player,
        classes: ix.careers.get(&account).cloned().unwrap_or_default(),
        years: ix.years.get(&account).cloned().unwrap_or_default(),
        built_at: ix.meta.get("built_at").and_then(|x| x.parse().ok()).unwrap_or(0),
        model: ix.meta.get("model").cloned().unwrap_or_default(),
    })
}

/// The name and other names logs.tf knows someone by: for a profile the
/// app has no name for.
pub fn names(account: u32) -> Option<(String, Vec<String>)> {
    let ix = index();
    let p = ix.players.get(*ix.by_account.get(&account)?)?;
    Some((p.name.clone(), p.aliases.clone()))
}

/// A career counts towards a lineup's strength from this many competitive
/// games: fewer is a few nights, not a level.
pub const STRENGTH_MIN_GAMES: u32 = 20;

/// One player of a lineup, as the summary knows them (Q78).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupRating {
    pub account_id: u32,
    /// The class the rating is on: the one asked for, else their most played.
    pub class: Option<&'static str>,
    pub rating: Option<f64>,
    pub games: u32,
    /// The rating is on the class they played here, not their usual one.
    pub on_class: bool,
}

/// Each player's competitive career rating, for a lineup's strength (Q78,
/// Flashy: "build the opponent strength on match pages"): on the class they
/// played if they have 20+ competitive games on it, else on their most played
/// class, else unknown -- never a stand-in 1.00.
pub fn lineup(players: &[(u32, Option<String>)]) -> Vec<LineupRating> {
    let ix = index();
    players
        .iter()
        .map(|(account, class)| {
            let wanted = class.as_deref().and_then(|c| TfClass::parse(c).ok()).map(|c| c.as_str());
            let comp: Vec<&ClassCareer> = ix
                .careers
                .get(account)
                .map(|v| v.iter().filter(|c| c.pool == "competitive" && c.games >= STRENGTH_MIN_GAMES).collect())
                .unwrap_or_default();
            let on = wanted.and_then(|w| comp.iter().find(|c| c.class == w));
            let pick = on.or_else(|| comp.iter().max_by_key(|c| c.games));
            match pick {
                Some(c) => LineupRating { account_id: *account, class: Some(c.class), rating: Some(c.rating), games: c.games, on_class: on.is_some() },
                None => LineupRating { account_id: *account, class: wanted, rating: None, games: 0, on_class: false },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use std::io::Write;

    fn gz(text: &str) -> Vec<u8> {
        let mut e = GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(text.as_bytes()).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn a_summary_reads_with_ranks_among_the_regulars() {
        let text = "M\tmodel\tv11\n\
            P\t1\tflashy\tFlashy\u{1f}SBQR flashy\t1130\t1081\t49\t2014\t2026\t7\n\
            P\t2\tother\t\t500\t500\t0\t2015\t2020\t7\n\
            C\t1\tc\t7\t1016\t1.03\t0.95\t1.61\t507\t467\t2014\t2026\n\
            C\t1\tp\t7\t18\t1.08\t1.08\t1.37\t5\t10\t2014\t2026\n\
            C\t2\tc\t7\t300\t1.10\t1.0\t1.5\t150\t150\t2015\t2020\n\
            Y\t1\tc\t7\t2024\t80\t1.05\n";
        let ix = parse(&gz(text));
        assert_eq!(ix.players.len(), 2);
        let mine = &ix.careers[&1];
        assert_eq!((mine[0].pool, mine[0].class, mine[0].games), ("competitive", "sniper", 1016), "competitive first, most played");
        assert_eq!((mine[0].rank, mine[0].of), (Some(2), Some(2)), "behind the 1.10");
        assert_eq!(mine[1].rank, None, "18 pickup games is not a regular");
        assert_eq!(ix.players[0].aliases, vec!["Flashy", "SBQR flashy"]);
        assert_eq!(ix.years[&1][0].year, 2024);
    }

    #[test]
    fn a_lineup_reads_the_class_played_else_the_usual_one() {
        let ix = index();
        // Someone with a 20+ game competitive career and a class they never
        // played: asked on the one, then the other. (Not just anyone: the map
        // is unordered, and someone who played all nine has no "other".)
        let never = |v: &[ClassCareer]| TfClass::ALL.iter().map(|c| c.as_str()).find(|c| !v.iter().any(|x| x.class == *c));
        let Some((account, career, other)) = ix.careers.iter().find_map(|(a, v)| {
            let c = v.iter().find(|c| c.pool == "competitive" && c.games >= STRENGTH_MIN_GAMES)?;
            Some((*a, c.clone(), never(v)?))
        }) else {
            return;
        };
        let r = lineup(&[(account, Some(career.class.to_string())), (account, Some(other.to_string())), (u32::MAX, Some("sniper".into()))]);
        assert!(r[0].on_class && r[0].rating.is_some());
        assert!(!r[1].on_class && r[1].rating.is_some(), "their usual class instead");
        assert_eq!((r[2].rating, r[2].games), (None, 0), "unknown is unknown, not 1.00");
    }

    #[test]
    fn a_broken_summary_is_an_empty_one() {
        let ix = parse(b"not gzip");
        assert!(ix.players.is_empty());
    }

    #[test]
    fn the_built_in_summary_reads() {
        // The shipped file: tens of thousands of players, each findable.
        let ix = index();
        assert!(ix.players.len() > 10_000, "{} players", ix.players.len());
        let p = &ix.players[0];
        assert!(career(p.account_id).is_some());
        assert!(search(&p.name.chars().take(4).collect::<String>(), 50).iter().any(|h| h.account_id == p.account_id) || p.name.chars().count() < 2);
    }
}
