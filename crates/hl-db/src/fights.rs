//! Kills in context per player and match (PLAN §11 B and D).

use crate::{Db, BATCH_ROWS};
use anyhow::Result;
use serde::Serialize;
use sqlx::Row;
use std::collections::HashMap;

/// The counted columns of `fight_stat`, in order.
pub const FIGHT_COLUMNS: [&str; 28] = [
    "rounds",
    "kills",
    "deaths",
    "opening_kills",
    "opening_deaths",
    "first_picks",
    "first_deaths",
    "traded_kills",
    "died_after_kill",
    "trade_kills",
    "cleanup_kills",
    "charged_picks",
    "drops",
    "forces",
    "deaths_before_uber",
    "deaths_during_uber",
    "deaths_after_uber",
    "traded_deaths",
    "deaths_to_sniper",
    "deaths_to_flank",
    "deaths_to_combo",
    "stationary_deaths",
    "fights_present",
    "fights_kast",
    "fights_kast_engaged",
    "caps_contested",
    "caps_mates_dead",
    "caps_spawn_delay",
];

/// One player's counts in one match, in [`FIGHT_COLUMNS`] order.
pub struct FightRow {
    pub account_id: u32,
    pub values: [i64; 28],
}

/// Summed counts over a set of rated performances.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FightTotals {
    pub games: i64,
    pub minutes: f64,
    /// In [`FIGHT_COLUMNS`] order.
    pub values: Vec<i64>,
}

/// One official, for grouping into seasons.
pub struct SeasonOfficial {
    pub competition: String,
    pub division: Option<String>,
    pub time: i64,
}

/// One rated game on a class with its box score on that class.
#[derive(Debug, Clone)]
pub struct ClassGame {
    pub log_id: i64,
    pub played_at: Option<i64>,
    pub kind: Option<String>,
    /// W, L or T; `None` without a score.
    pub result: Option<&'static str>,
    pub score: f64,
    pub minutes: f64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub dmg: i64,
    pub time_s: i64,
}

/// Which rated performances to sum.
#[derive(Debug, Clone, Default)]
pub struct FightFilter<'a> {
    pub class: &'a str,
    pub model_version: &'a str,
    /// official | scrim | pug
    pub kind: Option<&'a str>,
    /// Unix seconds, inclusive.
    pub from: Option<i64>,
    pub to: Option<i64>,
}

