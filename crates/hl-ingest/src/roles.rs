//! A 6v6 player's role in one log (the 6v6 plan, P3).
//!
//! Two players share a class in 6v6 and play it differently (boSe.: rate
//! "definitely by role"). The Medic tells them apart: the Soldier they heal
//! most is the pocket, the other the roamer; the Scout they heal most plays
//! with the combo, the other on the flank. logs.tf's `healspread` has who
//! healed whom. The confidence is how clear the split is: 0 when both got
//! the same, 1 when one got everything.
//!
//! Off-class time (Pyro, Heavy, Engineer, Sniper, Spy) is a role of its
//! own, `off:<class>`, with its seconds: each off-class is rated by its own
//! model (question 6). A player whose main class is an off-class (a Sniper
//! all map on Product) has it as their role.
//!
//! When the split cannot be told -- no Medic, one Soldier, an old log
//! without `healspread` -- the role is left out rather than guessed.

use hl_core::{Role, SteamId, TfClass};
use serde_json::Value;
use std::collections::HashMap;

/// One player's role in a log.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleRow {
    pub account_id: u32,
    pub role: Role,
    /// "healspread" (told apart by the Medic's heals) or "class" (the class
    /// says it: Demoman, Medic, an off-class).
    pub method: &'static str,
    pub confidence: Option<f64>,
    pub seconds: i64,
}

struct Player {
    account: u32,
    team: String,
    main: TfClass,
    main_secs: i64,
    classes: Vec<(TfClass, i64)>,
}

/// Every role in a 6v6 log, from its logs.tf JSON.
pub fn roles_of(json: &Value) -> Vec<RoleRow> {
    let Some(players) = json.get("players").and_then(Value::as_object) else { return Vec::new() };
    let mut ps: Vec<Player> = Vec::new();
    for (sid, p) in players {
        let Ok(id) = SteamId::parse(sid) else { continue };
        let team = p.get("team").and_then(Value::as_str).unwrap_or("").to_string();
        let classes: Vec<(TfClass, i64)> = p
            .get("class_stats")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| Some((TfClass::parse(c.get("type")?.as_str()?).ok()?, c.get("total_time")?.as_i64()?)))
            .filter(|(_, t)| *t > 0)
            .collect();
        let Some(&(main, main_secs)) = classes.iter().max_by_key(|(_, t)| *t) else { continue };
        ps.push(Player { account: id.account_id(), team, main, main_secs, classes });
    }
    // Heals by medic account, then target account.
    let mut heals: HashMap<u32, HashMap<u32, f64>> = HashMap::new();
    for (medic, targets) in json.get("healspread").and_then(Value::as_object).into_iter().flatten() {
        let Ok(m) = SteamId::parse(medic) else { continue };
        let row = heals.entry(m.account_id()).or_default();
        for (t, amount) in targets.as_object().into_iter().flatten() {
            if let (Ok(t), Some(a)) = (SteamId::parse(t), amount.as_f64()) {
                row.insert(t.account_id(), a);
            }
        }
    }

    let mut out: Vec<RoleRow> = Vec::new();
    let teams: Vec<String> = {
        let mut t: Vec<String> = ps.iter().map(|p| p.team.clone()).collect();
        t.sort();
        t.dedup();
        t
    };
    for team in &teams {
        let side: Vec<&Player> = ps.iter().filter(|p| &p.team == team).collect();
        let medic = side.iter().find(|p| p.main == TfClass::Medic).map(|p| p.account);
        let healed = |a: u32| medic.and_then(|m| heals.get(&m)).and_then(|h| h.get(&a)).copied();
        for (class, more, less) in [(TfClass::Soldier, Role::Pocket, Role::Roamer), (TfClass::Scout, Role::PocketScout, Role::FlankScout)] {
            let pair: Vec<&&Player> = side.iter().filter(|p| p.main == class).collect();
            if pair.len() != 2 {
                continue;
            }
            let (Some(a), Some(b)) = (healed(pair[0].account), healed(pair[1].account)) else { continue };
            if a + b <= 0.0 {
                continue;
            }
            let (first, second) = if a >= b { (pair[0], pair[1]) } else { (pair[1], pair[0]) };
            let confidence = ((a - b).abs() / (a + b) * 1000.0).round() / 1000.0;
            out.push(RoleRow { account_id: first.account, role: more, method: "healspread", confidence: Some(confidence), seconds: first.main_secs });
            out.push(RoleRow { account_id: second.account, role: less, method: "healspread", confidence: Some(confidence), seconds: second.main_secs });
        }
        for p in &side {
            match p.main {
                TfClass::Demoman => out.push(RoleRow { account_id: p.account, role: Role::Demoman, method: "class", confidence: None, seconds: p.main_secs }),
                TfClass::Medic => out.push(RoleRow { account_id: p.account, role: Role::Medic, method: "class", confidence: None, seconds: p.main_secs }),
                _ => {}
            }
            // Every off-class played, the main one too when it is one.
            for &(c, secs) in &p.classes {
                if let Some(role) = Role::off_class(c) {
                    out.push(RoleRow { account_id: p.account, role, method: "class", confidence: None, seconds: secs });
                }
            }
        }
    }
    out.sort_by(|a, b| a.account_id.cmp(&b.account_id).then(a.role.key().cmp(&b.role.key())));
    out
}

