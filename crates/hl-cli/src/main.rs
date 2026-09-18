//! Developer harness.
//!
//! Reads and writes the same database the GUI uses, so you can inspect state,
//! reproduce bugs and (from M1 on) run syncs without launching a window.

use anyhow::{bail, Context, Result};
use hl_core::{tfpath, SteamId};
use hl_db::{Db, MatchFilter};
use hl_ingest::{Progress, Sources, SyncOptions};
use std::path::PathBuf;

const USAGE: &str = "\
hl — Highlander rating system dev harness

USAGE:
    hl <COMMAND>

COMMANDS:
    status                 Show config, database path and readiness
    set-steamid <ID>       Set the owner (any SteamID format, or a profile URL)
    tf detect              Scan the usual Steam locations for a TF2 install
    tf inspect <PATH>      Validate a candidate `tf` directory
    tf set <PATH>          Validate and store the `tf` directory

    sync [--full] [--max N]
                           Index trends.tf + logs.tf, fetch and normalize new logs
    reprocess              Rebuild every derived table from stored sources (no network)
    stats                  Index and fetch counts
    matches [N] [--all] [--officials]
                           List recent matches (Highlander only unless --all)

OPTIONS:
    --db <PATH>            Override the database location
";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,hl_cli=info".into()),
        )
        .init();

    let mut args: Vec<String> = std::env::args().skip(1).collect();

    let db_path = match args.iter().position(|a| a == "--db") {
        Some(i) if i + 1 < args.len() => {
            let p = PathBuf::from(args.remove(i + 1));
            args.remove(i);
            p
        }
        Some(_) => bail!("--db needs a path"),
        None => default_db_path()?,
    };

    let command: Vec<&str> = args.iter().map(String::as_str).collect();
    match command.as_slice() {
        [] | ["help"] | ["--help"] | ["-h"] => {
            print!("{USAGE}");
            Ok(())
        }

        ["status"] => {
            let db = Db::connect(&db_path).await?;
            let config = db.get_config().await?;
            println!("database : {}", db_path.display());
            println!(
                "steamid  : {}",
                config
                    .steamid
                    .map(|s| format!("{} ({})", s.to_steamid64(), s.to_steamid3()))
                    .unwrap_or_else(|| "not set".into())
            );
            println!("tf path  : {}", config.tf_path.as_deref().unwrap_or("not set"));
            println!("ready    : {}", config.is_ready());
            Ok(())
        }

        ["set-steamid", raw] => {
            let id = SteamId::parse(raw)?;
            let db = Db::connect(&db_path).await?;
            db.set_me(id).await?;
            println!("owner set to {} ({})", id.to_steamid64(), id.to_steamid3());
            Ok(())
        }

        ["tf", "detect"] => match tfpath::detect() {
            Some(info) => {
                print_tf(&info);
                Ok(())
            }
            None => {
                println!("No TF2 install found in the usual Steam locations.");
                println!("Use `hl tf set <PATH>` to point at it directly.");
                Ok(())
            }
        },

        ["tf", "inspect", path] => {
            print_tf(&tfpath::inspect(path)?);
            Ok(())
        }

        ["tf", "set", path] => {
            let info = tfpath::inspect(path)?;
            if !info.valid {
                print_tf(&info);
                bail!("`{}` does not look like a TF2 `tf` directory", info.path);
            }
            let db = Db::connect(&db_path).await?;
            db.set_setting(hl_core::config::keys::TF_PATH, &info.path)
                .await?;
            print_tf(&info);
            println!("stored.");
            Ok(())
        }

        ["sync", rest @ ..] => {
            let opts = SyncOptions {
                full: rest.contains(&"--full"),
                max_fetch: flag_value(rest, "--max")?,
            };
            let db = Db::connect(&db_path).await?;
            let me = db
                .get_me()
                .await?
                .context("no owner set: run `hl set-steamid <ID>` first")?;
            let sources = Sources::new()?;
            let summary = hl_ingest::sync(&db, &sources, me, &opts, print_progress).await?;
            println!();
            println!("fetched {} log(s), {} failed", summary.fetched, summary.failed);
            print_stats(&summary.stats);
            Ok(())
        }

        ["reprocess"] => {
            let db = Db::connect(&db_path).await?;
            let started = std::time::Instant::now();
            let stats = hl_ingest::reprocess(&db, print_progress).await?;
            println!();
            println!("rebuilt in {:.1}s", started.elapsed().as_secs_f64());
            print_stats(&stats);
            Ok(())
        }

        ["stats"] => {
            let db = Db::connect(&db_path).await?;
            print_stats(&db.index_stats().await?);
            Ok(())
        }

        ["matches", rest @ ..] => {
            let limit = rest.iter().find_map(|a| a.parse::<i64>().ok()).unwrap_or(20);
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?;
            let filter = MatchFilter {
                format: (!rest.contains(&"--all")).then(|| "highlander".to_string()),
                officials_only: rest.contains(&"--officials"),
                limit,
                offset: 0,
            };
            let page = db.list_matches(me.map(|m| m.account_id()), &filter).await?;
            println!("{} match(es) total, showing {}\n", page.total, page.items.len());
            println!(
                "{:<9} {:<10} {:<20} {:<6} {:<9} {:<1} {:>8} {:>5}  {}",
                "log", "date", "map", "league", "class", "", "K/D/A", "dmg", "title"
            );
            for m in &page.items {
                let (class, res, kda, dmg) = match &m.me {
                    Some(me) => (
                        me.main_class.clone().unwrap_or_default(),
                        me.result.clone(),
                        format!("{}/{}/{}", me.kills, me.deaths, me.assists),
                        me.dmg.to_string(),
                    ),
                    None => ("-".into(), String::new(), String::new(), String::new()),
                };
                println!(
                    "{:<9} {:<10} {:<20} {:<6} {:<9} {:<1} {:>8} {:>5}  {}",
                    m.log_id,
                    m.played_at.map(fmt_date).unwrap_or_default(),
                    truncate(m.map.as_deref().unwrap_or(""), 20),
                    m.league.as_deref().unwrap_or(""),
                    class,
                    res,
                    kda,
                    dmg,
                    truncate(m.title.as_deref().unwrap_or(""), 40),
                );
            }
            Ok(())
        }

        other => {
            eprintln!("unknown command: {}\n", other.join(" "));
            print!("{USAGE}");
            std::process::exit(2);
        }
    }
}

