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

/// Every rateable performance in one 6v6 log, kills unvalued (the tests').
#[cfg(test)]
fn perfs_of(log_id: i64, json: &Value) -> Vec<Perf> {
    perfs_valued(log_id, json, None)
}

/// [`perfs_of`], with kills valued by their victim's class once those values
/// are measured (`sixes_round_win`): a main role's valued kills are its
/// kills by victim class, each worth its class's value.
fn perfs_valued(log_id: i64, json: &Value, victims: Option<&BTreeMap<String, f64>>) -> Vec<Perf> {
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
                // Space created (question 17): taking the team's damage and
                // attention. The log measures the share taken; going in first
                // and positions join it once every log's raw log and demos can.
                if let Some(share) = st.damage_taken_share {
                    v.insert("space", share);
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
        if let (Some(values), false) = (victims.filter(|m| !m.is_empty()), off) {
            if let Some(by_class) = json.get("classkills").and_then(|k| k.get(sid)).and_then(Value::as_object) {
                let valued: f64 = by_class
                    .iter()
                    .map(|(c, k)| {
                        let class = TfClass::parse(c).map(|t| t.as_str().to_string()).unwrap_or_else(|_| c.clone());
                        k.as_f64().unwrap_or(0.0) * values.get(&class).copied().unwrap_or(1.0)
                    })
                    .sum();
                v.insert("impact_kills", per10(valued));
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
    /// Each component: (name, percentile, share, raw value per game).
    pub parts: Vec<(String, f64, f64, f64)>,
}

/// Rate every performance against the others in its role.
fn rate(perfs: &[Perf]) -> Vec<SixesRating> {
    rate_with(perfs, None)
}

/// [`rate`], each component's percentile read from the league `pool` where
/// it has the role and the component (the built-in pool from the logs.tf
/// database), else against the performances here; the role's scale is the
/// pool's where it has one.
fn rate_with(perfs: &[Perf], pool: Option<&crate::sixes_pool::SixesPool>) -> Vec<SixesRating> {
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
        let league = pool.and_then(|p| p.roles.get(role));
        let comps: Vec<(&String, f64)> = weights.iter().filter(|(_, s)| **s > 0.0).map(|(c, s)| (c, *s)).collect();
        // Each component's values sorted, the role's and each player's own:
        // a percentile is then two binary searches, not a pass over the pool
        // (the logs.tf dump's pools run to millions).
        let mut dist: HashMap<&str, (Vec<f64>, HashMap<u32, Vec<f64>>)> = HashMap::new();
        for (c, _) in &comps {
            let (mut all, mut own) = (Vec::new(), HashMap::<u32, Vec<f64>>::new());
            for p in &group {
                if let Some(&x) = p.values.get(c.as_str()).filter(|x| x.is_finite()) {
                    all.push(x);
                    own.entry(p.account_id).or_default().push(x);
                }
            }
            all.sort_by(f64::total_cmp);
            own.values_mut().for_each(|v| v.sort_by(f64::total_cmp));
            dist.insert(c.as_str(), (all, own));
        }
        let mut scored: Vec<(&Perf, f64, Vec<(String, f64, f64, f64)>)> = Vec::new();
        for p in &group {
            let mut parts = Vec::new();
            for (c, share) in &comps {
                let Some(&x) = p.values.get(c.as_str()).filter(|x| x.is_finite()) else { continue };
                if let Some(q) = league.and_then(|l| l.components.get(c.as_str())) {
                    let pct = crate::sixes_pool::percentile(q, x, higher_is_better(c));
                    parts.push(((*c).clone(), (pct * 10.0).round() / 10.0, *share, x));
                    continue;
                }
                // Against everyone else's in the role, never the player's own.
                let (all, own) = &dist[c.as_str()];
                let mine: &[f64] = own.get(&p.account_id).map_or(&[], Vec::as_slice);
                let others = all.len() - mine.len();
                if others == 0 {
                    continue;
                }
                let lt = |v: &[f64]| v.partition_point(|o| *o < x);
                let le = |v: &[f64]| v.partition_point(|o| *o <= x);
                let equal = (le(all) - lt(all)) - (le(mine) - lt(mine));
                let below = if higher_is_better(c) { lt(all) - lt(mine) } else { (all.len() - le(all)) - (mine.len() - le(mine)) };
                let pct = (below as f64 + equal as f64 / 2.0) / others as f64 * 100.0;
                parts.push(((*c).clone(), (pct * 10.0).round() / 10.0, *share, x));
            }
            let total: f64 = parts.iter().map(|(_, _, s, _)| s).sum();
            if total <= 0.0 {
                continue;
            }
            let score = parts.iter().map(|(_, pct, s, _)| pct * s).sum::<f64>() / total;
            // Shares as the rating actually used them, after leaving out the unmeasured.
            let parts = parts.into_iter().map(|(c, pct, s, raw)| (c, pct, ((s / total) * 1000.0).round() / 1000.0, (raw * 1000.0).round() / 1000.0)).collect();
            scored.push((p, score, parts));
        }
        if scored.is_empty() {
            continue;
        }
        let (mean, sd) = match league {
            Some(l) if l.score_sd > 0.0 => (l.score_mean, l.score_sd),
            _ => {
                let mean = scored.iter().map(|(_, s, _)| s).sum::<f64>() / scored.len() as f64;
                (mean, (scored.iter().map(|(_, s, _)| (s - mean).powi(2)).sum::<f64>() / scored.len() as f64).sqrt())
            }
        };
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
/// The pools hold the 6v6 league sample's performances too, once it has
/// some (the 6v6 plan, P5): the owner's games are then read against the
/// league, not only against the players in them.
pub async fn rate_all(db: &hl_db::Db) -> Result<usize> {
    let (perfs, _) = collect(db).await?;
    let ratings = rate_with(&perfs, crate::sixes_pool::built_in());
    let rows: Vec<(i64, u32, String, f64, f64, f64, String)> = ratings
        .iter()
        .map(|r| (r.log_id, r.account_id, r.role.clone(), r.rating, r.score, r.minutes, serde_json::to_string(&r.parts).unwrap_or_else(|_| "[]".into())))
        .collect();
    db.replace_sixes_ratings(&model_version(), &rows).await?;
    Ok(rows.len())
}

/// Who won one log, and each player's side: what [`validate`] needs.
#[derive(Debug, Clone, Default)]
struct LogResult {
    /// The side with more rounds; `None` for a draw.
    winner: Option<String>,
    team: HashMap<u32, String>,
}

fn result_of(json: &Value) -> LogResult {
    let score = |t: &str| json.get("teams").and_then(|x| x.get(t)).and_then(|x| n(x, "score"));
    let winner = match (score("Red"), score("Blue")) {
        (Some(r), Some(b)) if r > b => Some("Red".to_string()),
        (Some(r), Some(b)) if b > r => Some("Blue".to_string()),
        _ => None,
    };
    let team = json
        .get("players")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(sid, p)| Some((SteamId::parse(sid).ok()?.account_id(), p.get("team")?.as_str()?.to_string())))
        .collect();
    LogResult { winner, team }
}

/// Every stored 6v6 performance, as [`rate_all`] rates them, and each log's result.
async fn collect(db: &hl_db::Db) -> Result<(Vec<Perf>, HashMap<i64, LogResult>)> {
    // Roles set by hand on the owner's matches (question 18) stand over the
    // worked-out ones.
    let by_hand: HashMap<(i64, u32), String> = db.roles_by_hand().await?.into_iter().map(|(l, a, r)| ((l, a), r)).collect();
    // 6v6's round-win table, once measured on the league sample: each
    // performance then has its round impact, half of most roles' weight.
    let table: Option<crate::sixes_round_win::WinTable> =
        db.get_setting(crate::sixes_round_win::SETTING).await?.and_then(|s| serde_json::from_str(&s).ok());
    let sample_raws: HashMap<i64, Vec<u8>> = if table.is_some() { db.sixes_league_raws().await?.into_iter().collect() } else { HashMap::new() };
    let mut perfs = Vec::new();
    let mut results = HashMap::new();
    for (log_id, body) in db.sixes_log_jsons().await?.into_iter().chain(db.sixes_league_jsons().await?) {
        let Ok(v) = serde_json::from_str::<Value>(&body) else { continue };
        results.insert(log_id, result_of(&v));
        let mut these = perfs_valued(log_id, &v, table.as_ref().map(|t| &t.victims));
        for p in these.iter_mut() {
            if let Some(r) = by_hand.get(&(log_id, p.account_id)) {
                if !p.role.starts_with("off:") {
                    p.role = r.clone();
                }
            }
        }
        if let Some(t) = &table {
            let zip = match sample_raws.get(&log_id) {
                Some(z) => Some(z.clone()),
                None => db.rawlog(log_id).await?,
            };
            if let Some(raw) = zip.and_then(|z| crate::rawlog::unzip(&z).ok()).map(|t| crate::rawlog::parse(&t)) {
                let impact = crate::sixes_round_win::impacts(&raw, t);
                for p in these.iter_mut().filter(|p| !p.role.starts_with("off:")) {
                    if let Some(x) = impact.get(&p.account_id) {
                        p.values.insert("round_impact", x / p.minutes.max(1.0) * 10.0);
                    }
                }
            }
        }
        perfs.extend(these);
    }
    Ok((perfs, results))
}

/// How often something picked the side that won: ties count half.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    pub pairs: usize,
    /// Wins plus half the ties, over pairs: 0.5 is a coin.
    pub rate: f64,
}

/// Running count behind a [`Pick`], always seen from the winner's side.
#[derive(Default)]
struct Tally(f64, usize);

impl Tally {
    fn push(&mut self, winner: f64, loser: f64) {
        self.0 += if winner == loser { 0.5 } else if winner > loser { 1.0 } else { 0.0 };
        self.1 += 1;
    }
    fn pick(&self) -> Pick {
        Pick { pairs: self.1, rate: if self.1 > 0 { (self.0 / self.1 as f64 * 1000.0).round() / 1000.0 } else { 0.0 } }
    }
}

/// `hl sixes-validate`: does the 6v6 rating pick the side that won
/// (the 6v6 plan, P6, "validated against who won")?
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    pub model: String,
    /// Decided logs with a rated main role on both sides.
    pub logs: usize,
    /// The side with the higher minute-weighted average rating (main roles).
    pub team: Pick,
    /// Per main role: the side whose player in it rated higher.
    pub roles: BTreeMap<String, Pick>,
    /// Per role and component: the side whose player was better at it.
    pub components: BTreeMap<String, BTreeMap<String, Pick>>,
}

impl std::fmt::Display for Validation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "6v6 rating {}: {} decided logs", self.model, self.logs)?;
        writeln!(f, "  team rating picks the winner {:.1}% of {}", self.team.rate * 100.0, self.team.pairs)?;
        for (role, p) in &self.roles {
            writeln!(f, "\n{role}: rating {:.1}% of {}", p.rate * 100.0, p.pairs)?;
            let mut cs: Vec<_> = self.components.get(role).into_iter().flatten().collect();
            cs.sort_by(|a, b| b.1.rate.total_cmp(&a.1.rate));
            for (c, p) in cs {
                writeln!(f, "  {c:<16} {:>5.1}% of {}", p.rate * 100.0, p.pairs)?;
            }
        }
        Ok(())
    }
}

