//! ETF2L transfers (Q48, Flashy; PLAN §27): who joined and left a team, and
//! when.
//!
//! ETF2L lists them per team (`/team/{id}/transfers`) and per player
//! (`/player/{id}/transfers`), newest first, twenty to a page. They are read
//! once and kept; a list read before is read again only until a page brings
//! nothing new. What they are for:
//!
//! - medals: a team's medal is for the players still on it at its last
//!   match of the season, not one who left in week two;
//! - a team page's roster history, and how long each player stayed;
//! - a profile's teams with the dates they were on them.

use crate::sources::Sources;
use anyhow::Result;
use hl_core::SteamId;
use hl_db::{Db, Transfer};
use serde::Serialize;
use serde_json::Value;

/// A team in the newest season is read again after a day; an older team's
/// list does not change and is read once.
const STALE: i64 = 24 * 3600;
/// "Active": played an official in the last four months.
const ACTIVE: i64 = 120 * 24 * 3600;
/// A player with no official in those four months has their list read again
/// after a month: it rarely changes.
const STALE_INACTIVE: i64 = 30 * 24 * 3600;

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A Steam account from ETF2L's `steam.id64`, when it is one.
fn account_of(steam: &Value) -> Option<u32> {
    let id64 = steam.get("id64").and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_u64().map(|n| n.to_string())))?;
    SteamId::parse(&id64).ok().map(|s| s.account_id())
}

/// One page of a transfer list: its rows and how many pages there are.
/// `subject` is the player whose list it is (a player's list does not name
/// them on every row): `(ETF2L id, account, name)`.
pub fn parse_page(body: &str, subject: Option<(i64, Option<u32>, &str)>) -> Option<(Vec<Transfer>, i64)> {
    let v: Value = serde_json::from_str(body).ok()?;
    let last_page = v.pointer("/meta/last_page").and_then(Value::as_i64).unwrap_or(1);
    let mut out = Vec::new();
    for r in v.get("data")?.as_array()? {
        let team = &r["team"];
        let Some(team_id) = team["id"].as_i64() else { continue };
        let (Some(time), Some(kind)) = (r["time"].as_i64(), r["type"].as_str()) else { continue };
        let (player_id, account_id, player_name) = match (r.get("who"), subject) {
            (Some(who), _) if who.is_object() => {
                let Some(id) = who["id"].as_i64() else { continue };
                (id, account_of(&who["steam"]), who["name"].as_str().unwrap_or("").to_string())
            }
            (_, Some((id, account, name))) => (id, account, name.to_string()),
            _ => continue,
        };
        let by = &r["by"];
        out.push(Transfer {
            team_id,
            team_name: team["name"].as_str().unwrap_or("").to_string(),
            team_type: team["type"].as_str().map(str::to_string),
            player_id,
            account_id,
            player_name,
            kind: kind.to_string(),
            time,
            by_id: by["id"].as_i64(),
            by_name: by["name"].as_str().map(str::to_string),
        });
    }
    Some((out, last_page))
}

/// Read a list page by page; stop early once a page read before brings
/// nothing new. Returns how many transfers were new.
async fn read_list(db: &Db, sources: &Sources, path: &str, subject: Option<(i64, Option<u32>, &str)>, read_before: bool) -> Result<usize> {
    let mut added = 0;
    let mut page = 1;
    loop {
        let Some(body) = sources.etf2l_page(path, page).await? else { break };
        let Some((rows, last_page)) = parse_page(&body, subject) else { break };
        let new = db.put_transfers(&rows).await?;
        added += new;
        if page >= last_page || rows.is_empty() || (read_before && new == 0) {
            break;
        }
        page += 1;
    }
    Ok(added)
}

/// Read a team's transfers (again). Returns how many were new.
pub async fn fetch_team(db: &Db, sources: &Sources, team: i64) -> Result<usize> {
    let read_before = db.transfers_fetched("team", team).await?.is_some();
    let added = read_list(db, sources, &format!("/team/{team}/transfers"), None, read_before).await?;
    db.set_transfers_fetched("team", team, now()).await?;
    Ok(added)
}

