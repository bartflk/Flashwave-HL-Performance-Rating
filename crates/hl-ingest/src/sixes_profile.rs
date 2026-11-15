//! The owner's 6v6 profile, to the Highlander profile's standard (Flashy:
//! "i dont see this back on the 6v6 page. please bring them up to standard").
//!
//! The same [`hl_rating::profile`] summary -- form, career, trend, best and
//! worst games, officials/scrims/pugs, who you played -- built from
//! `sixes_rating` by role instead of `rating` by class, so the profile page
//! draws both formats with the same components. What differs is the
//! breakdown: 6v6's components are its own (`weights.sixes.toml`), so they
//! are summarised here, with their labels and units.

use crate::rating::{headline_of, Headline};
use crate::sixes_rating::model_version;
use anyhow::Result;
use hl_core::{Role, SteamId};
use hl_rating::model::Rating;
use hl_rating::profile::{self, HistoryRow, MIN_OPPONENT_GAMES};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// A 6v6 component's label and unit, for the profile's breakdown.
pub fn component_label(c: &str) -> (&'static str, &'static str) {
    match c {
        "round_impact" => ("Round impact", "per 10 min"),
        "impact_kills" => ("Kills", "per 10 min"),
        "impact_assists" => ("Assists", "per 10 min"),
        "deaths" => ("Deaths", "per 10 min"),
        "dpm" => ("Damage per minute", "dmg/min"),
        "pressure" => ("Pressure", "dmg/min"),
        "space" => ("Space created", "of the team's damage taken"),
        "airshots" => ("Airshots", "per 10 min"),
        "dmg_ratio" => ("Damage ratio", "dealt per taken"),
        "medic_picks" => ("Medic picks", "per 10 min"),
        "deaths_vs_medic" => ("Deaths beside the Medic", "per 10 min"),
        "traps" => ("Sticky kills", "per 10 min"),
        "sentry" => ("Sentry kills", "per 10 min"),
        "reflects" => ("Reflect kills", "per 10 min"),
        "eng_deaths" => ("Engineer deaths", "per 10 min"),
        "headshot_share" => ("Headshot share", "of kills"),
        "backstabs" => ("Backstabs", "per 10 min"),
        "heal" => ("Healing per minute", "heal/min"),
        "ubers_vs" => ("Übers against theirs", "more"),
        "drops_vs" => ("Drops against theirs", "more"),
        "deaths_vs" => ("Deaths against theirs", "more"),
        "adv_lost" => ("Advantages lost", "per game"),
        _ => ("", ""),
    }
}

/// Components stored as a share, 0-1: shown as a percentage.
fn is_share(c: &str) -> bool {
    matches!(c, "space" | "headshot_share")
}

/// One 6v6 component's average over recent form and the career.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SixesComponent {
    pub component: String,
    pub label: String,
    pub unit: String,
    pub weight: f64,
    pub form_pct: f64,
    pub career_pct: f64,
    pub form_raw: f64,
}