/// One performance as `hl logsdump-sixes` stores it, read back by
/// [`validate_stored`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredPerf {
    pub account_id: u32,
    /// "Red" or "Blue".
    pub team: Option<String>,
    pub role: String,
    pub minutes: f64,
    pub values: BTreeMap<String, f64>,
}

/// One logs.tf log's winning side and its rateable performances, kills
/// unvalued (a dump has no raw logs to value them with).
pub fn stored_perfs(log_id: i64, json: &Value) -> (Option<String>, Vec<StoredPerf>) {
    let res = result_of(json);
    let perfs = perfs_valued(log_id, json, None)
        .into_iter()
        .map(|p| StoredPerf {
            account_id: p.account_id,
            team: res.team.get(&p.account_id).cloned(),
            role: p.role,
            minutes: p.minutes,
            values: p.values.into_iter().map(|(k, v)| (k.to_string(), (v * 1e4).round() / 1e4)).collect(),
        })
        .collect();
    (res.winner, perfs)
}

/// A component's name as the `'static` key a [`Perf`] holds; the names are
/// the weights file's few dozen, so keeping each once costs nothing.
fn intern(s: &str) -> &'static str {
    static SEEN: std::sync::Mutex<BTreeMap<String, &'static str>> = std::sync::Mutex::new(BTreeMap::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(k) = seen.get(s) {
        return k;
    }
    let k: &'static str = Box::leak(s.to_string().into_boxed_str());
    seen.insert(s.to_string(), k);
    k
}

