//! Rating every stored performance, and loading a player's profile.
//!
//! Rating is two passes over the stored logs, because a percentile needs the
//! whole pool before any single value can be placed in it:
//!
//! ```text
//! 1. normalize every kept Highlander log, extract every rateable performance
//! 2. build per-class baselines from all of them except the owner's
//! 3. score every performance against those baselines
//! ```
//!
//! No network. Runs after every sync and reprocess.

use crate::normalize::normalize;
use crate::Progress;
use anyhow::{Context, Result};
use hl_core::{SteamId, TfClass};
use hl_db::{Db, RatingRow};
use hl_rating::model::{extract, rate, Baseline, Component, Performance, Scale};
use hl_rating::profile::{self, Extra, HistoryRow};
use hl_rating::{Profile, Rating, Weights, MODEL_VERSION};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateSummary {
    pub logs: usize,
    pub performances: usize,
    pub rated: usize,
    /// The owner's rated games.
    pub mine: usize,
}

pub async fn rate_all(
    db: &Db,
    me: Option<SteamId>,
    w: &Weights,
    mut progress: impl FnMut(Progress),
) -> Result<RateSummary> {
    let summary = rate_all_inner(db, me, w, &mut progress).await?;
    // Every log's numbers for the season tables and profiles: only the logs
    // that have none yet, so after the first pass this is the new ones.
    progress(Progress::Stage { what: "Reading every player's numbers" });
    crate::logstat::fill_missing(db, None, |_, _| {}).await?;
    Ok(summary)
}

async fn rate_all_inner(
    db: &Db,
    me: Option<SteamId>,
    w: &Weights,
    progress: &mut impl FnMut(Progress),
) -> Result<RateSummary> {
    // Pass 1: every rateable performance in every kept Highlander log.
    let (total, perfs) = collect_performances(db, w, &mut *progress).await?;
    // ...and in the league sample: officials from every ETF2L division,
    // so the pool is the league and not only the owner's matches.
    progress(Progress::Stage { what: "Rating the league sample" });
    let league = crate::league_rating::performances(db, w).await?;
    tracing::info!(own = perfs.len(), league = league.len(), "rating pool");

    // An install rating on the league snapshot's scale (Q43) keeps it while
    // its own pool is the smaller: measuring a few hundred of its own games
    // would undo the league's "1.00".
    if let Some(snapshot) = crate::snapshot::pool(db).await? {
        if ((perfs.len() + league.len()) as i64) < snapshot {
            if let Some(summary) = rate_on_stored_scale(db, me, w, total, &perfs, &league).await? {
                progress(Progress::Rating { done: total, total });
                return Ok(summary);
            }
        }
    }

    // Pass 2: the pools. The owner is in them now (Flashy): with the league
    // in the pool they are one player among thousands, where in their own
    // matches alone they were half of their class's pool and were left out
    // so as not to be measured against themselves.
    let baseline = Baseline::build(perfs.iter().chain(league.iter()).map(|(_, p)| p), None);
    let stored: Vec<(String, String, Option<String>, Vec<f64>)> = baseline
        .parts()
        .map(|(class, c, map, vals)| {
            (class.as_str().to_string(), c.key().to_string(), map.map(str::to_string), vals.to_vec())
        })
        .collect();
    db.replace_baselines(MODEL_VERSION, &stored).await?;

    // Pass 3: score everyone, the owner included.
    let mut scored: Vec<(i64, u32, hl_rating::Rating)> = Vec::with_capacity(perfs.len());
    let mut mine = 0;
    for (log_id, perf) in &perfs {
        let Some(r) = rate(perf, &baseline, w) else { continue };
        if me.is_some_and(|m| m.account_id() == perf.account_id) {
            mine += 1;
        }
        scored.push((*log_id, perf.account_id, r));
    }

    // Pass 4: the scale, and the ratings themselves. A rating says how far
    // from an ordinary game this was, so it needs the pool's middle and
    // spread — measured here and stored, so a game rated on its own as it
    // arrives lands on the same scale as the games it is listed beside.
    // Measured over the league too: 1.00 is a typical league game. Only the
    // owner's own logs' ratings are stored; the sample's are not theirs.
    let league_rated: Vec<(i64, u32, hl_rating::Rating)> =
        league.iter().filter_map(|(log_id, p)| Some((*log_id, p.account_id, rate(p, &baseline, w)?))).collect();
    let raw: Vec<f64> = scored.iter().chain(league_rated.iter()).map(|(_, _, r)| r.score).collect();
    let Some(scale) = Scale::of(&raw) else {
        // Nothing to measure: leave the last scale and the last ratings alone
        // rather than writing numbers that mean nothing.
        return Ok(RateSummary { logs: total, performances: perfs.len(), rated: 0, mine: 0 });
    };
    db.replace_rating_scale(MODEL_VERSION, scale.mean, scale.sd, raw.len()).await?;

    let mut rows = Vec::with_capacity(scored.len());
    for (log_id, account_id, r) in scored {
        let r = r.scaled(&scale);
        rows.push(RatingRow {
            log_id,
            account_id,
            class: r.class.as_str(),
            score: r.score,
            minutes: r.minutes,
            parts_json: serde_json::to_string(&r.parts)?,
        });
    }
    db.replace_ratings(MODEL_VERSION, &rows).await?;

    // The league sample's ratings, apart from the owner's (Q36): every
    // player's rating, stat bars and rank, not only the players the owner met.
    let league_rows: Vec<(i64, u32, &str, f64, f64, String)> = league_rated
        .into_iter()
        .map(|(log_id, account, r)| {
            let r = r.scaled(&scale);
            (log_id, account, r.class.as_str(), r.score, r.minutes, hl_rating::guide::group_scores_json(&r.parts))
        })
        .collect();
    db.replace_league_ratings(MODEL_VERSION, &league_rows).await?;
    progress(Progress::Rating { done: total, total });

    Ok(RateSummary { logs: total, performances: perfs.len(), rated: rows.len(), mine })
}

