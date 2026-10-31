//! Aim in one match, read from its demo (PLAN §14, Q16b).
//!
//! The demo knows the shot exactly: it carries its own kill events, stamped
//! with the tick. The log knows what the kill was: the victim's class, the
//! weapon, whether it was a headshot. This joins the two.
//!
//! The join is by victim and time. A demo's start is an estimate (file time
//! minus its own duration), so demo time and log time can sit a few seconds
//! apart; a kill of the same victim within [`JOIN_WINDOW_S`] is the same kill,
//! and the closest one wins. A kill with no match in the log still comes back,
//! with what the demo alone knows.
//!
//! **Whose aim gets stored.** The pass itself answers for every player the
//! demo carried, but not every answer is worth keeping:
//!
//! - An **STV** demo is the server's own recording. It carries all eighteen
//!   players throughout, with the angles the server had, so every one of them
//!   is stored.
//! - A **POV** demo is one client's. It carries its recorder always and
//!   everyone else only while the recorder could see them, with eye angles
//!   quantized for the wire rather than sampled from a mouse. Only the
//!   recorder is stored from one.
//!
//! This is the same rule routes already follow, and it is why the Aim tab can
//! now answer for a teammate on a match with an STV demo and says plainly
//! that it cannot on a match with only your own.

use anyhow::Result;
use hl_core::{SteamId, TfClass};
use hl_db::Db;
use hl_demos::aim::Shot;
use serde::Serialize;
use std::path::Path;

/// How far apart the demo's clock and the log's clock may be for two kills of
/// the same victim to be the same kill.
pub const JOIN_WINDOW_S: f64 = 15.0;

/// One kill, with what the demo says about the aim behind it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimKill {
    pub demo_id: i64,
    /// Whose kill it was.
    pub shooter: u32,
    pub shooter_name: Option<String>,
    /// The kill's time in the log's clock, where the log knew about it.
    pub at_raw: Option<i64>,
    pub victim: Option<u32>,
    pub victim_name: Option<String>,
    pub victim_class: Option<String>,
    pub headshot: bool,
    #[serde(flatten)]
    pub shot: Shot,
}

/// One death, joined to the log the same way a kill is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimDeath {
    pub demo_id: i64,
    /// Who died.
    pub who: u32,
    pub at_raw: Option<i64>,
    pub killer: Option<u32>,
    pub killer_name: Option<String>,
    #[serde(flatten)]
    pub death: hl_demos::aim::Death,
}

/// Every kill in one match a demo could answer for, with the aim behind it.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimReport {
    /// Kills of the owner's the log has, for "the demo answered for 34 of
    /// your 41 kills".
    pub log_kills: usize,
    /// Kills of the owner's the demo holds that belong to another match in
    /// the same recording: one demo often spans two logs. Counted for the
    /// owner alone, so the number still means what it says.
    pub other_matches: usize,
    pub kills: Vec<AimKill>,
    pub deaths: Vec<AimDeath>,
    /// Where everyone walked, one route per life, with the round it started in.
    pub paths: Vec<hl_db::PathRow>,
    /// Per demo and player: ticks alive, and of those, ticks scoped in.
    pub life: Vec<(i64, u32, i64, i64)>,
    /// Demos actually opened, and those the database lists but disk does not
    /// have. A demo can go missing because it was moved, renamed, or deleted
    /// to save space (Q23) -- none of which is an error, and all of which
    /// used to abort the pass for every match queued behind it.
    pub demos_read: usize,
    pub demos_missing: usize,
    /// Demos kept as a timeline during this read (Q3).
    pub timelines_recorded: usize,
}

/// Read the demos linked to `log_id` and measure the aim behind every kill
/// they can answer for. Empty when nothing is linked, or a demo has no clock
/// of its own.
///
/// `me` is the owner, who is the recorder of any local POV demo: it decides
/// whose numbers a POV demo is allowed to contribute, and which kills the
/// headline counts are about.
pub async fn for_log(db: &Db, log_id: i64, me: SteamId) -> Result<AimReport> {
    for_log_with(db, log_id, me, &mut |_| {}).await
}

/// Where reading a match's demos has got to, for a status line. A match can
/// have two (your own recording and the server's), read one after the other.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum ReadStep {
    /// Walking demo `demo` of `of` ("pov" or "stv"), `pct` of the way.
    #[serde(rename_all = "camelCase")]
    Reading { demo: usize, of: usize, kind: String, pct: u8 },
    /// Compressing that demo into its timeline and storing it (Q3).
    #[serde(rename_all = "camelCase")]
    Keeping { demo: usize, of: usize },
    /// Writing the aim, deaths and routes to the database.
    Saving,
}

