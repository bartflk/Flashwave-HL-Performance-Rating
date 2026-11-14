//! A preview of the 6v6 rating (the 6v6 plan, P6), on the owner's own
//! stored 6v6 logs.
//!
//! The method is Highlander's: each component of a performance becomes a
//! percentile against everyone else's performances in the same role, the
//! rating is their weighted average, put on the app's scale (1.00 average,
//! 0.25 a standard deviation). What differs is the unit: a role (pocket,
//! roamer, flank and pocket Scout, Demoman, Medic) or an off-class, each
//! with the weights in `weights.sixes.toml`.
//!
//! It is a preview, said so wherever it shows: the pool is the players in
//! the owner's own logs, not the league's (P5), and the components still to
//! be measured -- round impact above all -- are left out and the rest scaled
//! up. The 6v6 rating replaces it once fitted on the league sample.

use crate::roles::roles_of;
use crate::sixes_stats::match_stats;
use anyhow::Result;
use hl_core::{Role, SteamId, TfClass};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

const WEIGHTS: &str = include_str!("weights.sixes.toml");
/// Performances shorter than this on a main role are not rated.
const MIN_MAIN_S: i64 = 5 * 60;
/// An off-class stint is short by nature: a minute is enough to say something.
const MIN_OFF_S: i64 = 60;

#[derive(Debug, Deserialize)]
struct Weights {
    model_version: String,
    role: BTreeMap<String, BTreeMap<String, f64>>,
    off: BTreeMap<String, BTreeMap<String, f64>>,
}

fn weights() -> Weights {
    toml::from_str(WEIGHTS).expect("weights.sixes.toml reads")
}

/// The preview's model version, stored with every rating.
pub fn model_version() -> String {
    weights().model_version
}

/// Whether a component is better high (true) or low.
fn higher_is_better(c: &str) -> bool {
    !matches!(c, "deaths" | "deaths_vs_medic" | "drops_vs" | "deaths_vs" | "adv_lost" | "eng_deaths")
}

/// One rated performance's measurements, by component.
#[derive(Debug, Clone)]
struct Perf {
    log_id: i64,
    account_id: u32,
    role: String,
    minutes: f64,
    values: HashMap<&'static str, f64>,
}

fn n(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))
}

