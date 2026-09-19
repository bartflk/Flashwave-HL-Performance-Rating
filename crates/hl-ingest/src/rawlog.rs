//! The raw server log behind a logs.tf page: every kill, with time, both
//! players' classes and teams, and where each of them stood.
//!
//! Pure. The rules that decide what counts were checked against logs.tf's own
//! totals: counting only kills inside a round (between `Round_Start` and
//! `Round_Win`, `Round_Stalemate` or `Game_Over`) and skipping Dead Ringer feign deaths reproduces every
//! player's kills *and* kills-by-victim-class exactly, on a 2026 and a 2014 log.
//!
//! ```text
//! L 09/15/2026 - 19:45:09: "rzeke<14><[U:1:227974365]><Blue>" killed "ymberto<10><[U:1:220713409]><Red>"
//!     with "letranger" (attacker_position "2423 -658 -636") (victim_position "2493 -530 -602")
//! ```
//!
//! The victim's class is not on the kill line; it is tracked from each
//! player's latest `spawned as` / `changed role to`.
//!
//! **Clock.** Raw timestamps are the game server's local time. logs.tf's own
//! round times sit a whole number of hours away from them (0, 1 or 2 on this
//! account, from the server's summer time). [`frame_offset`] finds that shift
//! by matching `Round_Start` lines to logs.tf's rounds: on 740 logs every one
//! of 2,474 rounds matches exactly. Kills are stored shifted into logs.tf's
//! frame, so they place into rounds and demos like every other event.

use anyhow::{Context, Result};
use hl_core::matchdata::Team;
use hl_core::TfClass;
use std::collections::HashMap;
use std::io::Read;

#[derive(Debug, Clone, Default)]
pub struct RawLog {
    pub kills: Vec<Kill>,
    pub chat: Vec<Chat>,
    /// Rounds started.
    pub rounds: u32,
    /// Every `Round_Start`, in the raw log's clock.
    pub round_starts: Vec<i64>,
    /// Every damage line: one per hit, so thousands per log. Not stored;
    /// summed on demand for a match page.
    pub damage: Vec<Damage>,
    /// `World triggered "meta_data" (map "...")`: newer uploads write one at
    /// each map load, which names the map of the rounds after it.
    pub map_loads: Vec<(i64, String)>,
}

/// One hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Damage {
    pub at: i64,
    pub live: bool,
    pub attacker: Actor,
    pub victim: Actor,
    /// As logged. A backstab logs six times the victim's health (1,986 on a Heavy).
    pub amount: i64,
}

/// logs.tf counts at most this much from a single hit, which keeps a Spy's
/// stabs from dwarfing everyone's damage. Found by matching two Spies' totals;
/// no other cap reproduces both.
pub const HIT_CAP: i64 = 450;

/// logs.tf started capping hits between December 2014 and June 2016: on this
/// account the 25 logs uploaded up to 2014-12-30 match only uncapped, and the
/// 712 from 2016-06-29 on match only capped. There are no logs in between to
/// narrow it, so the switch is put at the midpoint, 2015-09-30.
pub const HIT_CAP_SINCE: i64 = 1_443_600_000;

/// Whether logs.tf capped hits for a log uploaded at `uploaded` (unix seconds).
pub fn hits_capped(uploaded: Option<i64>) -> bool {
    uploaded.is_none_or(|t| t >= HIT_CAP_SINCE)
}

impl Damage {
    /// The damage logs.tf counts: inside a round, and capped per hit on logs
    /// where logs.tf capped. Summed per player this is logs.tf's damage dealt
    /// exactly, on all 740 logs checked.
    pub fn counted(&self, capped: bool) -> i64 {
        match (self.live, capped) {
            (false, _) => 0,
            (true, true) => self.amount.min(HIT_CAP),
            (true, false) => self.amount,
        }
    }
}

impl RawLog {
    /// Move every time by `shift` seconds, into another clock.
    pub fn shift(&mut self, shift: i64) {
        for k in &mut self.kills {
            k.at += shift;
        }
        for c in &mut self.chat {
            c.at += shift;
        }
        for r in &mut self.round_starts {
            *r += shift;
        }
        for d in &mut self.damage {
            d.at += shift;
        }
        for m in &mut self.map_loads {
            m.0 += shift;
        }
    }
}

