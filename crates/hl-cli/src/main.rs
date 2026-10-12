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
    state <LOG_ID> [--at T] [--json]
                           One match's game state; --at lists who is alive at T
                           (raw clock)
    state --check [--max N]
                           Rebuild the game state (alive, charges, caps) from every
                           raw log and check it against logs.tf
    validate CLASS [--weights PATH]... [--split YYYY-MM-DD] [--league] [--json]
                           How often each component, and each weighting, picks
                           the team that won (PLAN §12 step 0, Q8). Any of the
                           nine classes. Ends with a model proposed from the
                           fit and what it is worth cross-validated. --weights
                           takes a TOML file with a [model.CLASS] table; repeatable
    guide [--json]         Every class's model: its components, their share of
                           the rating and what each one means (the app's 'How
                           ratings work' page)
    league-sample [status|discover|run N]
                           The ETF2L sample from every division: where it
                           stands, list and choose the matches, or run N
                           download steps (one request each, 5 s apart)
    who NAME|STEAMID       Look up another player in your matches
    failed                 Logs that would not import, and why
    import ID|URL          Fetch one log now, whatever the index thinks of it
    situation [--toml [--round]] | --victims [--json]
                           What a kill is worth by numbers and uber advantage
                           (PLAN §12 step 3); --toml prints the [situation] table
                           (from winning the fight, or the round). --victims
                           instead asks what killing each class was worth, KOTH
                           against stopwatch, over what the situation predicted
    aim <LOG_ID> [--json]  Your aim behind every kill in a match, from its demo:
                           crosshair error, flick and range (PLAN §14)
    aim --derive [--all]   Read every linked demo and store the aim behind every
                           kill; --all re-reads demos already done
    kept [DEMO_ID]         Demos kept as timelines (Q3); with an id, read one
                           back from the database alone and derive from it
    timeline <PATH> [--stride N] [--owner STEAMID3]
                           Keep a demo (Q3): record it, report the size, and
                           check what is stored against the demo itself
    demo <PATH> [--stride N] [--json]
                           Read a demo's packets: who is in it, and where you
                           stood and looked (PLAN §14)
    parts <LOG_ID> [--fetch]
                           The logs a combined log was built from, each scored
                           on its own; --fetch gets any that are missing
    stv <LOG_ID>           Download a match's SourceTV demo, link it and read it
    backup [--list]        Copy the database (kept beside it, newest five)
    owner [--refresh]      Your name and profile picture (--refresh looks them up)
    seasons [CLASS] [--json]
                           Your seasons, and how you played the class in each
    fights [CLASS] [--all] [--official|--scrim|--pug]
                           Kills in context: your totals against the pool
                           (derives any logs not yet read; --all re-reads every log)
    analysis <LOG_ID> [--json]
                           Kills, damage and play-by-play from a match's raw log
    mapview <MAP> [--json] A map's outline from every stored kill on it
    maps [--fetch] [--log ID]
                           Resolve every round's map (combined logs included);
                           --fetch first downloads the parts of combined logs
    maps --export-geometry <PATH> [--min N]
                           Write this database's map shapes, the file shipped
                           as maps/geometry.json (maps with N+ positions)
    demostats <DEMO> [--owner STEAMID3]
                           Every player's ping and every reflect in one demo
    snapshot [--out PATH]  Write the league snapshot the app ships (default
                           league/snapshot.sqlite3.gz) from this database,
                           rated for the current model. Run it on a copy.
    snapshot --check       What the built-in snapshot holds; fails when it is
                           for another rating model than this build's
    etf2l [--offline]      Fetch ETF2L officials and classify every match (official/scrim/pug)
    transfers team ID | player STEAMID | backfill [N] | medals
                           ETF2L transfers: a team's roster history, a player's
                           teams with dates, read every team's (medal winners
                           first), and the medals withheld for leaving early
    reread <LOG_ID>        Read one match again: its demos, then aim and fights
    teammates [--all] [--json]
                           Your teams and regular teammates (officials and scrims unless --all)
    profile [CLASS] [--official|--scrim|--pug] [--json]
                           Your rating profile (defaults to your most-rated class)
    matches [N] [--all] [--official|--scrim|--pug] [--class C] [--map M]
                           List recent matches (Highlander only unless --all);
                           --class and --map narrow them, a map without its
                           version (upward, product)

OPTIONS:
    --db <PATH>            Override the database location
";

fn main() -> Result<()> {
    // Every command is an arm of one async block, and in a debug build that
    // future outgrew the main thread's 1 MB stack (`hl fixtures` overflowed
    // after the 3 Oct merges): run it on a thread with room to spare.
    std::thread::Builder::new()
        .name("hl".into())
        .stack_size(64 << 20)
        .spawn(|| tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(run()))?
        .join()
        .map_err(|_| anyhow::anyhow!("the command panicked"))?
}

