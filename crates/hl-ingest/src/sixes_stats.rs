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
    /// Enemy Medics killed (the roamer's and flank Scout's "picks, Medic
    /// especially"), from logs.tf's kills by victim class.
    pub medic_picks: Option<i64>,
    /// First kills of a fight, from the fights pass; needs the raw log.
    pub picks: Option<i64>,
    /// Buildings destroyed as Spy (sapped); needs the raw log.
    pub saps: Option<i64>,
    /// Soldiers and Demomen killed as Heavy (boSe.: "denies on
    /// sollies/demo"); needs the raw log.
    pub denies: Option<i64>,
    /// PROPOSED, shown for the reviewers and in no rating until they agree
    /// (see [`add_proposed`]): enemy übers this player pressured out.
    pub forces_proposed: Option<i64>,
    /// PROPOSED: captures made without their own Medic on the point.
    pub backcaps_proposed: Option<i64>,
    /// PROPOSED: damage as Sniper on the enemy combo (Medic, Demoman, pocket).
    pub sniper_combo_damage: Option<i64>,
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
            medic_picks: json.get("classkills").and_then(|k| k.get(sid)).map(|k| n(k, "medic").unwrap_or(0)),
            picks: None,
            saps: None,
            denies: None,
            forces_proposed: None,
            backcaps_proposed: None,
            sniper_combo_damage: None,
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

/// What only the raw log has: buildings destroyed as Spy, and Soldiers and
/// Demomen killed as Heavy, by player.
pub fn add_raw(s: &mut SixesMatchStats, raw: &crate::rawlog::RawLog) {
    use crate::rawlog::EventKind;
    let mut saps: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
    let mut denies: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
    for e in &raw.events {
        if let EventKind::ObjectKilled { by, assist: false, .. } = &e.kind {
            if by.class == Some(TfClass::Spy) {
                *saps.entry(by.account).or_default() += 1;
            }
        }
    }
    for k in raw.kills.iter().filter(|k| k.counts() && k.killer.team != k.victim.team) {
        if k.killer.class == Some(TfClass::Heavy) && matches!(k.victim.class, Some(TfClass::Soldier | TfClass::Demoman)) {
            *denies.entry(k.killer.account).or_default() += 1;
        }
    }
    for p in &mut s.players {
        p.saps = Some(saps.get(&p.account_id).copied().unwrap_or(0));
        p.denies = Some(denies.get(&p.account_id).copied().unwrap_or(0));
    }
}

/// How long before an enemy über a player's damage counts as forcing it.
const FORCE_WINDOW_S: i64 = 4;
/// Damage on the enemy team in that window that counts as pressure.
const FORCE_MIN_DAMAGE: i64 = 50;