/// [`validate`] over stored performances: `(log, winner, performances)`.
pub fn validate_stored(logs: impl IntoIterator<Item = (i64, Option<String>, Vec<StoredPerf>)>) -> Validation {
    let (mut perfs, mut results) = (Vec::new(), HashMap::new());
    for (log_id, winner, ps) in logs {
        let mut res = LogResult { winner, team: HashMap::new() };
        for p in ps {
            if let Some(t) = p.team {
                res.team.insert(p.account_id, t);
            }
            perfs.push(Perf { log_id, account_id: p.account_id, role: p.role, minutes: p.minutes, values: p.values.iter().map(|(k, v)| (intern(k), *v)).collect() });
        }
        results.insert(log_id, res);
    }
    validate_ratings(&rate(&perfs), &results)
}

/// One role's weights fitted to who won (`hl sixes-fit`): the shares now
/// and fitted, and how often each picks the winning side on the games the
/// fit never saw.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoleFit {
    pub role: String,
    pub train_pairs: usize,
    pub test_pairs: usize,
    /// Share of the rating, the components the logs measure (round impact,
    /// which only raw logs give, keeps its share and is not in these).
    pub now: BTreeMap<String, f64>,
    pub fitted: BTreeMap<String, f64>,
    pub now_test: f64,
    pub fitted_test: f64,
}

