//! Your aim in one match, read from its demo (PLAN §14).
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
    /// The kill's time in the log's clock, where the log knew about it.
    pub at_raw: Option<i64>,
    pub victim: Option<u32>,
    pub victim_name: Option<String>,
    pub victim_class: Option<String>,
    pub headshot: bool,
    #[serde(flatten)]
    pub shot: Shot,
}

/// One death of yours, joined to the log the same way a kill is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimDeath {
    pub demo_id: i64,
    pub at_raw: Option<i64>,
    pub killer: Option<u32>,
    pub killer_name: Option<String>,
    #[serde(flatten)]
    pub death: hl_demos::aim::Death,
}

/// Every kill of yours in one match, with the aim behind those a demo covers.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimReport {
    /// Kills of yours the log has, and those a demo could answer for.
    pub log_kills: usize,
    /// Kills the demo holds that belong to another match in the same
    /// recording: one demo often spans two logs.
    pub other_matches: usize,
    pub kills: Vec<AimKill>,
    pub deaths: Vec<AimDeath>,
    /// Per demo: ticks alive, and of those, ticks scoped in.
    pub life: Vec<(i64, i64, i64)>,
}

/// Read the demos linked to `log_id` and measure the aim behind each of `me`'s
/// kills. Empty when nothing is linked, or a demo has no clock of its own.
pub async fn for_log(db: &Db, log_id: i64, me: SteamId) -> Result<AimReport> {
    let demos = db.demos_for_log(log_id).await?;
    if demos.is_empty() {
        return Ok(AimReport::default());
    }
    let offset = db.log_clock_offset(log_id).await?;
    let names = db.player_names(log_id).await?;
    let kills = db.kills_for_log(log_id).await?;
    let mine: Vec<_> = kills
        .iter()
        .filter(|k| k.killer == me.account_id() && k.live && k.custom.as_deref() != Some("feign_death"))
        .collect();

    let mut out = AimReport { log_kills: mine.len(), ..AimReport::default() };
    for d in &demos {
        let (Some(start), Some(rate)) = (d.start_utc, d.tick_rate) else { continue };
        let pass = hl_demos::aim::pass(Path::new(&d.path), &me.to_steamid3(), rate)?;
        out.life.push((d.demo_id, i64::from(pass.alive_ticks), i64::from(pass.scoped_ticks)));
        // The log's deaths of yours, to join the demo's against.
        let my_deaths: Vec<_> = kills.iter().filter(|k| k.victim == me.account_id() && k.live).collect();
        for death in pass.deaths {
            let at = offset.map(|o| start + f64::from(death.tick) / rate - o as f64);
            let killer = SteamId::parse(&death.killer).ok().map(SteamId::account_id);
            let log_death = at.and_then(|at| {
                my_deaths
                    .iter()
                    .filter(|k| (k.at_raw as f64 - at).abs() <= JOIN_WINDOW_S)
                    .min_by(|a, b| {
                        let d = |k: &&&hl_db::StoredKill| (k.at_raw as f64 - at).abs();
                        d(a).total_cmp(&d(b))
                    })
                    .copied()
            });
            if log_death.is_none() && offset.is_some() {
                continue;
            }
            out.deaths.push(AimDeath {
                demo_id: d.demo_id,
                at_raw: log_death.map(|k| k.at_raw),
                killer,
                killer_name: killer.and_then(|k| names.get(&k).cloned()),
                death,
            });
        }
        for shot in pass.shots {
            // The demo's tick, put on the log's clock.
            let at = offset.map(|o| start + f64::from(shot.tick) / rate - o as f64);
            let victim = SteamId::parse(&shot.victim).ok().map(SteamId::account_id);
            let log_kill = at.and_then(|at| {
                mine.iter()
                    .filter(|k| Some(k.victim) == victim && (k.at_raw as f64 - at).abs() <= JOIN_WINDOW_S)
                    .min_by(|a, b| {
                        let d = |k: &&&hl_db::StoredKill| (k.at_raw as f64 - at).abs();
                        d(a).total_cmp(&d(b))
                    })
                    .copied()
            });
            // One demo often holds two logs; a kill the log never saw is the
            // other match's, unless the log has no clock to compare against.
            if log_kill.is_none() && offset.is_some() {
                out.other_matches += 1;
                continue;
            }
            out.kills.push(AimKill {
                demo_id: d.demo_id,
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
    out.kills.sort_by_key(|k| (k.demo_id, k.shot.tick));
    Ok(out)
}

/// Bump to re-read every demo on the next pass.
/// 1: crosshair error, flick, range.
/// 2: deaths (who was near, scoped) and time spent scoped.
/// 3: which way the crosshair was off, not just how far.
/// 4: where the player who killed you was, relative to your view.
pub const VERSION: i64 = 4;

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AimSummary {
    /// Logs read this time, and those with a demo to read at all.
    pub read: usize,
    pub total: usize,
    pub kills: usize,
}

/// Read every linked demo the current [`VERSION`] has not read yet, or all of
/// them with `all`, and store the aim behind each of the owner's kills.
///
/// About 4 s a demo, so a first pass over a full history is minutes, not
/// seconds: it reports progress and is meant for the background.
pub async fn derive_all(db: &Db, me: SteamId, all: bool, mut progress: impl FnMut(usize, usize)) -> Result<AimSummary> {
    let ids = db.aim_queue().await?;
    let done = if all { Default::default() } else { db.aim_logs(VERSION).await? };
    let todo: Vec<i64> = ids.iter().copied().filter(|id| !done.contains(id)).collect();
    let mut out = AimSummary { total: ids.len(), ..AimSummary::default() };
    for (i, log_id) in todo.iter().copied().enumerate() {
        progress(i, todo.len());
        let report = for_log(db, log_id, me).await?;
        let rows: Vec<hl_db::AimRow> = report.kills.iter().map(store_row).collect();
        let deaths: Vec<hl_db::DeathRow> = report.deaths.iter().map(death_row).collect();
        db.replace_aim(log_id, VERSION, &rows).await?;
        db.replace_deaths(log_id, &deaths, &report.life).await?;
        out.read += 1;
        out.kills += rows.len();
    }
    progress(todo.len(), todo.len());
    Ok(out)
}

fn death_row(d: &AimDeath) -> hl_db::DeathRow {
    hl_db::DeathRow {
        demo_id: d.demo_id,
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
    }
}

fn store_row(k: &AimKill) -> hl_db::AimRow {
    hl_db::AimRow {
        demo_id: k.demo_id,
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
        flick_deg: f64::from(k.shot.flick_deg),
        range_units: f64::from(k.shot.range),
        height: f64::from(k.shot.height),
        victim_seen: k.shot.victim_seen,
        headshot: k.headshot,
    }
}