impl Db {
    /// Logs already read by fights pass `version`.
    pub async fn fight_logs(&self, version: i64) -> Result<Vec<i64>> {
        let rows = sqlx::query("SELECT log_id FROM fight_log WHERE version = ?1").bind(version).fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|r| r.get("log_id")).collect())
    }

    /// Replace several logs' fights-pass output -- each player's counts, each
    /// kill's situation, each kill's credit shares -- in one transaction.
    ///
    /// It was two transactions a log with a statement per row: 363,000 credit
    /// rows and 194,000 situations on a rebuild, each its own round trip, and
    /// a commit per log that on its own cost more than the writing (see
    /// `replace_kill_events_many`). Callers pass a group at a time, so a
    /// failure loses one group's work and a rerun redoes it.
    pub async fn replace_fights_many(&self, version: i64, logs: &[FightsWrite]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        for w in logs {
            write_fights(&mut tx, version, w).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// One log's kill credits: `seq -> [(account, share)]`.
    pub async fn kill_credits(&self, log_id: i64) -> Result<HashMap<i64, Vec<(u32, f64)>>> {
        let rows: Vec<(i64, i64, f64)> =
            sqlx::query_as("SELECT seq, account_id, share FROM kill_credit WHERE log_id = ?1")
                .bind(log_id)
                .fetch_all(self.pool())
                .await?;
        let mut out: HashMap<i64, Vec<(u32, f64)>> = HashMap::new();
        for (seq, acc, share) in rows {
            out.entry(seq).or_default().push((acc as u32, share));
        }
        Ok(out)
    }

    /// Every stored kill credit: per log, `seq -> [(account, share)]`.
    pub async fn all_kill_credits(&self) -> Result<HashMap<i64, HashMap<i64, Vec<(u32, f64)>>>> {
        let rows: Vec<(i64, i64, i64, f64)> =
            sqlx::query_as("SELECT log_id, seq, account_id, share FROM kill_credit").fetch_all(self.pool()).await?;
        let mut out: HashMap<i64, HashMap<i64, Vec<(u32, f64)>>> = HashMap::new();
        for (log, seq, acc, share) in rows {
            out.entry(log).or_default().entry(seq).or_default().push((acc as u32, share));
        }
        Ok(out)
    }

    /// Every stored kill situation: per log, `seq -> (diff, adv)`.
    pub async fn all_kill_situations(&self) -> Result<HashMap<i64, HashMap<i64, (i8, i8)>>> {
        let rows: Vec<(i64, i64, i64, i64)> = sqlx::query_as("SELECT log_id, seq, diff, adv FROM kill_situation")
            .fetch_all(self.pool())
            .await?;
        let mut out: HashMap<i64, HashMap<i64, (i8, i8)>> = HashMap::new();
        for (log, seq, diff, adv) in rows {
            out.entry(log).or_default().insert(seq, (diff as i8, adv as i8));
        }
        Ok(out)
    }

    /// One log's kill situations, `seq -> (diff, adv)`.
    pub async fn kill_situations(&self, log_id: i64) -> Result<HashMap<i64, (i8, i8)>> {
        let rows: Vec<(i64, i64, i64)> = sqlx::query_as("SELECT seq, diff, adv FROM kill_situation WHERE log_id = ?1")
            .bind(log_id)
            .fetch_all(self.pool())
            .await?;
        Ok(rows.into_iter().map(|(seq, diff, adv)| (seq, (diff as i8, adv as i8))).collect())
    }

    /// Counts summed over rated performances on one class: the owner's, and
    /// everyone else's (the pool the owner is rated against), under one filter.
    pub async fn fight_totals(&self, me: u32, f: &FightFilter<'_>) -> Result<(FightTotals, FightTotals)> {
        let sums = FIGHT_COLUMNS.iter().map(|c| format!("SUM(s.{c}) AS {c}")).collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT (r.account_id = ?1) AS mine, COUNT(*) AS games, SUM(r.minutes) AS minutes, {sums}
             FROM rating r
             JOIN fight_stat s   ON s.log_id = r.log_id AND s.account_id = r.account_id
             JOIN match m        ON m.log_id = r.log_id
             JOIN log_index i    ON i.log_id = r.log_id
             LEFT JOIN match_context c ON c.log_id = r.log_id
             WHERE r.class = ?2 AND r.model_version = ?3 AND i.superseded_by IS NULL
               AND (?4 IS NULL OR c.kind = ?4)
               AND (?5 IS NULL OR m.played_at >= ?5)
               AND (?6 IS NULL OR m.played_at <= ?6)
             GROUP BY mine"
        );
        let rows = sqlx::query(&sql)
            .bind(me as i64)
            .bind(f.class)
            .bind(f.model_version)
            .bind(f.kind)
            .bind(f.from)
            .bind(f.to)
            .fetch_all(self.pool())
            .await?;
        let mut mine = FightTotals { values: vec![0; FIGHT_COLUMNS.len()], ..Default::default() };
        let mut pool = mine.clone();
        for r in rows {
            let t = FightTotals {
                games: r.get("games"),
                minutes: r.get::<Option<f64>, _>("minutes").unwrap_or(0.0),
                values: FIGHT_COLUMNS.iter().map(|c| r.get::<Option<i64>, _>(*c).unwrap_or(0)).collect(),
            };
            if r.get::<i64, _>("mine") == 1 {
                mine = t;
            } else {
                pool = t;
            }
        }
        Ok((mine, pool))
    }

    /// Per player, for every log the fights pass has read or for one log:
    /// `(account, [opening kills, opening deaths, kills, traded kills, deaths,
    /// traded deaths, deaths to flankers, stationary deaths, fights present,
    /// KAST fights, engaged KAST fights])`.
    pub async fn fight_counts(&self, log_id: Option<i64>) -> Result<std::collections::HashMap<i64, Vec<(u32, [u32; 14])>>> {
        // One log is whatever format it is; every log means the Highlander ones.
        let rows = sqlx::query(&format!(
            "SELECT log_id, account_id, opening_kills, opening_deaths, kills, traded_kills,
                    deaths, traded_deaths, deaths_to_flank, stationary_deaths,
                    fights_present, fights_kast, fights_kast_engaged,
                    caps_contested, caps_mates_dead, caps_spawn_delay
             FROM fight_stat WHERE log_id = ?1 OR (?1 IS NULL AND {})",
            crate::hl_only("log_id")
        ))
        .bind(log_id)
        .fetch_all(self.pool())
        .await?;
        let mut out: std::collections::HashMap<i64, Vec<(u32, [u32; 14])>> = std::collections::HashMap::new();
        for r in rows {
            let n = |c: &str| r.get::<i64, _>(c) as u32;
            out.entry(r.get("log_id")).or_default().push((
                r.get::<i64, _>("account_id") as u32,
                [
                    n("opening_kills"),
                    n("opening_deaths"),
                    n("kills"),
                    n("traded_kills"),
                    n("deaths"),
                    n("traded_deaths"),
                    n("deaths_to_flank"),
                    n("stationary_deaths"),
                    n("fights_present"),
                    n("fights_kast"),
                    n("fights_kast_engaged"),
                    n("caps_contested"),
                    n("caps_mates_dead"),
                    n("caps_spawn_delay"),
                ],
            ));
        }
        Ok(out)
    }

    /// The owner's Highlander officials with a time, oldest first.
    pub async fn officials_played(&self) -> Result<Vec<SeasonOfficial>> {
        let rows = sqlx::query(
            "SELECT competition, division, time FROM etf2l_match
             WHERE comp_type = 'Highlander' AND competition IS NOT NULL AND time IS NOT NULL
             ORDER BY time",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SeasonOfficial { competition: r.get("competition"), division: r.get("division"), time: r.get("time") })
            .collect())
    }

    /// Every rated game of one player on one class, with the box score on that class.
    pub async fn class_games(&self, account_id: u32, class: &str, version: &str) -> Result<Vec<ClassGame>> {
        let rows = sqlx::query(
            "SELECT r.log_id, m.played_at, c.kind, p.team, m.red_score, m.blue_score, r.score, r.minutes,
                    pc.kills, pc.deaths, pc.assists, pc.dmg, pc.time_s
             FROM rating r
             JOIN match m        ON m.log_id = r.log_id
             JOIN log_index i    ON i.log_id = r.log_id
             JOIN match_player p ON p.log_id = r.log_id AND p.account_id = r.account_id
             JOIN match_player_class pc ON pc.log_id = r.log_id AND pc.account_id = r.account_id AND pc.class = r.class
             LEFT JOIN match_context c ON c.log_id = r.log_id
             WHERE r.account_id = ?1 AND r.class = ?2 AND r.model_version = ?3 AND i.superseded_by IS NULL
             ORDER BY m.played_at",
        )
        .bind(account_id as i64)
        .bind(class)
        .bind(version)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let team: String = r.get("team");
                let (red, blue): (Option<i64>, Option<i64>) = (r.get("red_score"), r.get("blue_score"));
                let (mine, theirs) = if team == "Red" { (red, blue) } else { (blue, red) };
                let result = match (mine, theirs) {
                    (Some(a), Some(b)) if a > b => Some("W"),
                    (Some(a), Some(b)) if a < b => Some("L"),
                    (Some(_), Some(_)) => Some("T"),
                    _ => None,
                };
                ClassGame {
                    log_id: r.get("log_id"),
                    played_at: r.get("played_at"),
                    kind: r.get("kind"),
                    result,
                    score: r.get("score"),
                    minutes: r.get("minutes"),
                    kills: r.get("kills"),
                    deaths: r.get("deaths"),
                    assists: r.get("assists"),
                    dmg: r.get("dmg"),
                    time_s: r.get("time_s"),
                }
            })
            .collect())
    }
}