/// Fit each role's weights to who won (Flashy chose "fit them from the
/// data"): per decided log, the winning side's player in a role against the
/// losing side's, each component's percentile difference; a logistic model
/// with no intercept and weights kept at zero or more, fitted on the older
/// `train` share of the logs (by log id, which rises with time) and scored
/// with the weights now on the newer rest.
pub fn fit_stored(logs: Vec<(i64, Option<String>, Vec<StoredPerf>)>, train: f64) -> Vec<RoleFit> {
    let (mut perfs, mut results) = (Vec::new(), HashMap::new());
    for (log_id, winner, ps) in logs {
        let mut res = LogResult { winner, team: HashMap::new() };
        for p in ps {
            if let Some(t) = p.team {
                res.team.insert(p.account_id, t);
            }
            perfs.push(Perf { log_id, account_id: p.account_id, role: p.role, minutes: p.minutes, values: p.values.iter().map(|(k, v)| (intern(k), *v)).collect() });
        }
        results.insert(log_id, res);
    }
    let ratings = rate(&perfs);
    // Per role: (log, the winner's parts, the loser's parts).
    let mut by_log: BTreeMap<i64, Vec<&SixesRating>> = BTreeMap::new();
    for r in ratings.iter().filter(|r| !r.role.starts_with("off:")) {
        by_log.entry(r.log_id).or_default().push(r);
    }
    let mut pairs: BTreeMap<String, Vec<(i64, HashMap<String, f64>)>> = BTreeMap::new();
    for (log, rs) in by_log {
        let Some(res) = results.get(&log) else { continue };
        let Some(winner) = &res.winner else { continue };
        let (won, lost): (Vec<&SixesRating>, Vec<&SixesRating>) =
            rs.into_iter().filter(|r| res.team.contains_key(&r.account_id)).partition(|r| res.team.get(&r.account_id) == Some(winner));
        for w in &won {
            let theirs: Vec<_> = lost.iter().filter(|l| l.role == w.role).collect();
            if theirs.len() != 1 || won.iter().filter(|m| m.role == w.role).count() != 1 {
                continue;
            }
            let l = theirs[0];
            let diff: HashMap<String, f64> = w
                .parts
                .iter()
                .filter_map(|(c, pct, _, _)| l.parts.iter().find(|(lc, ..)| lc == c).map(|(_, other, _, _)| (c.clone(), (pct - other) / 100.0)))
                .collect();
            pairs.entry(w.role.clone()).or_default().push((log, diff));
        }
    }
    let w = weights();
    let mut out = Vec::new();
    for (role, mut ps) in pairs {
        let Some(table) = w.role.get(&role) else { continue };
        let comps: Vec<String> = table.iter().filter(|(c, s)| **s > 0.0 && *c != "round_impact").map(|(c, _)| c.clone()).collect();
        ps.sort_by_key(|(log, _)| *log);
        let cut = ((ps.len() as f64) * train) as usize;
        let x = |d: &HashMap<String, f64>| comps.iter().map(|c| d.get(c).copied().unwrap_or(0.0)).collect::<Vec<f64>>();
        let train_x: Vec<Vec<f64>> = ps[..cut].iter().map(|(_, d)| x(d)).collect();
        let test_x: Vec<Vec<f64>> = ps[cut..].iter().map(|(_, d)| x(d)).collect();
        if train_x.len() < 200 || test_x.is_empty() {
            continue;
        }
        let total_now: f64 = comps.iter().map(|c| table[c]).sum();
        let now: Vec<f64> = comps.iter().map(|c| table[c] / total_now).collect();
        let fitted = logistic_nonneg(&train_x, 400, 0.5, 1e-3);
        let sum: f64 = fitted.iter().sum();
        let fitted: Vec<f64> = if sum > 0.0 { fitted.iter().map(|v| v / sum).collect() } else { now.clone() };
        let pick = |wts: &[f64], xs: &[Vec<f64>]| {
            let hits: f64 = xs
                .iter()
                .map(|d| {
                    let s: f64 = d.iter().zip(wts).map(|(a, b)| a * b).sum();
                    if s > 0.0 {
                        1.0
                    } else if s == 0.0 {
                        0.5
                    } else {
                        0.0
                    }
                })
                .sum();
            ((hits / xs.len() as f64) * 1000.0).round() / 1000.0
        };
        let round3 = |v: f64| (v * 1000.0).round() / 1000.0;
        out.push(RoleFit {
            role,
            train_pairs: train_x.len(),
            test_pairs: test_x.len(),
            now_test: pick(&now, &test_x),
            fitted_test: pick(&fitted, &test_x),
            now: comps.iter().cloned().zip(now.iter().map(|v| round3(*v))).collect(),
            fitted: comps.iter().cloned().zip(fitted.iter().map(|v| round3(*v))).collect(),
        });
    }
    out
}

/// A logistic model on differences (winner minus loser, label always "the
/// first won"), no intercept, weights held at zero or more: projected
/// gradient descent with a little L2.
fn logistic_nonneg(xs: &[Vec<f64>], iters: usize, lr: f64, l2: f64) -> Vec<f64> {
    let k = xs.first().map_or(0, Vec::len);
    let mut w = vec![1.0 / k.max(1) as f64; k];
    let n = xs.len() as f64;
    for _ in 0..iters {
        let mut g = vec![0.0; k];
        for x in xs {
            let s: f64 = x.iter().zip(&w).map(|(a, b)| a * b).sum();
            // d/dw of log(1 + e^-s) is -x * sigmoid(-s).
            let f = 1.0 / (1.0 + s.exp());
            for (gi, xi) in g.iter_mut().zip(x) {
                *gi -= xi * f;
            }
        }
        for (wi, gi) in w.iter_mut().zip(&g) {
            *wi = (*wi - lr * (gi / n + l2 * *wi)).max(0.0);
        }
    }
    w
}