/// Score this install's games, and whatever league logs it holds, against
/// the baselines and scale already stored (the league snapshot's), without
/// measuring new ones. `None` when there is nothing stored to score against.
async fn rate_on_stored_scale(
    db: &Db,
    me: Option<SteamId>,
    w: &Weights,
    total: usize,
    perfs: &[(i64, hl_rating::Performance)],
    league: &[(i64, hl_rating::Performance)],
) -> Result<Option<RateSummary>> {
    let baseline = load_baseline(db).await?;
    let Some(scale) = load_scale(db).await? else { return Ok(None) };
    if baseline.is_empty() {
        return Ok(None);
    }
    let mut rows = Vec::with_capacity(perfs.len());
    let mut mine = 0;
    for (log_id, perf) in perfs {
        let Some(r) = rate(perf, &baseline, w) else { continue };
        if me.is_some_and(|m| m.account_id() == perf.account_id) {
            mine += 1;
        }
        let r = r.scaled(&scale);
        rows.push(RatingRow {
            log_id: *log_id,
            account_id: perf.account_id,
            class: r.class.as_str(),
            score: r.score,
            minutes: r.minutes,
            parts_json: serde_json::to_string(&r.parts)?,
        });
    }
    db.replace_ratings(MODEL_VERSION, &rows).await?;
    // The snapshot's league ratings stay; logs downloaded here join them.
    let league_rows: Vec<(i64, u32, &str, f64, f64, String)> = league
        .iter()
        .filter_map(|(log_id, p)| {
            let r = rate(p, &baseline, w)?.scaled(&scale);
            Some((*log_id, p.account_id, r.class.as_str(), r.score, r.minutes, hl_rating::guide::group_scores_json(&r.parts)))
        })
        .collect();
    db.put_league_ratings(MODEL_VERSION, &league_rows).await?;
    Ok(Some(RateSummary { logs: total, performances: perfs.len(), rated: rows.len(), mine }))
}

