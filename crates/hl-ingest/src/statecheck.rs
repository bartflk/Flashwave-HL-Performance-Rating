//! Checks the game state (`state.rs`) against what logs.tf counted from the
//! same raw logs, and against the log's own "advantage lost" lines.
//! `hl state --check`.

use crate::normalize::normalize;
use crate::rawlog;
use crate::state::{Charge, ChargeKind, GameState, LifeEnd};
use anyhow::{Context, Result};
use hl_core::matchdata::Team;
use hl_db::Db;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Default)]
pub struct Report {
    pub logs: usize,
    /// Counted kills, and those whose victim had no open life to end.
    pub kills: usize,
    pub kills_without_life: usize,
    /// Medics compared with logs.tf's `ubers`, and those that differ.
    pub medics: usize,
    pub uber_mismatch: Vec<(i64, u32, i64, i64)>,
    /// Same for `drops`: died holding a ready charge.
    pub drop_mismatch: Vec<(i64, u32, i64, i64)>,
    /// Seconds inside rounds, and the most players alive on one colour.
    pub seconds: i64,
    pub max_alive: u8,
    pub seconds_over_nine: i64,
    /// Of those, the ones in a round's first 15 seconds: a sub swapping in.
    pub over_nine_at_start: i64,
    /// Logs with any second over nine.
    pub logs_over_nine: usize,
    /// The worst such moment: (log, time, [red, blue]).
    pub over_nine_example: Option<(i64, i64, [u8; 2])>,
    /// `lost_uber_advantage` lines, by what the state says happened.
    pub lost_ad: HashMap<&'static str, usize>,
    /// For the lines where the enemy just got a charge: |logged − our biggest lead| seconds.
    pub lost_ad_len_err: Vec<i64>,
    pub millis: u128,
}

