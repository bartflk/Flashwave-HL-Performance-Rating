//! `hl logsdump-scales`: the damage per minute and accuracy scales from the
//! whole logs.tf database (Flashy: "build the acc/dpm to compare to more than
//! just the games we have right now? like the 120 gb database").
//!
//! One pass over the dump, the Highlander logs of the last five years only
//! (Flashy chose five: damage has crept up, and a 2015 Sniper is another
//! game), each game uploaded twice counted once and combined logs left out,
//! as the index `hl logsdump` wrote decided. Every main-class line of 5
//! minutes or more is a game on its class, and on its map where the log was
//! one map; weighed as the app weighs its own -- an official 1, a scrim or
//! other match 0.5, a pug or lobby 0.25.
//!
//! Out goes `league/logstf-scales.json`: each class's values at the app's
//! percentiles, and each map's where a class has `MIN_MAP_GAMES` there, with
//! the games, officials and players behind each. The app ships it and uses
//! it in place of the scales from its own games (stat_scale.rs).

use anyhow::{bail, Context, Result};
use hl_ingest::stat_scale::{weighted_percentiles, ArchiveScales, ClassScale, MIN_MAP_GAMES, PERCENTILES};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::ConnectOptions;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::logsdump::{kind_of, parse_row, stream, Kind};

/// Years of games the scales are made from.
const YEARS: i64 = 5;
/// A main-class game counts from this long.
const MIN_SECS: i64 = 300;
/// Accuracy counts from this many shots, as the app's own.
const MIN_SHOTS: i64 = 30;

/// One game on a class.
struct Line {
    class: &'static str,
    map: Option<String>,
    dpm: f64,
    accuracy: Option<f64>,
    weight: f64,
    official: bool,
    account: u32,
}

#[derive(Default)]
struct Sample {
    values: Vec<(f64, f64)>,
    officials: usize,
    players: HashSet<u32>,
}

impl Sample {
    fn push(&mut self, x: f64, w: f64, official: bool, account: u32) {
        self.values.push((x, w));
        self.officials += usize::from(official);
        self.players.insert(account);
    }
}

fn weight(kind: Kind) -> f64 {
    match kind {
        Kind::Official => 1.0,
        Kind::Match => 0.5,
        Kind::Pug | Kind::Lobby => 0.25,
    }
}

/// The id at the start of a dump row, before its JSON is parsed: most rows
/// are skipped on it.
fn row_id(line: &[u8]) -> Option<i64> {
    let end = line.iter().position(|b| *b == b'\t')?;
    std::str::from_utf8(&line[..end]).ok()?.parse().ok()
}

fn read(line: &[u8], keep: &HashSet<i64>) -> Option<Vec<Line>> {
    if !keep.contains(&row_id(line)?) {
        return None;
    }
    let (_, log) = parse_row(line)?;
    let kind = kind_of(log.title.as_deref());
    // A log over several maps ("ashville + upward") counts on the class only.
    let map = log.map.as_deref().map(hl_core::maps::map_base).filter(|m| !m.is_empty() && !m.contains('+'));
    let out: Vec<Line> = log
        .players
        .iter()
        .filter_map(|p| {
            let main = p.main_class()?;
            let c = p.classes.iter().find(|c| c.class == main)?;
            if c.time_s < MIN_SECS {
                return None;
            }
            Some(Line {
                class: main.as_str(),
                map: map.clone(),
                dpm: c.dmg as f64 * 60.0 / c.time_s as f64,
                accuracy: (c.shots >= MIN_SHOTS && c.hits <= c.shots).then(|| c.hits as f64 / c.shots as f64),
                weight: weight(kind),
                official: kind == Kind::Official,
                account: p.id.account_id(),
            })
        })
        .collect();
    Some(out)
}

