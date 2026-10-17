//! `hl logsdump-summary`: the few megabytes of a logs.tf index the app ships
//! (Flashy: "if its only 3-6 mb, just bundle it in the app").
//!
//! From the index `hl logsdump` writes, keep what the app shows and nothing
//! more: every Highlander player's name and the other names they used most,
//! their careers by class in the competitive and the pickup pool, and a
//! rating a year for the regulars. Out goes one gzipped text file,
//! `league/logstf-summary.tsv.gz`, which `hl_ingest::logstf` reads. The
//! per-game ratings, the logs and the chat stay on the maintainer's disk.
//!
//! One record a line, fields by tab:
//!
//! - `M key value` -- what the file is (model, source, when, counts);
//! - `P account name aliases logs competitive pickup first last class` --
//!   aliases joined by U+001F, years as numbers;
//! - `C account pool class games rating form best won lost first last`;
//! - `Y account pool class year games rating`.
//!
//! Pools are `c` (competitive) and `p` (pickup), classes a digit in TF2's
//! order (0 Scout .. 8 Spy): long words on 300,000 lines are bytes too.

use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Row};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

/// A player is in the summary with this many Highlander logs: fewer in
/// thirteen years is someone nobody looks up (and 168k names were 5 MB).
const MIN_LOGS: i64 = 5;
/// Another name is kept once it was used this often, at most this many.
const ALIAS_MIN_LOGS: i64 = 3;
const MAX_ALIASES: usize = 3;
/// A career on a class is kept from this many games...
const MIN_CLASS_GAMES: i64 = 10;
/// ...and its years -- the competitive pool's only -- from this many, each
/// year from this many.
const MIN_YEARS_GAMES: i64 = 50;
const MIN_YEAR_GAMES: i64 = 5;

/// A name on one line of the file: no tabs, newlines or separators.
/// A pool and a class as the file writes them.
fn pool_code(pool: &str) -> &'static str {
    if pool == "pickup" {
        "p"
    } else {
        "c"
    }
}

fn class_code(class: &str) -> usize {
    hl_core::TfClass::parse(class).ok().and_then(|c| hl_core::TfClass::ALL.iter().position(|x| *x == c)).unwrap_or(9)
}

fn clean(s: &str) -> String {
    s.chars().map(|c| if c == '\t' || c == '\n' || c == '\r' || c == '\u{1f}' { ' ' } else { c }).collect::<String>().trim().to_string()
}

pub async fn run(index: &Path, out: &Path) -> Result<()> {
    let mut c = SqliteConnectOptions::new().filename(index).read_only(true).connect().await.with_context(|| format!("opening {}", index.display()))?;
    let mut text = String::new();

    let meta: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM meta").fetch_all(&mut c).await?;
    for (k, v) in &meta {
        if !k.ends_with("source") {
            text.push_str(&format!("M\t{k}\t{}\n", clean(v)));
        }
    }
    text.push_str(&format!("M\tbuilt_at\t{}\n", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs()));

    // Other names, most used first.
    let mut aliases: HashMap<i64, Vec<String>> = HashMap::new();
    for r in sqlx::query("SELECT account_id, name FROM player_name WHERE logs >= ?1 ORDER BY account_id, logs DESC")
        .bind(ALIAS_MIN_LOGS)
        .fetch_all(&mut c)
        .await?
    {
        aliases.entry(r.get("account_id")).or_default().push(clean(r.get::<&str, _>("name")));
    }

    let players = sqlx::query(
        "SELECT account_id, name, hl_logs, hl_official + hl_match AS comp, hl_lobby + hl_pug AS pickup, first_at, last_at, hl_main_class
           FROM player WHERE hl_logs >= ?1",
    )
    .bind(MIN_LOGS)
    .fetch_all(&mut c)
    .await?;
    let mut kept = std::collections::HashSet::new();
    for r in &players {
        let account: i64 = r.get("account_id");
        kept.insert(account);
        let name = clean(r.get::<Option<&str>, _>("name").unwrap_or(""));
        let other: Vec<String> = aliases.remove(&account).unwrap_or_default().into_iter().filter(|a| !a.is_empty() && *a != name).take(MAX_ALIASES).collect();
        text.push_str(&format!(
            "P\t{account}\t{name}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            other.join("\u{1f}"),
            r.get::<i64, _>("hl_logs"),
            r.get::<i64, _>("comp"),
            r.get::<i64, _>("pickup"),
            crate::logsdump::year_of(r.get("first_at")),
            crate::logsdump::year_of(r.get("last_at")),
            r.get::<Option<&str>, _>("hl_main_class").map(class_code).map_or(String::new(), |c| c.to_string()),
        ));
    }

    let mut careers = 0;
    for r in sqlx::query(
        "SELECT account_id, pool, class, games, rating, form, best, won, lost, first_at, last_at FROM player_class WHERE games >= ?1 ORDER BY account_id",
    )
    .bind(MIN_CLASS_GAMES)
    .fetch_all(&mut c)
    .await?
    {
        if !kept.contains(&r.get::<i64, _>("account_id")) {
            continue;
        }
        careers += 1;
        text.push_str(&format!(
            "C\t{}\t{}\t{}\t{}\t{:.2}\t{:.2}\t{:.2}\t{}\t{}\t{}\t{}\n",
            r.get::<i64, _>("account_id"),
            pool_code(r.get::<&str, _>("pool")),
            class_code(r.get::<&str, _>("class")),
            r.get::<i64, _>("games"),
            r.get::<f64, _>("rating"),
            r.get::<f64, _>("form"),
            r.get::<f64, _>("best"),
            r.get::<i64, _>("won"),
            r.get::<i64, _>("lost"),
            crate::logsdump::year_of(r.get("first_at")),
            crate::logsdump::year_of(r.get("last_at")),
        ));
    }

    let mut years = 0;
    let kept_years = &kept;
    for r in sqlx::query(
        "SELECT y.account_id, y.pool, y.class, y.year, y.games, y.rating FROM player_year y
           JOIN player_class c ON c.account_id = y.account_id AND c.pool = y.pool AND c.class = y.class
          WHERE c.games >= ?1 AND y.games >= ?2 AND y.pool = 'competitive' ORDER BY y.account_id, y.pool, y.class, y.year",
    )
    .bind(MIN_YEARS_GAMES)
    .bind(MIN_YEAR_GAMES)
    .fetch_all(&mut c)
    .await?
    {
        if !kept_years.contains(&r.get::<i64, _>("account_id")) {
            continue;
        }
        years += 1;
        text.push_str(&format!(
            "Y\t{}\t{}\t{}\t{}\t{}\t{:.2}\n",
            r.get::<i64, _>("account_id"),
            pool_code(r.get::<&str, _>("pool")),
            class_code(r.get::<&str, _>("class")),
            r.get::<i64, _>("year"),
            r.get::<i64, _>("games"),
            r.get::<f64, _>("rating"),
        ));
    }

    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    gz.write_all(text.as_bytes())?;
    let bytes = gz.finish()?;
    std::fs::write(out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "{}: {} players, {careers} careers, {years} years; {:.1} MB of text, {:.2} MB gzipped",
        out.display(),
        kept.len(),
        text.len() as f64 / 1e6,
        bytes.len() as f64 / 1e6
    );
    Ok(())
}