/// A stored part: (component, percentile, share, raw); ratings stored
/// before the raw value was kept have three.
fn parts_of(json: &str) -> Vec<(String, f64, f64, Option<f64>)> {
    let rows: Vec<Vec<Value>> = serde_json::from_str(json).unwrap_or_default();
    rows.into_iter()
        .filter_map(|r| Some((r.first()?.as_str()?.to_string(), r.get(1)?.as_f64()?, r.get(2)?.as_f64()?, r.get(3).and_then(Value::as_f64))))
        .collect()
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// The breakdown: each component's share and percentile over the last
/// `form` games (oldest first in `games`) and the whole career.
fn components(games: &[Vec<(String, f64, f64, Option<f64>)>], form: usize) -> Vec<SixesComponent> {
    #[derive(Default)]
    struct Acc {
        pct: Vec<f64>,
        form_pct: Vec<f64>,
        form_raw: Vec<f64>,
        form_share: Vec<f64>,
    }
    let start = games.len().saturating_sub(form);
    let mut by: BTreeMap<String, Acc> = BTreeMap::new();
    for (i, parts) in games.iter().enumerate() {
        for (c, pct, share, raw) in parts {
            let a = by.entry(c.clone()).or_default();
            a.pct.push(*pct);
            if i >= start {
                a.form_pct.push(*pct);
                a.form_share.push(*share);
                if let Some(r) = raw {
                    a.form_raw.push(if is_share(c) { r * 100.0 } else { *r });
                }
            }
        }
    }
    let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    let mut out: Vec<SixesComponent> = by
        .into_iter()
        .filter(|(_, a)| !a.form_pct.is_empty())
        .map(|(c, a)| {
            let (label, unit) = component_label(&c);
            SixesComponent {
                label: if label.is_empty() { c.replace('_', " ") } else { label.to_string() },
                unit: unit.to_string(),
                weight: (mean(&a.form_share) * 1000.0).round() / 1000.0,
                form_pct: round1(mean(&a.form_pct)),
                career_pct: round1(mean(&a.pct)),
                form_raw: (mean(&a.form_raw) * 100.0).round() / 100.0,
                component: c,
            }
        })
        .collect();
    out.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    out
}

/// The owner's 6v6 profile in one role, the Highlander profile's shape:
/// the profile as JSON (its `class` the role's key, its `components`
/// 6v6's), and the headline numbers on the role's class.
pub async fn load(
    db: &hl_db::Db,
    me: SteamId,
    role: &str,
    kind: Option<&str>,
    period: Option<(i64, i64)>,
) -> Result<(Option<Value>, Option<Headline>)> {
    load_for(db, me.account_id(), role, kind, period, false).await
}

/// [`load`] for any player (ETF2L › 6v6 › Players): their games in the
/// owner's logs and in the 6v6 league sample, the headline numbers read
/// from each game's logs.tf JSON (the sample's logs have no stored lines).
pub async fn load_player(db: &hl_db::Db, account: u32, role: &str, kind: Option<&str>, period: Option<(i64, i64)>) -> Result<(Option<Value>, Option<Headline>)> {
    load_for(db, account, role, kind, period, true).await
}

async fn load_for(
    db: &hl_db::Db,
    account: u32,
    role: &str,
    kind: Option<&str>,
    period: Option<(i64, i64)>,
    league: bool,
) -> Result<(Option<Value>, Option<Headline>)> {
    let Some(parsed) = Role::parse(role) else { return Ok((None, None)) };
    let class = parsed.class();
    let version = model_version();
    let mut db_rows = db.sixes_rating_history(account, role, &version).await?;
    let headline = if league {
        db_rows.extend(db.sixes_league_history(account, role, &version).await?);
        headline_from_jsons(db, account, class, &db_rows, kind, period).await?
    } else {
        headline_of(db, account, class, db_rows.iter().map(clone_row).collect(), kind, period).await?
    };

    let parts: HashMap<i64, Vec<(String, f64, f64, Option<f64>)>> = db_rows.iter().map(|r| (r.log_id, parts_of(&r.parts_json))).collect();
    let career: Vec<(f64, Vec<(String, f64, f64, Option<f64>)>)> = db_rows.iter().map(|r| (r.minutes, parts_of(&r.parts_json))).collect();
    let history: Vec<HistoryRow> = db_rows
        .into_iter()
        .map(|r| {
            let (mine, theirs) = if r.team == "Red" { (r.red_score, r.blue_score) } else { (r.blue_score, r.red_score) };
            let result = match (mine, theirs) {
                (Some(a), Some(b)) if a > b => Some("W"),
                (Some(a), Some(b)) if a < b => Some("L"),
                (Some(_), Some(_)) => Some("T"),
                _ => None,
            };
            HistoryRow {
                log_id: r.log_id,
                played_at: r.played_at,
                map: r.map,
                title: r.title,
                league: r.league,
                kind: r.kind,
                result: result.map(str::to_string),
                rating: Rating { class, score: r.score, minutes: r.minutes, parts: Vec::new() },
            }
        })
        .collect();
    let history: Vec<HistoryRow> = match period {
        Some((from, to)) => history.into_iter().filter(|r| r.played_at.is_some_and(|t| t >= from && t <= to)).collect(),
        None => history,
    };
    let contexts = profile::context_splits(&history);
    let opponents = db.sixes_role_opponents(account, role, &version).await?;
    let strength: HashMap<i64, f64> =
        opponents.iter().filter(|(_, _, _, games)| *games >= MIN_OPPONENT_GAMES).map(|(log_id, _, s, _)| (*log_id, *s)).collect();
    let opposition = profile::opposition_splits(&history, &strength);
    let mut history: Vec<HistoryRow> = match kind {
        Some(k) => history.into_iter().filter(|r| r.kind.as_deref() == Some(k)).collect(),
        None => history,
    };
    history.sort_by_key(|r| (r.played_at.unwrap_or(0), r.log_id));
    let ordered: Vec<Vec<(String, f64, f64, Option<f64>)>> = history.iter().map(|r| parts.get(&r.log_id).cloned().unwrap_or_default()).collect();
    let Some(mut p) = profile::build(class, history) else { return Ok((None, headline)) };
    p.contexts = contexts;
    p.opposition = opposition;
    p.filter = kind.map(str::to_string);
    let comps = components(&ordered, p.form_window);
    // Career records, as Highlander's cards: every game in the role, so a
    // period shows none.
    if period.is_none() {
        p.extras = career_extras(parsed, &career);
    }
    let mut v = serde_json::to_value(&p)?;
    v["class"] = Value::String(role.to_string());
    v["components"] = serde_json::to_value(comps)?;
    Ok((Some(v), headline))
}

/// A per-10-minutes component summed back to a count over every game.
fn career_total(career: &[(f64, Vec<(String, f64, f64, Option<f64>)>)], c: &str) -> Option<(f64, usize)> {
    let mut total = 0.0;
    let mut games = 0;
    for (minutes, parts) in career {
        if let Some(raw) = parts.iter().find(|(k, ..)| k == c).and_then(|(.., raw)| *raw) {
            total += raw * minutes / 10.0;
            games += 1;
        }
    }
    (games > 0).then_some((total, games))
}

/// A component's career average per game, as stored (a rate, not a count).
fn career_average(career: &[(f64, Vec<(String, f64, f64, Option<f64>)>)], c: &str) -> Option<(f64, usize)> {
    let xs: Vec<f64> = career.iter().filter_map(|(_, parts)| parts.iter().find(|(k, ..)| k == c).and_then(|(.., raw)| *raw)).collect();
    (!xs.is_empty()).then(|| (xs.iter().sum::<f64>() / xs.len() as f64, xs.len()))
}

/// The two career cards beside form and career, chosen per role: what the
/// role is asked to do most (boSe.'s answers, the 6v6 plan).
fn career_extras(role: Role, career: &[(f64, Vec<(String, f64, f64, Option<f64>)>)]) -> Vec<profile::Extra> {
    let count = |c: &str, label: &str, hint: &str| {
        career_total(career, c).map(|(n, games)| profile::Extra {
            label: label.to_string(),
            value: format!("{}", n.round() as i64),
            detail: Some(format!("{:.2} per game", n / games as f64)),
            hint: Some(hint.to_string()),
        })
    };
    let picks = || count("medic_picks", "Medic picks, career", "Enemy Medics you killed, across every rated game in this role.");
    let airshots = || count("airshots", "Airshots, career", "Enemies you hit in the air, across every rated game in this role.");
    let kills = || count("impact_kills", "Kills, career", "Kills across every rated game in this role.");
    let assists = || count("impact_assists", "Assists, career", "Assists across every rated game in this role.");
    let wanted: Vec<Option<profile::Extra>> = match role {
        Role::Pocket | Role::Roamer | Role::Demoman => vec![airshots(), picks()],
        Role::FlankScout | Role::PocketScout => vec![kills(), picks()],
        Role::Medic => vec![
            career_average(career, "heal").map(|(h, games)| profile::Extra {
                label: "Healing per minute, career".to_string(),
                value: format!("{}", h.round() as i64),
                detail: Some(format!("over {games} games")),
                hint: Some("Your healing per minute, averaged over every rated game as Medic.".to_string()),
            }),
            assists(),
        ],
        Role::OffClass(_) => vec![kills(), picks()],
    };
    wanted.into_iter().flatten().collect()
}

fn clone_row(r: &hl_db::HistoryDbRow) -> hl_db::HistoryDbRow {
    hl_db::HistoryDbRow {
        log_id: r.log_id,
        played_at: r.played_at,
        map: r.map.clone(),
        title: r.title.clone(),
        league: r.league.clone(),
        team: r.team.clone(),
        kind: r.kind.clone(),
        red_score: r.red_score,
        blue_score: r.blue_score,
        score: r.score,
        minutes: r.minutes,
        parts_json: r.parts_json.clone(),
    }
}

/// The headline numbers over these games from their logs.tf JSON: time
/// played, record, kills, deaths, assists, damage and accuracy on `class`.
async fn headline_from_jsons(
    db: &hl_db::Db,
    account: u32,
    class: hl_core::TfClass,
    rows: &[hl_db::HistoryDbRow],
    kind: Option<&str>,
    period: Option<(i64, i64)>,
) -> Result<Option<Headline>> {
    let rows: Vec<&hl_db::HistoryDbRow> = rows
        .iter()
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
    let (mut seconds, mut kills, mut deaths, mut assists, mut damage) = (0i64, 0i64, 0i64, 0i64, 0i64);
    let (mut shots, mut hits, mut accuracy_games) = (0i64, 0i64, 0usize);
    for (_, json) in db.sixes_jsons_of(&ids).await? {
        if let Some(t) = class_line(&json, account, class) {
            seconds += t.0;
            kills += t.1;
            deaths += t.2;
            assists += t.3;
            damage += t.4;
        }
        if let Some((s, h)) = crate::rating::weapon_accuracy(&json, account, class) {
            shots += s;
            hits += h;
            accuracy_games += 1;
        }
    }
    Ok(Some(Headline {
        games: rows.len(),
        seconds,
        won,
        lost,
        tied,
        kills,
        deaths,
        assists,
        damage,
        dpm: (seconds > 0).then(|| damage as f64 * 60.0 / seconds as f64),
        accuracy: (shots > 0).then(|| hits as f64 / shots as f64),
        accuracy_games,
    }))
}

/// One player's line on one class in a logs.tf log: (seconds, kills,
/// deaths, assists, damage).
fn class_line(json: &str, account: u32, class: hl_core::TfClass) -> Option<(i64, i64, i64, i64, i64)> {
    let raw: Value = serde_json::from_str(json).ok()?;
    let me = raw.get("players")?.as_object()?.iter().find(|(k, _)| SteamId::parse(k).is_ok_and(|id| id.account_id() == account))?.1;
    let cs = me.get("class_stats")?.as_array()?.iter().find(|c| c.get("type").and_then(Value::as_str).and_then(|t| hl_core::TfClass::parse(t).ok()) == Some(class))?;
    let n = |k: &str| cs.get(k).and_then(Value::as_i64).unwrap_or(0);
    Some((n("total_time"), n("kills"), n("deaths"), n("assists"), n("dmg")))
}

/// Any player's 6v6 roles, the owner's logs and the league sample's.
pub async fn player_roles(db: &hl_db::Db, account: u32) -> Result<Vec<(String, i64)>> {
    db.sixes_player_roles(account, &model_version()).await
}

/// The roles the owner has rated 6v6 games in, most played first.
pub async fn roles(db: &hl_db::Db, me: SteamId) -> Result<Vec<(String, i64)>> {
    db.sixes_rated_roles(me.account_id(), &model_version()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_class_line_is_read_from_the_log_s_class_stats() {
        let json = r#"{"players":{"[U:1:42]":{"class_stats":[
            {"type":"soldier","total_time":1200,"kills":20,"deaths":10,"assists":5,"dmg":6000},
            {"type":"heavyweapons","total_time":60,"kills":1,"deaths":1,"assists":0,"dmg":300}]}}}"#;
        assert_eq!(class_line(json, 42, hl_core::TfClass::Soldier), Some((1200, 20, 10, 5, 6000)));
        assert_eq!(class_line(json, 42, hl_core::TfClass::Heavy), Some((60, 1, 1, 0, 300)));
        assert_eq!(class_line(json, 7, hl_core::TfClass::Soldier), None, "not in the log");
    }

    #[test]
    fn career_cards_count_back_from_per_ten_minutes() {
        let game = |picks: f64| (20.0, vec![("medic_picks".to_string(), 50.0, 0.5, Some(picks)), ("airshots".to_string(), 50.0, 0.5, Some(1.0))]);
        // 1.5 and 0.5 Medic picks per 10 min over two 20-minute games: 3 + 1.
        let extras = career_extras(Role::Pocket, &[game(1.5), game(0.5)]);
        assert_eq!(extras.iter().map(|e| (e.label.as_str(), e.value.as_str())).collect::<Vec<_>>(), vec![("Airshots, career", "4"), ("Medic picks, career", "4")]);
        assert_eq!(extras[1].detail.as_deref(), Some("2.00 per game"));
    }

    #[test]
    fn parts_read_with_or_without_their_raw_value() {
        let p = parts_of(r#"[["dpm", 60.0, 0.5, 250.0], ["deaths", 40.0, 0.5]]"#);
        assert_eq!(p, vec![("dpm".into(), 60.0, 0.5, Some(250.0)), ("deaths".into(), 40.0, 0.5, None)]);
    }

    #[test]
    fn the_breakdown_averages_form_and_career_and_shows_shares_as_percentages() {
        let game = |pct: f64, raw: f64| vec![("space".to_string(), pct, 0.4, Some(raw)), ("dpm".to_string(), pct, 0.6, Some(200.0))];
        let games = vec![game(20.0, 0.1), game(60.0, 0.2), game(80.0, 0.3)];
        let c = components(&games, 2);
        assert_eq!(c[0].component, "dpm", "heaviest first");
        let space = c.iter().find(|x| x.component == "space").unwrap();
        assert_eq!((space.form_pct, space.career_pct), (70.0, 53.3));
        assert_eq!(space.form_raw, 25.0, "a share, as a percentage");
        assert_eq!(space.label, "Space created");
    }
}
