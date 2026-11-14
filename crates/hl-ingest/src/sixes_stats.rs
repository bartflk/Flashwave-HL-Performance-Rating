//! A 6v6 match's own numbers (the 6v6 plan, P4), from its logs.tf JSON:
//! what boSe. asked each role to be judged on (question 8) that a log
//! records directly.
//!
//! - every player: damage per minute, damage taken per minute, damage
//!   dealt against taken (pocket Scout, Demoman), airshots (both Soldiers),
//!   deaths beside their own Medic's (pocket Scout: "compared to his medics'
//!   number of death");
//! - kills by weapon, from logs.tf's per-class weapon table: sticky kills
//!   (Demoman), reflect kills (Pyro), sentry kills (Engineer);
//! - each Medic against the other: übers, drops, deaths, healing, advantages
//!   lost, deaths soon after an über (Medic: "compare them to the enemy
//!   medic's").
//!
//! Pure; numbers a log does not record are `None`, never 0.

use hl_core::{SteamId, TfClass};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SixesPlayerStats {
    pub account_id: u32,
    pub team: String,
    pub main_class: Option<TfClass>,
    pub minutes: f64,
    pub dpm: Option<f64>,
    /// Damage taken per minute.
    pub dtm: Option<f64>,
    /// Damage dealt over damage taken.
    pub damage_ratio: Option<f64>,
    pub airshots: Option<i64>,
    pub deaths: i64,
    /// Their own Medic's deaths, for setting theirs beside.
    pub medic_deaths: Option<i64>,
    pub sticky_kills: i64,
    pub reflect_kills: i64,
    pub sentry_kills: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SixesMedicStats {
    pub account_id: u32,
    pub team: String,
    pub ubers: i64,
    pub drops: i64,
    pub deaths: i64,
    pub heal_per_min: Option<f64>,
    pub advantages_lost: Option<i64>,
    pub deaths_after_uber: Option<i64>,
    /// Seconds, on average, to build an über.
    pub avg_build_s: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SixesMatchStats {
    pub players: Vec<SixesPlayerStats>,
    /// One a team, Red first: the two Medics side by side.
    pub medics: Vec<SixesMedicStats>,
}

fn n(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)).or_else(|| x.as_str().and_then(|s| s.parse().ok())))
}

fn per_min(x: Option<i64>, minutes: f64) -> Option<f64> {
    (minutes > 0.0).then_some(())?;
    x.map(|v| ((v as f64 / minutes) * 10.0).round() / 10.0)
}

/// Kills with weapons whose logs.tf name `pick` accepts, over every class.
fn weapon_kills(p: &Value, pick: impl Fn(&str) -> bool) -> i64 {
    p.get("class_stats")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("weapon").and_then(Value::as_object))
        .flatten()
        .filter(|(w, _)| pick(w))
        .map(|(_, s)| n(s, "kills").unwrap_or(0))
        .sum()
}