/// [`for_log`], telling `progress` each step.
pub async fn for_log_with(db: &Db, log_id: i64, me: SteamId, progress: &mut (dyn FnMut(ReadStep) + Send)) -> Result<AimReport> {
    let demos = db.demos_for_log(log_id).await?;
    if demos.is_empty() {
        return Ok(AimReport::default());
    }
    let offset = db.log_clock_offset(log_id).await?;
    let names = db.player_names(log_id).await?;
    let kills = db.kills_for_log(log_id).await?;
    // Every real kill in the match, to join the demo's against. A feign death
    // is not a death and was never a kill.
    let live: Vec<_> =
        kills.iter().filter(|k| k.live && k.custom.as_deref() != Some("feign_death")).collect();

    // Who was on which team, and every point captured: a life is worth more
    // when it took ground, so the caps its team made during it come with it.
    let teams: std::collections::HashMap<u32, String> = db.player_teams(log_id).await?;
    let caps = db.cap_events(log_id).await?;

    // The log's rounds on its own clock, to place each life and route.
    let rounds: Vec<(i64, f64, f64)> = db
        .round_spans(log_id)
        .await?
        .into_iter()
        .map(|(num, start, len)| (num, start as f64, (start + len) as f64))
        .collect();

    // A demo whose file was deleted to save space (Q23) is not a demo this
    // pass can read. Its rows stay as they are, derived when the file was
    // still here.
    let usable = |d: &&hl_db::LinkedDemo| !d.deleted && d.start_utc.is_some() && d.tick_rate.is_some();
    let pov = demos.iter().filter(usable).find(|d| d.kind == "pov");
    let stv = demos.iter().filter(usable).find(|d| d.kind == "stv");
    let Some(fallback) = pov.or(stv).or_else(|| demos.iter().find(usable)) else {
        return Ok(AimReport::default());
    };

    let mut out = AimReport {
        log_kills: live.iter().filter(|k| k.killer == me.account_id()).count(),
        ..AimReport::default()
    };
    // Which demo answers for whom, and which one draws the routes.
    //
    // A POV demo records its own client's view angles as the player made
    // them; an STV takes everyone's off the wire, quantized and only as
    // often as the server sends them. So when a match has both, the POV
    // answers for the owner and the STV for the other seventeen: two passes
    // over two files, but neither player's numbers are the second-best ones
    // available. Reading both for everybody would count every kill twice.
    //
    // A combined log has one STV per part (Q84: the S36 High final, six),
    // each covering its own rounds: every one of them is read, not only the
    // first, which had left five of the six maps' halves without movement.
    let stvs: Vec<&hl_db::LinkedDemo> = demos.iter().filter(usable).filter(|d| d.kind == "stv").collect();
    let mut jobs: Vec<(&hl_db::LinkedDemo, Scope, bool)> = Vec::new();
    match (pov, stv) {
        (Some(p), Some(_)) => {
            jobs.push((p, Scope::Owner, false));
            jobs.extend(stvs.iter().map(|s| (*s, Scope::Others, true)));
        }
        (Some(p), None) => jobs.push((p, Scope::Owner, true)),
        (None, Some(_)) => jobs.extend(stvs.iter().map(|s| (*s, Scope::All, true))),
        (None, None) => jobs.push((fallback, Scope::Owner, true)),
    }

    let my_id = me.to_steamid3();
    let of = jobs.len();
    for (n, (d, scope, for_paths)) in jobs.into_iter().enumerate() {
        let demo = n + 1;
        let (Some(start), Some(rate)) = (d.start_utc, d.tick_rate) else { continue };
        // An STV demo is the server's own recording and carries everybody. A
        // POV demo carries other players only while its recorder could see
        // them, and their eye angles arrive quantized for the wire: their
        // routes are a handful of disconnected seconds and their crosshair is
        // not a measurement. So a POV demo speaks for its recorder alone.
        let everyone = d.kind == "stv";
        let owner = if everyone { String::new() } else { my_id.clone() };
        // Whose aim and deaths this demo is being read for, and separately
        // whose routes: an STV draws everybody's, including the owner's.
        let keep = |steamid: &str| match scope {
            Scope::Owner => steamid == my_id,
            Scope::Others => steamid != my_id,
            Scope::All => true,
        };
        let keep_route = |steamid: &str| everyone || steamid == my_id;

        // A demo the database remembers and the disk does not. Nothing to
        // read, which is not the same as something going wrong.
        let file = Path::new(&d.path);
        if !file.exists() {
            tracing::info!(log_id, demo = %d.path, "demo file is gone; skipping it");
            out.demos_missing += 1;
            continue;
        }
        // Record the demo whole in the same walk, unless it already has been
        // at this format: the walk is the slow part, and this makes the file
        // optional from here on (Q3).
        let record = db.timeline_version(d.demo_id).await?.is_none_or(|v| v < TIMELINE_VERSION);
        // Progress as ticks walked over the header's count. Added up rather
        // than read off the tick, so a recording holding two matches (the
        // tick restarts) still only climbs.
        let total = d.ticks.max(1) as f64;
        let (mut walked, mut last, mut shown) = (0u64, None::<u32>, u8::MAX);
        let kind = d.kind.clone();
        progress(ReadStep::Reading { demo, of, kind: kind.clone(), pct: 0 });
        let (pass, timeline) = hl_demos::aim::pass_recording(
            file,
            &owner,
            rate,
            record.then_some(hl_demos::timeline::DEFAULT_STRIDE),
            &mut |tick| {
                if let Some(l) = last.filter(|l| tick > *l) {
                    walked += u64::from(tick - l);
                }
                last = Some(tick);
                let pct = ((walked as f64 / total) * 100.0).min(99.0) as u8;
                if pct != shown {
                    shown = pct;
                    progress(ReadStep::Reading { demo, of, kind: kind.clone(), pct });
                }
            },
        )?;
        out.demos_read += 1;
        if let Some(s) = timeline {
            progress(ReadStep::Keeping { demo, of });
            let row = hl_db::TimelineRow {
                version: s.version,
                tick_rate: s.tick_rate,
                stride: i64::from(s.stride),
                raw_bytes: s.raw_bytes as i64,
                stored_bytes: s.stored_bytes() as i64,
                head: s.head,
                samples: s.samples,
                changes: s.changes,
                objects: s.objects,
                events: s.events,
            };
            db.put_timeline(d.demo_id, &row).await?;
            out.timelines_recorded += 1;
            tracing::info!(log_id, demo = %d.path, kb = row.stored_bytes / 1000, "demo kept as a timeline");
        }

        {
            let mut life: Vec<(i64, u32, i64, i64)> = pass
                .time
                .iter()
                .filter(|(steamid, _)| keep(steamid))
                .filter_map(|(steamid, t)| {
                    Some((d.demo_id, account_of(steamid)?, i64::from(t.alive_ticks), i64::from(t.scoped_ticks)))
                })
                .collect();
            // A HashMap has no order, and these end up in a table.
            life.sort_unstable();
            out.life.extend(life);
        }

        for (seq, l) in pass.lives.iter().enumerate().take(if for_paths { usize::MAX } else { 0 }) {
            if !keep_route(&l.steamid) {
                continue;
            }
            let at = offset.map(|o| start + f64::from(l.from_tick) / rate - o as f64);
            let until = offset.map(|o| start + f64::from(l.to_tick) / rate - o as f64);
            let (round, from_tick) = match (at, until) {
                (Some(at), Some(until)) => round_of_life(&rounds, at, until, |s| ((s - at) * rate) as i64 + i64::from(l.from_tick)),
                _ => (None, i64::from(l.from_tick)),
            };
            // A life outside every round of this log belongs to another match
            // in the same recording.
            if round.is_none() && offset.is_some() {
                continue;
            }
            let account_id = account_of(&l.steamid).unwrap_or(0);
            // The life on the log's clock, to find the caps inside it.
            let life_caps = match (offset, teams.get(&account_id)) {
                (Some(o), Some(team)) => {
                    let from = start + from_tick as f64 / rate - o as f64;
                    let to = start + f64::from(l.to_tick) / rate - o as f64;
                    caps.iter()
                        .filter(|(at, t, _)| t == team && (*at as f64) >= from && (*at as f64) <= to)
                        .map(|(at, _, point)| ((*at as f64 - from).round() as i64, *point))
                        .collect()
                }
                _ => Vec::new(),
            };
            out.paths.push(hl_db::PathRow {
                demo_id: d.demo_id,
                seq: seq as i64,
                account_id,
                caps: life_caps,
                from_tick,
                to_tick: i64::from(l.to_tick),
                round_num: round,
                died: l.died,
                points: l.points.iter().filter(|p| i64::from(p.0) >= from_tick).map(|&(t, x, y, z)| (i64::from(t), x, y, z)).collect(),
            });
        }

        for death in pass.deaths {
            if !keep(&death.who) {
                continue;
            }
            let Some(who) = account_of(&death.who) else { continue };
            let at = offset.map(|o| start + f64::from(death.tick) / rate - o as f64);
            let killer = SteamId::parse(&death.killer).ok().map(SteamId::account_id);
            let log_death = at.and_then(|at| nearest(&live, at, |k| k.victim == who));
            if log_death.is_none() && offset.is_some() {
                continue;
            }
            out.deaths.push(AimDeath {
                demo_id: d.demo_id,
                who,
                at_raw: log_death.map(|k| k.at_raw),
                killer,
                killer_name: killer.and_then(|k| names.get(&k).cloned()),
                death,
            });
        }
        for shot in pass.shots {
            if !keep(&shot.shooter) {
                continue;
            }
            let Some(shooter) = account_of(&shot.shooter) else { continue };
            // The demo's tick, put on the log's clock.
            let at = offset.map(|o| start + f64::from(shot.tick) / rate - o as f64);
            let victim = SteamId::parse(&shot.victim).ok().map(SteamId::account_id);
            let log_kill =
                at.and_then(|at| nearest(&live, at, |k| k.killer == shooter && Some(k.victim) == victim));
            // One demo often holds two logs; a kill the log never saw is the
            // other match's, unless the log has no clock to compare against.
            if log_kill.is_none() && offset.is_some() {
                out.other_matches += usize::from(shooter == me.account_id());
                continue;
            }
            out.kills.push(AimKill {
                demo_id: d.demo_id,
                shooter,
                shooter_name: names.get(&shooter).cloned(),
                at_raw: log_kill.map(|k| k.at_raw),
                victim,
                victim_name: victim.and_then(|v| names.get(&v).cloned()),
                victim_class: log_kill
                    .and_then(|k| k.victim_class.as_deref())
                    .and_then(|c| TfClass::parse(c).ok())
                    .map(|c| c.as_str().to_string()),
                headshot: log_kill.is_some_and(|k| k.custom.as_deref() == Some("headshot")),
                shot,
            });
        }
    }
    out.kills.sort_by_key(|k| (k.demo_id, k.shot.tick, k.shooter));
    out.deaths.sort_by_key(|d| (d.demo_id, d.death.tick, d.who));
    Ok(out)
}

