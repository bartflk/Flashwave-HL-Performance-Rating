//! The league sample running in the background (Flashy): one step at a
//! time, for as long as the app is open and the sample is switched on.
//!
//! The pace is the point. logs.tf has banned this address for asking too
//! much, so a step is one request and the loop waits between steps; while
//! logs.tf is resting, more.tf gives the JSON instead; and while the owner's
//! own sync runs the sample waits, so their games come first. Everything it
//! has done is in the database: closing the app loses nothing but the wait.
//!
//! What it is doing right now is kept in [`Activity`], for the live bar in
//! Settings: without it a job that waits most of the time looks stuck.

use hl_db::Db;
use hl_ingest::league_sample::{self, Step};
use hl_ingest::sources::Sources;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Between two requests to logs.tf: with its own 2 s throttle, one about
/// every 6 s, ~600 an hour.
const STEP_GAP: Duration = Duration::from_secs(4);
/// After a log drops.tf gave: it has no limit to stay under, and answers
/// in a fraction of a second (its own 0.25 s throttle still applies).
const DROPS_GAP: Duration = Duration::from_secs(1);
/// Nothing to do but wait for logs.tf's rest to end.
const WAITING_GAP: Duration = Duration::from_secs(60);
/// A request that failed: back off before the next.
const FAILED_GAP: Duration = Duration::from_secs(30);
/// Everything picked is here: look again in a while for new officials.
const DONE_GAP: Duration = Duration::from_secs(30 * 60);
/// Switched off, or the owner's sync is running.
const IDLE_GAP: Duration = Duration::from_secs(20);
/// Actions kept for the list under the bar.
const RECENT: usize = 12;

/// One thing the job did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub at: i64,
    pub text: String,
    pub ok: bool,
}

/// What the job is doing, for the live bar.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    /// `starting`, `working`, `waiting` (between requests), `resting`
    /// (logs.tf asked us to slow down), `sync` (the owner's sync has the
    /// line), `paused`, or `done`.
    pub state: &'static str,
    /// The request in flight, while there is one.
    pub doing: Option<String>,
    /// When the next step starts, unix seconds.
    pub next_at: Option<i64>,
    /// Requests made in the last hour.
    pub last_hour: usize,
    /// Seconds left of logs.tf's rest.
    pub logstf_rest_left: Option<u64>,
    /// Newest first.
    pub recent: VecDeque<Event>,
    #[serde(skip)]
    stamps: VecDeque<i64>,
}

pub type SharedActivity = Arc<Mutex<Activity>>;

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

impl Activity {
    fn log(&mut self, text: String, ok: bool) {
        self.recent.push_front(Event { at: now(), text, ok });
        self.recent.truncate(RECENT);
    }

    fn request(&mut self) {
        let t = now();
        self.stamps.push_back(t);
        while self.stamps.front().is_some_and(|s| *s < t - 3600) {
            self.stamps.pop_front();
        }
        self.last_hour = self.stamps.len();
    }
}

/// The activity as the window should see it, with logs.tf's rest filled in.
pub fn snapshot(activity: &SharedActivity, sources: &Sources) -> Activity {
    let mut a = activity.lock().unwrap().clone();
    a.logstf_rest_left = sources.logstf_rest_left();
    let t = now();
    a.last_hour = a.stamps.iter().filter(|s| **s >= t - 3600).count();
    a
}

/// Sample logs rated per turn of the loop: ~10 ms each, and the first
/// turn after a long download catches up a few hundred at a time.
const RATE_BATCH: i64 = 200;