/// One stretch on an off-class (the 6v6 plan, P3; question 6: "check WHEN
/// the offclasses are done ... and if other offclasses have been made").
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stint {
    pub account_id: u32,
    pub class: TfClass,
    /// Seconds from the first round's start.
    pub start_s: i64,
    pub end_s: i64,
    /// The round it began in, from 1.
    pub round: u32,
    /// Teammates on an off-class at some moment of it.
    pub mates_off: u32,
}

/// Every off-class stint in a raw log, from each player's spawns: one
/// begins when a player whose main class is one of the six spawns as an
/// off-class, and ends when they spawn as anything else, or the round ends.
/// `mains` is each player's main class (from logs.tf's class times).
pub fn stints(raw: &crate::rawlog::RawLog, mains: &HashMap<u32, TfClass>) -> Vec<Stint> {
    use crate::rawlog::EventKind;
    let t0 = raw.round_starts.first().copied().unwrap_or(0);
    let round_of = |at: i64| raw.round_starts.iter().filter(|&&s| s <= at).count().max(1) as u32;
    let mut open: HashMap<u32, (TfClass, i64, Option<hl_core::matchdata::Team>)> = HashMap::new();
    let mut out: Vec<(Stint, Option<hl_core::matchdata::Team>)> = Vec::new();
    let close = |acc: u32, at: i64, open: &mut HashMap<u32, (TfClass, i64, Option<hl_core::matchdata::Team>)>, out: &mut Vec<(Stint, Option<hl_core::matchdata::Team>)>| {
        if let Some((class, start, team)) = open.remove(&acc) {
            if at > start {
                out.push((Stint { account_id: acc, class, start_s: start - t0, end_s: at - t0, round: round_of(start), mates_off: 0 }, team));
            }
        }
    };
    for e in &raw.events {
        match &e.kind {
            EventKind::Spawn(a) => {
                let Some(class) = a.class else { continue };
                let main = mains.get(&a.account).copied();
                let off = Role::off_class(class).is_some() && main.is_some_and(|m| Role::off_class(m).is_none());
                match open.get(&a.account) {
                    Some((c, ..)) if *c == class => {}
                    Some(_) => {
                        close(a.account, e.at, &mut open, &mut out);
                        if off {
                            open.insert(a.account, (class, e.at, a.team));
                        }
                    }
                    None if off => {
                        open.insert(a.account, (class, e.at, a.team));
                    }
                    None => {}
                }
            }
            EventKind::RoundWin(_) | EventKind::RoundStalemate | EventKind::GameOver => {
                let accs: Vec<u32> = open.keys().copied().collect();
                for acc in accs {
                    close(acc, e.at, &mut open, &mut out);
                }
            }
            _ => {}
        }
    }
    let end = raw.events.last().map_or(0, |e| e.at);
    let accs: Vec<u32> = open.keys().copied().collect();
    for acc in accs {
        close(acc, end, &mut open, &mut out);
    }
    // Teammates off-classing at the same time.
    let all = out.clone();
    let mut stints: Vec<Stint> = out
        .into_iter()
        .map(|(mut s, team)| {
            s.mates_off = all
                .iter()
                .filter(|(o, t)| o.account_id != s.account_id && *t == team && team.is_some() && o.start_s < s.end_s && o.end_s > s.start_s)
                .map(|(o, _)| o.account_id)
                .collect::<std::collections::HashSet<_>>()
                .len() as u32;
            s
        })
        .collect();
    stints.sort_by_key(|s| (s.start_s, s.account_id));
    stints
}

/// A stored 6v6 log's off-class stints, from its raw log and its logs.tf
/// JSON; empty while its raw log is not stored.
pub async fn match_stints(db: &hl_db::Db, log_id: i64) -> anyhow::Result<Vec<Stint>> {
    let Some(zip) = db.rawlog(log_id).await? else { return Ok(Vec::new()) };
    let Some((_, body)) = db.raw_jsons(&[log_id]).await?.into_iter().next() else { return Ok(Vec::new()) };
    let json: Value = serde_json::from_str(&body)?;
    let mains: HashMap<u32, TfClass> = json
        .get("players")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(sid, p)| {
            let id = SteamId::parse(sid).ok()?;
            let main = p
                .get("class_stats")?
                .as_array()?
                .iter()
                .filter_map(|c| Some((TfClass::parse(c.get("type")?.as_str()?).ok()?, c.get("total_time")?.as_i64()?)))
                .max_by_key(|(_, t)| *t)?
                .0;
            Some((id.account_id(), main))
        })
        .collect();
    let raw = crate::rawlog::parse(&crate::rawlog::unzip(&zip)?);
    Ok(stints(&raw, &mains))
}

