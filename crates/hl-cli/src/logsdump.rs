//! `hl logsdump`: a whole logs.tf database, read from a PostgreSQL dump of
//! its `logs_raw (id, json, time)` table (Flashy: "make a complete index of
//! players and get the most full comprehensive ratings roundup").
//!
//! One streaming pass over the dump, the JSON parsed on every core with the
//! app's own reader (`normalize`), format classifier and rating model. Out
//! comes a separate SQLite file -- never the app's database:
//!
//! - `log`: every log, its map, date, format and score;
//! - `player`, `player_name`: everyone in any log, every format, with the
//!   names they went by, their logs per format, class time and Highlander
//!   record;
//! - `rating`: every Highlander performance rated by model v11, all of them
//!   one pool; `player_class` and `player_year`: those as careers.
//!
//! The dump holds logs.tf's JSON and no server logs, so the components only
//! a server log gives (kills in context, opening duels, fight KAST ...) are
//! absent and the rest carry the weight -- as the app already rates any log
//! it has no server log for.

use anyhow::{bail, Context, Result};
use hl_core::matchdata::{Format, NormalizedLog, Team};
use hl_core::TfClass;
use hl_rating::model::{extract, rate, Scale};
use hl_rating::{Baseline, Performance, Weights, MODEL_VERSION};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Connection, Row};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Rows handed to a worker at a time: enough to keep 32 threads busy
/// without holding much of the 119 GB in memory.
const BATCH: usize = 64;

/// The last this many games make a player's form.
const FORM_GAMES: usize = 20;

/// One player in one log, for the index.
struct Appearance {
    account: u32,
    name: Option<String>,
    team: Team,
    class_secs: [u32; 9],
}

/// What a worker makes of one row.
struct LogOut {
    id: i64,
    played_at: i64,
    map: Option<String>,
    title: Option<String>,
    format: Format,
    duration: i64,
    red: i64,
    blue: i64,
    players: Vec<Appearance>,
    /// Highlander only: (performance, 1 won / 0 tied / -1 lost).
    perfs: Vec<(Performance, i8)>,
}

#[derive(Default)]
struct PlayerAgg {
    names: HashMap<Box<str>, (u32, i64)>,
    /// highlander, sixes, prolander, other
    logs: [u32; 4],
    first: i64,
    last: i64,
    class_secs: [u64; 9],
    hl_class_secs: [u64; 9],
    hl_record: [u32; 3],
}

fn fmt_ix(f: Format) -> usize {
    match f {
        Format::Highlander => 0,
        Format::Sixes => 1,
        Format::Prolander => 2,
        Format::Other => 3,
    }
}

fn class_ix(c: TfClass) -> usize {
    TfClass::ALL.iter().position(|x| *x == c).unwrap_or(0)
}

/// A COPY text field back to its bytes: `\\` is a backslash, `\n` a newline
/// and so on. jsonb's own escapes (`\"`, `é`) arrive doubled and come out
/// as JSON wrote them.
fn unescape(field: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(field.len());
    let mut i = 0;
    while i < field.len() {
        let b = field[i];
        if b == b'\\' && i + 1 < field.len() {
            match field[i + 1] {
                b'\\' => out.push(b'\\'),
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                b'b' => out.push(8),
                b'f' => out.push(12),
                b'v' => out.push(11),
                other => {
                    out.push(b'\\');
                    out.push(other);
                }
            }
            i += 2;
        } else {
            out.push(b);
            i += 1;
        }
    }
    out
}