/// ETF2L's transfers for every team (Q48), the medal winners first: each
/// team's list in turn at the ETF2L client's pace, then the teams still
/// playing again every six hours. Dev builds only, like the sample; a
/// release has them from the league snapshot and reads a team's when its
/// page is opened.
pub fn spawn_transfers(db: Db, sources: Arc<Sources>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(90)).await;
        loop {
            let mut teams = hl_ingest::transfers::backfill_order(&db).await.unwrap_or_else(|e| {
                tracing::warn!(error = %format!("{e:#}"), "listing the teams to read transfers for failed");
                Vec::new()
            });
            for team in hl_ingest::transfers::stale_active_teams(&db).await.unwrap_or_default() {
                if !teams.contains(&team) {
                    teams.push(team);
                }
            }
            let (mut read, mut added) = (0, 0);
            for team in teams {
                match hl_ingest::transfers::fetch_team(&db, &sources, team).await {
                    Ok(n) => {
                        read += 1;
                        added += n;
                    }
                    Err(e) => tracing::warn!(team, error = %format!("{e:#}"), "reading a team's transfers failed"),
                }
                // Room between teams for the requests you make yourself:
                // at 2 s ETF2L turned one away about once a minute.
                tokio::time::sleep(Duration::from_secs(8)).await;
            }
            if read > 0 {
                tracing::info!(teams = read, transfers = added, "ETF2L transfers read");
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

/// The 6v6 league sample's own loop (the 6v6 plan, P5): idle until it is
/// switched on (on the ETF2L tab in 6v6), and only in a build with 6v6; a
/// step at a time with the same gaps as Highlander's, never during a sync.
/// When a batch of logs has come in, the 6v6 rating preview is rebuilt so
/// its pools hold them.
pub fn spawn_sixes(db: Db, sources: Arc<Sources>, busy: Arc<AtomicBool>) {
    if !hl_core::format::SIXES_ENABLED {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(45)).await;
        let mut since_rating = 0;
        loop {
            let on = hl_ingest::sixes_sample::enabled(&db).await.unwrap_or(false);
            let gap = if !on || busy.load(Ordering::Acquire) {
                IDLE_GAP
            } else {
                match hl_ingest::sixes_sample::step(&db, &sources).await {
                    Ok(hl_ingest::sixes_sample::Step::Discovered { listed, competitions }) => {
                        tracing::info!(listed, competitions, "6v6 league sample listed");
                        STEP_GAP
                    }
                    Ok(hl_ingest::sixes_sample::Step::Json { .. }) => {
                        since_rating += 1;
                        DROPS_GAP
                    }
                    Ok(hl_ingest::sixes_sample::Step::Missing { log_id }) => {
                        tracing::debug!(log_id, "6v6 sample log not on drops.tf");
                        DROPS_GAP
                    }
                    Ok(hl_ingest::sixes_sample::Step::Raw { .. }) => {
                        since_rating += 1;
                        DROPS_GAP
                    }
                    Ok(hl_ingest::sixes_sample::Step::Failed { what }) => {
                        tracing::warn!(%what, "6v6 league sample step failed");
                        FAILED_GAP
                    }
                    Ok(hl_ingest::sixes_sample::Step::Done) => DONE_GAP,
                    Err(e) => {
                        tracing::warn!(error = %format!("{e:#}"), "6v6 league sample step failed");
                        FAILED_GAP * 4
                    }
                }
            };
            // The preview's pools follow the sample, every 200 logs or when done.
            if since_rating >= 200 || (since_rating > 0 && gap == DONE_GAP) {
                since_rating = 0;
                if !busy.load(Ordering::Acquire) {
                    // The round-win table first, once there are enough rounds.
                    match hl_ingest::sixes_sample::measure_round_win(&db).await {
                        Ok(Some(t)) => tracing::info!(rounds = t.rounds, kills = t.kills, "6v6 round-win table measured"),
                        Ok(None) => {}
                        Err(e) => tracing::warn!(error = %format!("{e:#}"), "6v6 round-win table failed"),
                    }
                    match hl_ingest::sixes_rating::rate_all(&db).await {
                        Ok(n) => tracing::info!(ratings = n, "6v6 preview rebuilt on the league sample"),
                        Err(e) => tracing::warn!(error = %format!("{e:#}"), "6v6 preview rating failed"),
                    }
                }
            }
            tokio::time::sleep(gap).await;
        }
    });
}