/// Whose numbers one pass over one demo is allowed to contribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The owner alone: a POV demo, which speaks only for its recorder.
    Owner,
    /// Everyone but the owner, whose own numbers a better demo already gave.
    Others,
    /// Everybody: an STV demo, with no POV beside it.
    All,
}

/// The account id behind a demo's SteamID, where it parses to a real one.
/// Zero is what an unparsed id would become, and a row belonging to nobody is
/// worse than no row at all.
pub(crate) fn account_of(steamid: &str) -> Option<u32> {
    SteamId::parse(steamid).ok().map(|s| s.account_id()).filter(|a| *a != 0)
}

/// The kill in the log closest to `at` that `which` accepts, within
/// [`JOIN_WINDOW_S`]. The two clocks disagree by seconds, so "the same kill"
/// is the nearest candidate rather than an exact time.
fn nearest<'a>(
    live: &[&'a hl_db::StoredKill],
    at: f64,
    which: impl Fn(&hl_db::StoredKill) -> bool,
) -> Option<&'a hl_db::StoredKill> {
    live.iter()
        .filter(|k| which(k) && (k.at_raw as f64 - at).abs() <= JOIN_WINDOW_S)
        .min_by(|a, b| (a.at_raw as f64 - at).abs().total_cmp(&(b.at_raw as f64 - at).abs()))
        .copied()
}