/// The shift that turns raw-log times into logs.tf's round-time frame: the
/// whole number of hours under which the most logs.tf round starts land
/// exactly on a raw `Round_Start`. `None` when no hour fits any round.
pub fn frame_offset(raw_round_starts: &[i64], logstf_round_starts: &[i64]) -> Option<i64> {
    let starts: std::collections::HashSet<i64> = raw_round_starts.iter().copied().collect();
    (-14..=14)
        .map(|h: i64| {
            let shift = -h * 3600;
            (logstf_round_starts.iter().filter(|s| starts.contains(&(**s - shift))).count(), shift)
        })
        .filter(|(n, _)| *n > 0)
        // Most matches wins; on a tie, the smaller shift.
        .max_by_key(|(n, shift)| (*n, std::cmp::Reverse(shift.abs())))
        .map(|(_, shift)| shift)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub account: u32,
    /// The colour at that moment; stopwatch swaps it between halves.
    pub team: Option<Team>,
    pub class: Option<TfClass>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Kill {
    /// The server's local clock read as if UTC. [`RawLog::shift`] moves it
    /// into logs.tf's round-time frame before it is stored.
    pub at: i64,
    /// 0 before the first round.
    pub round: u32,
    /// Inside a round, where logs.tf counts it.
    pub live: bool,
    pub killer: Actor,
    pub victim: Actor,
    pub weapon: String,
    pub custom: Option<String>,
    pub assister: Option<u32>,
    pub killer_pos: Option<[i32; 3]>,
    pub victim_pos: Option<[i32; 3]>,
}

impl Kill {
    /// What logs.tf counts as a kill: inside a round, and not a Dead Ringer
    /// feign (the victim did not actually die).
    pub fn counts(&self) -> bool {
        self.live && self.custom.as_deref() != Some("feign_death")
    }

    /// Whether logs.tf credits this kill's assist. It counts assists on feign
    /// deaths too, so this is only "inside a round".
    pub fn assist_counts(&self) -> bool {
        self.live && self.assister.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chat {
    pub at: i64,
    /// `None` for the server console.
    pub account: Option<u32>,
    pub team_chat: bool,
    pub message: String,
}

/// The single `.log` inside a logs.tf raw-log zip.
pub fn unzip(bytes: &[u8]) -> Result<String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("raw log is not a zip")?;
    let mut file = archive.by_index(0).context("raw log zip is empty")?;
    let mut buf = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buf).context("reading raw log from zip")?;
    // Player names are arbitrary bytes; a stray invalid one must not lose the log.
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

pub fn parse(text: &str) -> RawLog {
    let mut out = RawLog::default();
    let mut class: HashMap<u32, TfClass> = HashMap::new();
    let mut live = false;

    for line in text.lines() {
        let Some((at, body)) = split_time(line) else { continue };

        if let Some(ev) = body.strip_prefix("World triggered \"") {
            if ev.starts_with("Round_Start\"") {
                out.rounds += 1;
                out.round_starts.push(at);
                live = true;
            } else if ["Round_Win\"", "Round_Stalemate\"", "Game_Over\""].iter().any(|e| ev.starts_with(e)) {
                live = false;
            } else if ev.starts_with("meta_data\"") {
                if let Some(map) = prop(ev, "map") {
                    out.map_loads.push((at, map.to_string()));
                }
            }
            continue;
        }

        let Some((who, rest)) = actor(body) else { continue };

        if let Some(r) = rest.strip_prefix(" spawned as \"").or_else(|| rest.strip_prefix(" changed role to \"")) {
            let name = r.split('"').next().unwrap_or_default().to_ascii_lowercase();
            if let Ok(c) = TfClass::parse(&name) {
                class.insert(who.account, c);
            }
        } else if let Some(r) = rest.strip_prefix(" killed ") {
            let Some((victim, r)) = actor(r) else { continue };
            let Some(r) = r.strip_prefix(" with \"") else { continue };
            let (weapon, tail) = r.split_once('"').unwrap_or((r, ""));
            out.kills.push(Kill {
                at,
                round: out.rounds,
                live,
                killer: Actor { class: class.get(&who.account).copied(), ..who },
                victim: Actor { class: class.get(&victim.account).copied(), ..victim },
                weapon: weapon.to_string(),
                custom: prop(tail, "customkill").map(str::to_string),
                assister: None,
                killer_pos: prop(tail, "attacker_position").and_then(position),
                victim_pos: prop(tail, "victim_position").and_then(position),
            });
        } else if let Some(r) = rest.strip_prefix(" triggered \"damage\" against ") {
            let Some((victim, tail)) = actor(r) else { continue };
            let Some(amount) = prop(tail, "damage").and_then(|v| v.parse().ok()) else { continue };
            out.damage.push(Damage {
                at,
                live,
                attacker: Actor { class: class.get(&who.account).copied(), ..who },
                victim: Actor { class: class.get(&victim.account).copied(), ..victim },
                amount,
            });
        } else if let Some(r) = rest.strip_prefix(" triggered \"kill assist\" against ") {
            let Some((victim, _)) = actor(r) else { continue };
            // The assist follows its kill, sometimes a second later when the
            // clock ticks over between the two lines; attach it to the latest
            // kill of that victim that has no assister yet.
            if let Some(k) = out
                .kills
                .iter_mut()
                .rev()
                .take(4)
                .find(|k| k.victim.account == victim.account && at - k.at <= 1 && k.assister.is_none())
            {
                k.assister = Some(who.account);
            }
        } else if let Some(r) = rest.strip_prefix(" say_team \"").map(|r| (r, true)).or(rest.strip_prefix(" say \"").map(|r| (r, false))) {
            let (msg, team_chat) = r;
            out.chat.push(Chat {
                at,
                account: Some(who.account),
                team_chat,
                message: msg.strip_suffix('"').unwrap_or(msg).to_string(),
            });
        }
    }
    out
}

/// `L 09/15/2026 - 19:45:09: body` -> (seconds as if UTC, body).
fn split_time(line: &str) -> Option<(i64, &str)> {
    let rest = line.strip_prefix("L ")?;
    // "09/15/2026 - 19:45:09:" then a space.
    let (stamp, body) = (rest.get(..22)?, rest.get(23..)?);
    let b = stamp.as_bytes();
    if b[2] != b'/' || b[5] != b'/' || b[21] != b':' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| stamp.get(r)?.parse::<i64>().ok();
    let (mo, d, y) = (num(0..2)?, num(3..5)?, num(6..10)?);
    let (h, mi, s) = (num(13..15)?, num(16..18)?, num(19..21)?);
    Some((days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s, body))
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `"name<uid><[U:1:123]><Red>"` at the start of `s`, and what follows it.
/// Names can hold anything, quotes included, so the actor ends at the first
/// `>"` that closes a well-formed `<uid><steamid><team>` tag. Bots and the
/// console have no account and give `None`.
fn actor(s: &str) -> Option<(Actor, &str)> {
    if !s.starts_with('"') {
        return None;
    }
    for (end, _) in s.match_indices(">\"") {
        // "name<uid><[U:1:123]><Red"  ->  ["Red", "[U:1:123]>", "uid>", "\"name"]
        let mut parts = s[..end].rsplitn(4, '<');
        let (Some(team), Some(steam), Some(uid), Some(_)) = (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (Some(steam), Some(uid)) = (steam.strip_suffix('>'), uid.strip_suffix('>')) else { continue };
        if uid.is_empty() || !uid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if steam == "BOT" || steam == "Console" {
            return None;
        }
        let Some(account) = steam.strip_prefix("[U:1:").and_then(|x| x.strip_suffix(']')).and_then(|x| x.parse().ok())
        else {
            continue;
        };
        return Some((Actor { account, team: Team::parse(team), class: None }, &s[end + 2..]));
    }
    None
}

/// The value of `(key "value")` in the tail of a line.
fn prop<'a>(tail: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("({key} \"");
    let start = tail.find(&needle)? + needle.len();
    let len = tail[start..].find('"')?;
    Some(&tail[start..start + len])
}

fn position(s: &str) -> Option<[i32; 3]> {
    let mut it = s.split_whitespace().map(|v| v.parse::<f64>().ok().map(|f| f.round() as i32));
    Some([it.next()??, it.next()??, it.next()??])
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: &str = r#"L 09/15/2026 - 19:45:07: "flashy<6><[U:1:139131191]><Red>" changed role to "sniper"
L 09/15/2026 - 19:45:07: "Mifune<15><[U:1:164932866]><Blue>" spawned as "Heavy"
L 09/15/2026 - 19:45:08: "flashy<6><[U:1:139131191]><Red>" killed "Mifune<15><[U:1:164932866]><Blue>" with "sniperrifle" (attacker_position "1 2 3") (victim_position "4 5 6")
L 09/15/2026 - 19:45:10: World triggered "Round_Start"
L 09/15/2026 - 19:45:20: "flashy<6><[U:1:139131191]><Red>" killed "Mifune<15><[U:1:164932866]><Blue>" with "sniperrifle" (customkill "headshot") (attacker_position "-10 20 -30") (victim_position "40 -50 60")
L 09/15/2026 - 19:45:20: "odd "name"<9><[U:1:5]><Red>" triggered "kill assist" against "Mifune<15><[U:1:164932866]><Blue>" (assister_position "0 0 0")
L 09/15/2026 - 19:45:21: "Mifune<15><[U:1:164932866]><Blue>" killed "odd "name"<9><[U:1:5]><Red>" with "tomislav" (customkill "feign_death") (attacker_position "0 0 0") (victim_position "0 0 0")
L 09/15/2026 - 19:45:22: "flashy<6><[U:1:139131191]><Red>" say "gogo"
L 09/15/2026 - 19:46:00: World triggered "Round_Win" (winner "Red")
L 09/15/2026 - 19:46:05: "flashy<6><[U:1:139131191]><Red>" killed "Mifune<15><[U:1:164932866]><Blue>" with "kukri" (attacker_position "0 0 0") (victim_position "0 0 0")
L 09/15/2026 - 19:46:06: "SourceTV<2><BOT><>" say "bot line"
L 09/15/2026 - 19:46:08: World triggered "Round_Start"
L 09/15/2026 - 19:46:09: World triggered "Round_Stalemate"
L 09/15/2026 - 19:46:10: "flashy<6><[U:1:139131191]><Red>" killed "Mifune<15><[U:1:164932866]><Blue>" with "kukri" (attacker_position "0 0 0") (victim_position "0 0 0")
L 09/15/2026 - 19:46:07: "Bot<3><BOT><Blue>" killed "flashy<6><[U:1:139131191]><Red>" with "minigun" (attacker_position "0 0 0") (victim_position "0 0 0")
"#;

    #[test]
    fn times_are_the_server_clock_read_as_utc() {
        let (t, body) = split_time("L 09/15/2026 - 19:45:07: x").unwrap();
        assert_eq!(t, 1_789_501_507);
        assert_eq!(body, "x");
        assert_eq!(split_time("L 01/01/1970 - 00:00:00: y").unwrap().0, 0);
        assert!(split_time("garbage").is_none());
    }

    #[test]
    fn parses_kills_classes_positions_and_assists() {
        let log = parse(LINES);
        assert_eq!(log.rounds, 2);
        assert_eq!(log.kills.len(), 5, "a bot's kill is not credited to its victim");
        assert!(!log.kills[4].live, "a stalemate ends the round too");

        let pregame = &log.kills[0];
        assert!(!pregame.live && !pregame.counts(), "before Round_Start");

        let pick = &log.kills[1];
        assert!(pick.counts());
        assert_eq!(pick.round, 1);
        assert_eq!(pick.killer.class, Some(TfClass::Sniper));
        assert_eq!(pick.victim.class, Some(TfClass::Heavy), "`Heavy` in old logs is heavyweapons");
        assert_eq!(pick.victim.team, Some(Team::Blue));
        assert_eq!(pick.custom.as_deref(), Some("headshot"));
        assert_eq!(pick.killer_pos, Some([-10, 20, -30]));
        assert_eq!(pick.victim_pos, Some([40, -50, 60]));
        assert_eq!(pick.assister, Some(5), "a quote in the name does not break the actor");

        assert!(!log.kills[2].counts(), "a feign death is not a kill");
        assert!(!log.kills[3].live, "after Round_Win is humiliation");
    }

    #[test]
    fn frame_offset_is_the_hour_that_lines_rounds_up() {
        // The Swiftwater scrim: raw rounds at 19:45:12 CEST, logs.tf at 17:45:12.
        let raw = [1_789_501_512, 1_789_502_217, 1_789_499_987];
        let logstf = [1_789_494_312, 1_789_495_017, 1_789_492_787];
        assert_eq!(frame_offset(&raw, &logstf), Some(-7200));
        assert_eq!(frame_offset(&raw, &raw), Some(0));
        assert_eq!(frame_offset(&raw, &[5]), None, "nothing lines up");
    }

    #[test]
    fn chat_is_kept_and_bots_are_skipped() {
        let log = parse(LINES);
        assert_eq!(log.chat.len(), 1);
        assert_eq!(log.chat[0].message, "gogo");
        assert!(!log.chat[0].team_chat);
    }
}