pub fn spawn(db: Db, sources: Arc<Sources>, busy: Arc<AtomicBool>, activity: SharedActivity, weights_path: std::path::PathBuf) {
    activity.lock().unwrap().state = "starting";
    tauri::async_runtime::spawn(async move {
        // Let the startup passes have the database first.
        activity.lock().unwrap().next_at = Some(now() + 30);
        tokio::time::sleep(Duration::from_secs(30)).await;
        // Each sample log's players and main classes, for the profiles.
        match hl_ingest::catalogue::index_league_players(&db, 100_000).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(logs = n, "league players indexed"),
            Err(e) => tracing::warn!(error = %format!("{e:#}"), "indexing league players failed"),
        }
        // Sample logs rated as they settle, between full passes; the ones
        // with nothing rateable are not read again this run.
        let mut tried = std::collections::HashSet::new();
        loop {
            if !busy.load(Ordering::Acquire) {
                let (weights, _) = hl_rating::Weights::load(&weights_path);
                match hl_ingest::league_rating::rate_new(&db, &weights, &mut tried, RATE_BATCH).await {
                    Ok(0) => {}
                    Ok(n) => tracing::debug!(logs = n, "league sample logs rated"),
                    Err(e) => tracing::warn!(error = %format!("{e:#}"), "rating new league sample logs failed"),
                }
            }
            let (state, gap) = if !league_sample::enabled(&db).await.unwrap_or(false) {
                ("paused", IDLE_GAP)
            } else if busy.load(Ordering::Acquire) {
                ("sync", IDLE_GAP)
            } else {
                {
                    let mut a = activity.lock().unwrap();
                    a.state = "working";
                    a.doing = Some("Deciding what to fetch next".into());
                    a.next_at = None;
                }
                let act = activity.clone();
                let result = league_sample::step(&db, &sources, move |what: &str| {
                    let mut a = act.lock().unwrap();
                    a.doing = Some(what.to_string());
                    a.request();
                })
                .await;
                let mut a = activity.lock().unwrap();
                a.doing = None;
                match result {
                    Ok(Step::Discovered(d)) => {
                        tracing::info!(matches = d.picked_matches, logs = d.picked_logs, listed = d.logs_listed, "league sample chosen");
                        a.log(format!("Listed {} ETF2L logs and chose {} matches ({} logs)", d.logs_listed, d.picked_matches, d.picked_logs), true);
                        ("waiting", STEP_GAP)
                    }
                    Ok(Step::Json { log_id, source }) => {
                        a.log(format!("Log {log_id} from {source}"), true);
                        let db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = hl_ingest::catalogue::index_league_players(&db, 5).await;
                        });
                        ("waiting", if source == hl_ingest::sources::DROPS { DROPS_GAP } else { STEP_GAP })
                    }
                    Ok(Step::Raw { log_id, found, source }) => {
                        a.log(if found { format!("Server log of {log_id} from {source}") } else { format!("No server log for {log_id}") }, found);
                        ("waiting", if source == hl_ingest::sources::DROPS { DROPS_GAP } else { STEP_GAP })
                    }
                    Ok(Step::Roster { match_id }) => {
                        a.log(format!("Who played ETF2L match {match_id}"), true);
                        ("waiting", STEP_GAP)
                    }
                    Ok(Step::Searched { match_id, found }) => {
                        a.log(
                            if found > 0 { format!("Found {found} logs for ETF2L match {match_id} on logs.tf") } else { format!("logs.tf has no logs for ETF2L match {match_id}") },
                            found > 0,
                        );
                        ("waiting", STEP_GAP)
                    }
                    Ok(Step::Waiting) => {
                        a.log("logs.tf is resting and there is nothing more.tf can give meanwhile".into(), true);
                        ("resting", WAITING_GAP)
                    }
                    Ok(Step::Done) => ("done", DONE_GAP),
                    Ok(Step::Failed { what }) => {
                        tracing::warn!(%what, "league sample step failed");
                        a.log(what, false);
                        (if sources.logstf_resting() { "resting" } else { "waiting" }, FAILED_GAP)
                    }
                    Err(e) => {
                        tracing::warn!(error = %format!("{e:#}"), "league sample step failed");
                        a.log(format!("{e:#}"), false);
                        ("waiting", FAILED_GAP * 4)
                    }
                }
            };
            {
                let mut a = activity.lock().unwrap();
                // A step that ran into logs.tf's rest says so, whatever it did.
                a.state = if state == "waiting" && sources.logstf_resting() { "resting" } else { state };
                a.next_at = Some(now() + gap.as_secs() as i64);
            }
            tokio::time::sleep(gap).await;
        }
    });
}