fn scale(dpm: Sample, acc: Option<Sample>) -> ClassScale {
    let acc = acc.unwrap_or_default();
    ClassScale {
        dpm_games: dpm.values.len(),
        dpm_officials: dpm.officials,
        dpm_players: dpm.players.len(),
        dpm: weighted_percentiles(dpm.values),
        accuracy_games: acc.values.len(),
        accuracy_officials: acc.officials,
        accuracy_players: acc.players.len(),
        accuracy: weighted_percentiles(acc.values),
        archive: true,
    }
}

pub async fn run(dump: &Path, index: &Path, out: &Path, limit: Option<usize>) -> Result<()> {
    if out.exists() && limit.is_some() {
        bail!("{} exists: a trial run never overwrites one", out.display());
    }
    let started = Instant::now();
    let mins = || started.elapsed().as_secs_f64() / 60.0;

    // The logs to read, from the index: Highlander, the last five years, no
    // second upload and no combined log.
    let mut c = SqliteConnectOptions::new().filename(index).read_only(true).connect().await.with_context(|| format!("opening {}", index.display()))?;
    let (newest,): (i64,) = sqlx::query_as("SELECT MAX(played_at) FROM log WHERE format = 'highlander'").fetch_one(&mut c).await?;
    let since = newest - YEARS * 365 * 86_400;
    let ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT log_id FROM log WHERE format = 'highlander' AND played_at >= ?1
           AND log_id NOT IN (SELECT log_id FROM duplicate)",
    )
    .bind(since)
    .fetch_all(&mut c)
    .await?;
    let keep: Arc<HashSet<i64>> = Arc::new(ids.into_iter().map(|(i,)| i).collect());
    println!("{} Highlander logs from {} to {} to read", keep.len(), crate::logsdump::year_of(since), crate::logsdump::year_of(newest));

    let mut dpm: HashMap<(&'static str, Option<String>), Sample> = HashMap::new();
    let mut acc: HashMap<(&'static str, Option<String>), Sample> = HashMap::new();
    let mut logs = 0usize;
    let mut last_note = Instant::now();
    let k = Arc::clone(&keep);
    let (rows, _) = stream(dump, limit, move |line| read(line, &k), |batch| {
        for lines in batch {
            logs += 1;
            for l in lines {
                let mut keys = vec![(l.class, None)];
                if let Some(m) = &l.map {
                    keys.push((l.class, Some(m.clone())));
                }
                for key in keys {
                    dpm.entry(key.clone()).or_default().push(l.dpm, l.weight, l.official, l.account);
                    if let Some(a) = l.accuracy {
                        acc.entry(key).or_default().push(a, l.weight, l.official, l.account);
                    }
                }
            }
        }
        if last_note.elapsed().as_secs() >= 60 {
            last_note = Instant::now();
            println!("  {logs} logs read ({:.0} min in)", mins());
        }
    })?;
    println!("{rows} rows in the dump, {logs} logs read ({:.1} min in)", mins());

    let mut classes: BTreeMap<String, ClassScale> = BTreeMap::new();
    let mut maps: BTreeMap<String, BTreeMap<String, ClassScale>> = BTreeMap::new();
    let games: usize = dpm.iter().filter(|((_, m), _)| m.is_none()).map(|(_, s)| s.values.len()).sum();
    for (key, d) in dpm {
        let a = acc.remove(&key);
        match key {
            (class, None) => {
                classes.insert(class.to_string(), scale(d, a));
            }
            (class, Some(map)) if d.values.len() >= MIN_MAP_GAMES => {
                maps.entry(class.to_string()).or_default().insert(map, scale(d, a));
            }
            _ => {}
        }
    }
    let file = ArchiveScales {
        source: "logs.tf".into(),
        from: since,
        to: newest,
        logs,
        games,
        percentiles: PERCENTILES.to_vec(),
        classes,
        maps,
    };
    std::fs::write(out, serde_json::to_string(&file)?).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "{}: {} classes, {} class maps, {} games ({:.1} min)",
        out.display(),
        file.classes.len(),
        file.maps.values().map(|m| m.len()).sum::<usize>(),
        games,
        mins()
    );
    Ok(())
}