/// One `id \t json \t time` row to a log, or `None` for one logs.tf could not
/// parse (`"success": false`) or that has no players.
fn read_row(line: &[u8], w: &Weights) -> Option<LogOut> {
    let mut parts = line.splitn(3, |b| *b == b'\t');
    let id: i64 = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
    let json = unescape(parts.next()?);
    let raw: serde_json::Value = serde_json::from_slice(&json).ok()?;
    if raw.get("success").and_then(|s| s.as_bool()) == Some(false) {
        return None;
    }
    let log: NormalizedLog = hl_ingest::normalize::normalize(id, &raw).ok()?;
    if log.players.is_empty() {
        return None;
    }
    let format = hl_ingest::classify::classify(&log);
    let result = |team: Team| {
        let (us, them) = if team == Team::Red { (log.red_score, log.blue_score) } else { (log.blue_score, log.red_score) };
        (us > them) as i8 - (us < them) as i8
    };
    let players = log
        .players
        .iter()
        .map(|p| {
            let mut class_secs = [0u32; 9];
            for c in &p.classes {
                class_secs[class_ix(c.class)] += c.time_s.max(0) as u32;
            }
            Appearance { account: p.id.account_id(), name: p.name.clone(), team: p.team, class_secs }
        })
        .collect();
    let perfs = if format == Format::Highlander {
        let pool_map = log.map.as_deref().map(hl_core::maps::map_base);
        log.players
            .iter()
            .filter_map(|p| Some((extract(p, &log.flags, w, None, pool_map.as_deref())?, result(p.team))))
            .collect()
    } else {
        Vec::new()
    };
    Some(LogOut {
        id,
        played_at: log.played_at.unwrap_or(0),
        map: log.map.clone(),
        title: log.title.clone(),
        format,
        duration: log.duration_s,
        red: log.red_score,
        blue: log.blue_score,
        players,
        perfs,
    })
}