/// The league pool from stored performances (`hl logsdump-sixes-pool`):
/// per role, each component's value at every percentile, and the role's
/// score average and spread, the performances rated against each other.
pub fn pool_from_stored(perfs: Vec<StoredPerf>, source: &str, logs: usize) -> crate::sixes_pool::SixesPool {
    let perfs: Vec<Perf> = perfs
        .into_iter()
        .enumerate()
        .map(|(i, p)| Perf { log_id: i as i64, account_id: p.account_id, role: p.role, minutes: p.minutes, values: p.values.iter().map(|(k, v)| (intern(k), *v)).collect() })
        .collect();
    let ratings = rate(&perfs);
    let mut roles: BTreeMap<String, crate::sixes_pool::RolePool> = BTreeMap::new();
    let mut scores: HashMap<String, Vec<f64>> = HashMap::new();
    for r in &ratings {
        scores.entry(r.role.clone()).or_default().push(r.score);
    }
    let mut values: HashMap<(String, &'static str), Vec<f64>> = HashMap::new();
    for p in &perfs {
        for (c, v) in &p.values {
            values.entry((p.role.clone(), *c)).or_default().push(*v);
        }
    }
    for (role, s) in scores {
        let mean = s.iter().sum::<f64>() / s.len() as f64;
        let sd = (s.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
        roles.insert(
            role.clone(),
            crate::sixes_pool::RolePool {
                games: s.len(),
                score_mean: (mean * 1000.0).round() / 1000.0,
                score_sd: (sd * 1000.0).round() / 1000.0,
                components: BTreeMap::new(),
            },
        );
    }
    for ((role, c), v) in values {
        if let Some(r) = roles.get_mut(&role) {
            if v.len() >= 50 {
                r.components.insert(c.to_string(), crate::sixes_pool::quantiles(v));
            }
        }
    }
    crate::sixes_pool::SixesPool { source: source.to_string(), logs, roles }
}

/// Rate every stored 6v6 performance and measure who it picks to win.
pub async fn validate(db: &hl_db::Db) -> Result<Validation> {
    let (perfs, results) = collect(db).await?;
    Ok(validate_ratings(&rate_with(&perfs, crate::sixes_pool::built_in()), &results))
}

fn validate_ratings(ratings: &[SixesRating], results: &HashMap<i64, LogResult>) -> Validation {
    let mut by_log: BTreeMap<i64, Vec<&SixesRating>> = BTreeMap::new();
    for r in ratings.iter().filter(|r| !r.role.starts_with("off:")) {
        by_log.entry(r.log_id).or_default().push(r);
    }
    let (mut logs, mut team) = (0, Tally::default());
    let mut roles: BTreeMap<String, Tally> = BTreeMap::new();
    let mut comps: BTreeMap<String, BTreeMap<String, Tally>> = BTreeMap::new();
    for (log, rs) in by_log {
        let Some(res) = results.get(&log) else { continue };
        let Some(winner) = &res.winner else { continue };
        let (won, lost): (Vec<&SixesRating>, Vec<&SixesRating>) =
            rs.into_iter().filter(|r| res.team.contains_key(&r.account_id)).partition(|r| res.team.get(&r.account_id) == Some(winner));
        if won.is_empty() || lost.is_empty() {
            continue;
        }
        logs += 1;
        let avg = |v: &[&SixesRating]| {
            let m: f64 = v.iter().map(|r| r.minutes).sum();
            v.iter().map(|r| r.rating * r.minutes).sum::<f64>() / m.max(1e-9)
        };
        team.push(avg(&won), avg(&lost));
        for w in &won {
            // One player a role a side, or the role is left out of this log.
            let theirs: Vec<_> = lost.iter().filter(|l| l.role == w.role).collect();
            if theirs.len() != 1 || won.iter().filter(|m| m.role == w.role).count() != 1 {
                continue;
            }
            let l = theirs[0];
            roles.entry(w.role.clone()).or_default().push(w.rating, l.rating);
            for (c, pct, _, _) in &w.parts {
                if let Some((_, other, _, _)) = l.parts.iter().find(|(lc, _, _, _)| lc == c) {
                    comps.entry(w.role.clone()).or_default().entry(c.clone()).or_default().push(*pct, *other);
                }
            }
        }
    }
    Validation {
        model: model_version(),
        logs,
        team: team.pick(),
        roles: roles.into_iter().map(|(k, t)| (k, t.pick())).collect(),
        components: comps.into_iter().map(|(k, m)| (k, m.into_iter().map(|(c, t)| (c, t.pick())).collect())).collect(),
    }
}

/// One of the owner's best 6v6 games by the preview rating.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BestGame {
    pub log_id: i64,
    pub role: String,
    pub rating: f64,
    pub map: Option<String>,
    pub played_at: Option<i64>,
}

/// The owner's 6v6 officials in one season and division, from the stored
/// 6v6 lineups: matches won, drawn and lost by ETF2L's score.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SeasonLine {
    pub season: i64,
    pub division: String,
    pub team: String,
    pub won: u32,
    pub drawn: u32,
    pub lost: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SixesProfile {
    pub best: Vec<BestGame>,
    /// Newest season first.
    pub seasons: Vec<SeasonLine>,
}

/// One row of the 6v6 Players board.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardRow {
    pub account_id: u32,
    pub name: Option<String>,
    pub games: i64,
    pub average: f64,
    pub best: f64,
}

/// Games a player needs in a role to be on its board.
pub const BOARD_MIN_GAMES: i64 = 3;

/// The 6v6 Players board for one role or off-class (the 6v6 plan, P7): the
/// best average preview ratings over everyone rated -- the players in your
/// 6v6 logs and, once it is downloading, the whole league sample. Names are
/// ETF2L's where a stored lineup has them, else the latest a log gave.
pub async fn board(db: &hl_db::Db, role: &str) -> Result<Vec<BoardRow>> {
    let rows = db.sixes_board(&model_version(), role, BOARD_MIN_GAMES, 100).await?;
    let etf2l = db.etf2l_names().await?;
    let logs = db.log_names().await?;
    Ok(rows
        .into_iter()
        .map(|(a, games, average, best)| BoardRow {
            account_id: a,
            name: etf2l.get(&a).cloned().or_else(|| logs.get(&a).and_then(|n| n.first().cloned())),
            games,
            average: (average * 100.0).round() / 100.0,
            best: (best * 100.0).round() / 100.0,
        })
        .collect())
}

/// A 6v6 season's honours in one division (the 6v6 plan, P7).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DivisionHonours {
    pub division: String,
    /// ETF2L's own placings from its table, once the season is over:
    /// `(place, team, name)`.
    pub medals: Vec<(u8, i64, String)>,
    pub mvp: Option<Mvp>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mvp {
    pub account_id: u32,
    pub name: Option<String>,
    pub role: String,
    pub games: i64,
    pub average: f64,
}

/// Games in a season's division to be its MVP.
pub const MVP_MIN_GAMES: i64 = 3;