/// Every rateable performance in one 6v6 log.
fn perfs_of(log_id: i64, json: &Value) -> Vec<Perf> {
    let stats = match_stats(json);
    let roles = roles_of(json);
    let players = json.get("players").and_then(Value::as_object);
    let mut out = Vec::new();
    for r in &roles {
        let Some(sid) = players.and_then(|ps| ps.keys().find(|k| SteamId::parse(k).is_ok_and(|s| s.account_id() == r.account_id))) else { continue };
        let p = &json["players"][sid];
        let st = stats.players.iter().find(|x| x.account_id == r.account_id);
        let class = r.role.class();
        let cs = p.get("class_stats").and_then(Value::as_array).and_then(|a| {
            a.iter().find(|c| c.get("type").and_then(Value::as_str).and_then(|t| TfClass::parse(t).ok()) == Some(class))
        });
        let Some(cs) = cs else { continue };
        let secs = n(cs, "total_time").unwrap_or(0.0);
        let off = matches!(r.role, Role::OffClass(_));
        if secs < if off { MIN_OFF_S } else { MIN_MAIN_S } as f64 {
            continue;
        }
        let min = secs / 60.0;
        let per10 = |x: f64| x / min * 10.0;
        let weapon = |pick: &dyn Fn(&str) -> bool| -> f64 {
            cs.get("weapon").and_then(Value::as_object).into_iter().flatten().filter(|(w, _)| pick(w)).map(|(_, s)| n(s, "kills").unwrap_or(0.0)).sum()
        };
        let kills = n(cs, "kills").unwrap_or(0.0);
        let deaths = n(cs, "deaths").unwrap_or(0.0);
        let mut v: HashMap<&'static str, f64> = HashMap::new();
        v.insert("impact_kills", per10(kills));
        v.insert("impact_assists", per10(n(cs, "assists").unwrap_or(0.0)));
        v.insert("deaths", per10(deaths));
        v.insert("dpm", n(cs, "dmg").unwrap_or(0.0) / min);
        v.insert("pressure", n(cs, "dmg").unwrap_or(0.0) / min);
        v.insert("traps", per10(weapon(&|w| w == "tf_projectile_pipe_remote" || w == "sticky_resistance")));
        v.insert("sentry", per10(weapon(&|w| w.starts_with("obj_sentrygun") || w == "obj_minisentry")));
        v.insert("reflects", per10(weapon(&|w| w.starts_with("deflect_"))));
        if class == TfClass::Engineer {
            v.insert("eng_deaths", per10(deaths));
        }
        if class == TfClass::Sniper && kills > 0.0 {
            v.insert("headshot_share", n(p, "headshots").unwrap_or(0.0) / kills.max(1.0));
        }
        if class == TfClass::Spy {
            v.insert("backstabs", per10(n(p, "backstabs").unwrap_or(0.0)));
        }
        // Whole-log numbers, for a main role only: an off-class stint has
        // none of its own.
        if !off {
            if let Some(st) = st {
                if let Some(a) = st.airshots {
                    v.insert("airshots", per10(a as f64));
                }
                if let Some(r) = st.damage_ratio {
                    v.insert("dmg_ratio", r);
                }
                if let Some(m) = st.medic_picks {
                    v.insert("medic_picks", per10(m as f64));
                }
                if let Some(md) = st.medic_deaths {
                    v.insert("deaths_vs_medic", per10(st.deaths as f64 - md as f64));
                }
            }
            if r.role == Role::Medic {
                let mine = stats.medics.iter().find(|m| m.account_id == r.account_id);
                let theirs = mine.and_then(|m| stats.medics.iter().find(|o| o.team != m.team));
                if let Some(m) = mine {
                    if let Some(h) = m.heal_per_min {
                        v.insert("heal", h);
                    }
                    if let Some(a) = m.advantages_lost {
                        v.insert("adv_lost", a as f64);
                    }
                    if let Some(o) = theirs {
                        v.insert("ubers_vs", (m.ubers - o.ubers) as f64);
                        v.insert("drops_vs", (m.drops - o.drops) as f64);
                        v.insert("deaths_vs", (m.deaths - o.deaths) as f64);
                    }
                }
            }
        }
        out.push(Perf { log_id, account_id: r.account_id, role: r.role.key(), minutes: (min * 10.0).round() / 10.0, values: v });
    }
    out
}

/// One stored preview rating.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SixesRating {
    pub log_id: i64,
    pub account_id: u32,
    pub role: String,
    pub rating: f64,
    /// The weighted average percentile, 0-100.
    pub score: f64,
    pub minutes: f64,
    /// Each component's percentile and share, for the breakdown.
    pub parts: Vec<(String, f64, f64)>,
}

/// Rate every performance against the others in its role.
fn rate(perfs: &[Perf]) -> Vec<SixesRating> {
    let w = weights();
    let table = |role: &str| -> Option<&BTreeMap<String, f64>> {
        match role.strip_prefix("off:") {
            Some(c) => w.off.get(c),
            None => w.role.get(role),
        }
    };
    let mut by_role: HashMap<&str, Vec<&Perf>> = HashMap::new();
    for p in perfs {
        by_role.entry(p.role.as_str()).or_default().push(p);
    }
    let mut out = Vec::new();
    for (role, group) in by_role {
        let Some(weights) = table(role) else { continue };
        let mut scored: Vec<(&Perf, f64, Vec<(String, f64, f64)>)> = Vec::new();
        for p in &group {
            let mut parts = Vec::new();
            for (c, share) in weights.iter().filter(|(_, s)| **s > 0.0) {
                let Some(&x) = p.values.get(c.as_str()) else { continue };
                // Against everyone else's in the role, never the player's own.
                let others: Vec<f64> = group.iter().filter(|o| o.account_id != p.account_id).filter_map(|o| o.values.get(c.as_str()).copied()).collect();
                if others.is_empty() {
                    continue;
                }
                let below = others.iter().filter(|&&o| if higher_is_better(c) { o < x } else { o > x }).count() as f64;
                let equal = others.iter().filter(|&&o| o == x).count() as f64;
                let pct = (below + equal / 2.0) / others.len() as f64 * 100.0;
                parts.push((c.clone(), (pct * 10.0).round() / 10.0, *share));
            }
            let total: f64 = parts.iter().map(|(_, _, s)| s).sum();
            if total <= 0.0 {
                continue;
            }
            let score = parts.iter().map(|(_, pct, s)| pct * s).sum::<f64>() / total;
            // Shares as the rating actually used them, after leaving out the unmeasured.
            let parts = parts.into_iter().map(|(c, pct, s)| (c, pct, ((s / total) * 1000.0).round() / 1000.0)).collect();
            scored.push((p, score, parts));
        }
        if scored.is_empty() {
            continue;
        }
        let mean = scored.iter().map(|(_, s, _)| s).sum::<f64>() / scored.len() as f64;
        let sd = (scored.iter().map(|(_, s, _)| (s - mean).powi(2)).sum::<f64>() / scored.len() as f64).sqrt();
        for (p, score, parts) in scored {
            let z = if sd > 0.0 { (score - mean) / sd } else { 0.0 };
            out.push(SixesRating {
                log_id: p.log_id,
                account_id: p.account_id,
                role: p.role.clone(),
                rating: ((1.0 + 0.25 * z) * 100.0).round() / 100.0,
                score: (score * 10.0).round() / 10.0,
                minutes: p.minutes,
                parts,
            });
        }
    }
    out.sort_by(|a, b| a.log_id.cmp(&b.log_id).then(a.account_id.cmp(&b.account_id)).then(a.role.cmp(&b.role)));
    out
}