pub async fn check(db: &Db, max: Option<usize>) -> Result<Report> {
    let started = std::time::Instant::now();
    let mut r = Report::default();
    let mut ids = db.rawlog_ids().await?;
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.truncate(max.unwrap_or(usize::MAX));
    for log_id in ids {
        let (Some(json), Some(zip)) = (db.raw_log(log_id).await?, db.rawlog(log_id).await?) else { continue };
        let value: serde_json::Value = serde_json::from_str(&json).with_context(|| format!("log {log_id} JSON"))?;
        let log = normalize(log_id, &value)?;
        let raw = rawlog::parse(&rawlog::unzip(&zip)?);
        let s = GameState::build(&raw);
        r.logs += 1;

        // 1. Every counted kill ends a life.
        for k in raw.kills.iter().filter(|k| k.counts()) {
            r.kills += 1;
            let ended = s
                .lives
                .iter()
                .any(|l| l.account == k.victim.account && l.to == k.at && l.end == LifeEnd::Killed { by: k.killer.account });
            if !ended {
                r.kills_without_life += 1;
            }
        }

        // 2. Ubers and drops per Medic, inside rounds, against logs.tf.
        let in_round = |t: i64| s.rounds.iter().any(|rd| rd.start <= t && rd.end.is_none_or(|e| t <= e));
        let mut ubers: HashMap<u32, i64> = HashMap::new();
        let mut drops: HashMap<u32, i64> = HashMap::new();
        for c in &s.charges {
            if c.kind == ChargeKind::Deployed && in_round(c.from) {
                *ubers.entry(c.medic).or_default() += 1;
            }
            if c.kind == ChargeKind::Ready && in_round(c.to) {
                let died = s.lives.iter().any(|l| {
                    l.account == c.medic && l.to == c.to && matches!(l.end, LifeEnd::Killed { .. } | LifeEnd::Suicide)
                });
                if died {
                    *drops.entry(c.medic).or_default() += 1;
                }
            }
        }
        for p in &log.players {
            let id = p.id.account_id();
            let ours = (ubers.get(&id).copied().unwrap_or(0), drops.get(&id).copied().unwrap_or(0));
            if p.stats.ubers == 0 && p.stats.drops == 0 && ours == (0, 0) {
                continue;
            }
            r.medics += 1;
            if ours.0 != p.stats.ubers {
                r.uber_mismatch.push((log_id, id, p.stats.ubers, ours.0));
            }
            if ours.1 != p.stats.drops {
                r.drop_mismatch.push((log_id, id, p.stats.drops, ours.1));
            }
        }

        // 3. Players alive, second by second inside rounds.
        let before = r.seconds_over_nine;
        for rd in &s.rounds {
            let Some(end) = rd.end else { continue };
            for t in rd.start..end {
                let n = s.numbers_at(t);
                r.seconds += 1;
                r.max_alive = r.max_alive.max(n[0]).max(n[1]);
                if n[0] > 9 || n[1] > 9 {
                    r.seconds_over_nine += 1;
                    if t - rd.start < 15 {
                        r.over_nine_at_start += 1;
                    }
                    if r.over_nine_example.is_none_or(|(_, _, m)| n[0].max(n[1]) > m[0].max(m[1])) {
                        r.over_nine_example = Some((log_id, t, n));
                    }
                }
            }
        }

        if r.seconds_over_nine > before {
            r.logs_over_nine += 1;
        }

        // 4. Each "advantage lost" line: what does the state say happened?
        for la in &s.lost_advantages {
            let Some(team) = la.team else { continue };
            let enemy = if team == Team::Red { Team::Blue } else { Team::Red };
            let had = s.advantage_at(la.at - 1) == Some(team);
            let enemy_ready_now = s
                .charges
                .iter()
                .any(|c| c.team == enemy && c.kind == ChargeKind::Ready && (c.from - la.at).abs() <= 1);
            let medic_died = s.lives.iter().any(|l| l.account == la.medic && (l.to - la.at).abs() <= 1 && l.end != LifeEnd::LogEnd);
            let why = match (had, enemy_ready_now, medic_died) {
                (true, true, _) => "held the advantage, enemy charge became ready",
                (true, false, true) => "held the advantage, Medic died",
                (true, false, false) => "held the advantage, other",
                (false, true, _) => "no advantage in the state, enemy became ready",
                (false, false, _) => "no advantage in the state, other",
            };
            *r.lost_ad.entry(why).or_default() += 1;
            if enemy_ready_now {
                // MedicStats logs the biggest lead in seconds-to-charge since
                // the enemy last had one.
                let mut t = la.at - 1;
                let mut best = f64::MIN;
                while t > la.at - 300 && !matches!(s.charge_at(enemy, t), Charge::Ready | Charge::Deployed) {
                    best = best.max(s.uber_lead_s(team, t));
                    t -= 1;
                }
                if best > f64::MIN {
                    r.lost_ad_len_err.push((la.secs - best.round() as i64).abs());
                }
            }
        }
    }
    r.millis = started.elapsed().as_millis();
    Ok(r)
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} logs checked in {:.1}s", self.logs, self.millis as f64 / 1000.0)?;
        writeln!(f, "kills: {} counted, {} with no open life for the victim", self.kills, self.kills_without_life)?;
        writeln!(f, "medics: {} compared", self.medics)?;
        writeln!(f, "  ubers differ from logs.tf: {}", self.uber_mismatch.len())?;
        for (log, id, theirs, ours) in self.uber_mismatch.iter().take(8) {
            writeln!(f, "    log {log} [U:1:{id}] logs.tf {theirs}, state {ours}")?;
        }
        writeln!(f, "  drops differ from logs.tf: {}", self.drop_mismatch.len())?;
        for (log, id, theirs, ours) in self.drop_mismatch.iter().take(8) {
            writeln!(f, "    log {log} [U:1:{id}] logs.tf {theirs}, state {ours}")?;
        }
        writeln!(
            f,
            "alive: {} round seconds, at most {} on one colour, {} seconds with more than 9 ({} in a round's first 15 s) in {} logs",
            self.seconds, self.max_alive, self.seconds_over_nine, self.over_nine_at_start, self.logs_over_nine
        )?;
        if let Some((log, t, n)) = self.over_nine_example {
            writeln!(f, "  worst: log {log} at {t} (raw clock), {} red {} blue", n[0], n[1])?;
        }
        let total: usize = self.lost_ad.values().sum();
        writeln!(f, "advantage lost lines: {total}")?;
        let mut why: Vec<_> = self.lost_ad.iter().collect();
        why.sort_by(|a, b| b.1.cmp(a.1));
        for (k, n) in why {
            writeln!(f, "  {n:>5}  {k}")?;
        }
        let mut e = self.lost_ad_len_err.clone();
        e.sort_unstable();
        if !e.is_empty() {
            let within = |s: i64| e.iter().filter(|x| **x <= s).count() as f64 / e.len() as f64 * 100.0;
            writeln!(
                f,
                "  advantage size vs the log: median error {}s, {:.0}% within 2s, {:.0}% within 5s",
                e[e.len() / 2],
                within(2),
                within(5)
            )?;
        }
        Ok(())
    }
}