async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,hl_cli=info".into()),
        )
        .init();

    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // Anything that opens the app's database waits for the app to let go.
    // `--force` is for when you know the window is shut and the lock is not.
    let forced = args.iter().any(|a| a == "--force");
    args.retain(|a| a != "--force");

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
    // Help and the like never open the database, so they never wait for it.
    // `snapshot --check` reads only the snapshot built into this binary.
    let touches_db = !matches!(command.first().copied(), None | Some("help") | Some("--help") | Some("-h"))
        && !(command.first() == Some(&"snapshot") && command.contains(&"--check"));
    if touches_db {
        hl_ingest::lock::require_free(&db_path, forced)?;
    }
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
            let (weights, _) = hl_rating::Weights::load(&db_path.with_file_name("weights.toml"));
            let summary = hl_ingest::sync(&db, &sources, me, &opts, &weights, print_progress).await?;
            println!();
            println!("fetched {} log(s), {} failed", summary.fetched, summary.failed);
            let raw = hl_ingest::kills::fetch(&db, &sources, None, print_progress).await?;
            println!("\nraw logs: {} fetched, {} missing, {} failed", raw.fetched, raw.missing, raw.failed);
            // ETF2L was fetched inside the sync, before the queue; sorting the
            // logs into officials, scrims and pugs needs their player lists.
            classify(&db, me).await?;
            if let Some(tf) = db.get_config().await?.tf_path {
                hl_ingest::index_demos(&db, std::path::Path::new(&tf)).await?;
            }
            let m = hl_ingest::maps::resolve_all(&db).await?;
            println!("round maps: {} multi-map logs, {} rounds unresolved", m.multi_map_logs, m.unresolved);
            hl_ingest::fights::derive_all(&db, false, |_, _| {}).await?;
            print_stats(&db.index_stats().await?);
            rate(&db, &db_path).await
        }

        // Q7: what the teamfights in every stored log look like.
        ["teamfights"] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let (mut fights, mut together, mut mine, mut joined) = (0usize, 0usize, 0usize, 0usize);
            // Q7b: does arriving together win fights? For every side of
            // three or more in every teamfight: the share of it in within
            // TOGETHER_S of its own first arrival, and whether it lost fewer
            // players than the other side. Bucketed by that share.
            let mut by_share: [(usize, usize, usize); 5] = [(0, 0, 0); 5];
            let mut joins: Vec<i64> = Vec::new();
            let mut spreads: Vec<i64> = Vec::new();
            for log_id in db.rawlog_ids().await? {
                let Some(zip) = db.rawlog(log_id).await? else { continue };
                let raw = hl_ingest::rawlog::parse(&hl_ingest::rawlog::unzip(&zip)?);
                let gs = hl_ingest::state::GameState::build(&raw);
                let f = hl_ingest::fights::analyse(&raw, &gs);
                // Which fight each kill belongs to, as the situation pass groups them.
                let fight_of: Vec<Option<usize>> = hl_ingest::situation::kill_states(&raw, &gs, &f.tags)
                    .into_iter()
                    .map(|x| x.map(|(_, fight)| fight))
                    .collect();
                let tf = hl_ingest::teamfights::teamfights(&raw, &gs, &fight_of);
                for t in &tf {
                    for side in [hl_core::matchdata::Team::Red, hl_core::matchdata::Team::Blue] {
                        let us: Vec<_> = t.arrivals.iter().filter(|a| a.team == side).collect();
                        let them: Vec<_> = t.arrivals.iter().filter(|a| a.team != side).collect();
                        if us.len() < hl_ingest::teamfights::MIN_SIDE || them.len() < hl_ingest::teamfights::MIN_SIDE {
                            continue;
                        }
                        let first = us.iter().map(|a| a.joined_at).min().unwrap_or(0);
                        let share = us.iter().filter(|a| a.joined_at <= first + hl_ingest::teamfights::TOGETHER_S).count()
                            as f64
                            / us.len() as f64;
                        let lost = |v: &[&hl_ingest::teamfights::Arrival]| v.iter().filter(|a| a.died_at.is_some()).count();
                        let b = ((share * 5.0) as usize).min(4);
                        by_share[b].0 += 1;
                        by_share[b].1 += usize::from(lost(&us) < lost(&them));
                        by_share[b].2 += usize::from(lost(&us) > lost(&them));
                    }
                }
                // Which side the owner wore, from their own kills and deaths.
                let team = raw
                    .kills
                    .iter()
                    .find_map(|k| {
                        if k.killer.account == me.account_id() { k.killer.team }
                        else if k.victim.account == me.account_id() { k.victim.team }
                        else { None }
                    });
                let Some(team) = team else { continue };
                for t in &tf {
                    if let Some(spread) = t.collapse(team) {
                        fights += 1;
                        spreads.push(spread);
                        if spread <= hl_ingest::teamfights::TOGETHER_S { together += 1; }
                    }
                }
                let h = hl_ingest::teamfights::habits(&tf, me.account_id(), team);
                mine += h.fights;
                joined += h.joined;
                if h.joined > 0 { joins.push(h.median_join_s); }
            }
            spreads.sort_unstable();
            joins.sort_unstable();
            let med = |v: &[i64]| v.get(v.len() / 2).copied().unwrap_or(0);
            println!("teamfights seen              {fights}");
            println!("  your side arrived together {together} ({}%)", together * 100 / fights.max(1));
            println!("  median arrival spread      {}s", med(&spreads));
            println!("fights your side turned up to {mine}");
            println!("  you were in                {joined} ({}%)", joined * 100 / mine.max(1));
            println!("  your median arrival        {}s after the first kill", med(&joins));
            println!("\nevery side of 3+, by how much of it arrived within {}s of its first:", hl_ingest::teamfights::TOGETHER_S);
            println!("{:<12} {:>7} {:>8} {:>8}", "together", "fights", "won", "lost");
            for (i, (n, w, l)) in by_share.iter().enumerate() {
                println!(
                    "{:<12} {:>7} {:>7.1}% {:>7.1}%",
                    format!("{}-{}%", i * 20, i * 20 + 20),
                    n,
                    *w as f64 * 100.0 / (*n).max(1) as f64,
                    *l as f64 * 100.0 / (*n).max(1) as f64
                );
            }
            Ok(())
        }

        // A consistent copy to work on, so measuring never touches the
        // original. `cp` of a live SQLite file is not this: the write-ahead
        // log holds pages the main file does not have yet, and copying one
        // without the other gives a torn database that reads as corrupt.
        ["copy", to] => {
            let db = Db::connect(&db_path).await?;
            let to = PathBuf::from(to);
            if to.exists() {
                bail!("{} already exists; pick a name that does not", to.display());
            }
            db.vacuum_into(&to).await?;
            let bytes = std::fs::metadata(&to).map(|m| m.len()).unwrap_or(0);
            println!("{} -> {} ({:.0} MB)", db_path.display(), to.display(), bytes as f64 / 1e6);
            println!("Work on the copy: pass --db {}", to.display());
            Ok(())
        }

        ["find-demos"] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let sources = Sources::new()?;
            let before = db.logs_without_demo_id().await?.len();
            let f = hl_ingest::demostf::index(&db, &sources, me).await?;
            println!(
                "demos.tf listed {} demos; matched {} of {} logs that had no id",
                f.listed, f.matched, before
            );
            Ok(())
        }

        ["reprocess"] => {
            let db = Db::connect(&db_path).await?;
            let started = std::time::Instant::now();
            let stats = hl_ingest::reprocess(&db, print_progress).await?;
            let derived = started.elapsed().as_secs_f64();
            let kills = hl_ingest::kills::rederive_all(&db, print_progress).await?;
            println!();
            println!(
                "{kills} kills re-derived from stored raw logs in {:.1}s (the rest took {derived:.1}s)",
                started.elapsed().as_secs_f64() - derived
            );
            println!("rebuilt in {:.1}s", started.elapsed().as_secs_f64());
            if let Some(me) = db.get_me().await? {
                classify(&db, me).await?;
            }
            let m = hl_ingest::maps::resolve_all(&db).await?;
            println!("round maps: {} multi-map logs, {} rounds unresolved", m.multi_map_logs, m.unresolved);
            hl_ingest::fights::derive_all(&db, true, |_, _| {}).await?;
            print_stats(&stats);
            rate(&db, &db_path).await
        }

        ["rate"] => {
            let db = Db::connect(&db_path).await?;
            rate(&db, &db_path).await
        }

        ["snapshot", rest @ ..] => {
            if rest.contains(&"--check") {
                let m = hl_ingest::snapshot::built_in_meta().await?;
                println!(
                    "built-in league snapshot: model {}, {} officials rated, pool {}, newest official {}, made {}",
                    m.model, m.logs, m.pool, m.newest, m.made_at
                );
                if m.model != hl_rating::MODEL_VERSION {
                    bail!(
                        "the snapshot is for model {} and this build rates with {}: run `hl snapshot` on a rated copy of the database before releasing",
                        m.model,
                        hl_rating::MODEL_VERSION
                    );
                }
                return Ok(());
            }
            let out = flag_value::<String>(rest, "--out")?.unwrap_or_else(|| "league/snapshot.sqlite3.gz".into());
            let db = Db::connect(&db_path).await?;
            let (w, _) = hl_rating::Weights::load(&db_path.with_file_name("weights.toml"));
            // On a worker thread: the export is too deep for the main
            // thread's stack on Windows (1 MB).
            let (src, dest) = (db_path.clone(), std::path::PathBuf::from(&out));
            let m = tokio::spawn(async move { hl_ingest::snapshot::export(&db, &src, &w, &dest).await }).await??;
            let size = std::fs::metadata(&out).map(|f| f.len()).unwrap_or(0);
            println!(
                "{out}: model {}, {} officials rated, pool {}, {:.1} MB",
                m.model,
                m.logs,
                m.pool,
                size as f64 / 1e6
            );
            Ok(())
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
            let Some(p) = hl_ingest::load_profile(&db, me, class, kind, None).await? else {
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
            println!("career     {:>5.2}", p.career_avg);
            println!(
                "form       {:>5.2}   (last {}{})",
                p.form_avg,
                p.form_window,
                p.prev_form_avg
                    .map(|prev| format!(", {:+.2} on the {} before", p.form_avg - prev, p.form_window))
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
                    "{:<24} {:>5.2}   {} games{}",
                    format!("{}s", c.kind),
                    c.avg,
                    c.games,
                    c.win_rate.map(|w| format!(", {w:.0}% won")).unwrap_or_default()
                );
            }
            if !p.opposition.is_empty() {
                println!();
                println!("against               you   opponent  games");
                for o in &p.opposition {
                    println!(
                        "{:<18} {:>6.2} {:>9.2} {:>6}{}",
                        match o.band.as_str() {
                            "weaker" => "weaker opponents",
                            "stronger" => "stronger opponents",
                            _ => "an even match",
                        },
                        o.avg,
                        o.opponent_avg,
                        o.games,
                        o.win_rate.map(|w| format!("   {w:.0}% won")).unwrap_or_default()
                    );
                }
            }
            if !p.by_division.is_empty() {
                println!();
                println!("their division        you   games");
                for d in &p.by_division {
                    println!("{:<18} {:>6.2} {:>7}{}", d.division, d.avg, d.games, d.win_rate.map(|w| format!("   {w:.0}% won")).unwrap_or_default());
                }
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

        ["state", rest @ ..] if rest.contains(&"--check") => {
            let db = Db::connect(&db_path).await?;
            let report = hl_ingest::statecheck::check(&db, flag_value(rest, "--max")?).await?;
            print!("{report}");
            Ok(())
        }

        ["state", id, rest @ ..] => {
            let log_id: i64 = id.parse().context("log id must be a number")?;
            let db = Db::connect(&db_path).await?;
            let zip = db.rawlog(log_id).await?.with_context(|| format!("log {log_id} has no stored raw log"))?;
            let raw = hl_ingest::rawlog::parse(&hl_ingest::rawlog::unzip(&zip)?);
            let s = hl_ingest::state::GameState::build(&raw);
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&s)?);
                return Ok(());
            }
            println!(
                "{} lives, {} charge spans, {} rounds, {} caps, {} sentries",
                s.lives.len(),
                s.charges.len(),
                s.rounds.len(),
                s.caps.len(),
                s.sentries.len()
            );
            if let Some(t) = flag_value::<i64>(rest, "--at")? {
                let n = s.numbers_at(t);
                println!("at {t}: {} red, {} blue alive", n[0], n[1]);
                for l in s.alive_at(t) {
                    println!("  {:?} {:<9} [U:1:{}] {}..{} {:?}", l.team, l.class.as_str(), l.account, l.from, l.to, l.end);
                }
            }
            Ok(())
        }

        ["validate", class, rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let class = TfClass::parse(class)?;
            let (live, warning) = hl_rating::Weights::load(&db_path.with_file_name("weights.toml"));
            if let Some(w) = warning {
                eprintln!("warning: {w}");
            }
            let mut candidates = Vec::new();
            for (i, a) in rest.iter().enumerate() {
                if *a == "--weights" {
                    let path = rest.get(i + 1).context("--weights needs a file")?;
                    let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
                    let name = std::path::Path::new(path).file_stem().map_or(path.to_string(), |s| s.to_string_lossy().into_owned());
                    candidates.push((name, hl_rating::Weights::model_from_toml(&text, class)?));
                }
            }
            let split = match flag_value::<String>(rest, "--split")? {
                Some(d) => Some(parse_day(&d)?),
                None => None,
            };
            let started = std::time::Instant::now();
            let report = hl_ingest::validate::run(&db, class, &live, candidates, split, rest.contains(&"--league")).await?;
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&report)?);
            } else {
                print!("{report}");
                println!("
({:.1}s)", started.elapsed().as_secs_f64());
            }
            Ok(())
        }

        ["situation", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let started = std::time::Instant::now();
            // Q4: what killing each class was worth, KOTH against stopwatch.
            if rest.contains(&"--victims") {
                let v = hl_ingest::situation::victim_worth(&db).await?;
                if rest.contains(&"--json") {
                    println!("{}", serde_json::to_string(&v.by.iter().map(|((c, m), w)| (format!("{}/{}", c.as_str(), m.as_str()), w)).collect::<Vec<_>>())?);
                } else {
                    print!("{v}");
                    println!("
({:.1}s)", started.elapsed().as_secs_f64());
                }
                return Ok(());
            }
            // Q25 (§18b): what a capture did to the capping team's dead.
            if rest.contains(&"--spawn-delay") {
                let s = hl_ingest::spawns::spawn_delays(&db).await?;
                print!("{s}");
                println!("({:.1}s)", started.elapsed().as_secs_f64());
                return Ok(());
            }
            // Q17: what a capture was worth, by how much was left to stop it.
            if rest.contains(&"--caps") {
                let c = hl_ingest::situation::cap_worth(&db).await?;
                print!("{c}");
                println!("({:.1}s)", started.elapsed().as_secs_f64());
                return Ok(());
            }
            let t = hl_ingest::situation::measure(&db).await?;
            if rest.contains(&"--toml") || rest.contains(&"--swing") {
                let (outcome, table) = if rest.contains(&"--round") { ("round", &t.round) } else { ("fight", &t.fight) };
                let swing = rest.contains(&"--swing");
                let what = if swing { "--swing" } else { "--toml" };
                let source = format!("From `hl situation {what}`: winning the {outcome}, {} kills in {} logs.", t.kills, t.logs);
                print!(
                    "{}",
                    if swing {
                        hl_ingest::situation::swing_table(&hl_ingest::situation::swings(table), &source)
                    } else {
                        hl_ingest::situation::toml_table(&hl_ingest::situation::factors(table), &source)
                    }
                );
                return Ok(());
            }
            print!("{t}");
            println!("
({:.1}s)", started.elapsed().as_secs_f64());
            Ok(())
        }

        // PLAN §14: read every linked demo and store the aim behind each kill.
        ["aim", "--all", rest @ ..] | ["aim", "--derive", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let started = std::time::Instant::now();
            let all = command.contains(&"--all");
            let s = hl_ingest::aim::derive_all(&db, me, all, |done, total, _log_id, _what| {
                print!("\r  reading demos {done}/{total}          ");
                let _ = std::io::Write::flush(&mut std::io::stdout());
            })
            .await?;
            println!(
                "\r{} of {} matches with a demo read in {:.0}s, {} kills stored",
                s.read,
                s.total,
                started.elapsed().as_secs_f64(),
                s.kills
            );
            let everything = hl_db::AimFilter { me: me.account_id(), ..Default::default() };
            if let Some(t) = db.aim_totals(&everything).await? {
                println!(
                    "\nOver {} kills: crosshair {:.1}° off at the shot, {:.1}° a second before, \
                     {:.1}° of flick, {:.0} units away. The crosshair was already within 3° \
                     a second before in {:.0}% of them.",
                    t.kills, t.error_deg, t.before_deg, t.flick_deg, t.range_units, t.held_share * 100.0
                );
            }
            let _ = rest;
            Ok(())
        }

        // PLAN §14: the aim behind every kill of yours in one match.
        ["aim", log_id, rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let started = std::time::Instant::now();
            let report = hl_ingest::aim::for_log(&db, log_id.parse()?, me).await?;
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&report)?);
                return Ok(());
            }
            // An STV demo answers for all eighteen players (Q16b), so the
            // default stays "your kills" and --all opens it up.
            let everyone = rest.contains(&"--everyone");
            let kills: Vec<&hl_ingest::aim::AimKill> =
                report.kills.iter().filter(|k| everyone || k.shooter == me.account_id()).collect();
            if kills.is_empty() {
                println!("No aim to read: no demo is linked to this match, or you are not in it.");
                if !everyone && !report.kills.is_empty() {
                    println!("The demo answered for {} kills by other players; try --everyone.", report.kills.len());
                }
                return Ok(());
            }
            println!(
                "{} of {} kills in this match are in the demo, read in {:.1}s{}\n",
                kills.len(),
                if everyone { report.kills.len() } else { report.log_kills },
                started.elapsed().as_secs_f64(),
                if report.other_matches > 0 {
                    format!(
                        "\n({} more kills in the recording belong to another match in it)",
                        report.other_matches
                    )
                } else {
                    String::new()
                }
            );
            println!(
                "{:<18} {:<18} {:<9} {:>7} {:>7} {:>7} {:>7} {:>7}  weapon",
                "killer", "victim", "class", "error", "1s", "flick", "range", "height"
            );
            for k in &kills {
                let name: String = k
                    .victim_name
                    .clone()
                    .unwrap_or_else(|| k.shot.victim.clone())
                    .chars()
                    .take(17)
                    .collect();
                let killer: String = k
                    .shooter_name
                    .clone()
                    .unwrap_or_else(|| k.shot.shooter.clone())
                    .chars()
                    .take(17)
                    .collect();
                // A shooter the demo lost has angles from wherever it last
                // saw them, which is not a reading of anything.
                let seen = match (k.shot.shooter_seen, k.shot.victim_seen) {
                    (true, true) => "",
                    (false, _) => "  (the killer was not carried by the demo)",
                    _ => "  (not carried by the demo)",
                };
                println!(
                    "{:<18} {:<18} {:<9} {:>6.1}° {:>6.1}° {:>6.1}° {:>7.0} {:>7.0}  {}{}",
                    killer,
                    name,
                    k.victim_class.clone().unwrap_or_default(),
                    k.shot.error_deg,
                    k.shot.error_before_deg,
                    k.shot.flick_deg,
                    k.shot.range,
                    k.shot.height,
                    if k.headshot { format!("{} (hs)", k.shot.weapon) } else { k.shot.weapon.clone() },
                    seen
                );
            }
            let seen: Vec<_> = kills.iter().filter(|k| k.shot.victim_seen && k.shot.shooter_seen).collect();
            if !seen.is_empty() {
                let mean =
                    |f: fn(&&&hl_ingest::aim::AimKill) -> f32| seen.iter().map(f).sum::<f32>() / seen.len() as f32;
                println!(
                    "\n{} kills with both players on screen: crosshair {:.1}° off at the shot, {:.1}° a second before, {:.1}° of flick, {:.0} units away.",
                    seen.len(),
                    mean(|k| k.shot.error_deg),
                    mean(|k| k.shot.error_before_deg),
                    mean(|k| k.shot.flick_deg),
                    mean(|k| k.shot.range),
                );
            }
            Ok(())
        }

        // Copies of the database, taken before every sync and rebuild.
        ["backup", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            if rest.contains(&"--list") {
                let all = hl_ingest::backup::list(&db_path);
                if all.is_empty() {
                    println!("No copies yet. They live in {}", hl_ingest::backup::dir(&db_path).display());
                    return Ok(());
                }
                println!("{:<24} {:>10}  path", "made", "size");
                for b in &all {
                    println!("{:<24} {:>9.0} MB  {}", fmt_date(b.made_at), b.bytes as f64 / 1_000_000.0, b.path);
                }
                return Ok(());
            }
            match hl_ingest::backup::run(&db, &db_path, true).await? {
                Some(b) => println!("copied to {} ({:.0} MB)", b.path, b.bytes as f64 / 1_000_000.0),
                None => println!("a recent copy already exists"),
            }
            Ok(())
        }

        // The logs a combined log was built from, each scored on its own.
        ["parts", log_id, rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?;
            let (w, _) = hl_rating::Weights::load(&db_path.with_file_name("weights.toml"));
            let log_id: i64 = log_id.parse()?;
            let mut parts = hl_ingest::parts::scores(&db, log_id, me, &w).await?;
            if rest.contains(&"--fetch") {
                let sources = Sources::new()?;
                for p in &mut parts {
                    if p.detail.is_none() {
                        p.detail = hl_ingest::parts::fetch(&db, &sources, p.log_id, me, &w).await?;
                    }
                }
            }
            if parts.is_empty() {
                println!("log {log_id} was not combined from other logs");
                return Ok(());
            }
            for p in &parts {
                match &p.detail {
                    Some(d) => println!(
                        "log {:<9} {:<22} {:>3} min  {} players  {}-{}",
                        p.log_id,
                        p.map.as_deref().unwrap_or("?"),
                        d.detail.duration_s / 60,
                        d.detail.players.len(),
                        d.detail.red_score,
                        d.detail.blue_score
                    ),
                    None => println!("log {:<9} {:<22} not fetched (use --fetch)", p.log_id, p.map.as_deref().unwrap_or("?")),
                }
            }
            Ok(())
        }

        // Fetch a match's SourceTV demo, link it, and read it: the same
        // chain the match page's button runs.
        ["stv", log_id, ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let tf = db.get_config().await?.tf_path.context("no TF2 folder set")?;
            let log_id: i64 = log_id.parse()?;
            let started = std::time::Instant::now();
            let mut last = 0u64;
            let done = hl_ingest::fetch_stv(&db, &Sources::new()?, std::path::Path::new(&tf), log_id, |bytes, total| {
                if bytes / 5_000_000 != last {
                    last = bytes / 5_000_000;
                    let of = total.map_or(String::new(), |t| format!(" of {:.0} MB", t as f64 / 1e6));
                    print!("\r  {:.0} MB{of}          ", bytes as f64 / 1e6);
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
            })
            .await?;
            println!("\rdownloaded {} in {:.0}s", done.file_name, started.elapsed().as_secs_f64());
            hl_ingest::index_demos(&db, std::path::Path::new(&tf)).await?;
            let routes = hl_ingest::aim::derive_log(&db, me, log_id).await?;
            println!("linked and read: {routes} routes stored");
            Ok(())
        }

        // PLAN §14: read a demo's packets, not just its header.
        ["demo", path, rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?;
            let stride = flag_value::<u32>(rest, "--stride")?.unwrap_or(hl_demos::parse::DEFAULT_STRIDE);
            let mine = me.map(|m| m.to_steamid3());
            let started = std::time::Instant::now();
            let scan = hl_demos::parse::scan(std::path::Path::new(path), mine.as_deref(), stride)?;
            let took = started.elapsed().as_secs_f64();
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&scan)?);
                return Ok(());
            }
            println!(
                "{} · {} ticks ({:.0} s of play) · parsed in {:.1}s ({:.0}x real time)",
                scan.map,
                scan.header_ticks,
                scan.seconds(),
                took,
                if took > 0.0 { scan.seconds() / took } else { 0.0 }
            );
            println!("\n{:<20} {:<20} {:<6} {:>8}", "player", "steamid", "team", "ticks");
            for p in scan.players.iter().take(20) {
                let name: String = p.name.chars().take(19).collect();
                println!("{:<20} {:<20} {:<6} {:>8}", name, p.steamid, p.team, p.ticks);
            }
            if scan.samples.is_empty() {
                println!("\nNo samples: you are not in this demo, or no SteamID is set.");
            } else {
                let alive = scan.samples.iter().filter(|s| s.alive).count();
                println!(
                    "\n{} samples of you, every {stride} ticks; alive in {alive} of them. First five:",
                    scan.samples.len()
                );
                println!("{:>8} {:>8} {:>8} {:>8} {:>7} {:>7} {:>6}", "tick", "x", "y", "z", "yaw", "pitch", "hp");
                for s in scan.samples.iter().take(5) {
                    println!(
                        "{:>8} {:>8.0} {:>8.0} {:>8.0} {:>7.1} {:>7.1} {:>6}",
                        s.tick, s.pos[0], s.pos[1], s.pos[2], s.yaw, s.pitch, s.health
                    );
                }
            }
            Ok(())
        }

        ["timeline", path, rest @ ..] => {
            // A file on disk and nothing else: no database is opened, so this
            // is safe to run while the app is up.
            let stride = flag_value::<u32>(rest, "--stride")?.unwrap_or(hl_demos::timeline::DEFAULT_STRIDE);
            let owner = flag_value::<String>(rest, "--owner")?.unwrap_or_default();
            let file = std::path::Path::new(path);
            let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
            let header = hl_demos::DemoHeader::parse(&bytes)?;
            // An unreadable rate means a broken header; TF2 servers run at 66.67.
            let rate = header.tick_rate().unwrap_or(66.67);
            let started = std::time::Instant::now();
            let (pass, stored) = hl_demos::aim::pass_recording(file, &owner, rate, Some(stride), &mut |_| {})?;
            // The timeline is written during the walk, so this is both.
            let walked = started.elapsed().as_secs_f64();
            let stored = stored.context("the pass was asked for a timeline and gave none")?;
            let started = std::time::Instant::now();
            let back = hl_demos::timeline::Timeline::decode(&stored)?;
            let decoded = started.elapsed().as_secs_f64();
            // Stored and read back, then stored again: the same bytes, or
            // recording and re-encoding have drifted apart.
            let again = back.encode()?;
            let tl = &back;

            let samples: usize = tl.tracks.iter().map(|t| t.samples.len()).sum();
            let changes: usize = tl.tracks.iter().map(|t| t.changes.len()).sum();
            let mb = |b: usize| b as f64 / 1e6;
            println!(
                "{} · {} ticks at {rate:.1}/s · stride {stride} · walked and recorded in {walked:.1}s, decoded in {decoded:.2}s",
                header.map, header.ticks
            );
            println!("{} people · {} stretch(es) · {samples} samples · {changes} changes · {} objects · {} events", tl.people.len(), tl.seams.len(), tl.objects.len(), tl.events.len());
            println!(
                "stored {:.2} MB (from {:.2} MB raw): samples {:.2} · changes {:.2} · objects {:.2} · events {:.2} · head {:.3}",
                mb(stored.stored_bytes()),
                mb(stored.raw_bytes),
                mb(stored.samples.len()),
                mb(stored.changes.len()),
                mb(stored.objects.len()),
                mb(stored.events.len()),
                mb(stored.head.len())
            );

            let same = again.samples == stored.samples
                && again.changes == stored.changes
                && again.objects == stored.objects
                && again.events == stored.events
                && again.head == stored.head;
            println!("round trip: {}", if same { "identical, byte for byte" } else { "DIFFERS" });
            // How the demo was written: a server records every tick, a
            // client only as often as it was sent updates.
            if let Some(track) = tl.tracks.iter().max_by_key(|t| t.samples.len()) {
                let mut gaps: Vec<u32> = track.samples.windows(2).map(|w| w[1].t - w[0].t).filter(|g| *g <= 8).collect();
                gaps.sort_unstable();
                if let Some(mid) = gaps.get(gaps.len() / 2) {
                    let ones = gaps.iter().filter(|g| **g == 1).count() as f64 / gaps.len() as f64;
                    println!("ticks between frames: median {mid}, {:.0}% of frames one tick apart", ones * 100.0);
                }
            }

            let agree = hl_demos::aim::agreement(&pass, &back);
            println!(
                "crosshair a second before the kill, recomputed from the timeline: {} kills, mean difference {:.3}°, worst {:.3}° ({} not comparable)",
                agree.shots_compared, agree.before_mean_diff, agree.before_max_diff, agree.shots_skipped
            );
            let worst = agree.scoped.iter().map(|(_, a, b)| (a - b).abs()).fold(0.0, f64::max);
            let snipers: Vec<_> = agree.scoped.iter().filter(|(_, a, _)| *a > 0.05).collect();
            println!("scoped share, pass against timeline: {} players, worst difference {:.4}", agree.scoped.len(), worst);
            for (who, a, b) in snipers {
                println!("  {who:<22} {:>6.1}% {:>6.1}%", a * 100.0, b * 100.0);
            }
            Ok(())
        }

        ["events", path] => {
            // Which game events a demo carries, and when the round ones fall:
            // for building passes over the timeline. A file only; no database.
            let file = std::path::Path::new(path);
            let header = hl_demos::DemoHeader::parse(&std::fs::read(file)?)?;
            let rate = header.tick_rate().unwrap_or(66.67);
            let (_, stored) = hl_demos::aim::pass_recording(file, "", rate, Some(hl_demos::timeline::DEFAULT_STRIDE), &mut |_| {})?;
            let tl = hl_demos::timeline::Timeline::decode(&stored.context("no timeline")?)?;
            let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
            for (t, e) in &tl.events {
                let name = format!("{e:?}");
                let name = name.split(['(', ' ', '{']).next().unwrap_or("").to_string();
                if name.starts_with("TeamPlay") {
                    println!("{:>7.1}s  tick {:>7}  {}", tl.seconds(*t), tl.tick_of(*t), format!("{e:?}").chars().take(140).collect::<String>());
                }
                *counts.entry(name).or_default() += 1;
            }
            for (n, c) in counts {
                println!("{c:>7}  {n}");
            }
            let carts = tl.objects.iter().filter(|o| o.kind == "cart").count();
            println!("{carts} cart rows");
            Ok(())
        }

        ["momentum", log_id, rest @ ..] => {
            // Q12: the holds of a payload match, from its STV timelines.
            let db = Db::connect(&db_path).await?;
            let log_id: i64 = log_id.parse()?;
            let data = db_path.parent().context("no data folder")?.to_path_buf();
            let json: serde_json::Value = serde_json::from_str(&db.raw_log(log_id).await?.context("no such match")?)?;
            let map = json.pointer("/info/map").and_then(|m| m.as_str()).unwrap_or_default().to_string();
            let Some(v) = hl_ingest::cart::for_log(&db, log_id, Some((data.as_path(), map.as_str()))).await? else {
                println!("{map}: no STV timeline with a cart");
                return Ok(());
            };
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&v)?);
                return Ok(());
            }
            for (i, r) in v.rounds.iter().enumerate() {
                let fights: Vec<_> = v.all_fights.iter().filter(|f| f.round == i).collect();
                let won = fights.iter().filter(|f| f.lost_defenders > f.lost_attackers).count();
                println!("round {}: {}s live, cart {} units, {} fights ({} won by BLU)", i + 1, r.seconds, r.progress.last().copied().unwrap_or(0), fights.len(), won);
                for h in v.holds.iter().filter(|h| h.round == i) {
                    println!(
                        "  hold at {:>3}s for {:>3}s{}: {} pushes turned back of {}, lost {}-{}, {}  (demo_gototick {})",
                        h.from_s, h.seconds, h.zone.as_deref().map(|z| format!(" at {z}")).unwrap_or_default(),
                        h.pushes_failed, h.fights, h.lost_attackers, h.lost_defenders,
                        if h.broke { "broke" } else { "held" }, h.jump_tick
                    );
                }
            }
            Ok(())
        }

        ["demostats", path, rest @ ..] => {
            // Q44 and Q45: every player's ping and every reflect in one demo,
            // each reflect with its demo tick to check in game. A file only.
            let owner = flag_value::<String>(rest, "--owner")?.unwrap_or_default();
            let file = std::path::Path::new(path);
            let header = hl_demos::DemoHeader::parse(&std::fs::read(file)?)?;
            let rate = header.tick_rate().unwrap_or(66.67);
            // On a thread of its own: a debug build's walk needs more stack
            // than the main thread has on Windows.
            let (f, o) = (file.to_path_buf(), owner.clone());
            let (_, stored) = std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn(move || hl_demos::aim::pass_recording(&f, &o, rate, Some(hl_demos::timeline::DEFAULT_STRIDE), &mut |_| {}))?
                .join()
                .map_err(|_| anyhow::anyhow!("reading the demo panicked"))??;
            let tl = hl_demos::timeline::Timeline::decode(&stored.context("no timeline")?)?;
            let name = |s: Option<usize>| s.and_then(|s| tl.people.get(s)).map_or("?".to_string(), |p| p.name.clone());
            let clock = |s: f64| format!("{}:{:02}", (s as u32) / 60, (s as u32) % 60);
            println!("{} · {}", header.map, clock(tl.seconds(tl.end())));
            println!("
{:<24} {:>5} {:>6} {:>5} {:>5}  spikes", "ping", "avg", "median", "min", "max");
            for p in hl_demos::demostats::pings(&tl) {
                let spikes: Vec<String> = p.spikes.iter().map(|(a, b, peak)| format!("{}-{} {peak}ms", clock(*a), clock(*b))).collect();
                println!("{:<24} {:>5.0} {:>6} {:>5} {:>5}  {}", name(Some(p.slot)), p.avg, p.median, p.min, p.max, spikes.join(", "));
            }
            let reflects = hl_demos::demostats::reflects(&tl);
            println!("
{} reflects:", reflects.len());
            for r in &reflects {
                let victims: Vec<String> = r.victims.iter().map(|v| name(Some(*v))).collect();
                println!(
                    "  {} {:<16} {:<7} {:?}{}{} threat {:?} · demo_gototick {}",
                    clock(r.at_s),
                    name(r.slot),
                    r.what,
                    r.outcome,
                    if victims.is_empty() { String::new() } else { format!(" {} dmg on {}", r.damage, victims.join(", ")) },
                    if r.killed { " (kill)" } else { "" },
                    r.threat,
                    tl.tick_of(r.t)
                );
            }
            Ok(())
        }

        ["cart", path] => {
            // Q11: the cart in a numbers advantage, from one demo. A file only.
            let file = std::path::Path::new(path);
            let header = hl_demos::DemoHeader::parse(&std::fs::read(file)?)?;
            let rate = header.tick_rate().unwrap_or(66.67);
            let (_, stored) = hl_demos::aim::pass_recording(file, "", rate, Some(hl_demos::timeline::DEFAULT_STRIDE), &mut |_| {})?;
            let tl = hl_demos::timeline::Timeline::decode(&stored.context("no timeline")?)?;
            let Some(r) = hl_demos::cart::cart(&tl) else {
                println!("{}: no cart in this demo", header.map);
                return Ok(());
            };
            let clock = |s: u32| format!("{}:{:02}", s / 60, s % 60);
            println!("{} · {} rounds · {} s up {}+ with the cart still", header.map, r.rounds.len(), r.wasted_s(), hl_demos::cart::UP);
            println!("\n{:>5} {:>7} {:>7} {:>7} {:>7} {:>8}", "round", "live", "moving", "up 3+", "still", "nobody");
            for (i, x) in r.rounds.iter().enumerate() {
                println!("{:>5} {:>6}s {:>6}s {:>6}s {:>6}s {:>7}s", i + 1, x.seconds, x.moving_s, x.up_s, x.up_still_s, x.up_still_empty_s);
            }
            println!("\nstalls of 3 s or more while up 3+:");
            for s in &r.stalls {
                println!("  round {} at {} for {:>3}s, up to +{}, {}s with no attacker near · demo_gototick {}", s.round + 1, clock(s.from_s), s.seconds, s.most_up, s.empty_s, s.jump_tick);
            }
            println!("\nafter a won fight, seconds of the next {} the cart moved:", hl_demos::cart::AFTER_S);
            for n in [Some(1), Some(2), Some(3), None] {
                let (k, mean, window) = r.after(n);
                let label = n.map_or("every".to_string(), |n| format!("{n}{}", ["st", "nd", "rd"][(n - 1) as usize]));
                println!("  {label:<6} fight won: {k:>3} fights, {mean:>4.1}s of {window:.1}s");
            }
            Ok(())
        }

        ["synth", path, out] => {
            // Q18: a log from a demo alone, written out to compare against a
            // real one. A file only; no database.
            let file = std::path::Path::new(path);
            let header = hl_demos::DemoHeader::parse(&std::fs::read(file)?)?;
            let rate = header.tick_rate().unwrap_or(66.67);
            let (_, stored) = hl_demos::aim::pass_recording(file, "", rate, Some(hl_demos::timeline::DEFAULT_STRIDE), &mut |_| {})?;
            let tl = hl_demos::timeline::Timeline::decode(&stored.context("no timeline")?)?;
            let start = std::fs::metadata(file)?.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64 - tl.seconds(tl.end()) as i64;
            let s = hl_demos::synth::synthesize(&tl, &header.map, start, "synthesized");
            let out = std::path::Path::new(out);
            std::fs::write(out.with_extension("log"), &s.text)?;
            std::fs::write(out.with_extension("json"), serde_json::to_string_pretty(&s.json)?)?;
            println!("{} · {} rounds · {} kills · {} people · {} lines", header.map, s.rounds, s.kills, s.players, s.text.lines().count());
            Ok(())
        }

        ["link-demo", log_id, path] => {
            // A demo linked to a match by hand, as dropping it on the match
            // page does. On a copy, with --db.
            let db = Db::connect(&db_path).await?;
            let tf = db.get_config().await?.tf_path.context("set the TF2 folder first")?;
            let me = db.get_me().await?;
            hl_ingest::kills::ensure_one(&db, &Sources::new()?, log_id.parse()?).await?;
            let got = hl_ingest::demo_import::link_to_log(&db, std::path::Path::new(&tf), std::path::Path::new(path), log_id.parse()?, me, |s| eprintln!("  {s}")).await?;
            println!("linked {} (demo {}): {} of {} log kills line up, {} players in both", got.file_name, got.demo_id, got.kills_matched, got.log_kills, got.players_shared);
            Ok(())
        }

        ["reread", log_id] => {
            // One match read again with this version: its demos from their
            // files, then aim and fights, as the match page's button does.
            // On a copy, with --db.
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?;
            let r = hl_ingest::demo_import::reread_log(&db, me, log_id.parse()?, |s| eprintln!("  {s}")).await?;
            println!("read again: {} demo(s), {} missing; aim {}, fights {}", r.demos, r.missing, r.aim, r.fights);
            Ok(())
        }

        ["import-demo", path] => {
            // Q18: a match from a demo alone, into the database -- a copy,
            // with --db, like every write here.
            let db = Db::connect(&db_path).await?;
            let tf = db.get_config().await?.tf_path.context("set the TF2 folder first: the demo is kept in tf/demos")?;
            let (w, _) = hl_rating::Weights::load(&db_path.with_file_name("weights.toml"));
            let me = db.get_me().await?;
            let started = std::time::Instant::now();
            let got = hl_ingest::demo_import::import(&db, &w, std::path::Path::new(&tf), std::path::Path::new(path), me, |s| eprintln!("  {s}")).await?;
            hl_ingest::demo_import::derive(&db, &w, me, |s| eprintln!("  {s}")).await?;
            println!(
                "log {} · {} · {} rounds · {} kills · {} players{} · {:.1}s",
                got.log.log_id,
                got.log.map.as_deref().unwrap_or("?"),
                got.rounds,
                got.kills,
                got.log.players,
                if got.log.yours { " · yours" } else { "" },
                started.elapsed().as_secs_f64()
            );
            Ok(())
        }

        ["leagues", "fetch"] => {
            // Q29: a year of ETF2L Highlander seasons, every team.
            let db = Db::connect(&db_path).await?;
            let sources = Sources::new()?;
            let started = std::time::Instant::now();
            let s = hl_ingest::leagues::fetch(&db, &sources, |done, total| eprint!("\r  competitions {done}/{total}  ")).await?;
            eprintln!();
            println!("{} competitions, {} results, {} match pages ({} failed) in {:.0}s", s.competitions, s.results, s.details, s.failed, started.elapsed().as_secs_f64());
            Ok(())
        }

        ["leagues", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let season = rest.first().and_then(|s| s.parse::<i64>().ok());
            let v = hl_ingest::leagues::season(&db, season).await?;
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&v)?);
                return Ok(());
            }
            println!("seasons: {}", v.seasons.iter().map(|s| format!("{} ({})", s.season, s.name)).collect::<Vec<_>>().join(", "));
            if let Some(s) = &v.season {
                println!("\nSeason {} ({}) · pool: {} · {} matches still to read in detail", s.season, s.name, s.pool.join(", "), v.pending_details);
            }
            for d in &v.divisions {
                println!("\n{}", d.division);
                for t in &d.teams {
                    let name: String = t.name.chars().take(28).collect();
                    println!("  {:>6}  {:<28} {:>2}-{:<2} {:>2}d  {:>4}:{:<4}", t.team_id, name, t.record.won, t.record.lost, t.record.drawn, t.score_for, t.score_against);
                }
            }
            Ok(())
        }

        ["team", id, rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let Some(t) = hl_ingest::leagues::team(&db, id.parse()?).await? else {
                println!("no team {id} stored; run `hl leagues fetch`");
                return Ok(());
            };
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&t)?);
                return Ok(());
            }
            println!("{} ({}) · {}-{}-{} · seasons {}", t.name, t.country.as_deref().unwrap_or("?"), t.record.won, t.record.lost, t.record.drawn, t.seasons.iter().map(|(s, d)| format!("S{s} {d}")).collect::<Vec<_>>().join(", "));
            println!("\n{:<24} {:>5} {:>5} {:>7}", "map", "W-L", "win%", "rounds");
            for m in &t.maps {
                let pct = if m.record.played > 0 { 100.0 * f64::from(m.record.won) / f64::from(m.record.played) } else { 0.0 };
                println!("{:<24} {:>2}-{:<2} {:>4.0}% {:>3}:{:<3}{}", m.map, m.record.won, m.record.lost, pct, m.rounds_for, m.rounds_against, if m.in_pool { "" } else { "  (not in pool)" });
            }
            println!("\nroster:");
            for r in t.roster.iter().take(15) {
                let rating = r.rating.map_or("-".to_string(), |x| format!("{x:.2} {} ({} games)", r.class.as_deref().unwrap_or(""), r.games));
                println!("  {:<22} {:>3} matches  {rating}", r.name.chars().take(22).collect::<String>(), r.matches);
            }
            Ok(())
        }

        ["league-sample", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let sources = Sources::new()?;
            let print_status = |st: &hl_ingest::league_sample::Status| {
                println!("{} logs listed, {:.0} MB held", st.logs_listed, st.bytes as f64 / 1e6);
                println!("  tier division         matches  logs  json(logs.tf/more.tf)  raw(ok/missing)  maps  rosters");
                for t in &st.tiers {
                    println!(
                        "  {:>4} {:<16} {:>4}/{}  {:>5}  {:>6} / {:<6}          {:>5} / {:<5}     {:>3}   {:>4}",
                        t.tier, t.division, t.matches, st.target_per_tier, t.logs, t.json_logstf, t.json_moretf, t.raw, t.raw_missing, t.maps, t.rosters
                    );
                }
            };
            match rest {
                ["select"] => {
                    // Choose the sample again from what is listed, no requests.
                    let (m, l) = hl_ingest::league_sample::select(&db).await?;
                    println!("picked {m} matches ({l} logs)");
                }
                ["discover"] => {
                    let d = hl_ingest::league_sample::discover(&db, &sources, |what| eprintln!("{what}")).await?;
                    println!("{} competitions, {} results, {} logs listed; picked {} matches ({} logs)", d.competitions, d.results, d.logs_listed, d.picked_matches, d.picked_logs);
                }
                ["run", n] => {
                    let n: usize = n.parse()?;
                    for i in 0..n {
                        let s = hl_ingest::league_sample::step(&db, &sources, |what| eprintln!("  {what}")).await?;
                        println!("{i:>4} {}", serde_json::to_string(&s)?);
                        if matches!(s, hl_ingest::league_sample::Step::Done) {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
                _ => {}
            }
            print_status(&hl_ingest::league_sample::status(&db, &sources).await?);
            Ok(())
        }

        ["catalogue", query @ ..] => {
            // Q35: search the player catalogue; the first hit's profile.
            let db = Db::connect(&db_path).await?;
            let sources = Sources::new()?;
            let n = hl_ingest::catalogue::index_league_players(&db, 100_000).await?;
            if n > 0 {
                eprintln!("indexed {n} league logs' players");
            }
            let hits = hl_ingest::catalogue::search(&db, &query.join(" "), 10).await?;
            for h in &hits {
                println!(
                    "  {:<22} {:<12} {:<9} {}g {}s {}b  {} officials",
                    h.name.chars().take(22).collect::<String>(),
                    h.highest.as_ref().map_or("-".into(), |d| d.name.clone()),
                    h.main_class.as_deref().unwrap_or("-"),
                    h.medals[0], h.medals[1], h.medals[2],
                    h.officials
                );
            }
            if let Some(h) = hits.first() {
                let p = hl_ingest::catalogue::profile(&db, &sources, h.account_id).await?;
                println!("
{} ({}) {} · main {:?} declared {:?} · highest {:?}", p.name, p.steamid64, p.country.as_deref().unwrap_or("?"), p.main_class, p.declared_classes, p.highest.as_ref().map(|d| &d.name));
                println!("aka {}", p.aliases.join(", "));
                for m in &p.medals {
                    println!("  medal {} S{} {} with {} ({})", ["gold", "silver", "bronze"][usize::from(m.place - 1)], m.season, m.division, m.team.name, m.how);
                }
                for s in &p.seasons {
                    println!("  S{:<3} {:<12} {:<24} {:>2}-{:<2} of {:>2}{}", s.season, s.division, s.team.name.chars().take(24).collect::<String>(), s.won, s.lost, s.played, s.place.map_or(String::new(), |p| format!("  #{p}")));
                }
                let st = hl_ingest::catalogue::player_stats(&db, h.account_id).await?;
                for c in &st.classes {
                    println!(
                        "  {:<9} {:>4} games  career {:.2}  recent {}  best {:.2}  bars{}: {}",
                        c.class, c.games, c.career, c.recent.map_or("-".into(), |r| format!("{r:.2}")), c.best,
                        if c.groups_recent { " (recent)" } else { "" },
                        c.groups.iter().map(|(g, v)| format!("{g} {v:.0}")).collect::<Vec<_>>().join(", ")
                    );
                }
                for r in &st.ranks {
                    println!("  #{} of {} {} in {} S{} ({:.2} over {} games)", r.rank, r.of, r.class, r.division, r.season, r.avg, r.games);
                }
                println!("  {} officials; latest: {}", p.officials.len(), p.officials.iter().take(3).map(|o| format!("{} vs {} {}-{}", o.team.name, o.opponent.name, o.score_for.unwrap_or(0), o.score_against.unwrap_or(0))).collect::<Vec<_>>().join("; "));
            }
            Ok(())
        }

        ["divisions", log_id] => {
            // Q38: everyone's ETF2L division at the time of a match.
            let db = Db::connect(&db_path).await?;
            let d = hl_ingest::catalogue::match_divisions(&db, log_id.parse()?).await?;
            let names = db.log_names().await?;
            let mut rows: Vec<_> = d.players.iter().collect();
            rows.sort_by_key(|(_, p)| p.tier);
            for (a, p) in rows {
                println!("  {:<22} {:<12} S{}{}", names.get(a).and_then(|n| n.first()).map_or("?", |n| n.as_str()), p.division, p.season, if p.exact { "" } else { " (nearest)" });
            }
            println!("{} of the players have a division", d.players.len());
            Ok(())
        }

        ["fixtures", rest @ ..] => {
            // Every upcoming Highlander official, with its head-to-head.
            let db = Db::connect(&db_path).await?;
            let list = hl_ingest::fixtures::upcoming(&db, &Sources::new()?).await?;
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&list)?);
            } else {
                for f in &list {
                    println!("  {} {:<28} {:<24} vs {:<24} h2h {}-{} ({})", fmt_date(f.time), f.competition, f.clan1.name, f.clan2.name, f.h2h.won, f.h2h.lost, f.round.as_deref().unwrap_or(""));
                }
            }
            Ok(())
        }

        ["h2h", a, b, rest @ ..] => {
            // Every official between two teams.
            let db = Db::connect(&db_path).await?;
            let h = hl_ingest::fixtures::head_to_head(&db, a.parse()?, b.parse()?).await?;
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&h)?);
            } else {
                println!("{}-{}-{} in {}", h.record.won, h.record.lost, h.record.drawn, h.record.played);
                for m in &h.matches {
                    println!("  S{:<4} {:<12} {:<20} {:?}-{:?}", m.season, m.division.as_deref().unwrap_or(""), m.round.as_deref().unwrap_or(""), m.score_for, m.score_against);
                }
            }
            Ok(())
        }

        ["team-info", id, rest @ ..] => {
            // A team as ETF2L's API has it: tag, links, former names, roles, cups.
            let db = Db::connect(&db_path).await?;
            let i = hl_ingest::team_info::team_info(&db, &Sources::new()?, id.parse()?).await?;
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&i)?);
            } else {
                println!("tag {:?}  homepage {:?}  steam group {:?}", i.tag, i.homepage, i.steam_group);
                for n in &i.former_names {
                    println!("  was {} until {}", n.from, fmt_date(n.time));
                }
                for m in &i.members {
                    println!("  {:<24} {}", m.name, m.role);
                }
                for c in &i.cups {
                    println!("  cup {:<48} {:<10} {}", c.name, c.division.as_deref().unwrap_or("-"), c.place.as_deref().unwrap_or(""));
                }
            }
            Ok(())
        }

        ["transfers", "team", id, "--json"] => {
            let db = Db::connect(&db_path).await?;
            let t = hl_ingest::transfers::team_transfers(&db, &Sources::new()?, id.parse()?).await?;
            println!("{}", serde_json::to_string(&t)?);
            Ok(())
        }

        ["transfers", "player", who, "--json"] => {
            let db = Db::connect(&db_path).await?;
            let account = hl_core::SteamId::parse(who).map(|s| s.account_id()).or_else(|_| who.parse::<u32>())?;
            println!("{}", serde_json::to_string(&hl_ingest::transfers::player_teams(&db, &Sources::new()?, account).await?)?);
            Ok(())
        }

        ["transfers", "team", id] => {
            // A team's ETF2L transfers, read when due, and its roster history.
            let db = Db::connect(&db_path).await?;
            let sources = Sources::new()?;
            let t = hl_ingest::transfers::team_transfers(&db, &sources, id.parse()?).await?;
            let day = fmt_date;
            println!("{} transfers; stays:", t.rows.len());
            for s in &t.stays {
                println!("  {:<24} {:>10} - {:<10}", s.name, s.from.map(day).unwrap_or("?".into()), s.to.map(day).unwrap_or("now".into()));
            }
            for r in t.rows.iter().take(15) {
                println!("  {} {:<6} {:<24} {}", day(r.time), if r.joined { "joined" } else { "left" }, r.name, r.by.as_deref().map(|b| format!("by {b}")).unwrap_or_default());
            }
            Ok(())
        }

        ["transfers", "player", who] => {
            // A player's teams with dates.
            let db = Db::connect(&db_path).await?;
            let sources = Sources::new()?;
            let account = hl_core::SteamId::parse(who).map(|s| s.account_id()).or_else(|_| who.parse::<u32>())?;
            let day = fmt_date;
            for s in hl_ingest::transfers::player_teams(&db, &sources, account).await? {
                println!("  {:<28} {:<20} {:>10} - {:<10}", s.team_name, s.team_type.unwrap_or_default(), s.from.map(day).unwrap_or("?".into()), s.to.map(day).unwrap_or("now".into()));
            }
            Ok(())
        }

        ["transfers", "backfill", rest @ ..] => {
            // Read the transfers of teams never read, medal winners first.
            let limit: usize = rest.first().map(|n| n.parse()).transpose()?.unwrap_or(usize::MAX);
            let db = Db::connect(&db_path).await?;
            let sources = Sources::new()?;
            let order = hl_ingest::transfers::backfill_order(&db).await?;
            println!("{} teams to read", order.len());
            for (i, team) in order.into_iter().take(limit).enumerate() {
                let n = hl_ingest::transfers::fetch_team(&db, &sources, team).await?;
                println!("  {:>4}. team {team}: {n} transfers", i + 1);
            }
            Ok(())
        }

        ["transfers", "medals"] => {
            // Medals withheld because the player had left before the team's
            // last match of the season.
            let db = Db::connect(&db_path).await?;
            let cat = hl_ingest::catalogue::Catalogue::load(&db).await?;
            let teams = db.etf2l_teams().await?;
            let withheld = cat.withheld_medals();
            for (season, team, _, name) in &withheld {
                println!("  S{season:<4} {:<28} {name}", teams.get(team).map(|t| t.0.as_str()).unwrap_or("?"));
            }
            println!("{} medals withheld", withheld.len());
            Ok(())
        }

        ["medals", season] => {
            // Every medal of one season, per division.
            let db = Db::connect(&db_path).await?;
            for (division, medals) in hl_ingest::catalogue::season_medals(&db, season.parse()?).await? {
                for (place, team, how) in medals {
                    println!("  {division:<12} {place}  {team:<32} {how}");
                }
            }
            Ok(())
        }

        ["medals", rest @ ..] => {
            // Every medal of every season and division, for checking them
            // (Flashy's audit, 1 Oct 2026).
            let db = Db::connect(&db_path).await?;
            let teams = db.etf2l_teams().await?;
            let cat = hl_ingest::catalogue::Catalogue::load(&db).await?;
            let only: Option<i64> = rest.first().and_then(|s| s.parse().ok());
            for ((season, division), list) in cat.medals() {
                if only.is_some_and(|s| s != season) {
                    continue;
                }
                let names: Vec<String> = list.iter().map(|(p, t, how)| format!("{p}: {} [{how}]", teams.get(t).map_or("?", |x| x.0.as_str()))).collect();
                println!("S{season} {division}: {}", names.join(" | "));
            }
            Ok(())
        }

        ["season-tiles", rest @ ..] => {
            // The Teams tab's season tiles; --banners looks for each banner.
            let db = Db::connect(&db_path).await?;
            let sources = Sources::new()?;
            for s in hl_ingest::catalogue::seasons_overview(&db).await? {
                let banner = if rest.contains(&"--banners") { hl_ingest::catalogue::season_banner(&db, &sources, s.season, &s.season_name).await? } else { None };
                println!(
                    "S{:<4} {:<14} {} divisions, {} teams, {} matches; champion {} ({}); you {}{}",
                    s.season,
                    s.season_name,
                    s.divisions.len(),
                    s.teams,
                    s.matches,
                    s.champion.as_ref().map_or("-", |t| t.name.as_str()),
                    s.champion_division.as_deref().unwrap_or("-"),
                    s.you.as_ref().map_or("-".to_string(), |y| format!("{} {} {:?}", y.team.name, y.division, y.place)),
                    banner.map(|b| format!("
      {b}")).unwrap_or_default()
                );
            }
            Ok(())
        }

        ["finals"] => {
            // Every league Grand Final and whether its logs are held.
            let db = Db::connect(&db_path).await?;
            let teams = db.etf2l_teams().await?;
            let cat = hl_ingest::catalogue::Catalogue::load(&db).await?;
            let held = db.league_logs_by_match().await?;
            let (mut ok, mut missing) = (0, 0);
            for (season, division, m) in cat.finals() {
                let (logs, json, raw) = held.get(&m.match_id).copied().unwrap_or((0, 0, 0));
                if json > 0 { ok += 1 } else { missing += 1 }
                let name = |t: i64| teams.get(&t).map_or("?".to_string(), |x| x.0.chars().take(22).collect());
                println!(
                    "{} S{:<4} {:<12} {:>7} {:<22} vs {:<22} logs {:>2} json {:>2} raw {:>2}",
                    if json > 0 { "  " } else { "!!" },
                    season, division, m.match_id, name(m.clan1), name(m.clan2), logs, json, raw
                );
            }
            println!("{ok} finals with logs, {missing} without");
            Ok(())
        }

        ["mvp", rest @ ..] => {
            // Each event's MVPs: per class from the Grand Final, and the event's.
            let db = Db::connect(&db_path).await?;
            let cat = hl_ingest::catalogue::Catalogue::load(&db).await?;
            let names = db.log_names().await?;
            let rated = db.rated_games(hl_rating::MODEL_VERSION, None).await?;
            let only: Option<i64> = rest.first().and_then(|s| s.parse().ok());
            for m in cat.mvps(&rated) {
                if only.is_some_and(|s| s != m.season) {
                    continue;
                }
                let who = names.get(&m.account_id).and_then(|n| n.first().cloned()).unwrap_or_else(|| m.account_id.to_string());
                println!(
                    "S{:<4} {:<12} {:<12} {:<20} {:<24} {:.2}  (final {:.2} over {}{})",
                    m.season,
                    m.division,
                    if m.event { "EVENT MVP".to_string() } else { m.class.clone() },
                    who.chars().take(20).collect::<String>(),
                    m.team.name.chars().take(24).collect::<String>(),
                    m.score,
                    m.final_rating,
                    m.final_maps,
                    m.playoffs_rating.map(|p| format!(", playoffs {p:.2} over {}", m.playoffs_maps)).unwrap_or_default()
                );
            }
            Ok(())
        }

        ["drops-check", ids @ ..] => {
            // The same logs from drops.tf and logs.tf, through the app's own
            // code: JSON and raw server log, compared.
            let sources = Sources::new()?;
            for id in ids {
                let id: i64 = id.parse()?;
                let drops: Option<serde_json::Value> = sources.drops_log(id).await?.map(|b| serde_json::from_str(&b)).transpose()?;
                let logstf: serde_json::Value = serde_json::from_str(&sources.logstf_log(id).await?)?;
                let raw_d = sources.drops_rawlog(id).await?.map(|z| hl_ingest::rawlog::unzip(&z)).transpose()?;
                let raw_l = sources.logstf_rawlog(id).await?.map(|z| hl_ingest::rawlog::unzip(&z)).transpose()?;
                println!(
                    "  log {id}: json {} | raw log {}",
                    match &drops { None => "not on drops.tf".to_string(), Some(d) => (if same_json(d, &logstf) { "identical" } else { "DIFFERENT" }).to_string() },
                    match (&raw_d, &raw_l) {
                        (None, _) => "not on drops.tf".to_string(),
                        (Some(a), Some(b)) => format!("{} ({} bytes)", if a == b { "identical" } else { "DIFFERENT" }, a.len()),
                        (Some(a), None) => format!("only on drops.tf ({} bytes)", a.len()),
                    }
                );
            }
            Ok(())
        }

        ["league-search", rest @ ..] => {
            // Look on logs.tf for officials trends.tf linked no log to.
            let n: i64 = rest.first().map(|n| n.parse()).transpose()?.unwrap_or(10);
            let db = Db::connect(&db_path).await?;
            let sources = Sources::new()?;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
            let todo = db.league_matches_without_logs(now - 6 * 365 * 86400, now - 86400, n).await?;
            println!("{} officials to search", todo.len());
            for (match_id, time, a, b) in todo {
                let found = hl_ingest::league_sample::search_match_logs(&db, &sources, match_id, time, a as u32, b as u32).await?;
                println!("  match {match_id} ({}): {found} logs", fmt_date(time));
            }
            Ok(())
        }

        ["buildings", log_id] => {
            // Every building a match's demos saw (Emiel's ticket).
            let db = Db::connect(&db_path).await?;
            let b = hl_ingest::buildings::for_log(&db, log_id.parse()?).await?;
            println!("{} buildings", b.len());
            for x in &b {
                println!("  {:<10} {:<9} team {:?} lvl {} {:<16} at ({}, {}) ticks {}-{}", x.kind, x.end.as_deref().unwrap_or("-"), x.team, x.level, x.builder_name.as_deref().unwrap_or("?"), x.x, x.y, x.from_tick, x.to_tick);
            }
            Ok(())
        }

        ["lives", account, rest @ ..] => {
            // Q57: a player's lives on one map; with no map, the maps.
            let db = Db::connect(&db_path).await?;
            let account: u32 = match account.parse() {
                Ok(a) => a,
                Err(_) => SteamId::parse(account)?.account_id(),
            };
            match rest.first() {
                None => {
                    for m in hl_ingest::lives::maps(&db, account).await? {
                        println!("  {:<14} {:>3} matches {:>5} lives  ({})", m.map, m.matches, m.lives, m.name);
                    }
                }
                Some(map) => {
                    let v = hl_ingest::lives::on_map(&db, account, map).await?;
                    println!("{}: {} lives in {} matches ({} with an STV)", v.map, v.lives.len(), v.matches, v.stv_matches);
                    let n = |f: fn(&hl_ingest::lives::LifeOnMap) -> bool| v.lives.iter().filter(|l| f(l)).count();
                    println!(
                        "  died {}, round end {}, with a cap {}, after the other team capped {}, mid-fight {}",
                        n(|l| l.died),
                        n(|l| !l.died),
                        n(|l| l.capped),
                        n(|l| l.after_enemy_cap),
                        n(|l| l.mid_fight)
                    );
                    let mut by: std::collections::BTreeMap<(String, String), usize> = std::collections::BTreeMap::new();
                    for l in &v.lives {
                        *by.entry((l.class.clone().unwrap_or("?".into()), l.team.clone().unwrap_or("?".into()))).or_default() += 1;
                    }
                    for ((c, t), n) in by {
                        println!("  {c:<9} {t:<5} {n}");
                    }
                }
            }
            Ok(())
        }

        ["rankings", class, rest @ ..] => {
            // Q36: one season's ranking of a class in a division.
            let db = Db::connect(&db_path).await?;
            let tier = rest.first().and_then(|t| t.parse().ok());
            let season = rest.get(1).and_then(|s| s.parse().ok());
            let r = hl_ingest::catalogue::rankings(&db, season, tier, class).await?;
            println!("{} in {} (tier {}), S{} {}: {} ranked", r.class, r.division, r.tier, r.season, r.season_name, r.rows.len());
            for x in r.rows.iter().take(20) {
                println!("  #{:<3} {:<22} {:<24} {:.2} over {}", x.rank, x.name.chars().take(22).collect::<String>(), x.team.as_ref().map_or(String::new(), |t| t.name.chars().take(24).collect()), x.avg, x.games);
            }
            Ok(())
        }

        ["guide", rest @ ..] => {
            let (w, warning) = hl_rating::Weights::load(&db_path.with_file_name("weights.toml"));
            if let Some(w) = warning {
                eprintln!("warning: {w}");
            }
            let g = hl_rating::guide::guide(&w);
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string_pretty(&g)?);
                return Ok(());
            }
            println!("model {} -- 1.00 is a typical game, one standard deviation is {:.2}", g.model_version, g.rating_spread);
            for c in &g.classes {
                println!("
{}{}", c.class, if c.own_model { "" } else { " (generic model)" });
                for x in &c.components {
                    println!("  {:>4.0}%  {}", x.share * 100.0, x.label);
                }
            }
            Ok(())
        }

        ["tendencies", class] => {
            // Q28: the owner's habits per map on a class, over every match.
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let class = TfClass::parse(class)?;
            let data = db_path.parent().context("no data folder")?.to_path_buf();
            for m in hl_ingest::callouts::tendencies(&db, &data, me.account_id(), class.as_str()).await? {
                println!("{}{} -- {} matches, {} STVs ({} min alive)", m.map, if m.draft { " (draft)" } else { "" }, m.matches, m.stvs, m.alive_s / 60);
                let fights: u32 = m.fights.iter().map(|f| f.kills + f.deaths).sum::<u32>() + m.unzoned;
                for f in m.fights.iter().take(8) {
                    println!("  fights  {:<18} {:>4} kills {:>4} deaths  {:>3.0}%", f.zone, f.kills, f.deaths, 100.0 * f64::from(f.kills + f.deaths) / f64::from(fights.max(1)));
                }
                for z in m.time.iter().take(6) {
                    println!("  stands  {:<18} {:>3.0}%", z.zone, 100.0 * z.share);
                }
                for p in m.paths.iter().take(5) {
                    println!("  moves   {} -> {} ({}x)", p.from, p.to, p.times);
                }
            }
            Ok(())
        }

        ["positions", log_id] => {
            // Q28: where each player spent their live time, by callout.
            let db = Db::connect(&db_path).await?;
            let log_id: i64 = log_id.parse()?;
            let data = db_path.parent().context("no data folder")?.to_path_buf();
            let json: serde_json::Value = serde_json::from_str(&db.raw_log(log_id).await?.context("no such match")?)?;
            let map = json.pointer("/info/map").and_then(|m| m.as_str()).unwrap_or_default().to_string();
            let Some(v) = hl_ingest::callouts::positions(&db, &data, log_id, &map).await? else {
                println!("{map}: no STV timeline, or no zones drawn for this map");
                return Ok(());
            };
            const CLASSES: [&str; 10] = ["?", "scout", "sniper", "soldier", "demoman", "medic", "heavy", "pyro", "spy", "engineer"];
            println!("{} · {} zones{}", v.map, v.zones, if v.draft { " (draft)" } else { "" });
            for p in &v.players {
                let top: Vec<String> = p.zones.iter().take(4).map(|z| format!("{} {:.0}%", z.zone, 100.0 * f64::from(z.seconds) / f64::from(p.alive_s.max(1)))).collect();
                println!("  {:<4} {:<9} {:<20} {}", if p.team == 2 { "RED" } else { "BLU" }, CLASSES[usize::from(p.class)], p.name.chars().take(20).collect::<String>(), top.join(", "));
            }
            Ok(())
        }

        ["spychecks", path, rest @ ..] => {
            // Q27's spike: hits on fully cloaked Spies in one demo, listed
            // with the demo's own tick so each can be checked in game with
            // `demo_gototick`. A file on disk only; no database.
            let file = std::path::Path::new(path);
            let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
            let header = hl_demos::DemoHeader::parse(&bytes)?;
            drop(bytes);
            let rate = header.tick_rate().unwrap_or(66.67);
            let (_, stored) = hl_demos::aim::pass_recording(file, "", rate, Some(hl_demos::timeline::DEFAULT_STRIDE), &mut |_| {})?;
            let tl = hl_demos::timeline::Timeline::decode(&stored.context("no timeline")?)?;
            let found = hl_demos::spy::spychecks(&tl);
            let s = found.skipped;
            println!(
                "{} · {} spychecks · not counted: {} fading in, {} blinking, {} marked (fire, jarate, milk, bleed), {} within the cooldown",
                header.map,
                found.checks.len(),
                s.fading,
                s.blinking,
                s.marked,
                s.cooldown
            );
            let name = |slot: usize| tl.people[slot].name.chars().take(20).collect::<String>();
            let mut rows: Vec<(usize, usize, usize)> = (0..tl.people.len()).map(|p| (p, found.by(p), found.on(p))).filter(|r| r.1 + r.2 > 0).collect();
            rows.sort_by_key(|r| std::cmp::Reverse((r.1, r.2)));
            println!("\n{:<21} {:>6} {:>6}", "player", "checks", "found");
            for (p, by, on) in rows {
                println!("{:<21} {by:>6} {on:>6}", name(p));
            }
            if rest.contains(&"--list") {
                println!("\n{:>8} {:>7}  {:<21} {:<21} {:>4}", "tick", "time", "by", "spy", "dmg");
                for c in &found.checks {
                    let secs = tl.seconds(c.t) as u32;
                    println!(
                        "{:>8} {:>4}:{:02}  {:<21} {:<21} {:>4}{}",
                        tl.tick_of(c.t),
                        secs / 60,
                        secs % 60,
                        name(c.attacker),
                        name(c.spy),
                        c.damage,
                        if c.killed { "  killed" } else { "" }
                    );
                }
            }
            Ok(())
        }

        ["kept", rest @ ..] => {
            // Q3: what the database holds of demos, read without the files.
            use hl_demos::timeline::{GameEvent, PlayerCondition as C, Stored, Timeline};
            let db = Db::connect(&db_path).await?;
            let totals = db.timeline_totals().await?;
            println!(
                "{} demos kept as timelines, {:.1} MB; {} of them without their file",
                totals.demos,
                totals.stored_bytes as f64 / 1e6,
                totals.file_gone
            );
            let Some(id) = rest.first().and_then(|s| s.parse::<i64>().ok()) else {
                for (id, file, bytes, gone) in db.timeline_list().await? {
                    println!("{id:>6}  {:>6.1} MB  {file}{}", bytes as f64 / 1e6, if gone { "  (file gone)" } else { "" });
                }
                println!("Pass a demo id to read one back.");
                return Ok(());
            };
            let row = db.timeline(id).await?.with_context(|| format!("demo {id} has no timeline"))?;
            let started = std::time::Instant::now();
            let tl = Timeline::decode(&Stored {
                version: row.version,
                tick_rate: row.tick_rate,
                stride: row.stride as u32,
                head: row.head,
                samples: row.samples,
                changes: row.changes,
                objects: row.objects,
                events: row.events,
                raw_bytes: row.raw_bytes as usize,
            })?;
            println!(
                "demo {id}: {} people, {:.0} min, read back in {:.2}s",
                tl.people.len(),
                tl.seconds(tl.end()) / 60.0,
                started.elapsed().as_secs_f64()
            );

            // None of these is measured anywhere else in the app: each is a
            // few lines over the stored timeline, which is the point.
            const CLASSES: [&str; 10] = ["?", "scout", "sniper", "soldier", "demoman", "medic", "heavy", "pyro", "spy", "engineer"];
            let pops = |slot: usize| {
                tl.events
                    .iter()
                    .filter(|(_, e)| matches!(e, GameEvent::PlayerChargeDeployed(c) if tl.slot_of_user(c.user_id) == Some(slot)))
                    .count()
            };
            println!(
                "\n{:<20} {:<9} {:>6} {:>7} {:>7} {:>7} {:>7} {:>5}",
                "player", "class", "alive", "ubered", "burning", "scoped", "healing", "pops"
            );
            for (slot, p) in tl.people.iter().enumerate() {
                let live = tl.ticks_where(slot, |_| true);
                if live == 0 {
                    continue;
                }
                let class = (1u8..=9).max_by_key(|c| tl.ticks_where(slot, |n| n.class == *c)).unwrap_or(0);
                let pct = |ticks: u32| format!("{:.0}%", 100.0 * f64::from(ticks) / f64::from(live));
                let name: String = p.name.chars().take(19).collect();
                println!(
                    "{:<20} {:<9} {:>5.1}m {:>6.0}s {:>6.0}s {:>7} {:>7} {:>5}",
                    name,
                    CLASSES[usize::from(class)],
                    tl.seconds(live) / 60.0,
                    tl.seconds(tl.ticks_where(slot, |n| n.has(C::Invulnerable))),
                    tl.seconds(tl.ticks_where(slot, |n| n.has(C::Burning))),
                    if class == 2 { pct(tl.ticks_where(slot, |n| n.has(C::Zoomed))) } else { "-".into() },
                    if class == 5 { pct(tl.ticks_where(slot, |n| n.heal_target.is_some())) } else { "-".into() },
                    if class == 5 { pops(slot).to_string() } else { "-".into() },
                );
            }

            // The cart: how far it went. A jump of more than a few hundred
            // units is a round resetting it, not the cart moving.
            let mut travelled = 0.0f64;
            let mut last: std::collections::HashMap<u32, [i32; 3]> = std::collections::HashMap::new();
            for o in tl.objects.iter().filter(|o| o.kind == "cart") {
                if let Some(p) = last.insert(o.entity, o.pos) {
                    let d = ((0..3).map(|k| f64::from(o.pos[k] - p[k]).powi(2)).sum::<f64>()).sqrt();
                    if d < 400.0 {
                        travelled += d;
                    }
                }
            }
            let caps = tl.events.iter().filter(|(_, e)| matches!(e, GameEvent::TeamPlayPointCaptured(_))).count();
            // Every movement, rolling back included: distance, not progress.
            // Progress is Q11's question, and needs the track, not just this.
            println!("\ncart travelled {travelled:.0} units, back and forth · {caps} points captured · {} events kept", tl.events.len());
            Ok(())
        }

        ["owner", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let o = if rest.contains(&"--refresh") {
                hl_ingest::owner::refresh(&db, &Sources::new()?, me).await?
            } else {
                hl_ingest::owner::load(&db, me).await?
            };
            println!("steamid64 {}", o.steamid64);
            println!("name      {}", o.name.as_deref().unwrap_or("(none)"));
            println!("avatar    {}", o.avatar.as_ref().map_or("(none)".to_string(), |a| format!("{} ({} chars)", &a[..a.len().min(30)], a.len())));
            Ok(())
        }

        ["seasons", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let class = TfClass::parse(rest.iter().find(|a| !a.starts_with("--")).copied().unwrap_or("sniper"))?;
            let v = hl_ingest::seasons::by_season(&db, me, class).await?;
            if rest.contains(&"--json") {
                println!("{}", serde_json::to_string(&v)?);
                return Ok(());
            }
            let date = |t: i64| fmt_date(t);
            println!("{:<26} {:<23} {:>5} {:>4} {:>6} {:>6} {:>5} {:>5} {:>6} {:>6}", "season", "dates", "games", "off", "W-L", "rating", "dpm", "k/d", "open%", "traded");
            let pct = |x: Option<f64>| x.map_or("-".to_string(), |v| format!("{:.0}%", v * 100.0));
            let num = |x: Option<f64>, d: usize| x.map_or("-".to_string(), |v| format!("{v:.d$}"));
            for r in &v.seasons {
                let s = &r.stats;
                println!(
                    "{:<26} {} – {} {:>5} {:>4} {:>6} {:>6} {:>5} {:>5} {:>6} {:>6}",
                    truncate(&r.season.name, 26),
                    date(r.season.from),
                    date(r.season.to),
                    s.games,
                    s.officials,
                    format!("{}-{}", s.wins, s.losses),
                    num(s.rating, 1),
                    num(s.dpm, 0),
                    num(s.kd, 2),
                    pct(s.opening_won),
                    pct(s.traded)
                );
            }
            let s = &v.all_time;
            println!("{:<26} {:<23} {:>5} {:>4} {:>6} {:>6} {:>5} {:>5} {:>6} {:>6}", "all time", "", s.games, s.officials, format!("{}-{}", s.wins, s.losses), num(s.rating, 1), num(s.dpm, 0), num(s.kd, 2), pct(s.opening_won), pct(s.traded));
            Ok(())
        }

        ["fights", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let started = std::time::Instant::now();
            let d = hl_ingest::fights::derive_all(&db, rest.contains(&"--all"), print_fight_progress).await?;
            println!("{} of {} logs read in {:.1}s", d.derived, d.total, started.elapsed().as_secs_f64());
            let class = rest.iter().find(|a| !a.starts_with("--")).copied().unwrap_or("sniper");
            if rest.contains(&"--json") {
                let card = hl_ingest::seasons::fights_card(&db, me, TfClass::parse(class)?, kind_flag(rest), None, None).await?;
                println!("{}", serde_json::to_string(&card)?);
                return Ok(());
            }
            let f = hl_db::FightFilter { class, model_version: hl_rating::MODEL_VERSION, kind: kind_flag(rest), from: None, to: None };
            let (mine, pool) = db.fight_totals(me.account_id(), &f).await?;
            println!("{class}: you {} games / {:.0} min, pool {} games / {:.0} min", mine.games, mine.minutes, pool.games, pool.minutes);
            println!("{:<20} {:>9} {:>9}   (per 10 min)", "", "you", "pool");
            for (i, c) in hl_db::FIGHT_COLUMNS.iter().enumerate() {
                let rate = |t: &hl_db::FightTotals| if t.minutes > 0.0 { t.values[i] as f64 / t.minutes * 10.0 } else { 0.0 };
                println!("{c:<20} {:>9.2} {:>9.2}", rate(&mine), rate(&pool));
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
            if let Some(out) = flag_value::<String>(rest, "--export-geometry")? {
                let min = flag_value::<u32>(rest, "--min")?.unwrap_or(500);
                let f = hl_ingest::maps::export_geometry(&db, min).await?;
                std::fs::write(&out, serde_json::to_string(&f)?)?;
                for (base, m) in &f.maps {
                    println!("  {base:<14} {:<22} {:>6} cells {:>8} positions", m.name, m.cells.len(), m.cells.iter().map(|c| c[2]).sum::<i64>());
                }
                println!("{} maps written to {out}", f.maps.len());
                return Ok(());
            }
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

        // Logs that would not import, and importing one by hand.
        ["failed"] => {
            let db = Db::connect(&db_path).await?;
            let rows = db.failed_logs().await?;
            if rows.is_empty() {
                println!("nothing failed to import");
                return Ok(());
            }
            println!("{} logs would not import:", rows.len());
            for r in &rows {
                println!(
                    "  {:>9}  {:<22} {:>2} tries, last {}
             {}",
                    r.log_id,
                    r.map.as_deref().unwrap_or("?"),
                    r.attempts,
                    r.last_attempt_at,
                    r.error
                );
            }
            println!("
Try one again: hl import <id or logs.tf link>");
            Ok(())
        }

        ["import", what] => {
            let db = Db::connect(&db_path).await?;
            let (weights, warning) = hl_rating::Weights::load(&db_path.with_file_name("weights.toml"));
            if let Some(w) = warning {
                eprintln!("warning: {w}");
            }
            let log_id = hl_ingest::parse_log_id(what)
                .with_context(|| format!("`{what}` is not a log id or a logs.tf link"))?;
            let sources = Sources::new()?;
            let got = hl_ingest::import_log(&db, &sources, &weights, log_id).await?;
            println!(
                "imported {} — {} on {}, {} players{}",
                got.log_id,
                got.title.as_deref().unwrap_or("untitled"),
                got.map.as_deref().unwrap_or("?"),
                got.players,
                if got.yours { "" } else { " (you are not in it: it joins the pool, not your matches)" }
            );
            if let Some(combined) = got.part_of {
                println!("it is part of combined log {combined}, which is what the match list shows");
            }
            Ok(())
        }

        // Q14: look someone else up.
        ["who", rest @ ..] => {
            let db = Db::connect(&db_path).await?;
            let me = db.get_me().await?.context("no owner set")?;
            let query = rest.iter().find(|a| !a.starts_with("--")).context("who <name or steamid>")?;
            let hits = db.search_players(query, 25).await?;
            if hits.is_empty() {
                println!("nobody matching `{query}` has played in your matches");
                return Ok(());
            }
            if hits.len() > 1 && !rest.contains(&"--first") {
                println!("{} players match `{query}`:", hits.len());
                for h in &hits {
                    println!(
                        "  {:>10}  {:<24} {:>4} games  {}",
                        h.account_id,
                        h.name.chars().take(24).collect::<String>(),
                        h.games,
                        h.top_class.as_deref().unwrap_or("")
                    );
                }
                println!("
Pick one: hl who <steamid>");
                return Ok(());
            }
            let hit = &hits[0];
            let s = db
                .player_summary(hit.account_id, me.account_id(), hl_rating::MODEL_VERSION)
                .await?
                .context("no games")?;
            println!("{} — {} games in your matches", s.name, s.games);
            if !s.also_known_as.is_empty() {
                println!("also known as {}", s.also_known_as.join(", "));
            }
            println!("{}  ({})", s.steamid64, s.account_id);
            println!(
                "with you {}, against you {} ({}-{} to you)",
                s.with_you, s.against_you, s.you_beat_them, s.they_beat_you
            );
            println!("
class       games  rating");
            for c in &s.classes {
                println!("{:<12} {:>4}   {:>5.2}", c.class, c.games, c.avg);
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
                from: None,
                to: None,
                limit,
                offset: 0,
                class: rest.iter().position(|a| *a == "--class").and_then(|i| rest.get(i + 1)).map(|s| s.to_string()),
                map: rest.iter().position(|a| *a == "--map").and_then(|i| rest.get(i + 1)).map(|s| s.to_string()),
                sort: rest.iter().position(|a| *a == "--sort").and_then(|i| rest.get(i + 1)).map(|s| s.to_string()),
                ascending: false,
                model_version: hl_rating::MODEL_VERSION.to_string(),
                stv_only: rest.contains(&"--stv"),
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
/// `YYYY-MM-DD` to unix seconds at midnight UTC.
fn parse_day(s: &str) -> Result<i64> {
    let mut it = s.split('-').map(str::parse::<i64>);
    let (Some(Ok(y)), Some(Ok(m)), Some(Ok(d)), None) = (it.next(), it.next(), it.next(), it.next()) else {
        anyhow::bail!("dates are YYYY-MM-DD, got `{s}`");
    };
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Ok((era * 146_097 + doe - 719_468) * 86_400)
}

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

/// `hl fights` runs the pass on its own, so it prints its own line.
fn print_fight_progress(done: usize, total: usize) {
    use std::io::Write;
    print!("\rfights {done}/{total}          ");
    let _ = std::io::stdout().flush();
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
        Progress::StandIns { done, total } => print!("\rfrom more.tf {done}/{total}          "),
        Progress::Stage { what } => print!("\r{what}...          "),
        Progress::Fights { done, total } => print!("\rfights {done}/{total}          "),
        Progress::ReadingDemos { done, total, log_id, .. } => match log_id {
            Some(id) => print!("\rreading demos {done}/{total}  (log {id})          "),
            None => print!("\rreading demos {done}/{total}          "),
        },
        Progress::SourceFailed { source, error } => println!("
  ! {source}: {error}"),
        Progress::GaveUp { source, done, total } => {
            println!("
  ! {source} stopped answering at {done} of {total}; the rest waits for the next sync")
        }
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

/// Two logs' JSON alike, numbers to one part in a billion: drops.tf
/// writes a few averages back with the last digit rounded differently
/// (9.533333333333331 for 9.533333333333333), nothing else.
fn same_json(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap_or(f64::NAN), y.as_f64().unwrap_or(f64::NAN));
            x == y || (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same_json(v, w))),
        _ => a == b,
    }
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