/// Read a player's transfers (again), over every team they were on.
pub async fn fetch_player(db: &Db, sources: &Sources, account: u32) -> Result<usize> {
    let steam = SteamId::from_account_id(account).to_steamid64();
    // A player's list names the teams but not the player: who they are
    // comes from their ETF2L profile, stored or read now.
    // The same lookup the profile's header makes, shared: never a second
    // request for it.
    let Some((id, name)) = crate::catalogue::etf2l_player(db, sources, account).await?.and_then(|p| Some((p.etf2l_id?, p.name.unwrap_or_default()))) else {
        return Ok(0);
    };
    let read_before = db.transfers_fetched("player", i64::from(account)).await?.is_some();
    let added = read_list(db, sources, &format!("/player/{steam}/transfers"), Some((id, Some(account), &name)), read_before).await?;
    db.set_transfers_fetched("player", i64::from(account), now()).await?;
    Ok(added)
}

/// Read a team's list when it was never read, or when the team is still
/// playing and the list is a day old. Best effort: a failure keeps what is
/// stored.
pub async fn refresh_team(db: &Db, sources: &Sources, team: i64) -> Result<()> {
    let fetched = db.transfers_fetched("team", team).await?;
    let active = db.team_last_official(team).await?.is_some_and(|t| now() - t < ACTIVE);
    if fetched.is_none() || (active && now() - fetched.unwrap_or(0) > STALE) {
        if let Err(e) = fetch_team(db, sources, team).await {
            tracing::warn!(team, error = %format!("{e:#}"), "reading a team's transfers failed");
        }
    }
    Ok(())
}

/// Read a player's list when it is a day old, or a month for a player with
/// no official in four months. Best effort; a failure waits a while.
pub async fn refresh_player(db: &Db, sources: &Sources, account: u32) -> Result<()> {
    let fetched = db.transfers_fetched("player", i64::from(account)).await?;
    let active = db.player_last_official(account).await?.is_some_and(|t| now() - t < ACTIVE);
    let stale = if active { STALE } else { STALE_INACTIVE };
    let key = format!("etf2l-transfers:{account}");
    if (fetched.is_none() || now() - fetched.unwrap_or(0) > stale) && !crate::backoff::waiting(&key) {
        match fetch_player(db, sources, account).await {
            Ok(_) => crate::backoff::worked(&key),
            Err(e) => {
                crate::backoff::failed(&key);
                tracing::warn!(account, error = %format!("{e:#}"), "reading a player's transfers failed");
            }
        }
    }
    Ok(())
}

/// The teams whose lists are still to be read, in the order worth reading
/// them: the medal winners first (their medals depend on it), newest
/// season first, then every other team, most recently active first.
pub async fn backfill_order(db: &Db) -> Result<Vec<i64>> {
    let todo: std::collections::HashMap<i64, i64> = db.teams_without_transfers().await?.into_iter().collect();
    let catalogue = crate::catalogue::Catalogue::load(db).await?;
    let mut medals: Vec<(i64, i64)> = catalogue.medals().into_iter().flat_map(|((season, _), list)| list.into_iter().map(move |(_, team, _)| (season, team))).collect();
    medals.sort_by_key(|(season, _)| std::cmp::Reverse(crate::leagues::season_order(*season)));
    let mut out: Vec<i64> = Vec::new();
    for (_, team) in medals {
        if todo.contains_key(&team) && !out.contains(&team) {
            out.push(team);
        }
    }
    let mut rest: Vec<(i64, i64)> = todo.into_iter().filter(|(t, _)| !out.contains(t)).collect();
    rest.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    out.extend(rest.into_iter().map(|(t, _)| t));
    Ok(out)
}