/// Rate a few logs now, against the baselines already stored.
///
/// A sync downloads newest first, so a match you played last night appears in
/// the list within seconds — and used to sit there with an empty rating until
/// the whole corpus was re-rated at the end, minutes later. This fills it in
/// as it lands.
///
/// The number is real, not a placeholder. It is measured against the pool as
/// it stood at the last full pass, which a handful of new games barely move;
/// and the components that need the raw server log — openings, trades, kill
/// situations — are *skipped* rather than scored as zero, exactly as they are
/// for the logs logs.tf never had a raw log for. The full pass at the end of
/// the sync refines both.
///
/// Returns how many performances were scored. Does nothing before the first
/// full pass, when there are no baselines to measure against.
pub async fn rate_logs(db: &Db, w: &Weights, log_ids: &[i64]) -> Result<usize> {
    let baseline = load_baseline(db).await?;
    // Both come from the last full pass, and neither exists before it.
    let Some(scale) = load_scale(db).await? else { return Ok(0) };
    if baseline.is_empty() || log_ids.is_empty() {
        return Ok(0);
    }
    let mut scored = 0;
    for &log_id in log_ids {
        let Some(json) = db.raw_log(log_id).await? else { continue };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else { continue };
        let Ok(log) = normalize(log_id, &value) else { continue };
        let pool_map = log.map.as_deref().map(hl_core::maps::map_base);

        // Per log rather than the whole corpus: this runs between fetches.
        let kills = db.kills_for_log(log_id).await?;
        let situations = db.kill_situations(log_id).await?;
        let credits = db.kill_credits(log_id).await?;
        let windows = db.round_windows(log_id).await?;
        let mut impact =
            crate::kills::impacts_for(&kills, Some(&situations), &windows, log.map.as_deref(), w);
        crate::kills::attach_shared_swing(&mut impact, &kills, Some(&situations), Some(&credits), w);
        let fights = db.fight_counts(Some(log_id)).await?;
        crate::kills::attach_fights(&mut impact, fights.get(&log_id).map_or(&[][..], |f| f.as_slice()));

        let rows: Vec<RatingRow> = log
            .players
            .iter()
            .filter_map(|p| extract(p, &log.flags, w, impact.get(&p.id.account_id()), pool_map.as_deref()))
            .filter_map(|perf| {
                let r = rate(&perf, &baseline, w)?.scaled(&scale);
                Some(RatingRow {
                    log_id,
                    account_id: perf.account_id,
                    class: perf.class.as_str(),
                    score: r.score,
                    minutes: r.minutes,
                    parts_json: serde_json::to_string(&r.parts).ok()?,
                })
            })
            .collect();
        scored += rows.len();
        db.put_ratings_for_log(MODEL_VERSION, log_id, &rows).await?;
    }
    Ok(scored)
}

/// Pass 1 of rating: every rateable performance in every kept Highlander log,
/// with the components `w` names for each class. Kills from raw logs are
/// valued one by one, and fight counts attached, where a raw log exists.
/// Returns the number of logs read and the performances.
pub async fn collect_performances(
    db: &Db,
    w: &Weights,
    mut progress: impl FnMut(Progress),
) -> Result<(usize, Vec<(i64, Performance)>)> {
    let ids = db.rateable_log_ids().await?;
    let total = ids.len();

    let kills = db.all_kills().await?;
    let windows = db.all_round_windows().await?;
    let fights = db.fight_counts(None).await?;
    let situations = db.all_kill_situations().await?;
    let credits = db.all_kill_credits().await?;
    let mut perfs: Vec<(i64, Performance)> = Vec::new();
    for (i, log_id) in ids.iter().copied().enumerate() {
        if i % 25 == 0 {
            progress(Progress::Rating { done: i, total });
        }
        let Some(json) = db.raw_log(log_id).await? else { continue };
        let value: serde_json::Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(log_id, error = %e, "skipping unparseable stored log");
                continue;
            }
        };
        let Ok(log) = normalize(log_id, &value) else { continue };
        let pool_map = log.map.as_deref().map(hl_core::maps::map_base);
        let log_kills = kills.get(&log_id).map_or(&[][..], |k| k.as_slice());
        let mut impact = crate::kills::impacts_for(
            log_kills,
            situations.get(&log_id),
            windows.get(&log_id).map_or(&[][..], |k| k.as_slice()),
            log.map.as_deref(),
            w,
        );
        crate::kills::attach_shared_swing(&mut impact, log_kills, situations.get(&log_id), credits.get(&log_id), w);
        crate::kills::attach_fights(&mut impact, fights.get(&log_id).map_or(&[][..], |f| f.as_slice()));
        perfs.extend(
            log.players
                .iter()
                .filter_map(|p| extract(p, &log.flags, w, impact.get(&p.id.account_id()), pool_map.as_deref()))
                .map(|p| (log_id, p)),
        );
    }

    Ok((total, perfs))
}

/// The stored scale for the current model; `None` until the first full pass.
pub async fn load_scale(db: &Db) -> Result<Option<Scale>> {
    Ok(db.rating_scale(MODEL_VERSION).await?.map(|(mean, sd)| Scale { mean, sd }))
}

/// The stored baselines for the current model; empty until the first rating pass.
pub async fn load_baseline(db: &Db) -> Result<Baseline> {
    let parts = db
        .load_baselines(MODEL_VERSION)
        .await?
        .into_iter()
        .filter_map(|(class, comp, map, vals)| {
            Some((TfClass::parse(&class).ok()?, Component::parse(&comp)?, map, vals))
        });
    Ok(Baseline::from_parts(parts))
}

/// Classes the player has rated games on, most played first.
pub async fn rated_classes(db: &Db, me: SteamId) -> Result<Vec<(String, i64)>> {
    db.rated_classes(me.account_id(), MODEL_VERSION).await
}

