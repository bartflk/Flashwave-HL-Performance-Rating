//! One log's rows for `log_stat` and `log_weapon`: logs.tf's numbers, the
//! fights pass's and impact, per player and class (the MVP research,
//! October 2026). Pure: the caller has already parsed what it needs.

use crate::fights::Fights;
use crate::round_impact::PlayerImpact;
use hl_core::matchdata::NormalizedLog;
use hl_core::SteamId;
use hl_db::{LogStat, LogWeapon};
use serde_json::Value;
use std::collections::HashMap;

/// What the server log adds, when the log has one.
pub struct FromRaw<'a> {
    pub fights: &'a Fights,
    pub impact: &'a HashMap<u32, PlayerImpact>,
    pub buildings: &'a HashMap<(u32, hl_core::TfClass), (i64, i64)>,
}

/// Buildings each player destroyed, by the class they were on: `(sentries,
/// all buildings)`, from the server log's `killedobject` lines (Flashy: "is
/// there a way to see if a player kills a sentry like a sniper?"). The
/// destroyer only, not the assist line written beside it; a sentry, a
/// dispenser or a teleporter, not a sapper an Engineer removed; never an
/// Engineer's own building.
pub fn building_kills(raw: &crate::rawlog::RawLog) -> HashMap<(u32, hl_core::TfClass), (i64, i64)> {
    let mut out: HashMap<(u32, hl_core::TfClass), (i64, i64)> = HashMap::new();
    for e in &raw.events {
        let crate::rawlog::EventKind::ObjectKilled { by, owner, object, assist } = &e.kind else { continue };
        let Some(class) = by.class else { continue };
        let building = ["OBJ_SENTRYGUN", "OBJ_DISPENSER", "OBJ_TELEPORTER"].iter().any(|o| object.eq_ignore_ascii_case(o));
        if *assist || !building || *owner == Some(by.account) {
            continue;
        }
        let n = out.entry((by.account, class)).or_default();
        if object.eq_ignore_ascii_case("OBJ_SENTRYGUN") {
            n.0 += 1;
        }
        n.1 += 1;
    }
    out
}

/// Every player's rows in one log.
pub fn rows(log_id: i64, json: &Value, log: &NormalizedLog, raw: Option<FromRaw<'_>>) -> (Vec<LogStat>, Vec<LogWeapon>) {
    // The JSON's players by account, for what normalising leaves out: the
    // per-weapon figures and the Medic's numbers.
    let mut by_account: HashMap<u32, &Value> = HashMap::new();
    if let Some(players) = json.get("players").and_then(Value::as_object) {
        for (key, p) in players {
            if let Ok(id) = SteamId::parse(key) {
                by_account.insert(id.account_id(), p);
            }
        }
    }
    let fights: HashMap<u32, &crate::fights::FightStats> =
        raw.as_ref().map(|r| r.fights.players.iter().map(|s| (s.account_id, s)).collect()).unwrap_or_default();

    let mut stats = Vec::new();
    let mut weapons = Vec::new();
    for p in &log.players {
        let account = p.id.account_id();
        let main = p.classes.iter().max_by_key(|c| c.time_s).map(|c| c.class);
        let jp = by_account.get(&account).copied();
        let medic = jp.and_then(|j| j.get("medicstats"));
        let num = |v: Option<&Value>, k: &str| v.and_then(|v| v.get(k)).and_then(Value::as_f64);
        for c in &p.classes {
            if c.time_s <= 0 {
                continue;
            }
            let is_main = Some(c.class) == main;
            let s = &p.stats;
            let per_player = |v: i64| if is_main { v } else { 0 };
            let mut row = LogStat {
                log_id,
                account_id: i64::from(account),
                class: c.class.as_str().to_string(),
                main: is_main,
                time_s: c.time_s,
                kills: c.kills,
                deaths: c.deaths,
                assists: c.assists,
                dmg: c.dmg,
                dt: per_player(s.dt),
                heal: per_player(s.heal),
                ubers: per_player(s.ubers),
                drops: per_player(s.drops),
                airshots: per_player(s.airshots),
                headshots: per_player(s.headshots),
                backstabs: per_player(s.backstabs),
                caps: per_player(s.cpc),
                medkits: per_player(s.medkits),
                medkits_hp: per_player(s.medkits_hp),
                longest_streak: per_player(s.lks),
                adv_lost: if is_main { num(medic, "advantages_lost").unwrap_or(0.0) as i64 } else { 0 },
                build_s: if is_main { num(medic, "avg_time_to_build") } else { None },
                use_s: if is_main { num(medic, "avg_time_before_using") } else { None },
                ..Default::default()
            };
            if let Some(r) = raw.as_ref() {
                let z = |v: u32| Some(if is_main { i64::from(v) } else { 0 });
                if let Some(f) = fights.get(&account) {
                    row.opening_kills = z(f.opening_kills);
                    row.opening_deaths = z(f.opening_deaths);
                    row.first_picks = z(f.first_picks);
                    row.trade_kills = z(f.trade_kills);
                    row.traded_deaths = z(f.traded_deaths);
                    row.cleanup_kills = z(f.cleanup_kills);
                    row.forces = z(f.forces);
                    row.fights_present = z(f.fights_present);
                    row.fights_kast = z(f.fights_kast);
                } else {
                    for v in [
                        &mut row.opening_kills,
                        &mut row.opening_deaths,
                        &mut row.first_picks,
                        &mut row.trade_kills,
                        &mut row.traded_deaths,
                        &mut row.cleanup_kills,
                        &mut row.forces,
                        &mut row.fights_present,
                        &mut row.fights_kast,
                    ] {
                        *v = Some(0);
                    }
                }
                let i = r.impact.get(&account).copied().unwrap_or_default();
                let f = |v: f64| Some(if is_main { v } else { 0.0 });
                row.impact = f(i.impact);
                row.impact_attack = f(i.impact_attack);
                row.impact_defence = f(i.impact_defence);
                row.kills_attack = z(i.kills_attack);
                row.deaths_attack = z(i.deaths_attack);
                row.kills_defence = z(i.kills_defence);
                row.deaths_defence = z(i.deaths_defence);
                row.secs_attack = Some(if is_main { i.secs_attack } else { 0 });
                row.secs_defence = Some(if is_main { i.secs_defence } else { 0 });
                row.down_kills = z(i.down_kills);
                row.multi2 = z(i.multi2);
                row.multi3 = z(i.multi3);
                // By the class they were on, so every class row has its own.
                let (sentries, buildings) = r.buildings.get(&(account, c.class)).copied().unwrap_or_default();
                row.sentry_kills = Some(sentries);
                row.building_kills = Some(buildings);
            }
            stats.push(row);
        }
        // Weapons, from the JSON's per-class breakdown.
        for cs in jp.and_then(|j| j.get("class_stats")).and_then(Value::as_array).into_iter().flatten() {
            let Some(class) = cs.get("type").and_then(Value::as_str).and_then(|t| hl_core::TfClass::parse(t).ok()) else { continue };
            for (weapon, w) in cs.get("weapon").and_then(Value::as_object).into_iter().flatten() {
                let n = |k: &str| w.get(k).and_then(Value::as_i64).unwrap_or(0);
                if n("kills") == 0 && n("dmg") == 0 {
                    continue;
                }
                weapons.push(LogWeapon {
                    log_id,
                    account_id: i64::from(account),
                    class: class.as_str().to_string(),
                    weapon: weapon.clone(),
                    kills: n("kills"),
                    dmg: n("dmg"),
                    shots: n("shots"),
                    hits: n("hits"),
                });
            }
        }
    }
    (stats, weapons)
}