/// One 6v6 season's medals and MVPs by division: the medals ETF2L's tables
/// give (1st, 2nd, 3rd), and the MVP the best average preview rating with
/// [`MVP_MIN_GAMES`] games or more in a main role -- from your logs and the
/// league sample, so it fills in as the sample downloads.
pub async fn season_honours(db: &hl_db::Db, season: i64) -> Result<Vec<DivisionHonours>> {
    let comps: HashMap<i64, i64> = db.competitions(hl_core::GameFormat::Sixes).await?.into_iter().map(|c| (c.competition_id, c.season)).collect();
    let mut by: BTreeMap<String, DivisionHonours> = BTreeMap::new();
    for (competition, _, json) in db.etf2l_raw("tables").await? {
        if comps.get(&competition) != Some(&season) {
            continue;
        }
        for (division, place, team) in crate::catalogue::table_places(&json) {
            let name = db.etf2l_team(team).await?.map(|t| t.0).unwrap_or_default();
            by.entry(division.clone()).or_insert_with(|| DivisionHonours { division, medals: Vec::new(), mvp: None }).medals.push((place, team, name));
        }
    }
    let names = db.etf2l_names().await?;
    for (division, account, role, games, average) in db.sixes_season_ratings(&model_version(), season).await? {
        if games < MVP_MIN_GAMES {
            continue;
        }
        let e = by.entry(division.clone()).or_insert_with(|| DivisionHonours { division, medals: Vec::new(), mvp: None });
        if e.mvp.as_ref().is_none_or(|m| average > m.average) {
            e.mvp = Some(Mvp { account_id: account, name: names.get(&account).cloned(), role, games, average: (average * 100.0).round() / 100.0 });
        }
    }
    let mut out: Vec<DivisionHonours> = by.into_values().collect();
    for d in &mut out {
        d.medals.sort();
    }
    Ok(out)
}