/// The owner's profile on one class; `kind` narrows it to officials, scrims
/// or pugs, and `period` to games played between two unix times (a season).
pub async fn load_profile(
    db: &Db,
    me: SteamId,
    class: TfClass,
    kind: Option<&str>,
    period: Option<(i64, i64)>,
) -> Result<Option<Profile>> {
    let rows = db.rating_history(me.account_id(), class.as_str(), MODEL_VERSION).await?;
    let history: Vec<HistoryRow> = rows
        .into_iter()
        .map(|r| {
            let parts = serde_json::from_str(&r.parts_json).context("stored rating parts")?;
            let (mine, theirs) = if r.team == "Red" {
                (r.red_score, r.blue_score)
            } else {
                (r.blue_score, r.red_score)
            };
            let result = match (mine, theirs) {
                (Some(a), Some(b)) if a > b => Some("W"),
                (Some(a), Some(b)) if a < b => Some("L"),
                (Some(_), Some(_)) => Some("T"),
                _ => None,
            };
            Ok(HistoryRow {
                log_id: r.log_id,
                played_at: r.played_at,
                map: r.map,
                title: r.title,
                league: r.league,
                kind: r.kind,
                result: result.map(str::to_string),
                rating: Rating { class, score: r.score, minutes: r.minutes, parts },
            })
        })
        .collect::<Result<_>>()?;

    let history: Vec<HistoryRow> = match period {
        Some((from, to)) => history.into_iter().filter(|r| r.played_at.is_some_and(|t| t >= from && t <= to)).collect(),
        None => history,
    };
    let contexts = profile::context_splits(&history);
    // Q9: who each of those games was against. Computed over the whole
    // history like the context split, so it stays comparable while the rest
    // of the profile is filtered.
    let opponents = db.class_opponents(me.account_id(), class.as_str(), MODEL_VERSION).await?;
    let strength: std::collections::HashMap<i64, f64> = opponents
        .iter()
        .filter(|(_, _, _, games)| *games >= profile::MIN_OPPONENT_GAMES)
        .map(|(log_id, _, strength, _)| (*log_id, *strength))
        .collect();
    let opposition = profile::opposition_splits(&history, &strength);
    // ...and their ETF2L division at the time: any opposite number with a
    // division, however few games of theirs are rated.
    let when: std::collections::HashMap<i64, i64> = history.iter().filter_map(|r| Some((r.log_id, r.played_at?))).collect();
    let pairs: Vec<(i64, u32, i64)> = opponents.iter().filter_map(|(log_id, opp, _, _)| Some((*log_id, *opp, *when.get(log_id)?))).collect();
    let tiers = crate::catalogue::opponent_divisions(db, &pairs).await?;
    let by_division = profile::division_splits(&history, &tiers);
    let history: Vec<HistoryRow> = match kind {
        Some(k) => history.into_iter().filter(|r| r.kind.as_deref() == Some(k)).collect(),
        None => history,
    };
    let Some(mut p) = profile::build(class, history) else {
        return Ok(None);
    };
    p.contexts = contexts;
    p.opposition = opposition;
    p.by_division = by_division;
    p.filter = kind.map(str::to_string);
    // Career records count every game, so a period shows none.
    if period.is_some() {
        return Ok(Some(p));
    }

    // Career records that read straight off the logs, no model involved.
    let mirror = db.vs_totals(me.account_id(), class.as_str(), class.as_str(), MODEL_VERSION).await?;
    if mirror.kills + mirror.deaths > 0 {
        let label = if class == TfClass::Sniper {
            "Sniper duel, career".to_string()
        } else {
            format!("vs enemy {}, career", class.display_name())
        };
        let net = mirror.kills - mirror.deaths;
        p.extras.push(Extra {
            label,
            value: format!("{} – {}", mirror.kills, mirror.deaths),
            detail: Some(format!("{}{} net", if net > 0 { "+" } else { "" }, net)),
            hint: Some(format!(
                "Your kills on the enemy {c} against their kills on you, across every rated game as {c}.",
                c = class.display_name()
            )),
        });
    }
    if class != TfClass::Medic {
        let medic = db.vs_totals(me.account_id(), class.as_str(), "medic", MODEL_VERSION).await?;
        p.extras.push(Extra {
            label: "Medic picks, career".to_string(),
            value: medic.kills.to_string(),
            detail: Some(format!("{:.2} per game", medic.kills as f64 / p.games as f64)),
            hint: Some("Enemy Medics you killed, across every rated game on this class.".to_string()),
        });
    }
    Ok(Some(p))
}