/// Bump to re-read every demo on the next pass.
/// 1: crosshair error, flick, range.
/// 2: deaths (who was near, scoped) and time spent scoped.
/// 3: which way the crosshair was off, not just how far.
/// 4: where the player who killed you was, relative to your view.
/// 5: the crosshair's path over the second before each kill, and the
///    sideways and vertical angles corrected to mean what they say.
/// 6: where you walked, one route per life.
/// 7: routes for every player the demo carried, not only the owner's.
/// 8: one demo per job, so a match with both a POV and an STV demo is not
///    counted twice.
/// 9: routes for other players only from an STV demo.
/// 10: the points captured during each life.
/// 11: every player's aim and deaths from an STV demo, not only the owner's.
/// 13: a life begun before its round (spawned in the pre-round freeze, a
///     moment before the log's Round_Start, or alive from the pregame) is
///     the round's, from its start, where it was left out as another
///     match's: most players' first life of each round had no route.
/// 12: conditions read off the wire (the parser's own check missed scoped or
///     cloaked whenever a higher condition shared the byte), "a second
///     before" and the flick measured in ticks rather than frames (a POV
///     demo skips ticks), and the demo kept as a timeline in the same walk
///     (Q3).
pub const VERSION: i64 = 13;

/// The round a life from `at` to `until` (the log's clock) belongs to, and
/// the tick it starts at in it: the round it began in, else the first one
/// that began while it was alive (players spawn in the pre-round freeze, a
/// moment before the log's Round_Start; round 1's are alive from the
/// pregame), from that round's start, the tick `tick_at` gives. `None` for a
/// life in no round. Pure, for testing: `rounds` are `(number, start, end)`.
pub fn round_of_life(rounds: &[(i64, f64, f64)], at: f64, until: f64, tick_at: impl Fn(f64) -> i64) -> (Option<i64>, i64) {
    if let Some(&(n, ..)) = rounds.iter().find(|(_, f, t)| at >= *f && at <= *t) {
        return (Some(n), tick_at(at));
    }
    match rounds.iter().find(|(_, f, _)| at < *f && until >= *f) {
        Some(&(n, f, _)) => (Some(n), tick_at(f)),
        None => (None, tick_at(at)),
    }
}