/// Work out the roles of stored 6v6 logs that have none, up to `limit`
/// logs; how many were done. A log whose JSON does not read is skipped.
pub async fn refresh(db: &hl_db::Db, limit: i64) -> anyhow::Result<usize> {
    let mut done = 0;
    for (log_id, body) in db.sixes_logs_without_roles(limit).await? {
        let Ok(v) = serde_json::from_str::<Value>(&body) else { continue };
        let rows: Vec<(u32, String, &str, Option<f64>, i64)> =
            roles_of(&v).into_iter().map(|r| (r.account_id, r.role.key(), r.method, r.confidence, r.seconds)).collect();
        // A log with no roles to give is marked with nothing, and so asked
        // again; one with a Demoman and a Medic always has some.
        db.put_roles(log_id, &rows).await?;
        done += 1;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        let body = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sixes_4115969.json")).unwrap();
        serde_json::from_str(&body).unwrap()
    }

    fn role_of(rows: &[RoleRow], account: u32) -> Vec<String> {
        rows.iter().filter(|r| r.account_id == account).map(|r| r.role.key()).collect()
    }

    #[test]
    fn the_medic_s_heals_tell_the_pocket_from_the_roamer_and_the_combo_scout_from_the_flank() {
        let rows = roles_of(&fixture());
        // Red: the Medic healed one Soldier 3150, the other 1742.
        assert_eq!(role_of(&rows, 91333396), ["pocket"]);
        assert_eq!(role_of(&rows, 904025220), ["roamer"]);
        // Red's Scouts: 2650 (and some Heavy) against 1325 (and some Sniper).
        assert_eq!(role_of(&rows, 1110702759), ["off:heavy", "pocket_scout"]);
        assert_eq!(role_of(&rows, 95959380), ["flank_scout", "off:sniper"]);
        // Blue.
        assert_eq!(role_of(&rows, 209920373), ["pocket"]);
        assert_eq!(role_of(&rows, 188377144), ["off:sniper", "roamer"]);
        assert_eq!(role_of(&rows, 176016543), ["pocket_scout"]);
        assert_eq!(role_of(&rows, 213430090), ["flank_scout"]);
        assert_eq!(role_of(&rows, 41435165), ["demoman"]);
        assert_eq!(role_of(&rows, 139131191), ["medic"]);
        let pocket = rows.iter().find(|r| r.account_id == 91333396).unwrap();
        assert_eq!((pocket.method, pocket.confidence), ("healspread", Some(0.288)));
        let heavy = rows.iter().find(|r| r.role == Role::OffClass(TfClass::Heavy)).unwrap();
        assert_eq!(heavy.seconds, 161);
    }

    #[test]
    fn an_off_class_stint_runs_from_the_spawn_to_the_swap_back_with_the_mates_beside_it() {
        let text = [
            r#"L 11/13/2026 - 20:00:00: World triggered "Round_Start""#,
            r#"L 11/13/2026 - 20:00:01: "a<1><[U:1:10]><Red>" spawned as "soldier""#,
            r#"L 11/13/2026 - 20:00:01: "b<2><[U:1:20]><Red>" spawned as "scout""#,
            r#"L 11/13/2026 - 20:00:01: "c<3><[U:1:30]><Blue>" spawned as "sniper""#,
            r#"L 11/13/2026 - 20:02:00: "a<1><[U:1:10]><Red>" spawned as "heavyweapons""#,
            r#"L 11/13/2026 - 20:02:30: "b<2><[U:1:20]><Red>" spawned as "engineer""#,
            r#"L 11/13/2026 - 20:03:00: "a<1><[U:1:10]><Red>" spawned as "heavyweapons""#,
            r#"L 11/13/2026 - 20:04:00: "a<1><[U:1:10]><Red>" spawned as "soldier""#,
            r#"L 11/13/2026 - 20:05:00: World triggered "Round_Win" (winner "Red")"#,
        ]
        .join("\n");
        let raw = crate::rawlog::parse(&text);
        let mains: HashMap<u32, TfClass> = [(10, TfClass::Soldier), (20, TfClass::Scout), (30, TfClass::Sniper)].into_iter().collect();
        let s = stints(&raw, &mains);
        let got: Vec<(u32, TfClass, i64, i64, u32, u32)> = s.iter().map(|x| (x.account_id, x.class, x.start_s, x.end_s, x.round, x.mates_off)).collect();
        assert_eq!(
            got,
            vec![(10, TfClass::Heavy, 120, 240, 1, 1), (20, TfClass::Engineer, 150, 300, 1, 1)],
            "Heavy from 2:00 until the swap back (a second Heavy spawn is the same stint); the Engineer until the round's end; a Sniper main is not off-classing"
        );
    }

    #[test]
    fn no_medic_or_one_soldier_leaves_the_split_out_rather_than_guessing() {
        let mut v = fixture();
        v["healspread"] = serde_json::json!({});
        let rows = roles_of(&v);
        assert!(rows.iter().all(|r| !matches!(r.role, Role::Pocket | Role::Roamer | Role::PocketScout | Role::FlankScout)));
        assert!(rows.iter().any(|r| r.role == Role::Demoman), "the class still says the rest");
    }
}
