//! `hl logsdump`: a whole logs.tf database, read from a PostgreSQL dump of
//! its `logs_raw (id, json, time)` table (Flashy: "make a complete index of
//! players and get the most full comprehensive ratings roundup").
//!
//! Two streaming passes over the dump, the JSON parsed on every core with the
//! app's own reader (`normalize`), format classifier and rating model:
//!
//! 1. **Duplicates.** The same game uploaded twice -- same players, map and
//!    score, as long within a minute, within three hours -- is one game; the
//!    first upload is kept. A "combined" log is left out too: logs.tf builds
//!    those from parts that are uploaded already.
//! 2. **Index and ratings**, over the logs kept.
//!
//! Every log is given a kind from its title: a TF2Center `lobby`, a `pug`
//! (tf2pickup, hlpugs.tf, ...), a league `official` (UGC, RGL, ozfortress,
//! ETF2L servers) or another team `match` -- scrims and officials on rented
//! servers. Highlander games are rated in two pools, each with its own 1.00:
//! `competitive` (official + match) and `pickup` (lobby + pug), so a lobby
//! stomp is measured against lobbies and not against league play (Flashy:
//! "do the dedupe and lobby/league split").
//!
//! Out comes a separate SQLite file -- never the app's database:
//!
//! - `log`: every log, its map, date, format, kind and score, and the log
//!   it duplicates if it is one;
//! - `player`, `player_name`: everyone in any log kept, every format, with
//!   the names they went by, logs per format, class time, Highlander record;
//! - `rating`: every Highlander performance rated by model v11 in its pool;
//!   `player_class` and `player_year`: those as careers, per pool.
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
use sqlx::{Connection, QueryBuilder, Row, Sqlite, SqliteConnection};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Rows handed to a worker at a time: enough to keep 32 threads busy
/// without holding much of the 119 GB in memory.
const BATCH: usize = 64;

/// Rows a single INSERT carries: far faster than one statement a row (the
/// first run spent 70 of its 81 minutes writing them one by one).
const INSERT_ROWS: usize = 500;

/// The last this many games make a player's form.
const FORM_GAMES: usize = 20;

/// Two uploads of one game: this close in time, and in length.
const DUP_WINDOW_S: i64 = 3 * 3600;
const DUP_LENGTH_S: i64 = 60;