/// One log's rows from its logs.tf JSON and, where there is one, its raw
/// server log.
pub fn derive(log_id: i64, json: &str, zip: Option<&[u8]>) -> anyhow::Result<(Vec<LogStat>, Vec<LogWeapon>)> {
    let value: Value = serde_json::from_str(json)?;
    let log = crate::normalize::normalize(log_id, &value)?;
    let Some(zip) = zip else { return Ok(rows(log_id, &value, &log, None)) };
    let raw = crate::rawlog::parse(&crate::rawlog::unzip(zip)?);
    let gs = crate::state::GameState::build(&raw);
    let map = log.map.clone();
    let fights = crate::fights::analyse_on(&raw, &gs, &|_| map.clone());
    let impact = crate::round_impact::impacts(
        &raw,
        &gs,
        &|i| fights.tags.get(i).is_some_and(Option::is_some),
        log.flags.attack_defend,
        crate::round_impact::WinTable::shipped(),
    );
    let buildings = building_kills(&raw);
    Ok(rows(log_id, &value, &log, Some(FromRaw { fights: &fights, impact: &impact, buildings: &buildings })))
}

/// Derives and stores the rows of the given logs, league or the owner's,
/// skipping any whose files are gone. Returns how many were written.
pub async fn fill(db: &hl_db::Db, ids: &[i64]) -> anyhow::Result<usize> {
    let mut n = 0;
    for &log_id in ids {
        let (json, zip) = match db.league_log_files(log_id).await? {
            (Some(json), zip) => (json, zip),
            _ => match db.raw_log(log_id).await? {
                Some(json) => (json, db.rawlog(log_id).await?),
                None => continue,
            },
        };
        let out = tokio::task::spawn_blocking(move || derive(log_id, &json, zip.as_deref())).await?;
        match out {
            Ok((stats, weapons)) => {
                db.put_log_stats(log_id, &stats, &weapons).await?;
                n += 1;
            }
            Err(e) => tracing::warn!(log_id, error = %format!("{e:#}"), "log stats not derived"),
        }
    }
    Ok(n)
}

/// Fills every log that has its files but no rows yet: the league sample's
/// and the owner's. `limit` caps a background step.
pub async fn fill_missing(db: &hl_db::Db, limit: Option<i64>, mut progress: impl FnMut(usize, usize)) -> anyhow::Result<usize> {
    let ids = db.logs_without_stats(limit.unwrap_or(i64::MAX)).await?;
    let mut n = 0;
    for (i, chunk) in ids.chunks(50).enumerate() {
        progress(i * 50, ids.len());
        n += fill(db, chunk).await?;
    }
    progress(ids.len(), ids.len());
    // Rows from before buildings were counted: their server logs read again
    // for that alone, once.
    let todo = db.logs_without_building_kills(limit.unwrap_or(i64::MAX)).await?;
    for (i, &log_id) in todo.iter().enumerate() {
        if i % 50 == 0 {
            progress(i, todo.len());
        }
        let zip = match db.league_log_files(log_id).await?.1 {
            Some(z) => Some(z),
            None => db.rawlog(log_id).await?,
        };
        let Some(zip) = zip else { continue };
        let counted = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let raw = crate::rawlog::parse(&crate::rawlog::unzip(&zip)?);
            Ok(building_kills(&raw))
        })
        .await?;
        match counted {
            Ok(b) => {
                let rows: Vec<(i64, String, i64, i64)> =
                    b.into_iter().map(|((a, c), (s, all))| (i64::from(a), c.as_str().to_string(), s, all)).collect();
                db.put_building_kills(log_id, &rows).await?;
            }
            Err(e) => tracing::warn!(log_id, error = %format!("{e:#}"), "building kills not counted"),
        }
    }
    Ok(n)
}