/// Three PROPOSED stats for the reviewers (the 6v6 plan, P4: über forces,
/// backcaps and Sniper pressure need a definition everyone agrees on). They
/// are shown, marked as proposals, and the rating does not read them: their
/// names are not in `weights.sixes.toml`.
///
/// - **Über forces**: when a Medic deploys an über, every enemy who dealt
///   [`FORCE_MIN_DAMAGE`] or more to the Medic's team in the
///   [`FORCE_WINDOW_S`] seconds before it gets one.
/// - **Backcaps**: a capture whose cappers do not include their own team's
///   Medic: the point was taken away from the combo.
/// - **Sniper pressure**: damage dealt as Sniper to the enemy's Medic,
///   Demoman and pocket Soldier.
pub fn add_proposed(s: &mut SixesMatchStats, raw: &crate::rawlog::RawLog, roles: &[crate::roles::RoleRow]) {
    use crate::rawlog::EventKind;
    use hl_core::Role;
    let role_of = |a: u32, want: &[Role]| roles.iter().any(|r| r.account_id == a && want.contains(&r.role));
    let medics: std::collections::HashSet<u32> = roles.iter().filter(|r| r.role == Role::Medic).map(|r| r.account_id).collect();
    let mut forces: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
    let mut backcaps: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
    let mut combo: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
    for e in &raw.events {
        match &e.kind {
            EventKind::ChargeDeployed { medic, .. } => {
                let Some(team) = medic.team else { continue };
                let mut dealt: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
                for d in raw.damage.iter().filter(|d| d.live && d.at <= e.at && d.at >= e.at - FORCE_WINDOW_S) {
                    if d.victim.team == Some(team) && d.attacker.team.is_some_and(|t| t != team) {
                        *dealt.entry(d.attacker.account).or_default() += d.amount.min(crate::rawlog::HIT_CAP);
                    }
                }
                for (a, dmg) in dealt {
                    if dmg >= FORCE_MIN_DAMAGE {
                        *forces.entry(a).or_default() += 1;
                    }
                }
            }
            EventKind::PointCaptured { cappers, .. } if !cappers.is_empty() && !medics.is_empty() && !cappers.iter().any(|c| medics.contains(c)) => {
                for c in cappers {
                    *backcaps.entry(*c).or_default() += 1;
                }
            }
            _ => {}
        }
    }
    for d in raw.damage.iter().filter(|d| d.live && d.attacker.class == Some(TfClass::Sniper) && d.attacker.team != d.victim.team) {
        if role_of(d.victim.account, &[Role::Medic, Role::Demoman, Role::Pocket]) {
            *combo.entry(d.attacker.account).or_default() += d.amount.min(crate::rawlog::HIT_CAP);
        }
    }
    for p in &mut s.players {
        p.forces_proposed = Some(forces.get(&p.account_id).copied().unwrap_or(0));
        p.backcaps_proposed = Some(backcaps.get(&p.account_id).copied().unwrap_or(0));
        p.sniper_combo_damage = combo.get(&p.account_id).copied();
    }
}