/// What kind of game a log was, from its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Lobby,
    Pug,
    Official,
    Match,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Lobby => "lobby",
            Kind::Pug => "pug",
            Kind::Official => "official",
            Kind::Match => "match",
        }
    }

    /// The pool a Highlander game is rated in.
    fn pool(self) -> Pool {
        match self {
            Kind::Lobby | Kind::Pug => Pool::Pickup,
            Kind::Official | Kind::Match => Pool::Competitive,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Pool {
    Competitive,
    Pickup,
}

impl Pool {
    fn as_str(self) -> &'static str {
        match self {
            Pool::Competitive => "competitive",
            Pool::Pickup => "pickup",
        }
    }
}

/// The kind of game a title names. Read off 1.04 million Highlander titles:
/// 307k are "TF2Center Lobby #n"; the pug sites name themselves
/// ("hl.tf2pickup.eu #n", "hlpugs.tf eu #n"); league servers say whose they
/// are ("UGC Highlander Match", "RGL.gg Match Server", "ozfortress.com"); the
/// rest are rented servers -- serveme.tf, Qixalite, Matcha -- that teams book
/// for scrims and officials.
pub(crate) fn kind_of(title: Option<&str>) -> Kind {
    let t = title.unwrap_or("").to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if has(&["tf2center", "tf2lobby", "lobby #"]) {
        Kind::Lobby
    } else if has(&["tf2pickup", "hlpugs", "pug", "pickup", "mix #", " mix", "mixes", "tf2stadium"]) {
        Kind::Pug
    } else if has(&["ugc", "rgl.gg", "rgl ", "ozfortress", "etf2l", "official", "week ", "playoff", "grand final"]) {
        Kind::Official
    } else {
        Kind::Match
    }
}

/// One log's fingerprint, for finding the same game uploaded twice.
struct Print {
    id: i64,
    /// The players, without sides: one upload may have the colours swapped.
    players: u64,
    map: u64,
    at: i64,
    length: i64,
    /// The two scores, low first, for the same reason.
    score: (i64, i64),
    /// Everyone's kills and damage summed: an upload of the same server log
    /// carries the same game, so these match too, where a rematch on the
    /// same map with the same lineup and score does not.
    kills: i64,
    damage: i64,
    combined: bool,
}

fn hash_of<T: Hash>(x: &T) -> u64 {
    let mut h = DefaultHasher::new();
    x.hash(&mut h);
    h.finish()
}

/// One player in one log, for the index.
struct Appearance {
    account: u32,
    name: Option<String>,
    team: Team,
    class_secs: [u32; 9],
}

/// What a worker makes of one row in the second pass.
struct LogOut {
    id: i64,
    played_at: i64,
    map: Option<String>,
    title: Option<String>,
    format: Format,
    kind: Kind,
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
    /// Highlander logs by kind: lobby, pug, official, match.
    hl_kinds: [u32; 4],
}

fn fmt_ix(f: Format) -> usize {
    match f {
        Format::Highlander => 0,
        Format::Sixes => 1,
        Format::Prolander => 2,
        Format::Other => 3,
    }
}

fn kind_ix(k: Kind) -> usize {
    match k {
        Kind::Lobby => 0,
        Kind::Pug => 1,
        Kind::Official => 2,
        Kind::Match => 3,
    }
}

fn class_ix(c: TfClass) -> usize {
    TfClass::ALL.iter().position(|x| *x == c).unwrap_or(0)
}

/// A COPY text field back to its bytes: `\\` is a backslash, `\n` a newline
/// and so on. jsonb's own escapes arrive doubled and come out as JSON wrote
/// them.
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

/// One `id \t json \t time` row to its id and normalized log, or `None` for
/// one logs.tf could not parse (`"success": false`) or with no players.
pub(crate) fn parse_row(line: &[u8]) -> Option<(i64, NormalizedLog)> {
    let mut parts = line.splitn(3, |b| *b == b'\t');
    let id: i64 = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
    let json = unescape(parts.next()?);
    let raw: serde_json::Value = serde_json::from_slice(&json).ok()?;
    if raw.get("success").and_then(|s| s.as_bool()) == Some(false) {
        return None;
    }
    let log = hl_ingest::normalize::normalize(id, &raw).ok()?;
    (!log.players.is_empty()).then_some((id, log))
}

fn print_of(id: i64, log: &NormalizedLog) -> Print {
    let mut accounts: Vec<u32> = log.players.iter().map(|p| p.id.account_id()).collect();
    accounts.sort_unstable();
    accounts.dedup();
    let (a, b) = (log.red_score, log.blue_score);
    Print {
        id,
        players: hash_of(&accounts),
        map: hash_of(&log.map.as_deref().map(hl_core::maps::map_base)),
        at: log.played_at.unwrap_or(0),
        length: log.duration_s,
        score: (a.min(b), a.max(b)),
        kills: log.players.iter().map(|p| p.stats.kills).sum(),
        damage: log.players.iter().map(|p| p.stats.dmg).sum(),
        combined: log.title.as_deref().is_some_and(|t| t.to_lowercase().contains("combined")),
    }
}

fn read_row(line: &[u8], w: &Weights, skip: &HashSet<i64>) -> Option<LogOut> {
    let (id, log) = parse_row(line)?;
    if skip.contains(&id) {
        return None;
    }
    let format = hl_ingest::classify::classify(&log);
    let kind = kind_of(log.title.as_deref());
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
        kind,
        duration: log.duration_s,
        red: log.red_score,
        blue: log.blue_score,
        players,
        perfs,
    })
}