/// The profile's headline numbers on one class -- what trends.tf leads with:
/// time played, wins and losses, damage per minute, accuracy, K/D -- over the
/// same games as the rest of the profile (its class, kind and period).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Headline {
    pub games: usize,
    /// On this class, in these games.
    pub seconds: i64,
    pub won: usize,
    pub lost: usize,
    pub tied: usize,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub damage: i64,
    pub dpm: Option<f64>,
    /// Hits over shots, from logs.tf's weapon stats; only logs with the
    /// accuracy plugin have them.
    pub accuracy: Option<f64>,
    pub accuracy_games: usize,
}

pub async fn headline(db: &Db, me: SteamId, class: TfClass, kind: Option<&str>, period: Option<(i64, i64)>) -> Result<Option<Headline>> {
    let account = me.account_id();
    let rows: Vec<_> = db
        .rating_history(account, class.as_str(), MODEL_VERSION)
        .await?
        .into_iter()
        .filter(|r| kind.is_none_or(|k| r.kind.as_deref() == Some(k)))
        .filter(|r| period.is_none_or(|(from, to)| r.played_at.is_some_and(|t| t >= from && t <= to)))
        .collect();
    if rows.is_empty() {
        return Ok(None);
    }
    let (mut won, mut lost, mut tied) = (0, 0, 0);
    for r in &rows {
        let (us, them) = if r.team == "Red" { (r.red_score, r.blue_score) } else { (r.blue_score, r.red_score) };
        match (us, them) {
            (Some(a), Some(b)) if a > b => won += 1,
            (Some(a), Some(b)) if a < b => lost += 1,
            (Some(_), Some(_)) => tied += 1,
            _ => {}
        }
    }
    let ids: Vec<i64> = rows.iter().map(|r| r.log_id).collect();
    let t = db.class_totals(account, class.as_str(), &ids).await?;
    let (mut shots, mut hits, mut accuracy_games) = (0i64, 0i64, 0usize);
    for (_, json) in db.raw_jsons(&ids).await? {
        if let Some((s, h)) = weapon_accuracy(&json, account, class) {
            shots += s;
            hits += h;
            accuracy_games += 1;
        }
    }
    Ok(Some(Headline {
        games: rows.len(),
        seconds: t.seconds,
        won,
        lost,
        tied,
        kills: t.kills,
        deaths: t.deaths,
        assists: t.assists,
        damage: t.damage,
        dpm: (t.seconds > 0).then(|| t.damage as f64 * 60.0 / t.seconds as f64),
        accuracy: (shots > 0).then(|| hits as f64 / shots as f64),
        accuracy_games,
    }))
}

/// `account`'s shots and hits on `class` in one logs.tf log, from the weapon
/// stats under `class_stats`; `None` where the log has none for them.
fn weapon_accuracy(json: &str, account: u32, class: TfClass) -> Option<(i64, i64)> {
    let raw: serde_json::Value = serde_json::from_str(json).ok()?;
    let players = raw.get("players")?.as_object()?;
    let me = players.iter().find(|(k, _)| SteamId::parse(k).is_ok_and(|id| id.account_id() == account))?.1;
    let (mut shots, mut hits) = (0, 0);
    for cs in me.get("class_stats")?.as_array()? {
        let Some(t) = cs.get("type").and_then(|v| v.as_str()) else { continue };
        if TfClass::parse(t).ok() != Some(class) {
            continue;
        }
        for w in cs.get("weapon").and_then(|w| w.as_object()).into_iter().flatten().map(|(_, w)| w) {
            shots += w.get("shots").and_then(|v| v.as_i64()).unwrap_or(0);
            hits += w.get("hits").and_then(|v| v.as_i64()).unwrap_or(0);
        }
    }
    (shots > 0).then_some((shots, hits))
}

#[cfg(test)]
mod headline_tests {
    use super::*;

    #[test]
    fn accuracy_sums_the_class_weapons_only() {
        let json = r#"{"players": {"[U:1:42]": {"class_stats": [
            {"type": "sniper", "weapon": {"sniperrifle": {"shots": 100, "hits": 40}, "smg": {"shots": 50, "hits": 10}}},
            {"type": "scout", "weapon": {"scattergun": {"shots": 30, "hits": 20}}}
        ]}, "[U:1:7]": {"class_stats": [{"type": "sniper", "weapon": {"sniperrifle": {"shots": 9, "hits": 9}}}]}}}"#;
        assert_eq!(weapon_accuracy(json, 42, TfClass::Sniper), Some((150, 50)));
        assert_eq!(weapon_accuracy(json, 42, TfClass::Medic), None, "no shots on a class not played");
        assert_eq!(weapon_accuracy(json, 99, TfClass::Sniper), None, "not in the log");
    }
}
