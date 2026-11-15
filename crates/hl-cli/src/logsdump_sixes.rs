//! `hl logsdump-sixes`: every 6v6 game in the logs.tf dump, read the way the
//! 6v6 rating reads the owner's own (the 6v6 plan, P5; Flashy: the release
//! is "when the 6v6 matches have been parsed from the 120 gig sample").
//!
//! One pass over the dump. The logs read are the index's (`hl logsdump`):
//! 6v6 by its classifier, each game uploaded twice read once, officials,
//! scrims and other matches, and pugs; lobbies are left out. Each log gives
//! its winner and, per player, the role worked out from the Medic's heals,
//! minutes and every measurement the 6v6 weights use, from the logs.tf JSON
//! alone: the dump has no raw logs, so round impact and kills valued by
//! victim stay with the league sample.
//!
//! Out goes a file of its own, never the app's database: `log` (one row a
//! game) and `perf` (one a player and role, measurements as JSON), which
//! `hl sixes-validate --from` reads.

use anyhow::{bail, Context, Result};
use hl_ingest::sixes_rating::{stored_perfs, validate_stored, StoredPerf, Validation};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{ConnectOptions, Connection, QueryBuilder, Row, Sqlite, SqliteConnection};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::logsdump::{kind_of, stream, unescape};

/// The kinds of game read: lobbies are pickup games with strangers.
const KINDS: [&str; 3] = ["official", "match", "pug"];

/// Logs written to the file at a time.
const WRITE_LOGS: usize = 5_000;

const SCHEMA: &[&str] = &[
    "CREATE TABLE log (log_id INTEGER PRIMARY KEY, played_at INTEGER, map TEXT, kind TEXT NOT NULL, winner TEXT)",
    "CREATE TABLE perf (log_id INTEGER NOT NULL, account_id INTEGER NOT NULL, team TEXT, role TEXT NOT NULL, minutes REAL NOT NULL, vals TEXT NOT NULL)",
];
const INDEXES: &[&str] = &["CREATE INDEX perf_log ON perf (log_id)", "CREATE INDEX perf_role ON perf (role)", "CREATE INDEX log_kind ON log (kind)"];

/// One game as read.
struct Game {
    id: i64,
    played_at: Option<i64>,
    map: Option<String>,
    kind: &'static str,
    winner: Option<String>,
    perfs: Vec<StoredPerf>,
}

/// The id at the start of a dump row, before its JSON is parsed.
fn row_id(line: &[u8]) -> Option<i64> {
    let end = line.iter().position(|b| *b == b'\t')?;
    std::str::from_utf8(&line[..end]).ok()?.parse().ok()
}

fn read(line: &[u8], keep: &HashSet<i64>) -> Option<Game> {
    let id = row_id(line)?;
    if !keep.contains(&id) {
        return None;
    }
    let json = unescape(line.splitn(3, |b| *b == b'\t').nth(1)?);
    let v: Value = serde_json::from_slice(&json).ok()?;
    let info = v.get("info");
    let text = |k: &str| info.and_then(|i| i.get(k)).and_then(Value::as_str).map(str::to_string);
    let (winner, perfs) = stored_perfs(id, &v);
    Some(Game {
        id,
        played_at: info.and_then(|i| i.get("date")).and_then(Value::as_i64),
        map: text("map"),
        kind: kind_of(text("title").as_deref()).as_str(),
        winner,
        perfs,
    })
}

async fn write(c: &mut SqliteConnection, games: &mut Vec<Game>) -> Result<()> {
    if games.is_empty() {
        return Ok(());
    }
    let mut tx = c.begin().await?;
    for chunk in games.chunks(500) {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("INSERT OR REPLACE INTO log (log_id, played_at, map, kind, winner) ");
        qb.push_values(chunk, |mut b, g| {
            b.push_bind(g.id).push_bind(g.played_at).push_bind(g.map.clone()).push_bind(g.kind).push_bind(g.winner.clone());
        });
        qb.build().execute(&mut *tx).await?;
    }
    let rows: Vec<(i64, &StoredPerf)> = games.iter().flat_map(|g| g.perfs.iter().map(move |p| (g.id, p))).collect();
    for chunk in rows.chunks(500) {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("INSERT INTO perf (log_id, account_id, team, role, minutes, vals) ");
        qb.push_values(chunk, |mut b, (id, p)| {
            b.push_bind(*id)
                .push_bind(p.account_id)
                .push_bind(p.team.clone())
                .push_bind(p.role.clone())
                .push_bind(p.minutes)
                .push_bind(serde_json::to_string(&p.values).unwrap_or_else(|_| "{}".into()));
        });
        qb.build().execute(&mut *tx).await?;
    }
    tx.commit().await?;
    games.clear();
    Ok(())
}