/// Read the dump's rows on every core: `work` turns a row into whatever the
/// pass needs, `gather` takes the results in batches as they come.
pub(crate) fn stream<T: Send + 'static>(
    dump: &Path,
    limit: Option<usize>,
    work: impl Fn(&[u8]) -> Option<T> + Send + Sync + 'static,
    mut gather: impl FnMut(Vec<T>),
) -> Result<(u64, u64)> {
    let threads = std::thread::available_parallelism().map_or(8, |n| n.get()).saturating_sub(2).max(2);
    let (job_tx, job_rx) = mpsc::sync_channel::<Vec<Vec<u8>>>(threads * 4);
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (out_tx, out_rx) = mpsc::sync_channel::<Vec<T>>(threads * 4);
    let work = Arc::new(work);
    let mut workers = Vec::new();
    for _ in 0..threads {
        let rx = Arc::clone(&job_rx);
        let tx = out_tx.clone();
        let work = Arc::clone(&work);
        workers.push(std::thread::spawn(move || loop {
            let batch = { rx.lock().unwrap().recv() };
            let Ok(batch) = batch else { break };
            let done: Vec<T> = batch.iter().filter_map(|l| work(l)).collect();
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
            let n = r.read_until(b'\n', &mut line)?;
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

    for batch in out_rx {
        gather(batch);
    }
    for t in workers {
        let _ = t.join();
    }
    reader.join().map_err(|_| anyhow::anyhow!("the reader stopped"))?
}

/// The duplicates among the prints: `(duplicate, the log it repeats)`. Within
/// one set of players on one map, sorted by time, a log that follows a kept
/// one within the window, as long and with the same score, is that game
/// again. Combined logs repeat their parts and point at none.
fn duplicates(mut prints: Vec<Print>) -> Vec<(i64, Option<i64>)> {
    let mut out: Vec<(i64, Option<i64>)> = prints.iter().filter(|p| p.combined).map(|p| (p.id, None)).collect();
    prints.retain(|p| !p.combined);
    prints.sort_unstable_by_key(|p| (p.players, p.map, p.at, p.id));
    let mut kept: Vec<&Print> = Vec::new();
    for p in &prints {
        // Kept logs of this group still inside the window.
        kept.retain(|k| k.players == p.players && k.map == p.map && p.at - k.at <= DUP_WINDOW_S);
        match kept.iter().find(|k| (k.length - p.length).abs() <= DUP_LENGTH_S && k.score == p.score && near(k.kills, p.kills) && near(k.damage, p.damage)) {
            Some(k) => out.push((p.id, Some(k.id))),
            None => kept.push(p),
        }
    }
    out
}

/// Within 2%: two uploads of one game may differ by a kill a parser counted
/// differently, never by a different game's worth.
fn near(a: i64, b: i64) -> bool {
    (a - b).abs() * 50 <= a.max(b).max(1)
}

async fn insert_rows<'a, T>(
    c: &mut SqliteConnection,
    head: &str,
    rows: impl IntoIterator<Item = T>,
    mut bind: impl FnMut(sqlx::query_builder::Separated<'_, 'a, Sqlite, &'static str>, T),
) -> Result<usize> {
    let mut n = 0;
    let mut chunk: Vec<T> = Vec::with_capacity(INSERT_ROWS);
    let mut iter = rows.into_iter().peekable();
    let mut tx = c.begin().await?;
    while iter.peek().is_some() {
        chunk.clear();
        chunk.extend(iter.by_ref().take(INSERT_ROWS));
        n += chunk.len();
        let mut qb: QueryBuilder<'a, Sqlite> = QueryBuilder::new(head);
        qb.push_values(chunk.drain(..), &mut bind);
        qb.build().execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(n)
}

pub async fn run(dump: &Path, out: &Path, limit: Option<usize>) -> Result<()> {
    if out.exists() {
        bail!("{} exists: pick a new file, this never overwrites one", out.display());
    }
    let w = Arc::new(Weights::default_weights());
    let started = Instant::now();
    let mins = || started.elapsed().as_secs_f64() / 60.0;
    let size = std::fs::metadata(dump).with_context(|| format!("reading {}", dump.display()))?.len();

    // ---- pass 1: the duplicates ----
    let mut prints: Vec<Print> = Vec::new();
    let mut last_note = Instant::now();
    stream(dump, limit, |line| parse_row(line).map(|(id, log)| print_of(id, &log)), |batch| {
        prints.extend(batch);
        if last_note.elapsed().as_secs() >= 60 {
            last_note = Instant::now();
            println!("  pass 1: {} logs fingerprinted ({:.0} min in)", prints.len(), mins());
        }
    })?;
    let seen = prints.len();
    let dups = duplicates(prints);
    let combined = dups.iter().filter(|(_, of)| of.is_none()).count();
    let twice = dups.len() - combined;
    println!("pass 1: {seen} logs, {} uploaded twice, {combined} combined ({:.1} min in)", dups.len() - combined, mins());
    let skip: Arc<HashSet<i64>> = Arc::new(dups.iter().map(|(id, _)| *id).collect());

    // ---- pass 2: read the logs kept, parse on every core, gather ----
    let mut logs: Vec<(i64, i64, Option<String>, Option<String>, u8, u8, i64, i64, i64, u16)> = Vec::new();
    let mut players: HashMap<u32, PlayerAgg> = HashMap::new();
    let mut perfs: Vec<(i64, i64, i8, Kind, Performance)> = Vec::new();
    let (wk, sk) = (Arc::clone(&w), Arc::clone(&skip));
    let mut last_note = Instant::now();
    let (rows, read) = stream(dump, limit, move |line| read_row(line, &wk, &sk), |batch| {
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
                    p.hl_kinds[kind_ix(l.kind)] += 1;
                }
            }
            for (perf, result) in l.perfs {
                perfs.push((l.id, l.played_at, result, l.kind, perf));
            }
            logs.push((l.id, l.played_at, l.map, l.title, fi as u8, kind_ix(l.kind) as u8, l.duration, l.red, l.blue, l.players.len() as u16));
        }
        if last_note.elapsed().as_secs() >= 60 {
            last_note = Instant::now();
            println!("  pass 2: {} logs read, {} players, {} Highlander performances ({:.0} min in)", logs.len(), players.len(), perfs.len(), mins());
        }
    })?;
    println!(
        "pass 2: {rows} rows ({:.1} of {:.1} GB), {} logs kept, {} players, {} Highlander performances ({:.1} min in)",
        read as f64 / 1e9,
        size as f64 / 1e9,
        logs.len(),
        players.len(),
        perfs.len(),
        mins()
    );

    // ---- write: the logs and players first, freed as they go ----
    let opts = SqliteConnectOptions::new()
        .filename(out)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Off)
        .synchronous(SqliteSynchronous::Off);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;
    let mut c = pool.acquire().await?;
    // A 256 MB page cache: the default 2 MB turns every index update into a
    // seek once the tables pass a few hundred MB.
    sqlx::query("PRAGMA cache_size = -262144").execute(&mut *c).await?;
    for sql in SCHEMA {
        sqlx::query(sql).execute(&mut *c).await?;
    }

    let n = insert_rows(
        &mut c,
        "INSERT OR REPLACE INTO log (log_id, played_at, map, title, format, kind, duration_s, red, blue, players) ",
        logs.drain(..),
        |mut b, (id, at, map, title, f, k, dur, red, blue, np)| {
            b.push_bind(id)
                .push_bind(at)
                .push_bind(map)
                .push_bind(title)
                .push_bind(["highlander", "sixes", "prolander", "other"][usize::from(f)])
                .push_bind(["lobby", "pug", "official", "match"][usize::from(k)])
                .push_bind(dur)
                .push_bind(red)
                .push_bind(blue)
                .push_bind(i64::from(np));
        },
    )
    .await?;
    println!("  wrote {n} logs ({:.1} min in)", mins());
    drop(logs);
    insert_rows(&mut c, "INSERT INTO duplicate (log_id, duplicate_of) ", dups, |mut b, (id, of)| {
        b.push_bind(id).push_bind(of);
    })
    .await?;

    let mut player_rows = Vec::with_capacity(players.len());
    let mut name_rows = Vec::new();
    for (account, p) in players.drain() {
        let name = p.names.iter().max_by_key(|(_, (n, last))| (*last, *n)).map(|(k, _)| k.to_string());
        let main = (0..9).max_by_key(|i| p.class_secs[*i]).filter(|i| p.class_secs[*i] > 0).map(|i| TfClass::ALL[i].as_str());
        let hl_main = (0..9).max_by_key(|i| p.hl_class_secs[*i]).filter(|i| p.hl_class_secs[*i] > 0).map(|i| TfClass::ALL[i].as_str());
        for (n, (count, last)) in p.names {
            name_rows.push((account, n.into_string(), count, last));
        }
        player_rows.push((account, name, main, hl_main, p.logs, p.first, p.last, p.hl_record, p.hl_kinds, p.class_secs, p.hl_class_secs));
    }
    drop(players);
    let n = insert_rows(
        &mut c,
        "INSERT INTO player (account_id, steamid64, name, main_class, hl_main_class, logs, hl_logs, sixes_logs, prolander_logs, other_logs, first_at, last_at,
           hl_won, hl_lost, hl_tied, hl_lobby, hl_pug, hl_official, hl_match, class_secs, hl_class_secs) ",
        player_rows,
        |mut b, (account, name, main, hl_main, logs, first, last, rec, kinds, cs, hcs)| {
            b.push_bind(i64::from(account))
                .push_bind((76561197960265728u64 + u64::from(account)).to_string())
                .push_bind(name)
                .push_bind(main)
                .push_bind(hl_main)
                .push_bind(i64::from(logs.iter().sum::<u32>()))
                .push_bind(i64::from(logs[0]))
                .push_bind(i64::from(logs[1]))
                .push_bind(i64::from(logs[2]))
                .push_bind(i64::from(logs[3]))
                .push_bind(first)
                .push_bind(last)
                .push_bind(i64::from(rec[0]))
                .push_bind(i64::from(rec[1]))
                .push_bind(i64::from(rec[2]))
                .push_bind(i64::from(kinds[0]))
                .push_bind(i64::from(kinds[1]))
                .push_bind(i64::from(kinds[2]))
                .push_bind(i64::from(kinds[3]))
                .push_bind(serde_json::to_string(&cs).unwrap_or_default())
                .push_bind(serde_json::to_string(&hcs).unwrap_or_default());
        },
    )
    .await?;
    insert_rows(&mut c, "INSERT INTO player_name (account_id, name, logs, last_at) ", name_rows, |mut b, (a, n, count, last)| {
        b.push_bind(i64::from(a)).push_bind(n).push_bind(i64::from(count)).push_bind(last);
    })
    .await?;
    println!("  wrote {n} players ({:.1} min in)", mins());

    // Rated only now, with the logs and players written and freed: the first
    // full run held them all through the rating and was swapped out to disk.
    // ---- the pools: each rated against itself, with its own 1.00 ----
    let mut scored: Vec<Option<f64>> = vec![None; perfs.len()];
    let mut scales: HashMap<Pool, (Scale, usize)> = HashMap::new();
    for pool in [Pool::Competitive, Pool::Pickup] {
        let idx: Vec<usize> = (0..perfs.len()).filter(|i| perfs[*i].3.pool() == pool).collect();
        let baseline = Baseline::build(idx.iter().map(|i| &perfs[*i].4), None);
        // The score alone: each rating's part-by-part breakdown, kept for
        // millions of performances, would not fit in memory.
        for i in &idx {
            scored[*i] = rate(&perfs[*i].4, &baseline, &w).map(|r| r.score);
        }
        drop(baseline);
        let raw: Vec<f64> = idx.iter().filter_map(|i| scored[*i]).collect();
        if let Some(s) = Scale::of(&raw) {
            println!("{} pool: {} performances; scale mean {:.2}, sd {:.2} ({:.1} min in)", pool.as_str(), raw.len(), s.mean, s.sd, mins());
            scales.insert(pool, (s, raw.len()));
        }
    }

    let mut meta: Vec<(String, String)> = vec![
        ("model".into(), MODEL_VERSION.into()),
        ("source".into(), dump.display().to_string()),
        ("note".into(), "logs.tf JSON only: the server-log components are absent and the others carry their weight".into()),
        ("duplicates".into(), twice.to_string()),
        ("combined_left_out".into(), combined.to_string()),
    ];
    for (p, (s, n)) in &scales {
        meta.push((format!("{}_scale_mean", p.as_str()), s.mean.to_string()));
        meta.push((format!("{}_scale_sd", p.as_str()), s.sd.to_string()));
        meta.push((format!("{}_performances", p.as_str()), n.to_string()));
    }
    insert_rows(&mut c, "INSERT INTO meta (key, value) ", meta, |mut b, (k, v)| {
        b.push_bind(k).push_bind(v);
    })
    .await?;

    // Ratings, and each player's careers by pool, class and year.
    let mut careers: HashMap<(u32, Pool, TfClass), Vec<(i64, f64, i8)>> = HashMap::new();
    let mut rating_rows = Vec::with_capacity(perfs.len());
    for ((log_id, at, result, kind, perf), score) in perfs.into_iter().zip(scored) {
        let pool = kind.pool();
        let (Some(score), Some((scale, _))) = (score, scales.get(&pool)) else { continue };
        let rating = scale.rating(score);
        careers.entry((perf.account_id, pool, perf.class)).or_default().push((at, rating, result));
        rating_rows.push((log_id, perf.account_id, perf.class, at, rating, round2(perf.minutes), result, kind, perf.map));
    }
    let n = insert_rows(
        &mut c,
        "INSERT OR REPLACE INTO rating (log_id, account_id, class, played_at, rating, minutes, result, kind, pool, map) ",
        rating_rows,
        |mut b, (log_id, account, class, at, rating, minutes, result, kind, map)| {
            b.push_bind(log_id)
                .push_bind(i64::from(account))
                .push_bind(class.as_str())
                .push_bind(at)
                .push_bind(rating)
                .push_bind(minutes)
                .push_bind(i64::from(result))
                .push_bind(kind.as_str())
                .push_bind(kind.pool().as_str())
                .push_bind(map);
        },
    )
    .await?;
    println!("  wrote {n} ratings ({:.1} min in)", mins());

    let mut class_rows = Vec::with_capacity(careers.len());
    let mut year_rows = Vec::new();
    for ((account, pool, class), mut games) in careers {
        games.sort_by_key(|g| g.0);
        let n = games.len();
        let avg = games.iter().map(|g| g.1).sum::<f64>() / n as f64;
        let form: Vec<f64> = games.iter().rev().take(FORM_GAMES).map(|g| g.1).collect();
        let form_avg = form.iter().sum::<f64>() / form.len() as f64;
        let best = games.iter().map(|g| g.1).fold(f64::MIN, f64::max);
        let won = games.iter().filter(|g| g.2 > 0).count();
        let lost = games.iter().filter(|g| g.2 < 0).count();
        class_rows.push((account, pool, class, n, round2(avg), round2(form_avg), round2(best), won, lost, games[0].0, games[n - 1].0));
        let mut years: HashMap<i32, (usize, f64)> = HashMap::new();
        for g in &games {
            let e = years.entry(year_of(g.0)).or_insert((0, 0.0));
            e.0 += 1;
            e.1 += g.1;
        }
        for (y, (n, sum)) in years {
            year_rows.push((account, pool, class, y, n, round2(sum / n as f64)));
        }
    }
    insert_rows(
        &mut c,
        "INSERT INTO player_class (account_id, pool, class, games, rating, form, best, won, lost, first_at, last_at) ",
        class_rows,
        |mut b, (account, pool, class, n, avg, form, best, won, lost, first, last)| {
            b.push_bind(i64::from(account))
                .push_bind(pool.as_str())
                .push_bind(class.as_str())
                .push_bind(n as i64)
                .push_bind(avg)
                .push_bind(form)
                .push_bind(best)
                .push_bind(won as i64)
                .push_bind(lost as i64)
                .push_bind(first)
                .push_bind(last);
        },
    )
    .await?;
    insert_rows(
        &mut c,
        "INSERT INTO player_year (account_id, pool, class, year, games, rating) ",
        year_rows,
        |mut b, (account, pool, class, y, n, avg)| {
            b.push_bind(i64::from(account)).push_bind(pool.as_str()).push_bind(class.as_str()).push_bind(y).push_bind(n as i64).push_bind(avg);
        },
    )
    .await?;
    for sql in INDEXES {
        sqlx::query(sql).execute(&mut *c).await?;
    }
    let players: i64 = sqlx::query("SELECT COUNT(*) AS n FROM player").fetch_one(&mut *c).await?.get("n");
    println!("done in {:.1} min: {} ({players} players)", mins(), out.display());
    Ok(())
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// The calendar year of a unix time (UTC).
pub fn year_of(unix: i64) -> i32 {
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
    "CREATE TABLE log (log_id INTEGER PRIMARY KEY, played_at INTEGER, map TEXT, title TEXT, format TEXT, kind TEXT, duration_s INTEGER, red INTEGER, blue INTEGER, players INTEGER)",
    "CREATE TABLE duplicate (log_id INTEGER PRIMARY KEY, duplicate_of INTEGER)",
    "CREATE TABLE player (account_id INTEGER PRIMARY KEY, steamid64 TEXT, name TEXT, main_class TEXT, hl_main_class TEXT,
       logs INTEGER, hl_logs INTEGER, sixes_logs INTEGER, prolander_logs INTEGER, other_logs INTEGER, first_at INTEGER, last_at INTEGER,
       hl_won INTEGER, hl_lost INTEGER, hl_tied INTEGER, hl_lobby INTEGER, hl_pug INTEGER, hl_official INTEGER, hl_match INTEGER,
       class_secs TEXT, hl_class_secs TEXT)",
    "CREATE TABLE player_name (account_id INTEGER, name TEXT, logs INTEGER, last_at INTEGER)",
    "CREATE TABLE rating (log_id INTEGER, account_id INTEGER, class TEXT, played_at INTEGER, rating REAL, minutes REAL, result INTEGER, kind TEXT, pool TEXT, map TEXT,
       PRIMARY KEY (log_id, account_id))",
    "CREATE TABLE player_class (account_id INTEGER, pool TEXT, class TEXT, games INTEGER, rating REAL, form REAL, best REAL, won INTEGER, lost INTEGER,
       first_at INTEGER, last_at INTEGER, PRIMARY KEY (account_id, pool, class))",
    "CREATE TABLE player_year (account_id INTEGER, pool TEXT, class TEXT, year INTEGER, games INTEGER, rating REAL)",
];