pub fn match_stats(json: &Value) -> SixesMatchStats {
    let has_dt = json.pointer("/info/hasDT").and_then(Value::as_bool).unwrap_or(true);
    let has_as = json.pointer("/info/hasAS").and_then(Value::as_bool).unwrap_or(true);
    let mut players = Vec::new();
    let mut medics = Vec::new();
    for (sid, p) in json.get("players").and_then(Value::as_object).into_iter().flatten() {
        let Ok(id) = SteamId::parse(sid) else { continue };
        let team = p.get("team").and_then(Value::as_str).unwrap_or("").to_string();
        let classes: Vec<(TfClass, i64)> = p
            .get("class_stats")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| Some((TfClass::parse(c.get("type")?.as_str()?).ok()?, c.get("total_time")?.as_i64()?)))
            .collect();
        let main = classes.iter().max_by_key(|(_, t)| *t).map(|(c, _)| *c);
        let minutes = classes.iter().map(|(_, t)| *t).sum::<i64>() as f64 / 60.0;
        if minutes <= 0.0 {
            continue;
        }
        let dmg = n(p, "dmg");
        let dt = if has_dt { n(p, "dt") } else { None };
        players.push(SixesPlayerStats {
            account_id: id.account_id(),
            team: team.clone(),
            main_class: main,
            minutes: (minutes * 10.0).round() / 10.0,
            dpm: per_min(dmg, minutes),
            dtm: per_min(dt, minutes),
            damage_ratio: match (dmg, dt) {
                (Some(d), Some(t)) if t > 0 => Some(((d as f64 / t as f64) * 100.0).round() / 100.0),
                _ => None,
            },
            airshots: if has_as { n(p, "as") } else { None },
            deaths: n(p, "deaths").unwrap_or(0),
            medic_deaths: None,
            sticky_kills: weapon_kills(p, |w| w == "tf_projectile_pipe_remote" || w == "sticky_resistance"),
            reflect_kills: weapon_kills(p, |w| w.starts_with("deflect_")),
            sentry_kills: weapon_kills(p, |w| w.starts_with("obj_sentrygun") || w == "obj_minisentry"),
        });
        if main == Some(TfClass::Medic) {
            let ms = p.get("medicstats");
            medics.push(SixesMedicStats {
                account_id: id.account_id(),
                team,
                ubers: n(p, "ubers").unwrap_or(0),
                drops: n(p, "drops").unwrap_or(0),
                deaths: n(p, "deaths").unwrap_or(0),
                heal_per_min: per_min(n(p, "heal"), minutes),
                advantages_lost: ms.and_then(|m| n(m, "advantages_lost")),
                deaths_after_uber: ms.and_then(|m| n(m, "deaths_within_20s_after_uber")),
                avg_build_s: ms.and_then(|m| m.get("avg_time_to_build")).and_then(Value::as_f64).map(|s| (s * 10.0).round() / 10.0),
            });
        }
    }
    // Each team's Medic's deaths, beside every teammate's.
    for p in &mut players {
        if p.main_class != Some(TfClass::Medic) {
            p.medic_deaths = medics.iter().find(|m| m.team == p.team).map(|m| m.deaths);
        }
    }
    players.sort_by(|a, b| a.team.cmp(&b.team).reverse().then(a.account_id.cmp(&b.account_id)));
    medics.sort_by(|a, b| a.team.cmp(&b.team).reverse());
    SixesMatchStats { players, medics }
}

/// A stored log's 6v6 numbers; `None` when its JSON is not stored.
pub async fn of_log(db: &hl_db::Db, log_id: i64) -> anyhow::Result<Option<SixesMatchStats>> {
    let Some((_, body)) = db.raw_jsons(&[log_id]).await?.into_iter().next() else { return Ok(None) };
    Ok(Some(match_stats(&serde_json::from_str(&body)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        let body = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sixes_4115969.json")).unwrap();
        serde_json::from_str(&body).unwrap()
    }

    #[test]
    fn a_6v6_log_gives_each_role_its_numbers_and_the_medics_side_by_side() {
        let s = match_stats(&fixture());
        assert_eq!(s.players.len(), 12);
        let pocket = s.players.iter().find(|p| p.account_id == 91333396).unwrap();
        // 4147 damage, 5022 taken, over 917 seconds.
        assert_eq!((pocket.dpm, pocket.dtm, pocket.damage_ratio), (Some(271.3), Some(328.6), Some(0.83)));
        assert_eq!(pocket.medic_deaths, s.medics.iter().find(|m| m.team == "Red").map(|m| m.deaths));
        assert_eq!(s.medics.iter().map(|m| m.team.as_str()).collect::<Vec<_>>(), ["Red", "Blue"]);
        let red = &s.medics[0];
        assert_eq!((red.account_id, red.advantages_lost, red.deaths_after_uber), (139131191, Some(0), Some(1)));
        let demo = s.players.iter().find(|p| p.account_id == 41435165).unwrap();
        assert!(demo.sticky_kills > 0, "{demo:?}");
        assert!(s.players.iter().all(|p| p.reflect_kills == 0), "no Pyro in this log");
    }
}