/// The owner's 6v6 profile beyond their ratings by role (the 6v6 plan, P7):
/// their best games by the preview rating (ten minutes or more in a main
/// role), and their officials by season and division.
pub async fn profile(db: &hl_db::Db, me: u32) -> Result<SixesProfile> {
    let mut rated: Vec<hl_db::SixesRatingRow> =
        db.sixes_ratings(&model_version(), None, Some(me)).await?.into_iter().filter(|r| !r.role.starts_with("off:") && r.minutes >= 10.0).collect();
    rated.sort_by(|a, b| b.rating.total_cmp(&a.rating).then(b.log_id.cmp(&a.log_id)));
    let mut best = Vec::new();
    for r in rated.into_iter().take(5) {
        best.push(BestGame { log_id: r.log_id, role: r.role, rating: r.rating, map: db.match_map(r.log_id).await?, played_at: db.match_played_at(r.log_id).await? });
    }
    let mine: HashMap<i64, i64> = db
        .season_officials_of(me, hl_core::GameFormat::Sixes)
        .await?
        .into_iter()
        .filter_map(|o| o.roster.get(&me).map(|t| (o.match_id, *t)))
        .collect();
    let mut lines: BTreeMap<(i64, String, String), (u32, u32, u32)> = BTreeMap::new();
    for m in db.season_matches(None, None, hl_core::GameFormat::Sixes).await? {
        let Some(&team) = mine.get(&m.match_id) else { continue };
        let (Some(r1), Some(r2)) = (m.r1, m.r2) else { continue };
        if r1 + r2 == 0 && !m.default_win {
            continue;
        }
        let (us, them, name) = if team == m.clan1_id { (r1, r2, m.clan1_name.clone()) } else { (r2, r1, m.clan2_name.clone()) };
        let e = lines.entry((m.season, m.division.clone(), name)).or_default();
        match us.cmp(&them) {
            std::cmp::Ordering::Greater => e.0 += 1,
            std::cmp::Ordering::Equal => e.1 += 1,
            std::cmp::Ordering::Less => e.2 += 1,
        }
    }
    let mut seasons: Vec<SeasonLine> =
        lines.into_iter().map(|((season, division, team), (won, drawn, lost))| SeasonLine { season, division, team, won, drawn, lost }).collect();
    seasons.sort_by(|a, b| b.season.cmp(&a.season).then(a.division.cmp(&b.division)));
    Ok(SixesProfile { best, seasons })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        let body = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sixes_4115969.json")).unwrap();
        serde_json::from_str(&body).unwrap()
    }

    #[tokio::test]
    async fn a_6v6_season_s_honours_are_etf2l_s_placings_and_the_best_rated_with_enough_games() {
        let db = hl_db::Db::connect_in_memory().await.unwrap();
        for sql in [
            "INSERT INTO etf2l_competition (competition_id, season, season_name, division, stage, name, fetched_at, format) VALUES (2, 52, 'Summer 2026', '', 'regular', '6v6 Season 52', 0, 'sixes')",
            "INSERT INTO etf2l_competition (competition_id, season, season_name, division, stage, name, fetched_at) VALUES (5, 52, 'x', 'High', 'regular', 'HL 52', 0)",
            "INSERT INTO etf2l_team (team_id, name) VALUES (3, 'Six One'), (4, 'Six Two')",
            "INSERT INTO etf2l_season_match (match_id, competition_id, division, time, clan1_id, clan2_id, r1, r2) VALUES (20, 2, 'Division 3', 200, 3, 4, 3, 1)",
            r#"INSERT INTO etf2l_raw (kind, id, fetched_at, json) VALUES ('tables', 2, 0, '{"tables":{"Division 3":[{"id":3,"ach":1},{"id":4,"ach":"2"}]}}'), ('tables', 5, 0, '{"tables":{"High":[{"id":9,"ach":1}]}}')"#,
        ] {
            sqlx::query(sql).execute(db.pool()).await.unwrap();
        }
        let row = |log: i64, sample: bool| (log, sample);
        for (log, _) in [row(11, true), row(12, true), row(13, true)] {
            sqlx::query("INSERT INTO sixes_league_log (log_id, etf2l_match_id, map, played_at) VALUES (?1, 20, 'cp_process_f12', 100)").bind(log).execute(db.pool()).await.unwrap();
        }
        let r = |log: i64, acc: u32, v: f64| (log, acc, "pocket".to_string(), v, 50.0, 20.0, "[]".to_string());
        db.replace_sixes_ratings(&model_version(), &[r(11, 7, 1.2), r(12, 7, 1.3), r(13, 7, 1.1), r(11, 8, 2.0), r(12, 8, 2.0)]).await.unwrap();
        let h = season_honours(&db, 52).await.unwrap();
        assert_eq!(h.len(), 1, "Highlander's table of the same season number is not 6v6's");
        assert_eq!(h[0].medals, vec![(1, 3, "Six One".to_string()), (2, 4, "Six Two".to_string())]);
        let mvp = h[0].mvp.as_ref().unwrap();
        assert_eq!((mvp.account_id, mvp.games, mvp.average), (7, 3, 1.2), "8 rates higher but has two games");
    }

    #[tokio::test]
    async fn the_profile_lists_the_owner_s_6v6_seasons_by_their_own_team_and_their_best_games() {
        let db = hl_db::Db::connect_in_memory().await.unwrap();
        for sql in [
            "INSERT INTO etf2l_competition (competition_id, season, season_name, division, stage, name, fetched_at, format) VALUES (2, 53, 'Autumn 2026', '', 'regular', '6v6 Season 53', 0, 'sixes')",
            "INSERT INTO etf2l_team (team_id, name) VALUES (3, 'Six One'), (4, 'Six Two')",
            "INSERT INTO etf2l_season_match (match_id, competition_id, division, time, clan1_id, clan2_id, r1, r2) VALUES (20, 2, 'Division 3', 200, 3, 4, 3, 1), (21, 2, 'Division 3', 300, 4, 3, 4, 0), (22, 2, 'Division 3', 400, 4, 3, 2, 2)",
            "INSERT INTO etf2l_season_player (match_id, account_id, team_id, name) VALUES (20, 7, 3, 'seven'), (21, 7, 3, 'seven'), (22, 7, 3, 'seven')",
        ] {
            sqlx::query(sql).execute(db.pool()).await.unwrap();
        }
        db.replace_sixes_ratings(&model_version(), &[(1, 7, "pocket".into(), 1.3, 70.0, 20.0, "[]".into()), (2, 7, "pocket".into(), 1.1, 60.0, 4.0, "[]".into()), (3, 7, "off:heavy".into(), 1.9, 90.0, 2.0, "[]".into())])
            .await
            .unwrap();
        let p = profile(&db, 7).await.unwrap();
        assert_eq!(p.seasons.iter().map(|l| (l.season, l.team.as_str(), l.won, l.drawn, l.lost)).collect::<Vec<_>>(), vec![(53, "Six One", 1, 1, 1)], "from their own side: a win, a loss and a draw");
        assert_eq!(p.best.iter().map(|b| b.log_id).collect::<Vec<_>>(), vec![1], "ten minutes in a main role; an off-class and a short game left out");
    }

    #[test]
    fn the_validator_counts_how_often_the_higher_rated_side_won() {
        let r = |log: i64, acc: u32, role: &str, rating: f64, pct: f64| SixesRating {
            log_id: log,
            account_id: acc,
            role: role.into(),
            rating,
            score: pct,
            minutes: 20.0,
            parts: vec![("dpm".into(), pct, 1.0, 0.0)],
        };
        let res = |winner: Option<&str>| LogResult {
            winner: winner.map(str::to_string),
            team: [(1, "Red".to_string()), (2, "Blue".to_string()), (3, "Red".to_string())].into_iter().collect(),
        };
        let ratings = vec![
            // Log 1: Red won and rated higher; log 2: Red won, Blue rated higher.
            r(1, 1, "pocket", 1.3, 80.0),
            r(1, 2, "pocket", 0.9, 20.0),
            r(2, 1, "pocket", 0.8, 30.0),
            r(2, 2, "pocket", 1.1, 30.0),
            // Log 3 is a draw; log 4 has one side only.
            r(3, 1, "pocket", 1.5, 90.0),
            r(3, 2, "pocket", 0.5, 10.0),
            r(4, 1, "pocket", 1.5, 90.0),
            r(4, 3, "pocket", 0.5, 10.0),
            // Off-classes are not part of the team's rating.
            r(2, 3, "off:heavy", 3.0, 99.0),
        ];
        let results: HashMap<i64, LogResult> = [(1, res(Some("Red"))), (2, res(Some("Red"))), (3, res(None)), (4, res(Some("Red")))].into_iter().collect();
        let v = validate_ratings(&ratings, &results);
        assert_eq!(v.logs, 2);
        assert_eq!(v.team, Pick { pairs: 2, rate: 0.5 });
        assert_eq!(v.roles["pocket"], Pick { pairs: 2, rate: 0.5 });
        assert_eq!(v.components["pocket"]["dpm"], Pick { pairs: 2, rate: 0.75 }, "a tie is half");
    }

    #[test]
    fn percentiles_leave_the_player_s_own_games_out_and_count_ties_half() {
        let p = |log: i64, acc: u32, dpm: f64, deaths: f64| Perf {
            log_id: log,
            account_id: acc,
            role: "pocket".into(),
            minutes: 20.0,
            values: [("dpm", dpm), ("deaths", deaths)].into_iter().collect(),
        };
        // Player 1 twice; against 2, 3 and 4 only.
        let perfs = vec![p(1, 1, 300.0, 5.0), p(2, 1, 900.0, 1.0), p(3, 2, 200.0, 5.0), p(4, 3, 300.0, 6.0), p(5, 4, 400.0, 4.0)];
        let r = rate(&perfs);
        let part = |log: i64, c: &str| r.iter().find(|x| x.log_id == log).unwrap().parts.iter().find(|(k, _, _, _)| k == c).unwrap().1;
        assert_eq!(part(1, "dpm"), 50.0, "above 200, level with 300, below 400; its own 900 left out");
        assert_eq!(part(1, "deaths"), 50.0, "fewer than 6, level with 5, more than 4");
        assert_eq!(part(3, "dpm"), 0.0, "below everyone else");
    }

    #[test]
    fn a_league_pool_rates_a_game_against_the_field_not_the_players_beside_it() {
        // The field: pockets of 100 to 300 DPM. Two players here, 250 and 260:
        // against each other one is bottom; against the field both are good.
        let field: Vec<StoredPerf> = (0..200)
            .map(|i| StoredPerf { account_id: 1000 + i, team: None, role: "pocket".into(), minutes: 30.0, values: [("dpm".to_string(), 100.0 + i as f64)].into_iter().collect() })
            .collect();
        let pool = pool_from_stored(field, "test", 200);
        let r = &pool.roles["pocket"];
        assert_eq!(r.components["dpm"].len(), 101);
        assert!(r.score_sd > 0.0);
        let p = |acc: u32, dpm: f64| Perf { log_id: acc as i64, account_id: acc, role: "pocket".into(), minutes: 30.0, values: [("dpm", dpm)].into_iter().collect() };
        let here = vec![p(1, 250.0), p(2, 260.0)];
        let alone = rate(&here);
        let pooled = rate_with(&here, Some(&pool));
        let pct = |rs: &[SixesRating], acc: u32| rs.iter().find(|x| x.account_id == acc).unwrap().parts[0].1;
        assert_eq!(pct(&alone, 1), 0.0, "against one other, the lower is bottom");
        assert!(pct(&pooled, 1) > 70.0, "against the field, 250 is high: {}", pct(&pooled, 1));
        assert!(pooled.iter().all(|x| x.rating > 1.0), "{pooled:?}");
    }

    #[test]
    fn the_fit_finds_the_component_that_decides_games_and_drops_the_one_that_does_not() {
        // Two components on the pocket: dpm always higher on the winning side,
        // space a coin flip. The fit leans on dpm.
        let mut logs = Vec::new();
        for i in 0..600i64 {
            let win = |acc: u32, team: &str, dpm: f64, air: f64| StoredPerf {
                account_id: acc,
                team: Some(team.into()),
                role: "pocket".into(),
                minutes: 30.0,
                values: [("dpm".to_string(), dpm), ("space".to_string(), air)].into_iter().collect(),
            };
            let coin = if i % 2 == 0 { 1.0 } else { -1.0 };
            logs.push((i, Some("Red".to_string()), vec![win(1 + 2 * i as u32, "Red", 300.0 + i as f64 % 50.0, 5.0 + coin), win(2 + 2 * i as u32, "Blue", 200.0 + i as f64 % 50.0, 5.0 - coin)]));
        }
        let fits = fit_stored(logs, 0.6);
        let f = fits.iter().find(|f| f.role == "pocket").unwrap();
        assert!(f.fitted["dpm"] > f.fitted["space"], "{f:?}");
        assert!(f.fitted_test >= f.now_test, "{f:?}");
    }

    #[test]
    fn stored_performances_validate_as_the_live_ones_do() {
        let (winner, perfs) = stored_perfs(1, &fixture());
        assert_eq!(winner.as_deref(), Some("Blue"));
        assert!(perfs.iter().all(|p| p.team.is_some()));
        let json = serde_json::to_string(&perfs).unwrap();
        let back: Vec<StoredPerf> = serde_json::from_str(&json).unwrap();
        let v = validate_stored([(1, winner, back)]);
        assert_eq!(v.logs, 1);
        assert_eq!(v.team.pairs, 1);
    }

    #[test]
    fn a_log_s_result_is_its_round_score_and_each_player_s_side() {
        let res = result_of(&fixture());
        assert_eq!(res.winner.as_deref(), Some("Blue"));
        assert_eq!(res.team.len(), fixture()["players"].as_object().unwrap().len());
    }

    #[test]
    fn measured_victim_values_weigh_a_role_s_kills_by_whom_they_killed() {
        let plain = perfs_of(1, &fixture());
        // Blue's pocket killed 2 Medics among 19: Medics worth three times.
        let values: BTreeMap<String, f64> = [("medic".to_string(), 3.0)].into_iter().collect();
        let valued = perfs_valued(1, &fixture(), Some(&values));
        let pk = |ps: &[Perf]| ps.iter().find(|p| p.account_id == 209920373 && p.role == "pocket").unwrap().values["impact_kills"];
        let (a, b) = (pk(&plain), pk(&valued));
        assert!(b > a, "{a} -> {b}");
    }

    #[test]
    fn the_weights_read_and_every_role_and_off_class_has_a_table() {
        let w = weights();
        assert_eq!(w.model_version, "s1");
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
        assert!(p.parts.iter().all(|(c, _, _, _)| c != "round_impact" && c != "forces"), "unmeasured components left out");
        assert!(p.parts.iter().any(|(c, _, _, _)| c == "space"), "space created is measured (question 17)");
        let shares: f64 = p.parts.iter().map(|(_, _, s, _)| s).sum();
        assert!((shares - 1.0).abs() < 0.01, "the rest scaled up to the whole: {shares}");
    }
}