/// The timeline format this build records; see `hl_demos::timeline`.
pub use hl_demos::timeline::TIMELINE_VERSION;

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimSummary {
    /// Logs read this time, and those with a demo to read at all.
    pub read: usize,
    pub total: usize,
    pub kills: usize,
    /// Logs skipped because reading them failed, and because every demo
    /// they had is no longer on disk. Neither stops the pass.
    pub failed: usize,
    pub missing: usize,
}

/// Read every linked demo the current [`VERSION`] has not read yet, or all of
/// them with `all`, and store the aim behind every kill they answer for.
///
/// About 4 s a demo, so a first pass over a full history is minutes, not
/// seconds: it reports progress and is meant for the background.
pub async fn derive_all(
    db: &Db,
    me: SteamId,
    all: bool,
    mut progress: impl FnMut(usize, usize, Option<i64>, Option<crate::sync::ReadingMatch>),
) -> Result<AimSummary> {
    let ids = db.aim_queue().await?;
    let mut done = if all { Default::default() } else { db.aim_logs(VERSION).await? };
    // Once: the matches with several STV demos, read when only the first
    // was (Q84: the S36 High final's five other halves had no movement).
    const MULTI_STV_KEY: &str = "aim_multi_stv_reread";
    let reread = db.get_setting(MULTI_STV_KEY).await?.is_none();
    if reread {
        for id in db.multi_stv_logs().await? {
            done.remove(&id);
        }
    }
    let todo: Vec<i64> = ids.iter().copied().filter(|id| !done.contains(id)).collect();
    let mut out = AimSummary { total: ids.len(), ..AimSummary::default() };
    for (i, log_id) in todo.iter().copied().enumerate() {
        progress(i, todo.len(), Some(log_id), crate::sync::ReadingMatch::of(db, log_id).await);
        // One unreadable demo is one match without aim, not a failed sync.
        // This used to propagate, so a single corrupt or moved file stopped
        // every match behind it from ever being read.
        let report = match for_log(db, log_id, me).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(log_id, error = %format!("{e:#}"), "reading this match's demos failed; skipping it");
                out.failed += 1;
                continue;
            }
        };
        // Every demo it had is off the disk. Whatever was derived from them
        // is the best that will ever exist for this match, so it stays:
        // storing an empty result here would delete good rows and then mark
        // the log read at a version no demo backs.
        if report.demos_read == 0 && report.demos_missing > 0 {
            out.missing += 1;
            continue;
        }
        let rows: Vec<hl_db::AimRow> = report.kills.iter().map(store_row).collect();
        let deaths: Vec<hl_db::DeathRow> = report.deaths.iter().map(death_row).collect();
        db.replace_aim(log_id, VERSION, &rows).await?;
        db.replace_deaths(log_id, &deaths, &report.life).await?;
        db.replace_paths(log_id, &report.paths).await?;
        out.read += 1;
        out.kills += rows.len();
    }
    progress(todo.len(), todo.len(), None, None);
    if reread {
        db.set_setting(MULTI_STV_KEY, "1").await?;
    }
    Ok(out)
}