fn print_tf(info: &hl_core::TfPathInfo) {
    println!("path     : {}", info.path);
    println!("valid    : {}", info.valid);
    println!("cfg      : {}", info.cfg_dir.as_deref().unwrap_or("-"));
    println!("demos    : {} total", info.demo_count);
    for d in &info.demo_dirs {
        println!("           {:>5}  {}", d.demo_count, d.path);
    }
    for note in &info.notes {
        println!("  - {note}");
    }
}

/// Mirrors Tauri's `app_data_dir()` so the CLI and the GUI share one database.
fn default_db_path() -> Result<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var("APPDATA").context("APPDATA is not set")?
    } else {
        format!(
            "{}/.local/share",
            std::env::var("HOME").context("HOME is not set")?
        )
    };
    Ok(PathBuf::from(base)
        .join("gg.highlander.rating")
        .join("hl.sqlite3"))
}

fn flag_value<T: std::str::FromStr>(args: &[&str], flag: &str) -> Result<Option<T>> {
    match args.iter().position(|a| *a == flag) {
        Some(i) => match args.get(i + 1).and_then(|v| v.parse().ok()) {
            Some(v) => Ok(Some(v)),
            None => bail!("{flag} needs a value"),
        },
        None => Ok(None),
    }
}

/// Overwrites one terminal line so a thousand-log sync stays readable.
fn print_progress(p: Progress) {
    use std::io::Write;
    match p {
        Progress::Indexing { source, rows } => print!("\rindexing {source}: {rows} rows          "),
        Progress::Indexed { trends_rows, logstf_rows, superseded } => println!(
            "\rindexed {trends_rows} trends.tf + {logstf_rows} logs.tf rows; {superseded} superseded by combined logs"
        ),
        Progress::Fetching { done, total, log_id } => {
            print!("\rfetching {done}/{total}  (log {log_id})          ")
        }
        Progress::FetchFailed { log_id, error } => println!("\n  ! log {log_id}: {error}"),
        Progress::Reprocessing { done, total } => print!("\rreprocessing {done}/{total}          "),
    }
    let _ = std::io::stdout().flush();
}

fn print_stats(s: &hl_db::IndexStats) {
    println!("indexed       {:>6}", s.indexed);
    println!("  superseded  {:>6}  (per-round parts of a combined log)", s.superseded);
    println!("  highlander  {:>6}  ({} ETF2L official)", s.highlander, s.officials);
    println!("  sixes       {:>6}", s.sixes);
    println!("  other       {:>6}", s.other);
    println!("  unknown     {:>6}", s.unclassified);
    println!("fetched       {:>6}", s.fetched);
    println!("normalized    {:>6}", s.normalized);
    println!("pending       {:>6}", s.pending);
    println!("failed        {:>6}", s.failed);
}

fn fmt_date(unix: i64) -> String {
    // Civil-from-days, to avoid pulling in a date crate for one column.
    let days = unix.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}