/// Rate every stored 6v6 log again and store the preview; how many ratings.
pub async fn rate_all(db: &hl_db::Db) -> Result<usize> {
    let mut perfs = Vec::new();
    for (log_id, body) in db.sixes_log_jsons().await? {
        let Ok(v) = serde_json::from_str::<Value>(&body) else { continue };
        perfs.extend(perfs_of(log_id, &v));
    }
    let ratings = rate(&perfs);
    let rows: Vec<(i64, u32, String, f64, f64, f64, String)> = ratings
        .iter()
        .map(|r| (r.log_id, r.account_id, r.role.clone(), r.rating, r.score, r.minutes, serde_json::to_string(&r.parts).unwrap_or_else(|_| "[]".into())))
        .collect();
    db.replace_sixes_ratings(&model_version(), &rows).await?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        let body = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sixes_4115969.json")).unwrap();
        serde_json::from_str(&body).unwrap()
    }

    #[test]
    fn the_weights_read_and_every_role_and_off_class_has_a_table() {
        let w = weights();
        assert_eq!(w.model_version, "s0");
        for r in Role::MAIN {
            assert!(w.role.contains_key(&r.key()), "{}", r.key());
        }
        for c in ["engineer", "heavy", "sniper", "pyro", "spy"] {
            assert!(w.off.contains_key(c), "{c}");
        }
    }

    #[test]
    fn a_log_s_performances_are_rated_against_their_role_on_the_app_s_scale() {
        let perfs = perfs_of(1, &fixture());
        // Twelve players, each in a main role (the Scouts and Soldiers told
        // apart by the heals), and a Heavy stint of 161 s; the Snipers'
        // 24 and 33 s are under a minute.
        assert_eq!(perfs.iter().filter(|p| !p.role.starts_with("off:")).count(), 12);
        assert_eq!(perfs.iter().filter(|p| p.role.starts_with("off:")).map(|p| p.role.as_str()).collect::<Vec<_>>(), ["off:heavy"]);
        // A second copy of the log gives every role a pool of two.
        let mut both = perfs.clone();
        both.extend(perfs_of(2, &fixture()).into_iter().map(|mut p| {
            p.account_id += 1;
            p.values.values_mut().for_each(|v| *v *= 0.8);
            p
        }));
        let r = rate(&both);
        let pocket: Vec<&SixesRating> = r.iter().filter(|x| x.role == "pocket").collect();
        assert_eq!(pocket.len(), 4);
        let mean = r.iter().map(|x| x.rating).sum::<f64>() / r.len() as f64;
        assert!((mean - 1.0).abs() < 0.05, "the scale centres on 1.00: {mean}");
        let p = pocket.iter().find(|x| x.log_id == 1).unwrap();
        assert!(p.parts.iter().all(|(c, _, _)| c != "round_impact" && c != "space"), "unmeasured components left out");
        let shares: f64 = p.parts.iter().map(|(_, _, s)| s).sum();
        assert!((shares - 1.0).abs() < 0.01, "the rest scaled up to the whole: {shares}");
    }
}