/// Teams still playing whose lists are a day old.
pub async fn stale_active_teams(db: &Db) -> Result<Vec<i64>> {
    let t = now();
    Ok(db.active_teams_transfers(t - ACTIVE).await?.into_iter().filter(|(_, at)| at.is_none_or(|at| t - at > STALE)).map(|(team, _)| team).collect())
}

// ---- What the pages show -------------------------------------------------

/// One stay on a team: from joining to leaving (`None`: still on it).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stay {
    pub team_id: i64,
    pub team_name: String,
    pub team_type: Option<String>,
    pub account_id: Option<u32>,
    pub name: String,
    /// `None` when ETF2L has no join (a team's founder, or older than its
    /// records).
    pub from: Option<i64>,
    pub to: Option<i64>,
}

/// The stays in a list of transfers (oldest first): each join opens one,
/// the next leave closes it.
pub fn stays(rows: &[Transfer]) -> Vec<Stay> {
    let mut open: std::collections::HashMap<(i64, i64), Stay> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for t in rows {
        let key = (t.team_id, t.player_id);
        let fresh = || Stay {
            team_id: t.team_id,
            team_name: t.team_name.clone(),
            team_type: t.team_type.clone(),
            account_id: t.account_id,
            name: t.player_name.clone(),
            from: None,
            to: None,
        };
        if t.joined() {
            // Joined twice without a leave between: the first stay ends here.
            if let Some(mut s) = open.remove(&key) {
                s.to = Some(t.time);
                out.push(s);
            }
            open.insert(key, Stay { from: Some(t.time), ..fresh() });
        } else {
            let mut s = open.remove(&key).unwrap_or_else(fresh);
            s.to = Some(t.time);
            s.name = t.player_name.clone();
            out.push(s);
        }
    }
    out.extend(open.into_values());
    out
}

/// One line of a team's roster history.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRow {
    pub time: i64,
    pub joined: bool,
    pub name: String,
    pub account_id: Option<u32>,
    /// Who made the change, when it was not the player themselves: a leader
    /// adding or removing them.
    pub by: Option<String>,
}

/// A team's roster history.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamTransfers {
    /// Newest first.
    pub rows: Vec<TransferRow>,
    /// Every stay, the current roster first (longest on the team first),
    /// then the players who left (longest stay first).
    pub stays: Vec<Stay>,
    pub fetched_at: Option<i64>,
}

/// A team's roster history, read from ETF2L first when it is due.
pub async fn team_transfers(db: &Db, sources: &Sources, team: i64) -> Result<TeamTransfers> {
    refresh_team(db, sources, team).await?;
    let rows = db.team_transfers(team).await?;
    Ok(TeamTransfers { stays: sort_stays(stays(&rows), now()), rows: history(&rows), fetched_at: db.transfers_fetched("team", team).await? })
}

fn history(rows: &[Transfer]) -> Vec<TransferRow> {
    rows.iter()
        .rev()
        .map(|t| TransferRow {
            time: t.time,
            joined: t.joined(),
            name: t.player_name.clone(),
            account_id: t.account_id,
            by: t.by_name.clone().filter(|_| t.by_id.is_some() && t.by_id != Some(t.player_id)),
        })
        .collect()
}

fn sort_stays(mut s: Vec<Stay>, at: i64) -> Vec<Stay> {
    let length = |x: &Stay| x.to.unwrap_or(at) - x.from.unwrap_or(x.to.unwrap_or(at));
    s.sort_by(|a, b| a.to.is_some().cmp(&b.to.is_some()).then(length(b).cmp(&length(a))));
    s
}