fn death_row(d: &AimDeath) -> hl_db::DeathRow {
    hl_db::DeathRow {
        demo_id: d.demo_id,
        who: d.who,
        tick: i64::from(d.death.tick),
        at_raw: d.at_raw,
        // The round is read back from the log's windows, not stored.
        round_num: None,
        killer: d.killer,
        killer_range: d.death.killer_range.map(f64::from),
        killer_dx_deg: d.death.killer_dx_deg.map(f64::from),
        killer_dy_deg: d.death.killer_dy_deg.map(f64::from),
        nearest_mate: d.death.nearest_mate.map(f64::from),
        mates_near: i64::from(d.death.mates_near),
        scoped: d.death.scoped,
        seen: d.death.seen,
    }
}

/// Read one match's demos again and store what they say, whether or not the
/// pass has seen it before. Used after an STV download: the new demo carries
/// every player, where a POV demo only carried its recorder.
pub async fn derive_log(db: &Db, me: SteamId, log_id: i64) -> Result<usize> {
    derive_log_with(db, me, log_id, &mut |_| {}).await
}

/// [`derive_log`], telling `progress` each step.
pub async fn derive_log_with(db: &Db, me: SteamId, log_id: i64, progress: &mut (dyn FnMut(ReadStep) + Send)) -> Result<usize> {
    let report = for_log_with(db, log_id, me, progress).await?;
    progress(ReadStep::Saving);
    let rows: Vec<hl_db::AimRow> = report.kills.iter().map(store_row).collect();
    let deaths: Vec<hl_db::DeathRow> = report.deaths.iter().map(death_row).collect();
    db.replace_aim(log_id, VERSION, &rows).await?;
    db.replace_deaths(log_id, &deaths, &report.life).await?;
    db.replace_paths(log_id, &report.paths).await?;
    Ok(report.paths.len())
}

fn store_row(k: &AimKill) -> hl_db::AimRow {
    hl_db::AimRow {
        demo_id: k.demo_id,
        shooter: k.shooter,
        tick: i64::from(k.shot.tick),
        at_raw: k.at_raw,
        round_num: None,
        victim: k.victim,
        error_deg: f64::from(k.shot.error_deg),
        before_deg: f64::from(k.shot.error_before_deg),
        dx_deg: f64::from(k.shot.dx_deg),
        dy_deg: f64::from(k.shot.dy_deg),
        before_dx_deg: f64::from(k.shot.before_dx_deg),
        before_dy_deg: f64::from(k.shot.before_dy_deg),
        path: k.shot.path.iter().map(|&(x, y)| (f64::from(x), f64::from(y))).collect(),
        flick_deg: f64::from(k.shot.flick_deg),
        range_units: f64::from(k.shot.range),
        height: f64::from(k.shot.height),
        victim_seen: k.shot.victim_seen,
        shooter_seen: k.shot.shooter_seen,
        headshot: k.headshot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_life_begun_before_its_round_is_the_rounds_from_its_start() {
        let rounds = [(1, 100.0, 400.0), (2, 410.0, 700.0)];
        // Ticks at 10 a second from 0 on the log's clock.
        let tick = |s: f64| (s * 10.0) as i64;
        // Begun inside a round: that round, from where it began.
        assert_eq!(round_of_life(&rounds, 150.0, 200.0, tick), (Some(1), 1500));
        // Spawned in the freeze a second before round 2, or alive from the
        // pregame into round 1: theirs, from the round's start.
        assert_eq!(round_of_life(&rounds, 409.0, 500.0, tick), (Some(2), 4100));
        assert_eq!(round_of_life(&rounds, 20.0, 130.0, tick), (Some(1), 1000));
        // Over before any round began, or after the last: none.
        assert_eq!(round_of_life(&rounds, 20.0, 90.0, tick).0, None);
        assert_eq!(round_of_life(&rounds, 800.0, 900.0, tick).0, None);
    }
}