pub async fn run(dump: &Path, index: &Path, out: &Path, limit: Option<usize>) -> Result<()> {
    if out.exists() {
        bail!("{} exists: pick a new file, this never overwrites one", out.display());
    }
    let started = Instant::now();
    let mins = || started.elapsed().as_secs_f64() / 60.0;

    let mut ix = SqliteConnectOptions::new().filename(index).read_only(true).connect().await.with_context(|| format!("opening {}", index.display()))?;
    let ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT log_id FROM log WHERE format = 'sixes' AND kind IN ('official', 'match', 'pug')
           AND log_id NOT IN (SELECT log_id FROM duplicate)",
    )
    .fetch_all(&mut ix)
    .await?;
    drop(ix);
    let keep: Arc<HashSet<i64>> = Arc::new(ids.into_iter().map(|(i,)| i).collect());
    println!("{} 6v6 games to read ({})", keep.len(), KINDS.join(", "));

    let mut c = SqliteConnectOptions::new()
        .filename(out)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Off)
        .synchronous(SqliteSynchronous::Off)
        .connect()
        .await?;
    sqlx::query("PRAGMA cache_size = -262144").execute(&mut c).await?;
    for sql in SCHEMA {
        sqlx::query(sql).execute(&mut c).await?;
    }

    // Written as it goes, from the gathering thread: 1.5 million games'
    // measurements would not fit in memory at once.
    let handle = tokio::runtime::Handle::current();
    let (mut games, mut perfs, mut buf) = (0usize, 0usize, Vec::with_capacity(WRITE_LOGS));
    let mut failed: Option<anyhow::Error> = None;
    let mut last_note = Instant::now();
    let k = Arc::clone(&keep);
    let (rows, _) = tokio::task::block_in_place(|| {
        stream(dump, limit, move |line| read(line, &k), |batch| {
            for g in batch {
                games += 1;
                perfs += g.perfs.len();
                buf.push(g);
            }
            if buf.len() >= WRITE_LOGS && failed.is_none() {
                if let Err(e) = handle.block_on(write(&mut c, &mut buf)) {
                    failed = Some(e);
                }
            }
            if last_note.elapsed().as_secs() >= 60 {
                last_note = Instant::now();
                println!("  {games} of {} games read, {perfs} performances ({:.0} min in)", keep.len(), mins());
            }
        })
    })?;
    if let Some(e) = failed {
        return Err(e.context("writing the file"));
    }
    write(&mut c, &mut buf).await?;
    for sql in INDEXES {
        sqlx::query(sql).execute(&mut c).await?;
    }
    let by_kind: Vec<(String, i64)> = sqlx::query_as("SELECT kind, COUNT(*) FROM log GROUP BY kind ORDER BY 2 DESC").fetch_all(&mut c).await?;
    println!(
        "{}: {rows} rows in the dump, {games} 6v6 games ({}), {perfs} performances ({:.1} min)",
        out.display(),
        by_kind.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", "),
        mins()
    );
    Ok(())
}

/// `hl logsdump-sixes-pool`: the built-in 6v6 league pool from the games
/// `run` wrote, officials only, as JSON for `hl-ingest/src/sixes_pool.json`.
pub async fn pool(file: &Path, out: &Path) -> Result<()> {
    let mut c = SqliteConnectOptions::new().filename(file).read_only(true).connect().await.with_context(|| format!("opening {}", file.display()))?;
    let (logs,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM log WHERE kind = 'official'").fetch_one(&mut c).await?;
    let rows = sqlx::query("SELECT p.account_id, p.role, p.minutes, p.vals FROM perf p JOIN log l ON l.log_id = p.log_id WHERE l.kind = 'official'").fetch_all(&mut c).await?;
    let perfs: Vec<StoredPerf> = rows
        .into_iter()
        .map(|r| {
            let vals: String = r.get(3);
            StoredPerf { account_id: r.get::<i64, _>(0) as u32, team: None, role: r.get(1), minutes: r.get(2), values: serde_json::from_str(&vals).unwrap_or_default() }
        })
        .collect();
    println!("{} performances in {logs} officials", perfs.len());
    let pool = hl_ingest::sixes_rating::pool_from_stored(perfs, "logs.tf database, every 6v6 official", logs as usize);
    for (role, r) in &pool.roles {
        println!("  {role:<14} {:>7} games, score {:.1} ± {:.1}, {} components", r.games, r.score_mean, r.score_sd, r.components.len());
    }
    std::fs::write(out, serde_json::to_string(&pool)?).with_context(|| format!("writing {}", out.display()))?;
    println!("{}", out.display());
    Ok(())
}

/// `hl sixes-validate --from`: the 6v6 rating checked against who won on
/// the games `run` wrote, of the kinds named (comma-separated).
pub async fn validate(file: &Path, kinds: &str, limit: Option<usize>) -> Result<Validation> {
    let kinds: Vec<&str> = kinds.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
    if let Some(k) = kinds.iter().find(|k| !KINDS.contains(k)) {
        bail!("--kinds: {k} is not one of {}", KINDS.join(", "));
    }
    let mut c = SqliteConnectOptions::new().filename(file).read_only(true).connect().await.with_context(|| format!("opening {}", file.display()))?;
    let list = kinds.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(", ");
    let logs: Vec<(i64, Option<String>)> =
        sqlx::query_as(&format!("SELECT log_id, winner FROM log WHERE kind IN ({list}) ORDER BY played_at DESC LIMIT ?1")).bind(limit.map_or(-1, |l| l as i64)).fetch_all(&mut c).await?;
    let wanted: HashMap<i64, Option<String>> = logs.into_iter().collect();
    let rows = sqlx::query(&format!(
        "SELECT p.log_id, p.account_id, p.team, p.role, p.minutes, p.vals FROM perf p JOIN log l ON l.log_id = p.log_id WHERE l.kind IN ({list})"
    ))
    .fetch_all(&mut c)
    .await?;
    let mut by_log: HashMap<i64, Vec<StoredPerf>> = HashMap::new();
    for r in rows {
        let log: i64 = r.get(0);
        if !wanted.contains_key(&log) {
            continue;
        }
        let vals: String = r.get(5);
        by_log.entry(log).or_default().push(StoredPerf {
            account_id: r.get::<i64, _>(1) as u32,
            team: r.get(2),
            role: r.get(3),
            minutes: r.get(4),
            values: serde_json::from_str(&vals).unwrap_or_default(),
        });
    }
    println!("{} games of {} read from {}", by_log.len(), kinds.join(", "), file.display());
    Ok(validate_stored(by_log.into_iter().map(|(id, ps)| (id, wanted.get(&id).cloned().flatten(), ps))))
}