/// A stored log's 6v6 numbers; `None` when its JSON is not stored. With its
/// raw log, saps and denies; with its fights read, picks.
pub async fn of_log(db: &hl_db::Db, log_id: i64) -> anyhow::Result<Option<SixesMatchStats>> {
    let Some((_, body)) = db.raw_jsons(&[log_id]).await?.into_iter().next() else { return Ok(None) };
    let json: Value = serde_json::from_str(&body)?;
    let mut s = match_stats(&json);
    if let Some(zip) = db.rawlog(log_id).await? {
        if let Ok(text) = crate::rawlog::unzip(&zip) {
            let raw = crate::rawlog::parse(&text);
            add_raw(&mut s, &raw);
            add_proposed(&mut s, &raw, &crate::roles::roles_of(&json));
        }
    }
    if let Some(rows) = db.fight_counts(Some(log_id)).await?.remove(&log_id) {
        for p in &mut s.players {
            // Opening kills are the first of the counts.
            p.picks = rows.iter().find(|(a, _)| *a == p.account_id).map(|(_, c)| i64::from(c[0]));
        }
    }
    Ok(Some(s))
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
        // Blue's pocket killed both Medics' worth: 2, from the kills by class.
        assert_eq!(s.players.iter().find(|p| p.account_id == 209920373).unwrap().medic_picks, Some(2));
    }

    #[test]
    fn saps_and_denies_come_from_the_raw_log() {
        let text = [
            r#"L 11/13/2026 - 20:00:00: World triggered "Round_Start""#,
            r#"L 11/13/2026 - 20:00:01: "spy<1><[U:1:10]><Red>" spawned as "spy""#,
            r#"L 11/13/2026 - 20:00:01: "hvy<2><[U:1:20]><Blue>" spawned as "heavyweapons""#,
            r#"L 11/13/2026 - 20:00:01: "sol<3><[U:1:30]><Red>" spawned as "soldier""#,
            r#"L 11/13/2026 - 20:00:30: "spy<1><[U:1:10]><Red>" triggered "killedobject" (object "OBJ_SENTRYGUN") (weapon "obj_attachment_sapper") (objectowner "eng<4><[U:1:40]><Blue>") (attacker_position "0 0 0")"#,
            r#"L 11/13/2026 - 20:00:40: "hvy<2><[U:1:20]><Blue>" killed "sol<3><[U:1:30]><Red>" with "minigun" (attacker_position "0 0 0") (victim_position "1 1 1")"#,
            r#"L 11/13/2026 - 20:01:00: World triggered "Round_Win" (winner "Blue")"#,
        ]
        .join("\n");
        let mut s = SixesMatchStats { players: [10, 20, 30].map(|a| SixesPlayerStats {
            account_id: a, team: String::new(), main_class: None, minutes: 1.0, dpm: None, dtm: None, damage_ratio: None, airshots: None,
            deaths: 0, medic_deaths: None, sticky_kills: 0, reflect_kills: 0, sentry_kills: 0, medic_picks: None, picks: None, saps: None, denies: None,
            forces_proposed: None, backcaps_proposed: None, sniper_combo_damage: None,
        }).to_vec(), medics: Vec::new() };
        add_raw(&mut s, &crate::rawlog::parse(&text));
        assert_eq!(s.players.iter().map(|p| (p.saps, p.denies)).collect::<Vec<_>>(), vec![(Some(1), Some(0)), (Some(0), Some(1)), (Some(0), Some(0))]);
    }

    #[test]
    fn the_proposed_forces_backcaps_and_sniper_pressure_follow_their_definitions() {
        use crate::roles::RoleRow;
        use hl_core::Role;
        let text = [
            r#"L 11/13/2026 - 20:00:00: World triggered "Round_Start""#,
            r#"L 11/13/2026 - 20:00:01: "rsol<1><[U:1:10]><Red>" spawned as "soldier""#,
            r#"L 11/13/2026 - 20:00:01: "rsnip<2><[U:1:11]><Red>" spawned as "sniper""#,
            r#"L 11/13/2026 - 20:00:01: "rmed<3><[U:1:12]><Red>" spawned as "medic""#,
            r#"L 11/13/2026 - 20:00:01: "bmed<4><[U:1:20]><Blue>" spawned as "medic""#,
            r#"L 11/13/2026 - 20:00:01: "bpocket<5><[U:1:21]><Blue>" spawned as "soldier""#,
            r#"L 11/13/2026 - 20:00:10: "rsol<1><[U:1:10]><Red>" triggered "damage" against "bpocket<5><[U:1:21]><Blue>" (damage "90") (weapon "quake_rl")"#,
            r#"L 11/13/2026 - 20:00:11: "rsnip<2><[U:1:11]><Red>" triggered "damage" against "bpocket<5><[U:1:21]><Blue>" (damage "150") (weapon "sniperrifle")"#,
            r#"L 11/13/2026 - 20:00:12: "bmed<4><[U:1:20]><Blue>" triggered "chargedeployed" (medigun "medigun")"#,
            r#"L 11/13/2026 - 20:00:40: Team "Red" triggered "pointcaptured" (cp "2") (cpname "Mid") (numcappers "1") (player1 "rsol<1><[U:1:10]><Red>") (position1 "0 0 0")"#,
            r#"L 11/13/2026 - 20:01:00: World triggered "Round_Win" (winner "Red")"#,
        ]
        .join("\n");
        let blank = |a: u32| SixesPlayerStats {
            account_id: a, team: String::new(), main_class: None, minutes: 1.0, dpm: None, dtm: None, damage_ratio: None, airshots: None,
            deaths: 0, medic_deaths: None, sticky_kills: 0, reflect_kills: 0, sentry_kills: 0, medic_picks: None, picks: None, saps: None, denies: None,
            forces_proposed: None, backcaps_proposed: None, sniper_combo_damage: None,
        };
        let mut s = SixesMatchStats { players: vec![blank(10), blank(11)], medics: Vec::new() };
        let role = |a: u32, role: Role| RoleRow { account_id: a, role, method: "class", confidence: None, seconds: 60 };
        let roles = [role(12, Role::Medic), role(20, Role::Medic), role(21, Role::Pocket)];
        add_proposed(&mut s, &crate::rawlog::parse(&text), &roles);
        let got: Vec<(Option<i64>, Option<i64>, Option<i64>)> = s.players.iter().map(|p| (p.forces_proposed, p.backcaps_proposed, p.sniper_combo_damage)).collect();
        assert_eq!(got, vec![(Some(1), Some(1), None), (Some(1), Some(0), Some(150))], "both pressured the über out; the Soldier capped without the Medic; the Sniper hit the pocket");
    }
}