/// A player's teams with dates, newest first, read from ETF2L first when due.
pub async fn player_teams(db: &Db, sources: &Sources, account: u32) -> Result<Vec<Stay>> {
    refresh_player(db, sources, account).await?;
    let rows = db.account_transfers(account).await?;
    let mut s = stays(&rows);
    s.sort_by_key(|x| std::cmp::Reverse((x.to.is_none(), x.to.or(x.from).unwrap_or(0))));
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEAM_PAGE: &str = r#"{"data":[
        {"who":{"id":112847,"name":"mob","steam":{"id64":"76561198099396919"}},"by":{"id":112847,"name":"mob"},
         "team":{"id":35600,"name":"SBQRRA","type":"Highlander"},"time":300,"type":"left"},
        {"who":{"id":112847,"name":"mob","steam":{"id64":"76561198099396919"}},"by":{"id":5,"name":"Belfast"},
         "team":{"id":35600,"name":"SBQRRA","type":"Highlander"},"time":100,"type":"joined"},
        {"who":{"id":9,"name":"nosteam","steam":{"id64":null}},"by":{"id":9,"name":"nosteam"},
         "team":{"id":35600,"name":"SBQRRA","type":"Highlander"},"time":200,"type":"joined"}],
      "meta":{"current_page":1,"last_page":6,"total":115}}"#;

    #[test]
    fn a_team_page_is_read_with_its_players() {
        let (rows, last) = parse_page(TEAM_PAGE, None).unwrap();
        assert_eq!(last, 6);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].account_id, Some(139131191));
        assert_eq!((rows[0].kind.as_str(), rows[0].time, rows[0].team_id), ("left", 300, 35600));
        assert_eq!(rows[1].by_name.as_deref(), Some("Belfast"));
        assert_eq!(rows[2].account_id, None, "no Steam account, still kept");
    }

    #[test]
    fn a_player_page_is_read_as_that_player() {
        let page = r#"{"data":[{"by":{"id":146554,"name":"sslony"},"team":{"id":37805,"name":"DD14","type":"Highlander"},"time":1787567190,"type":"joined"}],
                       "meta":{"last_page":2}}"#;
        let (rows, last) = parse_page(page, Some((97913, Some(139131191), "Flashy"))).unwrap();
        assert_eq!(last, 2);
        assert_eq!((rows[0].player_id, rows[0].account_id, rows[0].player_name.as_str()), (97913, Some(139131191), "Flashy"));
        assert_eq!(rows[0].team_name, "DD14");
    }

    fn tr(player: i64, kind: &str, time: i64) -> Transfer {
        Transfer {
            team_id: 1,
            team_name: "T".into(),
            team_type: None,
            player_id: player,
            account_id: Some(player as u32),
            player_name: format!("p{player}"),
            kind: kind.into(),
            time,
            by_id: Some(player),
            by_name: None,
        }
    }

    #[test]
    fn stays_pair_each_join_with_the_next_leave() {
        // Player 2 is a founder: a leave with no join.
        let rows = vec![tr(1, "joined", 10), tr(2, "left", 15), tr(1, "left", 20), tr(1, "joined", 30)];
        let mut s = stays(&rows);
        s.sort_by_key(|x| (x.account_id, x.from));
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].from, s[0].to), (Some(10), Some(20)));
        assert_eq!((s[1].from, s[1].to), (Some(30), None), "back again, still on");
        assert_eq!((s[2].from, s[2].to), (None, Some(15)));
    }

    #[test]
    fn the_current_roster_comes_first_longest_on_first() {
        let rows = vec![tr(1, "joined", 10), tr(2, "joined", 50), tr(3, "joined", 0), tr(3, "left", 90)];
        let order: Vec<Option<u32>> = sort_stays(stays(&rows), 100).iter().map(|s| s.account_id).collect();
        assert_eq!(order, vec![Some(1), Some(2), Some(3)]);
    }

    #[test]
    fn a_change_made_by_a_leader_names_them() {
        let mut a = tr(1, "joined", 10);
        a.by_id = Some(7);
        a.by_name = Some("leader".into());
        let mut b = tr(1, "left", 20);
        b.by_name = Some("p1".into());
        let h = history(&[a, b]);
        assert_eq!(h[0].by, None, "left by themselves");
        assert_eq!(h[1].by.as_deref(), Some("leader"));
    }
}