const INDEXES: &[&str] = &[
    "CREATE INDEX log_format ON log (format, kind, played_at)",
    "CREATE INDEX player_name_name ON player_name (name COLLATE NOCASE)",
    "CREATE INDEX player_name_account ON player_name (account_id)",
    "CREATE INDEX player_hl ON player (hl_logs)",
    "CREATE INDEX rating_account ON rating (account_id, played_at)",
    "CREATE INDEX player_class_rank ON player_class (pool, class, games, rating)",
    "CREATE INDEX player_year_account ON player_year (account_id, pool, class, year)",
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

    #[test]
    fn titles_name_the_kind_of_game() {
        assert_eq!(kind_of(Some("TF2Center Lobby #326678")), Kind::Lobby);
        assert_eq!(kind_of(Some("hl.tf2pickup.eu #4512")), Kind::Pug);
        assert_eq!(kind_of(Some("hlpugs.tf EU #12: RED vs BLU")), Kind::Pug);
        assert_eq!(kind_of(Some("UGC Highlander Match: RED vs BLU")), Kind::Official);
        assert_eq!(kind_of(Some("RGL.gg Match Server: BLU vs RED")), Kind::Official);
        assert_eq!(kind_of(Some("serveme.tf #1234 - DD14 vs TWICE")), Kind::Match);
        assert_eq!(kind_of(None), Kind::Match);
        assert_eq!(Kind::Lobby.pool(), Pool::Pickup);
        assert_eq!(Kind::Official.pool(), Pool::Competitive);
    }

    fn print(id: i64, players: u64, at: i64, length: i64, score: (i64, i64)) -> Print {
        Print { id, players, map: 1, at, length, score, kills: 300, damage: 90_000, combined: false }
    }

    #[test]
    fn the_same_game_uploaded_twice_is_kept_once() {
        let dups = duplicates(vec![
            print(10, 7, 1000, 1800, (2, 3)),
            // The same game again, from another uploader, 40 s shorter.
            print(11, 7, 1100, 1760, (2, 3)),
            // Same team the next night: a new game.
            print(12, 7, 1000 + 86_400, 1800, (2, 3)),
            // Same players, same evening, another score: the rematch.
            print(13, 7, 3000, 1800, (0, 5)),
            // The same lineup, map and score half an hour on, but another
            // game's worth of kills: the second half of a KOTH scrim.
            Print { kills: 260, damage: 81_000, ..print(16, 7, 2900, 1790, (2, 3)) },
            // Other players at the same time.
            print(14, 8, 1000, 1800, (2, 3)),
            Print { combined: true, ..print(15, 7, 5000, 3600, (4, 6)) },
        ]);
        assert_eq!(dups, vec![(15, None), (11, Some(10))]);
    }
}
