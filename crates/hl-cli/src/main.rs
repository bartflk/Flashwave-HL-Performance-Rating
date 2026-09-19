//! Developer harness.
//!
//! Reads and writes the same database the GUI uses, so you can inspect state,
//! reproduce bugs and (from M1 on) run syncs without launching a window.

use anyhow::{bail, Context, Result};
use hl_core::{tfpath, SteamId, TfClass};
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
    match <LOG_ID> [--json]
                           Matchups for one stored match
    rate                   Rebuild baselines and rate every stored performance
    demos                  Scan the TF2 folder for demos and link them to matches
    rawlogs [--max N] [--check]
                           Fetch logs.tf raw logs and derive every kill; --check
                           compares stored kills with logs.tf's totals
    analysis <LOG_ID> [--json]
                           Kills, damage and play-by-play from a match's raw log
    mapview <MAP> [--json] A map's outline from every stored kill on it
    maps [--fetch] [--log ID]
                           Resolve every round's map (combined logs included);
                           --fetch first downloads the parts of combined logs
    etf2l [--offline]      Fetch ETF2L officials and classify every match (official/scrim/pug)
    teammates [--all] [--json]
                           Your teams and regular teammates (officials and scrims unless --all)
    profile [CLASS] [--official|--scrim|--pug] [--json]
                           Your rating profile (defaults to your most-rated class)
    matches [N] [--all] [--official|--scrim|--pug]
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
            let raw = hl_ingest::kills::fetch(&db, &sources, None, print_progress).await?;
            println!("\nraw logs: {} fetched, {} missing, {} failed", raw.fetched, raw.missing, raw.failed);
            if let Err(e) = hl_ingest::etf2l::fetch(&db, &sources, me, |_, _| {}).await {
                println!("ETF2L unavailable, classifying from stored data: {e:#}");
            }
            classify(&db, me).await?;
            if let Some(tf) = db.get_config().await?.tf_path {
                hl_ingest::index_demos(&db, std::path::Path::new(&tf)).await?;
            }
            let m = hl_ingest::maps::resolve_all(&db).await?;
            println!("round maps: {} multi-map logs, {} rounds unresolved", m.multi_map_logs, m.unresolved);
            print_stats(&db.index_stats().await?);
            rate(&db, &db_path).await
        }

        ["reprocess"] => {
            let db = Db::connect(&db_path).await?;
            let started = std::time::Instant::now();
            let stats = hl_ingest::reprocess(&db, print_progress).await?;
            let kills = hl_ingest::kills::rederive_all(&db, print_progress).await?;
            println!();
            println!("{kills} kills re-derived from stored raw logs");
            println!("rebuilt in {:.1}s", started.elapsed().as_secs_f64());
            if let Some(me) = db.get_me().await? {
                classify(&db, me).await?;
            }
            let m = hl_ingest::maps::resolve_all(&db).await?;
            println!("round maps: {} multi-map logs, {} rounds unresolved", m.multi_map_logs, m.unresolved);
            print_stats(&stats);
            rate(&db, &db_path).await
        }

        ["rate"] => {
            let db = Db::connect(&db_path).await?;
            rate(&db, &db_path).await
        }

        ["demos"] => {
            let db = Db::connect(&db_path).await?;
            let tf = db
                .get_config()
                .await?
                .tf_path
                .context("no TF2 folder set: run `hl tf set <PATH>` first")?;
            let started = std::time::Instant::now();
            let s = hl_ingest::index_demos(&db, std::path::Path::new(&tf)).await?;
            println!("scanned {} demos in {:.1}s ({} unreadable, {} removed)", s.scanned, started.elapsed().as_secs_f64(), s.unreadable, s.removed);
            println!("logs placed on the real clock: {}", s.logs_placed);
            println!("demos linked: {} ({} links) -> {} matches with a demo", s.demos_linked, s.links, s.matches_with_demo);
            println!("sidecar markers: {}", s.markers);
            Ok(())
        }

        ["profile", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let json = rest.contains(&"--json");
            let class = match rest.iter().find(|a| !a.starts_with("--")) {
                Some(c) => TfClass::parse(c)?,
                None => {
                    let classes = hl_ingest::rated_classes(&db, me).await?;
                    let top = classes.first().context("nothing rated yet: run `hl rate`")?;
                    TfClass::parse(&top.0)?
                }
            };
            let kind = kind_flag(rest);
            let Some(p) = hl_ingest::load_profile(&db, me, class, kind).await? else {
                println!("No rated {} games.", class.display_name());
                return Ok(());
            };
            if json {
                let classes = hl_ingest::rated_classes(&db, me).await?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({ "classes": classes, "profile": p }))?
                );
                return Ok(());
            }
            println!("{} — {} rated games\n", class.display_name(), p.games);
            println!("career     {:>5.1}", p.career_avg);
            println!(
                "form       {:>5.1}   (last {}{})",
                p.form_avg,
                p.form_window,
                p.prev_form_avg
                    .map(|prev| format!(", {:+.1} on the {} before", p.form_avg - prev, p.form_window))
                    .unwrap_or_default()
            );
            if let Some(wr) = p.win_rate {
                println!("win rate   {:>5.1}%", wr);
            }
            for e in &p.extras {
                println!("{:<24} {}", e.label, e.value);
            }
            for c in &p.contexts {
                println!(
                    "{:<24} {:>5.1}   {} games{}",
                    format!("{}s", c.kind),
                    c.avg,
                    c.games,
                    c.win_rate.map(|w| format!(", {w:.0}% won")).unwrap_or_default()
                );
            }
            println!("\ncomponent         weight    form    career  recent avg");
            for c in &p.components {
                println!(
                    "{:<16} {:>6.0}% {:>7.1} {:>9.1}  {:.2} {}",
                    c.label, c.weight * 100.0, c.form_pct, c.career_pct, c.form_raw, c.unit
                );
            }
            println!("\nbest");
            for g in &p.best {
                println!("  {:>5.1}  {}  {}  {}", g.score, g.log_id, g.played_at.map(fmt_date).unwrap_or_default(), g.map.as_deref().unwrap_or("?"));
            }
            println!("worst");
            for g in &p.worst {
                println!("  {:>5.1}  {}  {}  {}", g.score, g.log_id, g.played_at.map(fmt_date).unwrap_or_default(), g.map.as_deref().unwrap_or("?"));
            }
            Ok(())
        }

        ["rawlogs", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            if !rest.contains(&"--check") {
                let sources = Sources::new()?;
                let max = flag_value(rest, "--max")?;
                let started = std::time::Instant::now();
                let s = hl_ingest::kills::fetch(&db, &sources, max, print_progress).await?;
                println!(
                    "\nfetched {} raw logs ({} kills) in {:.0}s; {} missing on logs.tf, {} failed",
                    s.fetched,
                    s.kills,
                    started.elapsed().as_secs_f64(),
                    s.missing,
                    s.failed
                );
            }
            let st = db.rawlog_stats().await?;
            println!(
                "stored {} ({:.1} MB), pending {}, missing {}, kills {}",
                st.stored,
                st.bytes as f64 / 1e6,
                st.pending,
                st.missing,
                st.kills
            );
            if rest.contains(&"--check") {
                let (logs, bad) = db.check_kills_against_logstf().await?;
                println!("{logs} logs compared with logs.tf; {} with a player whose kills differ", bad.len());
                for (log_id, players) in bad.iter().take(15) {
                    println!("  {log_id}: {players}");
                }
            }
            Ok(())
        }

        ["analysis", id, rest @ ..] => {
            let log_id: i64 = id.parse().context("log id must be a number")?;
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?;
            let started = std::time::Instant::now();
            let a = hl_ingest::analysis::load(&db, log_id, me)
                .await?
                .with_context(|| format!("log {log_id} has no stored raw log"))?;
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&a)?);
                return Ok(());
            }
            println!(
                "{} kills, {} events, {} damage rows, {:.0}s of game time, built in {} ms",
                a.kills.len(),
                a.events.len(),
                a.damage.len(),
                a.duration_s,
                started.elapsed().as_millis()
            );
            for r in &a.rounds {
                println!("  round {} {:>6.0}s - {:>6.0}s", r.round_num, r.start_s, r.end_s);
            }
            let jumpable = a.kills.iter().filter(|k| k.jump.is_some()).count();
            println!("jumpable kills {jumpable}; streaks {}", a.events.iter().filter(|e| e.kind == "streak").count());
            Ok(())
        }

        ["mapview", map, rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?;
            let Some(m) = hl_ingest::mapview::load(&db, map, me).await? else {
                println!("too few kills on {map} to draw it");
                return Ok(());
            };
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&m)?);
                return Ok(());
            }
            println!(
                "{}: {} games, {} positions, {}x{} cells of {:.0} units; your games {}",
                m.map_base, m.games, m.points, m.width, m.height, m.cell, m.my_games
            );
            // A coarse ASCII preview, every third cell.
            let max = *m.occupancy.iter().max().unwrap_or(&1) as f64;
            for y in (0..m.height).step_by(3) {
                let row: String = (0..m.width)
                    .step_by(2)
                    .map(|x| {
                        let n = m.occupancy[y * m.width + x] as f64;
                        match (n.ln_1p() / max.ln_1p() * 4.0) as usize {
                            0 => ' ',
                            1 => '.',
                            2 => ':',
                            3 => '*',
                            _ => '#',
                        }
                    })
                    .collect();
                println!("{row}");
            }
            Ok(())
        }

        ["maps", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            if rest.contains(&"--fetch") {
                let sources = Sources::new()?;
                let p = hl_ingest::maps::fetch_parts(&db, &sources, print_progress).await?;
                println!(
                    "\nparts: {} wanted, {} fetched, {} failed{}",
                    p.wanted,
                    p.fetched,
                    p.failed,
                    if p.gave_up { " (logs.tf not answering; stopped)" } else { "" }
                );
            }
            let started = std::time::Instant::now();
            let r = hl_ingest::maps::resolve_all(&db).await?;
            println!(
                "{} logs, {} rounds, {} unresolved, {} multi-map logs, in {:.1}s",
                r.logs,
                r.rounds,
                r.unresolved,
                r.multi_map_logs,
                started.elapsed().as_secs_f64()
            );
            for (src, n) in &r.by_source {
                println!("  {src:<10} {n:>5}");
            }
            if let Some(id) = flag_value::<i64>(rest, "--log")? {
                for s in db.segments(id).await? {
                    println!(
                        "  R{}-R{}  {:<24} {} rounds  red {} blue {}",
                        s.first_round,
                        s.last_round,
                        s.map.as_deref().unwrap_or("?"),
                        s.rounds,
                        s.red_wins,
                        s.blue_wins
                    );
                }
            }
            Ok(())
        }

        ["etf2l", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            if !rest.contains(&"--offline") {
                let sources = Sources::new()?;
                let s = hl_ingest::etf2l::fetch(&db, &sources, me, |done, total| {
                    if total > 0 && (done % 10 == 0 || done == total) {
                        eprintln!("  ETF2L matches {done}/{total}");
                    }
                })
                .await?;
                println!("ETF2L player {:?}: fetched {} matches, {} failed", s.player_id, s.fetched, s.failed);
            }
            classify(&db, me).await
        }

        ["teammates", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let scope = if rest.contains(&"--all") { hl_ingest::teammates::Scope::All } else { hl_ingest::teammates::Scope::Team };
            let t = hl_ingest::teammates::load(&db, me, scope).await?;
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string_pretty(&t)?);
                return Ok(());
            }
            println!("{} games

teams", t.games);
            for team in &t.teams {
                println!(
                    "  {:<28} {} – {}  {:>3} games ({} official)  {}-{}  you {}",
                    team.name,
                    fmt_date(team.first_played),
                    fmt_date(team.last_played),
                    team.games,
                    team.officials,
                    team.wins,
                    team.losses,
                    team.my_avg.map(|a| format!("{a:.1}")).unwrap_or("-".into())
                );
                let core: Vec<String> = team.core.iter().map(|m| format!("{} ({})", m.name, m.games)).collect();
                println!("      {}", core.join(", "));
            }
            println!("
teammates (≥{} games)", t.min_games);
            for m in t.teammates.iter().take(40) {
                println!(
                    "  {:<22} {:<9} {:>4} games {:>3} off  {:>3}-{:<3} last {}  with {:>5} ({:>5}) {}",
                    m.name.chars().take(22).collect::<String>(),
                    m.main_class.as_deref().unwrap_or("-"),
                    m.games,
                    m.officials,
                    m.wins,
                    m.losses,
                    fmt_date(m.last_played),
                    m.my_avg_with.map(|a| format!("{a:.1}")).unwrap_or("-".into()),
                    m.my_avg_delta.map(|a| format!("{a:+.1}")).unwrap_or("-".into()),
                    m.teams.join("/")
                );
            }
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
                kind: kind_flag(rest).map(str::to_string),
                limit,
                offset: 0,
            };
            let page = db.list_matches(me.map(|m| m.account_id()), &filter).await?;
            println!("{} match(es) total, showing {}\n", page.total, page.items.len());
            println!("log       date       map                  league class          K/D/A   dmg  title");
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

        ["match", id, rest @ ..] => {
            let log_id: i64 = id.parse().context("log id must be a number")?;
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?;
            let weights_path = db_path.with_file_name("weights.toml");
            let (weights, warning) = hl_rating::Weights::load(&weights_path);
            if let Some(w) = &warning {
                eprintln!("warning: {w}");
            }
            let detail = hl_ingest::match_detail(&db, log_id, me, &weights)
                .await?
                .with_context(|| format!("log {log_id} is not stored; run `hl sync` first"))?;

            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string_pretty(&detail)?);
                return Ok(());
            }

            println!(
                "{}  {}  {}–{} {}",
                detail.map.as_deref().unwrap_or("unknown map"),
                detail.played_at.map(fmt_date).unwrap_or_default(),
                detail.red_score,
                detail.blue_score,
                detail.result.unwrap_or(""),
            );
            println!("model {}\n", detail.model_version);
            println!("{:<9} {:>7}  {:<18} {:>6}  {:>6}  {:<18} {:>7}", "class", "h2h", "us", "score", "score", "them", "winner");
            for m in &detail.matchups {
                let side = |s: &Option<hl_rating::detail::Side>| match s {
                    Some(s) => (
                        truncate(&s.name, 18),
                        s.rating.as_ref().map(|r| format!("{:.1}", r.score)).unwrap_or_else(|| "—".into()),
                    ),
                    None => ("—".into(), String::new()),
                };
                let (ln, ls) = side(&m.left);
                let (rn, rs) = side(&m.right);
                let h2h = m.head_to_head.map(|(a, b)| format!("{a}–{b}")).unwrap_or_default();
                let winner = match m.winner {
                    Some("left") => "us",
                    Some("right") => "them",
                    Some(other) => other,
                    None => "",
                };
                println!(
                    "{:<9} {:>7}  {:<18} {:>6}  {:>6}  {:<18} {:>7}{}{}",
                    m.class.as_str(), h2h, ln, ls, rs, rn, winner,
                    if m.decisive { "  ◆ decisive" } else { "" },
                    if m.involves_me { "  ← you" } else { "" },
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
        Progress::Rating { done, total } => print!("\rrating {done}/{total}          "),
        Progress::Etf2l { done, total } => print!("\rETF2L matches {done}/{total}          "),
        Progress::RawLogs { done, total } => print!("\rraw logs {done}/{total}          "),
        Progress::Parts { done, total } => print!("\rparts {done}/{total}          "),
        Progress::Etf2lFailed { error } => println!("\n  ! ETF2L: {error}"),
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

/// Rate everything with the current weights, printing a one-line summary.
async fn rate(db: &Db, db_path: &std::path::Path) -> Result<()> {
    let (weights, warning) = hl_rating::Weights::load(&db_path.with_file_name("weights.toml"));
    if let Some(w) = warning {
        eprintln!("warning: {w}");
    }
    let me = db.get_me().await?;
    let started = std::time::Instant::now();
    let s = hl_ingest::rate_all(db, me, &weights, print_progress).await?;
    println!(
        "\rrated {} performances from {} logs ({} yours) in {:.1}s",
        s.rated,
        s.logs,
        s.mine,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// `--official`, `--scrim` or `--pug`, as a match context kind.
fn kind_flag<'a>(args: &[&str]) -> Option<&'a str> {
    ["official", "scrim", "pug"].into_iter().find(|k| args.iter().any(|a| a.strip_prefix("--") == Some(*k)))
}

/// Run the context pass and say what it found.
async fn classify(db: &Db, me: SteamId) -> Result<()> {
    let c = hl_ingest::etf2l::derive_context(db, me).await?;
    println!(
        "officials {} ({} found by roster), scrims {}, pugs {}",
        c.officials, c.roster_officials, c.scrims, c.pugs
    );
    Ok(())
}