pub async fn run(dump: &Path, out: &Path, limit: Option<usize>) -> Result<()> {
    if out.exists() {
        bail!("{} exists: pick a new file, this never overwrites one", out.display());
    }
    let w = Arc::new(Weights::default_weights());
    let started = Instant::now();
    let size = std::fs::metadata(dump).with_context(|| format!("reading {}", dump.display()))?.len();

    // ---- pass 1: read, parse on every core, gather ----
    let threads = std::thread::available_parallelism().map_or(8, |n| n.get()).saturating_sub(2).max(2);
    let (job_tx, job_rx) = mpsc::sync_channel::<Vec<Vec<u8>>>(threads * 4);
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (out_tx, out_rx) = mpsc::sync_channel::<Vec<LogOut>>(threads * 4);
    let mut workers = Vec::new();
    for _ in 0..threads {
        let rx = Arc::clone(&job_rx);
        let tx = out_tx.clone();
        let w = Arc::clone(&w);
        workers.push(std::thread::spawn(move || loop {
            let batch = { rx.lock().unwrap().recv() };
            let Ok(batch) = batch else { break };
            let done: Vec<LogOut> = batch.iter().filter_map(|l| read_row(l, &w)).collect();
            if tx.send(done).is_err() {
                break;
            }
        }));
    }
    drop(out_tx);

    let dump_path = dump.to_path_buf();
    let reader = std::thread::spawn(move || -> Result<(u64, u64)> {
        let file = std::fs::File::open(&dump_path)?;
        let mut r = BufReader::with_capacity(8 << 20, file);
        let mut line = Vec::with_capacity(1 << 20);
        let mut read: u64 = 0;
        // Up to the table's data.
        loop {
            line.clear();
            let n = r.read_until(b'\n', &mut line)?;
            if n == 0 {
                bail!("no `COPY public.logs_raw` in the dump");
            }
            read += n as u64;
            if line.starts_with(b"COPY public.logs_raw") {
                break;
            }
        }
        let mut rows: u64 = 0;
        let mut batch = Vec::with_capacity(BATCH);
        loop {
            line.clear();
            let n = r.by_ref().read_until(b'\n', &mut line)?;
            if n == 0 || line.starts_with(b"\\.") {
                break;
            }
            read += n as u64;
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            batch.push(std::mem::take(&mut line));
            rows += 1;
            if batch.len() == BATCH && job_tx.send(std::mem::replace(&mut batch, Vec::with_capacity(BATCH))).is_err() {
                break;
            }
            if limit.is_some_and(|l| rows as usize >= l) {
                break;
            }
        }
        if !batch.is_empty() {
            let _ = job_tx.send(batch);
        }
        Ok((rows, read))
    });

    let mut logs: Vec<(i64, i64, Option<String>, Option<String>, u8, i64, i64, i64, u16)> = Vec::new();
    let mut players: HashMap<u32, PlayerAgg> = HashMap::new();
    let mut perfs: Vec<(i64, i64, i8, Performance)> = Vec::new();
    let mut last_note = Instant::now();
    for batch in out_rx {
        for l in batch {
            let fi = fmt_ix(l.format);
            for a in &l.players {
                let p = players.entry(a.account).or_default();
                p.logs[fi] += 1;
                if l.played_at > 0 {
                    p.first = if p.first == 0 { l.played_at } else { p.first.min(l.played_at) };
                    p.last = p.last.max(l.played_at);
                }
                for (i, s) in a.class_secs.iter().enumerate() {
                    p.class_secs[i] += u64::from(*s);
                    if fi == 0 {
                        p.hl_class_secs[i] += u64::from(*s);
                    }
                }
                if let Some(n) = a.name.as_deref().filter(|n| !n.trim().is_empty()) {
                    let e = p.names.entry(n.into()).or_insert((0, 0));
                    e.0 += 1;
                    e.1 = e.1.max(l.played_at);
                }
                if fi == 0 {
                    let (us, them) = if a.team == Team::Red { (l.red, l.blue) } else { (l.blue, l.red) };
                    p.hl_record[if us > them { 0 } else if us < them { 1 } else { 2 }] += 1;
                }
            }
            for (perf, result) in l.perfs {
                perfs.push((l.id, l.played_at, result, perf));
            }
            logs.push((l.id, l.played_at, l.map, l.title, fi as u8, l.duration, l.red, l.blue, l.players.len() as u16));
        }
        if last_note.elapsed().as_secs() >= 30 {
            last_note = Instant::now();
            println!(
                "  {} logs read, {} players, {} Highlander performances ({:.0} min in)",
                logs.len(),
                players.len(),
                perfs.len(),
                started.elapsed().as_secs_f64() / 60.0
            );
        }
    }
    for t in workers {
        let _ = t.join();
    }
    let (rows, read) = reader.join().map_err(|_| anyhow::anyhow!("the reader stopped"))??;
    println!(
        "read {rows} rows ({:.1} of {:.1} GB), {} logs kept, {} players, {} Highlander performances in {:.1} min",
        read as f64 / 1e9,
        size as f64 / 1e9,
        logs.len(),
        players.len(),
        perfs.len(),
        started.elapsed().as_secs_f64() / 60.0
    );

    // ---- pass 2: one pool, every performance rated against it ----
    let baseline = Baseline::build(perfs.iter().map(|(_, _, _, p)| p), None);
    // The score alone: each rating's part-by-part breakdown, kept for ~17
    // million performances, would not fit in memory.
    let scored: Vec<Option<f64>> = perfs.iter().map(|(_, _, _, p)| rate(p, &baseline, &w).map(|r| r.score)).collect();
    drop(baseline);
    let raw: Vec<f64> = scored.iter().flatten().copied().collect();
    let scale = Scale::of(&raw).context("no Highlander performance to measure a scale on")?;
    println!("rated {} performances; scale mean {:.2}, sd {:.2} ({:.1} min in)", raw.len(), scale.mean, scale.sd, started.elapsed().as_secs_f64() / 60.0);

    // ---- write ----
    let opts = SqliteConnectOptions::new()
        .filename(out)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Off)
        .synchronous(SqliteSynchronous::Off);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
    let mut c = pool.acquire().await?;
    for sql in SCHEMA {
        sqlx::query(sql).execute(&mut *c).await?;
    }
    sqlx::query("INSERT INTO meta (key, value) VALUES ('model', ?1), ('scale_mean', ?2), ('scale_sd', ?3), ('source', ?4), ('built_at', strftime('%s','now')), ('note', ?5)")
        .bind(MODEL_VERSION)
        .bind(scale.mean.to_string())
        .bind(scale.sd.to_string())
        .bind(dump.display().to_string())
        .bind("logs.tf JSON only: the server-log components are absent and the others carry their weight")
        .execute(&mut *c)
        .await?;

    let mut tx = c.begin().await?;
    for (id, at, map, title, f, dur, red, blue, n) in &logs {
        sqlx::query("INSERT OR REPLACE INTO log (log_id, played_at, map, title, format, duration_s, red, blue, players) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")
            .bind(id)
            .bind(at)
            .bind(map)
            .bind(title)
            .bind(["highlander", "sixes", "prolander", "other"][usize::from(*f)])
            .bind(dur)
            .bind(red)
            .bind(blue)
            .bind(i64::from(*n))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    println!("  wrote {} logs", logs.len());
    drop(logs);

    let mut tx = c.begin().await?;
    for (account, p) in &players {
        let name = p.names.iter().max_by_key(|(_, (n, last))| (*last, *n)).map(|(k, _)| k.to_string());
        let main = (0..9).max_by_key(|i| p.class_secs[*i]).filter(|i| p.class_secs[*i] > 0).map(|i| TfClass::ALL[i].as_str());
        let hl_main = (0..9).max_by_key(|i| p.hl_class_secs[*i]).filter(|i| p.hl_class_secs[*i] > 0).map(|i| TfClass::ALL[i].as_str());
        sqlx::query(
            "INSERT INTO player (account_id, steamid64, name, logs, hl_logs, sixes_logs, prolander_logs, other_logs, first_at, last_at, main_class, hl_main_class, hl_won, hl_lost, hl_tied, class_secs, hl_class_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        )
        .bind(i64::from(*account))
        .bind((76561197960265728u64 + u64::from(*account)).to_string())
        .bind(name)
        .bind(i64::from(p.logs.iter().sum::<u32>()))
        .bind(i64::from(p.logs[0]))
        .bind(i64::from(p.logs[1]))
        .bind(i64::from(p.logs[2]))
        .bind(i64::from(p.logs[3]))
        .bind(p.first)
        .bind(p.last)
        .bind(main)
        .bind(hl_main)
        .bind(i64::from(p.hl_record[0]))
        .bind(i64::from(p.hl_record[1]))
        .bind(i64::from(p.hl_record[2]))
        .bind(serde_json::to_string(&p.class_secs)?)
        .bind(serde_json::to_string(&p.hl_class_secs)?)
        .execute(&mut *tx)
        .await?;
        for (n, (count, last)) in &p.names {
            sqlx::query("INSERT INTO player_name (account_id, name, logs, last_at) VALUES (?1, ?2, ?3, ?4)")
                .bind(i64::from(*account))
                .bind(&**n)
                .bind(i64::from(*count))
                .bind(last)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    println!("  wrote {} players", players.len());
    drop(players);

    // Ratings, and each player's careers by class and by year.
    let mut careers: HashMap<(u32, TfClass), Vec<(i64, f64, i8)>> = HashMap::new();
    let mut tx = c.begin().await?;
    for ((log_id, at, result, perf), r) in perfs.iter().zip(scored) {
        let Some(score) = r else { continue };
        let rating = scale.rating(score);
        sqlx::query("INSERT OR REPLACE INTO rating (log_id, account_id, class, played_at, rating, minutes, result, map) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")
            .bind(log_id)
            .bind(i64::from(perf.account_id))
            .bind(perf.class.as_str())
            .bind(at)
            .bind(rating)
            .bind(round2(perf.minutes))
            .bind(i64::from(*result))
            .bind(perf.map.as_deref())
            .execute(&mut *tx)
            .await?;
        careers.entry((perf.account_id, perf.class)).or_default().push((*at, rating, *result));
    }
    tx.commit().await?;
    println!("  wrote {} ratings", raw.len());

    let mut tx = c.begin().await?;
    for ((account, class), mut games) in careers {
        games.sort_by_key(|g| g.0);
        let n = games.len();
        let avg = games.iter().map(|g| g.1).sum::<f64>() / n as f64;
        let form: Vec<f64> = games.iter().rev().take(FORM_GAMES).map(|g| g.1).collect();
        let form_avg = form.iter().sum::<f64>() / form.len() as f64;
        let best = games.iter().map(|g| g.1).fold(f64::MIN, f64::max);
        let won = games.iter().filter(|g| g.2 > 0).count();
        let lost = games.iter().filter(|g| g.2 < 0).count();
        sqlx::query(
            "INSERT INTO player_class (account_id, class, games, rating, form, best, won, lost, first_at, last_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(i64::from(account))
        .bind(class.as_str())
        .bind(n as i64)
        .bind(round2(avg))
        .bind(round2(form_avg))
        .bind(round2(best))
        .bind(won as i64)
        .bind(lost as i64)
        .bind(games[0].0)
        .bind(games[n - 1].0)
        .execute(&mut *tx)
        .await?;
        let mut years: HashMap<i32, (usize, f64)> = HashMap::new();
        for g in &games {
            let y = year_of(g.0);
            let e = years.entry(y).or_insert((0, 0.0));
            e.0 += 1;
            e.1 += g.1;
        }
        for (y, (n, sum)) in years {
            sqlx::query("INSERT INTO player_year (account_id, class, year, games, rating) VALUES (?1, ?2, ?3, ?4, ?5)")
                .bind(i64::from(account))
                .bind(class.as_str())
                .bind(y)
                .bind(n as i64)
                .bind(round2(sum / n as f64))
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    for sql in INDEXES {
        sqlx::query(sql).execute(&mut *c).await?;
    }
    let players: i64 = sqlx::query("SELECT COUNT(*) AS n FROM player").fetch_one(&mut *c).await?.get("n");
    println!("done in {:.1} min: {} ({players} players)", started.elapsed().as_secs_f64() / 60.0, out.display());
    Ok(())
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// The calendar year of a unix time (UTC).
fn year_of(unix: i64) -> i32 {
    // Days to civil date (Howard Hinnant's algorithm), the year part only.
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2)) as i32
}

const SCHEMA: &[&str] = &[
    "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT)",
    "CREATE TABLE log (log_id INTEGER PRIMARY KEY, played_at INTEGER, map TEXT, title TEXT, format TEXT, duration_s INTEGER, red INTEGER, blue INTEGER, players INTEGER)",
    "CREATE TABLE player (account_id INTEGER PRIMARY KEY, steamid64 TEXT, name TEXT, logs INTEGER, hl_logs INTEGER, sixes_logs INTEGER, prolander_logs INTEGER, other_logs INTEGER,
       first_at INTEGER, last_at INTEGER, main_class TEXT, hl_main_class TEXT, hl_won INTEGER, hl_lost INTEGER, hl_tied INTEGER, class_secs TEXT, hl_class_secs TEXT)",
    "CREATE TABLE player_name (account_id INTEGER, name TEXT, logs INTEGER, last_at INTEGER)",
    "CREATE TABLE rating (log_id INTEGER, account_id INTEGER, class TEXT, played_at INTEGER, rating REAL, minutes REAL, result INTEGER, map TEXT, PRIMARY KEY (log_id, account_id))",
    "CREATE TABLE player_class (account_id INTEGER, class TEXT, games INTEGER, rating REAL, form REAL, best REAL, won INTEGER, lost INTEGER, first_at INTEGER, last_at INTEGER, PRIMARY KEY (account_id, class))",
    "CREATE TABLE player_year (account_id INTEGER, class TEXT, year INTEGER, games INTEGER, rating REAL)",
];

const INDEXES: &[&str] = &[
    "CREATE INDEX log_format ON log (format, played_at)",
    "CREATE INDEX player_name_name ON player_name (name COLLATE NOCASE)",
    "CREATE INDEX player_name_account ON player_name (account_id)",
    "CREATE INDEX player_hl ON player (hl_logs)",
    "CREATE INDEX rating_account ON rating (account_id, played_at)",
    "CREATE INDEX player_class_rank ON player_class (class, games, rating)",
    "CREATE INDEX player_year_account ON player_year (account_id, class, year)",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_text_unescapes_to_the_json_written() {
        // COPY doubles every backslash: jsonb's own escapes come back single.
        let field = [br#"{"a": "x"#.as_slice(), b"\\\\", br#""y"}"#].concat();
        assert_eq!(unescape(&field), [br#"{"a": "x"#.as_slice(), b"\\", br#""y"}"#].concat());
        // The escapes COPY writes for a tab, a newline and a backslash.
        assert_eq!(unescape(br"a\tb\nc\\d"), b"a\tb\nc\\d".to_vec());
    }

    #[test]
    fn years_from_unix_time() {
        assert_eq!(year_of(0), 1970);
        assert_eq!(year_of(1_423_261_223), 2015); // Fri 6 Feb 2015
        assert_eq!(year_of(1_704_067_199), 2023); // 31 Dec 2023 23:59:59
        assert_eq!(year_of(1_704_067_200), 2024);
    }
}