/// One log's fights-pass output, for [`Db::replace_fights_many`].
#[derive(Default)]
pub struct FightsWrite {
    pub log_id: i64,
    pub rows: Vec<FightRow>,
    /// `(seq, diff, adv)` per kill.
    pub situations: Vec<(i64, i8, i8)>,
    /// `(seq, account, share)` per contributor per kill (Q6b).
    pub credits: Vec<(i64, u32, f64)>,
}

/// Replace one log's fights-pass output inside an open transaction.
async fn write_fights(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, version: i64, w: &FightsWrite) -> Result<()> {
    let log_id = w.log_id;
    for table in ["fight_stat", "kill_situation", "kill_credit"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE log_id = ?1")).bind(log_id).execute(&mut **tx).await?;
    }
    for batch in w.situations.chunks(BATCH_ROWS) {
        let mut q = sqlx::QueryBuilder::<sqlx::Sqlite>::new("INSERT INTO kill_situation (log_id, seq, diff, adv) ");
        q.push_values(batch, |mut b, &(seq, diff, adv)| {
            b.push_bind(log_id).push_bind(seq).push_bind(i64::from(diff)).push_bind(i64::from(adv));
        });
        q.build().execute(&mut **tx).await?;
    }
    for batch in w.rows.chunks(BATCH_ROWS) {
        let mut q = sqlx::QueryBuilder::<sqlx::Sqlite>::new(format!(
            "INSERT INTO fight_stat (log_id, account_id, {}) ",
            FIGHT_COLUMNS.join(", ")
        ));
        q.push_values(batch, |mut b, r| {
            b.push_bind(log_id).push_bind(r.account_id as i64);
            for v in r.values {
                b.push_bind(v);
            }
        });
        q.build().execute(&mut **tx).await?;
    }
    for batch in w.credits.chunks(BATCH_ROWS) {
        // OR REPLACE as before: a contributor can be credited twice for one
        // kill, and the later share is the one kept.
        let mut q = sqlx::QueryBuilder::<sqlx::Sqlite>::new("INSERT OR REPLACE INTO kill_credit (log_id, seq, account_id, share) ");
        q.push_values(batch, |mut b, &(seq, account, share)| {
            b.push_bind(log_id).push_bind(seq).push_bind(i64::from(account)).push_bind(share);
        });
        q.build().execute(&mut **tx).await?;
    }
    sqlx::query("INSERT OR REPLACE INTO fight_log (log_id, version) VALUES (?1, ?2)")
        .bind(log_id)
        .bind(version)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
