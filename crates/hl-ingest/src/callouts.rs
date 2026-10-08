//! Callouts (Q28, Flashy): named zones on each map, and where players spent
//! their time in them.
//!
//! Callouts are the community's words, not data: no file in the game says
//! where "cliff" is, and teams disagree at the edges. So they are stored as
//! translations are (Q13): a seed set ships with the app (`callouts/*.json`
//! in the repo, drafts drawn from what the TF2 wiki describes), and a copy
//! the owner edits is kept in `<data>/callouts/` and always wins. Zones are
//! polygons in game units, so they survive a new overview render.

use anyhow::{Context, Result};
use hl_db::Db;
use hl_demos::map_base;
use hl_demos::timeline::{Stored, Timeline};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Zone {
    pub name: String,
    /// Game units, `[x, y]`, in order around the edge.
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalloutFile {
    pub map: String,
    /// Not yet checked by someone who plays the map.
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub source: String,
    /// Most specific first: a position counts for the first zone holding it.
    #[serde(default)]
    pub zones: Vec<Zone>,
    /// Callouts known by name and not yet drawn.
    #[serde(default)]
    pub names: Vec<String>,
    /// Who drew them, when a shared preset says (Q32).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Where this copy came from: "yours", "built in" or "none". Not saved.
    #[serde(default, skip_deserializing)]
    pub origin: String,
}

const BUILT_IN: &[(&str, &str)] = &[
    ("product", include_str!("../../../callouts/product.json")),
    ("upward", include_str!("../../../callouts/upward.json")),
    ("steel", include_str!("../../../callouts/steel.json")),
    ("swiftwater", include_str!("../../../callouts/swiftwater.json")),
    ("proot", include_str!("../../../callouts/proot.json")),
    ("ashville", include_str!("../../../callouts/ashville.json")),
    ("vigil", include_str!("../../../callouts/vigil.json")),
];

fn user_path(data: &Path, base: &str) -> PathBuf {
    data.join("callouts").join(format!("{base}.json"))
}

/// Maps with a built-in seed.
pub fn built_in_bases() -> impl Iterator<Item = &'static str> {
    BUILT_IN.iter().map(|(b, _)| *b)
}

/// Maps the player has their own callouts for.
pub fn user_bases(data: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(data.join("callouts")) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok()?.strip_suffix(".json").map(str::to_string))
        // The copy an import keeps for Undo is not a map of its own.
        .filter(|b| !b.ends_with(".previous"))
        .collect()
}

/// The callouts for `map`: the owner's copy, else the built-in seed, else
/// an empty file to start drawing on.
pub fn load(data: &Path, map: &str) -> Result<CalloutFile> {
    let base = map_base(map);
    let path = user_path(data, &base);
    if path.exists() {
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut f: CalloutFile = serde_json::from_str(&text).with_context(|| format!("{} is not a callout file", path.display()))?;
        f.origin = "yours".into();
        return Ok(f);
    }
    if let Some((_, text)) = BUILT_IN.iter().find(|(b, _)| *b == base) {
        let mut f: CalloutFile = serde_json::from_str(text).context("a built-in callout file")?;
        f.origin = "built in".into();
        return Ok(f);
    }
    Ok(CalloutFile { map: base, origin: "none".into(), ..Default::default() })
}

/// Save the owner's copy. Written whole, through a temporary file, so a crash
/// mid-write cannot leave half a map.
pub fn save(data: &Path, map: &str, file: &CalloutFile) -> Result<CalloutFile> {
    let base = map_base(map);
    let path = user_path(data, &base);
    std::fs::create_dir_all(path.parent().context("no folder")?)?;
    let mut f = file.clone();
    f.map = base.clone();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&f)?)?;
    std::fs::rename(&tmp, &path)?;
    // Edited since the import: Undo would throw the edits away too.
    forget_undo(data, &base)?;
    load(data, &base)
}

/// Drop the owner's copy and go back to the built-in one.
pub fn reset(data: &Path, map: &str) -> Result<CalloutFile> {
    let path = user_path(data, &map_base(map));
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    forget_undo(data, &map_base(map))?;
    load(data, map)
}

// ---- presets (Q32) ---------------------------------------------------------
//
// Callouts get passed round on Discord and corrected, like `.lang` files. A
// preset is the file the app already keeps, with `format` so a later version
// can change it, and optionally who drew it: `<map>.callouts.json`.

/// The preset format this version writes and reads.
pub const PRESET_FORMAT: u64 = 1;
/// Far above any real map's callouts; a file this big is something else.
const PRESET_MAX_BYTES: u64 = 1024 * 1024;
const PRESET_MAX_ZONES: usize = 500;

/// Where an import keeps what it replaced, for one Undo. `null` inside means
/// there was no copy of the player's own: Undo goes back to the built-in.
fn undo_path(data: &Path, base: &str) -> PathBuf {
    data.join("callouts").join(format!("{base}.previous.json"))
}

fn forget_undo(data: &Path, base: &str) -> Result<()> {
    let p = undo_path(data, base);
    if p.exists() {
        std::fs::remove_file(&p)?;
    }
    Ok(())
}

/// Whether the last import on this map can still be taken back.
pub fn has_undo(data: &Path, base: &str) -> bool {
    undo_path(data, base).exists()
}

/// Write a map's callouts, as this player has them, to a preset file.
pub fn export(data: &Path, map: &str, to: &Path) -> Result<()> {
    let f = load(data, map)?;
    let mut v = serde_json::to_value(&f)?;
    let o = v.as_object_mut().context("callouts as an object")?;
    o.remove("origin");
    o.insert("format".into(), PRESET_FORMAT.into());
    std::fs::write(to, serde_json::to_string_pretty(&v)?).with_context(|| format!("writing {}", to.display()))?;
    Ok(())
}

/// Read a preset and check it is one: the right format, every zone named
/// with at least three real corners, and not absurdly large.
pub fn read_preset(from: &Path) -> Result<CalloutFile> {
    let len = std::fs::metadata(from).with_context(|| format!("reading {}", from.display()))?.len();
    anyhow::ensure!(len <= PRESET_MAX_BYTES, "{} is larger than 1 MB: not a callout file", from.display());
    let text = std::fs::read_to_string(from).with_context(|| format!("reading {}", from.display()))?;
    let v: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("{} is not a callout file", from.display()))?;
    if let Some(n) = v.get("format").and_then(serde_json::Value::as_u64) {
        anyhow::ensure!(n <= PRESET_FORMAT, "{} was made by a newer version of the app; update to read it", from.display());
    }
    let mut f: CalloutFile = serde_json::from_value(v).with_context(|| format!("{} is not a callout file", from.display()))?;
    f.map = map_base(&f.map);
    anyhow::ensure!(!f.map.is_empty(), "{} does not say which map it is for", from.display());
    anyhow::ensure!(f.zones.len() <= PRESET_MAX_ZONES, "{} has {} zones; that is not a map's callouts", from.display(), f.zones.len());
    for (i, z) in f.zones.iter().enumerate() {
        anyhow::ensure!(!z.name.trim().is_empty(), "zone {} in {} has no name", i + 1, from.display());
        anyhow::ensure!(z.points.len() >= 3, "\"{}\" in {} has fewer than three corners", z.name, from.display());
        anyhow::ensure!(z.points.iter().flatten().all(|c| c.is_finite() && c.abs() < 100_000.0), "\"{}\" in {} has a corner off the map", z.name, from.display());
    }
    f.names.retain(|n| !n.trim().is_empty());
    f.origin = String::new();
    Ok(f)
}

/// What an import would do, for the player to confirm.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetCheck {
    /// The map the file says it is for.
    pub map: String,
    /// The map it would go to: the one asked for, else the file's own.
    pub target: String,
    pub zones: usize,
    pub names: usize,
    pub draft: bool,
    pub author: Option<String>,
    pub source: String,
    /// What the target has now.
    pub current_origin: String,
    pub current_zones: usize,
}

/// Check a preset without importing it. `map` is where it would go; with
/// `None` it goes to the map the file names (a file dropped on the window).
pub fn inspect(data: &Path, map: Option<&str>, from: &Path) -> Result<PresetCheck> {
    let f = read_preset(from)?;
    let target = map.map(map_base).unwrap_or_else(|| f.map.clone());
    let now = load(data, &target)?;
    Ok(PresetCheck {
        zones: f.zones.len(),
        names: f.names.len(),
        draft: f.draft,
        author: f.author.clone(),
        source: f.source.clone(),
        current_origin: now.origin,
        current_zones: now.zones.len(),
        map: f.map,
        target,
    })
}

/// Import a preset as the player's own callouts for `map`, keeping what it
/// replaces for one Undo. A file for another map is refused unless
/// `any_map`: Product's zones on Vigil would sit in the wrong places.
pub fn import(data: &Path, map: &str, from: &Path, any_map: bool) -> Result<CalloutFile> {
    let base = map_base(map);
    let mut f = read_preset(from)?;
    anyhow::ensure!(any_map || f.map == base, "this file is for {}, not {base}", f.map);
    let dir = data.join("callouts");
    std::fs::create_dir_all(&dir)?;
    let mine = user_path(data, &base);
    let previous = if mine.exists() { std::fs::read_to_string(&mine)? } else { "null".to_string() };
    std::fs::write(undo_path(data, &base), previous)?;
    f.map = base.clone();
    let tmp = mine.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&f)?)?;
    std::fs::rename(&tmp, &mine)?;
    load(data, &base)
}

/// Take the last import back: the player's previous copy, or the built-in
/// callouts if there was none.
pub fn undo(data: &Path, map: &str) -> Result<CalloutFile> {
    let base = map_base(map);
    let path = undo_path(data, &base);
    let previous = std::fs::read_to_string(&path).context("there is no import to undo")?;
    let mine = user_path(data, &base);
    if previous.trim() == "null" {
        if mine.exists() {
            std::fs::remove_file(&mine)?;
        }
    } else {
        std::fs::write(&mine, previous)?;
    }
    std::fs::remove_file(&path)?;
    load(data, &base)
}

fn inside(x: f64, y: f64, poly: &[[f64; 2]]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let ([x1, y1], [x2, y2]) = (poly[i], poly[(i + 1) % n]);
        if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
            c = !c;
        }
    }
    c
}

fn area(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    let twice: f64 = (0..n).map(|i| {
        let ([x1, y1], [x2, y2]) = (poly[i], poly[(i + 1) % n]);
        x1 * y2 - x2 * y1
    }).sum();
    (twice / 2.0).abs()
}

impl CalloutFile {
    /// The zone holding `(x, y)`, if any. Where zones overlap the smallest
    /// wins -- Shack inside Flank is Shack -- whatever order they are in, so
    /// an edited file cannot shadow a zone by where it put it.
    pub fn zone_at(&self, x: f64, y: f64) -> Option<usize> {
        self.zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.points.len() >= 3 && inside(x, y, &z.points))
            .min_by(|(_, a), (_, b)| area(&a.points).total_cmp(&area(&b.points)))
            .map(|(i, _)| i)
    }
}

// ---- time in each zone, from STV timelines ----------------------------

/// One player's seconds, summed over a match's demos: name, seconds on each
/// class, team, seconds alive, seconds in each zone.
type Tally = (String, [u32; 10], u8, u32, HashMap<usize, u32>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneTime {
    pub zone: String,
    pub seconds: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerPositions {
    pub account_id: u32,
    pub name: String,
    /// 1 Scout ... 9 Engineer (`tf_demo_parser`'s numbering): their most played.
    pub class: u8,
    /// 2 RED, 3 BLU: the player's team for the whole match, as the log has
    /// it -- not the colour worn in a demo, which swaps between the halves
    /// and maps of a combined log.
    pub team: u8,
    pub alive_s: u32,
    /// Most time first; time in no zone is left out.
    pub zones: Vec<ZoneTime>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionsView {
    pub map: String,
    pub zones: usize,
    pub draft: bool,
    pub players: Vec<PlayerPositions>,
}

/// Where each player spent their live time, zone by zone, from the match's
/// STV timelines: one position a second. `None` without an STV timeline or
/// without any drawn zone for the map.
pub async fn positions(db: &Db, data: &Path, log_id: i64, map: &str) -> Result<Option<PositionsView>> {
    let callouts = load(data, map)?;
    if callouts.zones.is_empty() {
        return Ok(None);
    }
    let mut acc: HashMap<u32, Tally> = HashMap::new();
    let mut any = false;
    for demo in db.demos_for_log(log_id).await?.into_iter().filter(|d| d.kind == "stv") {
        let Some(row) = db.timeline(demo.demo_id).await? else { continue };
        let stored = Stored {
            version: row.version,
            tick_rate: row.tick_rate,
            stride: row.stride as u32,
            head: row.head,
            samples: row.samples,
            changes: row.changes,
            objects: row.objects,
            events: row.events,
            raw_bytes: row.raw_bytes as usize,
        };
        let zones = callouts.clone();
        let per_slot = tokio::task::spawn_blocking(move || -> Result<_> {
            let tl = Timeline::decode(&stored)?;
            let step = tl.tick_rate.round().max(1.0) as u32;
            let mut out = Vec::new();
            for (slot, track) in tl.tracks.iter().enumerate() {
                let Some(account) = crate::aim::account_of(&tl.people[slot].steamid) else { continue };
                let stretches = tl.stretches(slot);
                let mut classes = [0u32; 10];
                let mut team_ticks: HashMap<u8, u32> = HashMap::new();
                let mut alive = 0;
                let mut in_zone: HashMap<usize, u32> = HashMap::new();
                let mut next = 0u32;
                let mut j = 0;
                for s in &track.samples {
                    if s.t < next {
                        continue;
                    }
                    next = s.t + step;
                    while j + 1 < stretches.len() && stretches[j].1 <= s.t {
                        j += 1;
                    }
                    let Some((from, to, n)) = stretches.get(j) else { continue };
                    if !(*from <= s.t && s.t < *to && n.live()) {
                        continue;
                    }
                    alive += 1;
                    classes[usize::from(n.class.min(9))] += 1;
                    *team_ticks.entry(n.team).or_default() += 1;
                    if let Some(z) = zones.zone_at(f64::from(s.pos[0]), f64::from(s.pos[1])) {
                        *in_zone.entry(z).or_default() += 1;
                    }
                }
                if alive > 0 {
                    let team = team_ticks.into_iter().max_by_key(|(_, n)| *n).map_or(0, |(t, _)| t);
                    out.push((account, tl.people[slot].name.clone(), classes, team, alive, in_zone));
                }
            }
            Ok(out)
        })
        .await??;
        any = true;
        for (account, name, classes, team, alive, zones) in per_slot {
            let e = acc.entry(account).or_insert_with(|| (name, [0; 10], team, 0, HashMap::new()));
            for (i, c) in classes.iter().enumerate() {
                e.1[i] += c;
            }
            e.3 += alive;
            for (z, s) in zones {
                *e.4.entry(z).or_default() += s;
            }
        }
    }
    if !any {
        return Ok(None);
    }
    // Sides from the log (Flashy): a combined log's demos can have the teams
    // in each other's colours, and a player's colour in whichever demo came
    // first put half of one team with the other. The log's team is one per
    // player for the whole match. Anyone the log does not have keeps the
    // colour they wore most.
    let sides = log_sides(db, log_id).await?;
    let mut players: Vec<PlayerPositions> = acc
        .into_iter()
        .map(|(account_id, (name, classes, worn, alive_s, zones))| {
            let team = sides.get(&account_id).copied().unwrap_or(worn);
            let class = (1..=9u8).max_by_key(|c| classes[usize::from(*c)]).unwrap_or(0);
            let mut zones: Vec<ZoneTime> = zones.into_iter().map(|(z, seconds)| ZoneTime { zone: callouts.zones[z].name.clone(), seconds }).collect();
            zones.sort_by_key(|z| std::cmp::Reverse(z.seconds));
            PlayerPositions { account_id, name, class, team, alive_s, zones }
        })
        .collect();
    players.sort_by_key(|p| (p.team, p.class));
    Ok(Some(PositionsView { map: callouts.map.clone(), zones: callouts.zones.len(), draft: callouts.draft, players }))
}

/// Each player's team for the whole match, from the stored log: 2 RED, 3 BLU.
async fn log_sides(db: &Db, log_id: i64) -> Result<HashMap<u32, u8>> {
    let Some(json) = db.raw_log(log_id).await? else { return Ok(HashMap::new()) };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else { return Ok(HashMap::new()) };
    let Ok(log) = crate::normalize::normalize(log_id, &value) else { return Ok(HashMap::new()) };
    Ok(log
        .players
        .iter()
        .map(|p| (p.id.account_id(), if p.team == hl_core::matchdata::Team::Red { 2 } else { 3 }))
        .collect())
}

// ---- tendencies across matches (Q28) ---------------------------------

/// `tf_demo_parser`'s class numbers to the app's names.
const DEMO_CLASSES: [&str; 10] = ["", "scout", "sniper", "soldier", "demoman", "medic", "heavyweapons", "pyro", "spy", "engineer"];

/// Fewest kills and deaths in drawn zones before a map's tendencies say
/// anything: below it the shares are a handful of fights.
const MIN_FIGHTS: usize = 20;

/// A zone by the player's own side: "RED Cliff" is "Own Cliff" to a RED
/// player and "Enemy Cliff" to a BLU one. A zone with no side keeps its name.
pub fn by_side(zone: &str, red: bool) -> String {
    for (prefix, is_red) in [("RED ", true), ("BLU ", false)] {
        if let Some(rest) = zone.strip_prefix(prefix) {
            return format!("{} {rest}", if is_red == red { "Own" } else { "Enemy" });
        }
    }
    zone.to_string()
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneFights {
    pub zone: String,
    pub kills: u32,
    pub deaths: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneShare {
    pub zone: String,
    /// Share of their alive time in a drawn zone, 0-1.
    pub share: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZonePath {
    pub from: String,
    pub to: String,
    pub times: u32,
}

/// One player's habits on one map and class, over every match held (Q28).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapTendencies {
    pub map: String,
    pub draft: bool,
    /// Matches their kills and deaths come from.
    pub matches: usize,
    /// Where their fights ended, by their own side; most first.
    pub fights: Vec<ZoneFights>,
    /// Kills and deaths outside any drawn zone.
    pub unzoned: u32,
    /// STV demos their time in zones comes from.
    pub stvs: usize,
    pub alive_s: u32,
    /// Where they stood while alive; most first.
    pub time: Vec<ZoneShare>,
    /// The moves they make most, zone to zone.
    pub paths: Vec<ZonePath>,
}

/// A player's tendencies on every map with drawn callouts, for one class:
/// where their fights end (every match with a raw log) and where they stand
/// and move (every match with an STV). Maps with too little are left out.
pub async fn tendencies(db: &Db, data: &Path, account: u32, class: &str) -> Result<Vec<MapTendencies>> {
    let kept = db.kept_maps().await?;
    let stv_logs = db.player_stv_logs(account).await?;
    let mut bases: Vec<String> = kept.iter().map(|m| map_base(m)).collect();
    bases.sort();
    bases.dedup();
    let mut out = Vec::new();
    for base in bases {
        let callouts = load(data, &base)?;
        if callouts.zones.is_empty() {
            continue;
        }
        let maps: Vec<String> = kept.iter().filter(|m| map_base(m) == base).cloned().collect();

        // Where their fights ended.
        let mut fights: HashMap<String, ZoneFights> = HashMap::new();
        let mut unzoned = 0;
        let mut logs = std::collections::HashSet::new();
        for (log_id, got, team, cls, x, y) in db.player_kill_positions(account, &maps).await? {
            if cls != class {
                continue;
            }
            logs.insert(log_id);
            let Some(z) = callouts.zone_at(f64::from(x), f64::from(y)) else {
                unzoned += 1;
                continue;
            };
            let name = by_side(&callouts.zones[z].name, team.eq_ignore_ascii_case("red"));
            let e = fights.entry(name.clone()).or_insert_with(|| ZoneFights { zone: name, ..Default::default() });
            if got {
                e.kills += 1;
            } else {
                e.deaths += 1;
            }
        }
        let mut fights: Vec<ZoneFights> = fights.into_values().collect();
        fights.sort_by_key(|f| std::cmp::Reverse(f.kills + f.deaths));
        let in_zones: u32 = fights.iter().map(|f| f.kills + f.deaths).sum();

        // Where they stood, and how they moved, from the STVs.
        let (stvs, alive_s, time, paths) = stand(db, &callouts, account, class, stv_logs.iter().filter(|(_, m)| map_base(m) == base).map(|(l, _)| *l)).await?;
        if (in_zones as usize) < MIN_FIGHTS && stvs == 0 {
            continue;
        }
        out.push(MapTendencies { map: callouts.map.clone(), draft: callouts.draft, matches: logs.len(), fights, unzoned, stvs, alive_s, time, paths });
    }
    out.sort_by_key(|m| std::cmp::Reverse(m.matches));
    Ok(out)
}

/// Time in each zone and the moves between them, summed over the STVs:
/// `(demos read, seconds alive, shares, paths)`. A move is counted when the
/// zone they are in changes to another drawn zone within ten seconds of
/// leaving the last one, so walking through undrawn ground still counts
/// and a death and respawn does not.
async fn stand(
    db: &Db,
    callouts: &CalloutFile,
    account: u32,
    class: &str,
    logs: impl Iterator<Item = i64>,
) -> Result<(usize, u32, Vec<ZoneShare>, Vec<ZonePath>)> {
    let class_no = DEMO_CLASSES.iter().position(|c| *c == class).unwrap_or(0) as u8;
    let mut demos = 0;
    let mut alive = 0u32;
    let mut in_zone: HashMap<String, u32> = HashMap::new();
    let mut moves: HashMap<(String, String), u32> = HashMap::new();
    for log_id in logs {
        let sides = log_sides(db, log_id).await?;
        let red = sides.get(&account).map(|t| *t == 2);
        for demo in db.demos_for_log(log_id).await?.into_iter().filter(|d| d.kind == "stv") {
            let Some(row) = db.timeline(demo.demo_id).await? else { continue };
            let stored = Stored {
                version: row.version,
                tick_rate: row.tick_rate,
                stride: row.stride as u32,
                head: row.head,
                samples: row.samples,
                changes: row.changes,
                objects: row.objects,
                events: row.events,
                raw_bytes: row.raw_bytes as usize,
            };
            let zones = callouts.clone();
            // Their zone each second they were alive on the class, or None.
            let seq = tokio::task::spawn_blocking(move || -> Result<Option<Vec<Option<(usize, bool)>>>> {
                let tl = Timeline::decode(&stored)?;
                let Some(slot) = tl.people.iter().position(|p| crate::aim::account_of(&p.steamid) == Some(account)) else { return Ok(None) };
                let step = tl.tick_rate.round().max(1.0) as u32;
                let stretches = tl.stretches(slot);
                let mut out = Vec::new();
                let (mut next, mut j) = (0u32, 0);
                for s in &tl.tracks[slot].samples {
                    if s.t < next {
                        continue;
                    }
                    next = s.t + step;
                    while j + 1 < stretches.len() && stretches[j].1 <= s.t {
                        j += 1;
                    }
                    let Some((from, to, n)) = stretches.get(j) else { continue };
                    if !(*from <= s.t && s.t < *to && n.live() && n.class == class_no) {
                        out.push(None);
                        continue;
                    }
                    let z = zones.zone_at(f64::from(s.pos[0]), f64::from(s.pos[1]));
                    // RED in the demo: a fallback for a player the log lacks.
                    out.push(Some((z.unwrap_or(usize::MAX), n.team == 2)));
                }
                Ok(Some(out))
            })
            .await??;
            let Some(seq) = seq else { continue };
            demos += 1;
            let mut last: Option<(String, usize)> = None;
            for (i, s) in seq.iter().enumerate() {
                let Some((z, worn_red)) = s else {
                    last = None;
                    continue;
                };
                alive += 1;
                if *z == usize::MAX {
                    continue;
                }
                let name = by_side(&callouts.zones[*z].name, red.unwrap_or(*worn_red));
                *in_zone.entry(name.clone()).or_default() += 1;
                if let Some((prev, at)) = &last {
                    if *prev != name && i - at <= 10 {
                        *moves.entry((prev.clone(), name.clone())).or_default() += 1;
                    }
                }
                last = Some((name, i));
            }
        }
    }
    let total: u32 = in_zone.values().sum();
    let mut time: Vec<ZoneShare> = in_zone.into_iter().map(|(zone, s)| ZoneShare { zone, share: f64::from(s) / f64::from(total.max(1)) }).collect();
    time.sort_by(|a, b| b.share.total_cmp(&a.share));
    let mut paths: Vec<ZonePath> = moves.into_iter().map(|((from, to), times)| ZonePath { from, to, times }).collect();
    paths.sort_by_key(|p| std::cmp::Reverse(p.times));
    paths.truncate(8);
    Ok((demos, alive, time, paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sided_zone_is_named_by_the_players_own_side() {
        assert_eq!(by_side("RED Cliff", true), "Own Cliff");
        assert_eq!(by_side("RED Cliff", false), "Enemy Cliff");
        assert_eq!(by_side("BLU House", false), "Own House");
        assert_eq!(by_side("Point", true), "Point");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-callouts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn square(name: &str) -> Zone {
        Zone { name: name.into(), points: vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]] }
    }

    #[test]
    fn proot_and_product_are_published_and_the_rest_are_drafts() {
        let dir = scratch("seeds");
        let proot = load(&dir, "koth_proot_b5b").unwrap();
        assert_eq!((proot.author.as_deref(), proot.draft, proot.zones.len()), (Some("Flashy"), false, 43));
        for (base, _) in BUILT_IN.iter().filter(|(b, _)| *b != "proot") {
            assert_eq!(load(&dir, base).unwrap().draft, *base != "product", "{base}");
        }
    }

    #[test]
    fn a_preset_goes_out_and_back_in_whole() {
        let dir = scratch("roundtrip");
        let mine = CalloutFile { map: "vigil".into(), zones: vec![square("Cliff"), square("House")], names: vec!["Lobby".into()], ..Default::default() };
        save(&dir, "pl_vigil_rc10", &mine).unwrap();
        let out = dir.join("vigil.callouts.json");
        export(&dir, "vigil", &out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("\"format\": 1") && !text.contains("origin"), "{text}");

        let other = scratch("roundtrip-other");
        let check = inspect(&other, None, &out).unwrap();
        assert_eq!((check.target.as_str(), check.zones, check.names, check.current_origin.as_str()), ("vigil", 2, 1, "built in"));
        let got = import(&other, "vigil", &out, false).unwrap();
        assert_eq!((got.origin.as_str(), got.zones.len()), ("yours", 2));
        assert_eq!(got.zones, mine.zones);
    }

    #[test]
    fn undo_goes_back_to_what_was_there() {
        let dir = scratch("undo");
        let preset = dir.join("p.callouts.json");
        std::fs::write(&preset, r#"{"map":"koth_product_final","format":1,"zones":[{"name":"Cliff","points":[[0,0],[1,0],[1,1]]}]}"#).unwrap();

        // Over the built-in copy: Undo goes back to built in.
        import(&dir, "product", &preset, false).unwrap();
        assert!(has_undo(&dir, "product"));
        assert_eq!(undo(&dir, "product").unwrap().origin, "built in");
        assert!(!has_undo(&dir, "product"));

        // Over the player's own: Undo gives it back.
        save(&dir, "product", &CalloutFile { map: "product".into(), zones: vec![square("Mine")], ..Default::default() }).unwrap();
        import(&dir, "product", &preset, false).unwrap();
        assert_eq!(undo(&dir, "product").unwrap().zones[0].name, "Mine");
        assert_eq!(user_bases(&dir), vec!["product".to_string()], "the Undo copy is not listed as a map");
    }

    #[test]
    fn a_bad_or_wrong_preset_is_refused() {
        let dir = scratch("refuse");
        let write = |name: &str, text: &str| {
            let p = dir.join(name);
            std::fs::write(&p, text).unwrap();
            p
        };
        let product = write("product.json", r#"{"map":"product","zones":[{"name":"Cliff","points":[[0,0],[1,0],[1,1]]}]}"#);
        assert!(import(&dir, "vigil", &product, false).unwrap_err().to_string().contains("for product, not vigil"));
        assert!(import(&dir, "vigil", &product, true).is_ok(), "unless the player says so");

        for (name, text) in [
            ("two.json", r#"{"map":"vigil","zones":[{"name":"Line","points":[[0,0],[1,0]]}]}"#),
            ("noname.json", r#"{"map":"vigil","zones":[{"name":" ","points":[[0,0],[1,0],[1,1]]}]}"#),
            ("far.json", r#"{"map":"vigil","zones":[{"name":"X","points":[[0,0],[1e9,0],[1,1]]}]}"#),
            ("newer.json", r#"{"map":"vigil","format":2,"zones":[]}"#),
            ("nomap.json", r#"{"zones":[]}"#),
            ("notjson.json", "Cliff, House, Lobby"),
        ] {
            assert!(read_preset(&write(name, text)).is_err(), "{name} should be refused");
        }
    }

    #[test]
    fn where_zones_overlap_the_smaller_one_wins_whatever_the_order() {
        let f = CalloutFile {
            zones: vec![
                Zone { name: "flank".into(), points: vec![[-50.0, -50.0], [50.0, -50.0], [50.0, 50.0], [-50.0, 50.0]] },
                Zone { name: "shack".into(), points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]] },
            ],
            ..Default::default()
        };
        assert_eq!(f.zone_at(5.0, 5.0), Some(1), "inside the shack is the shack, though flank comes first");
        assert_eq!(f.zone_at(-20.0, 30.0), Some(0));
        assert_eq!(f.zone_at(99.0, 0.0), None);
    }

    #[test]
    fn every_built_in_file_reads() {
        for (base, text) in BUILT_IN {
            let f: CalloutFile = serde_json::from_str(text).unwrap_or_else(|e| panic!("{base}: {e}"));
            assert_eq!(&f.map, base);
            assert!(f.zones.iter().all(|z| z.points.len() >= 3), "{base} has a zone with under three corners");
        }
    }

    #[test]
    fn a_saved_copy_wins_and_a_reset_brings_the_seed_back() {
        let dir = std::env::temp_dir().join(format!("hl-callouts-{}", std::process::id()));
        let seed = load(&dir, "koth_product_final").unwrap();
        assert_eq!(seed.origin, "built in");
        let mut mine = seed.clone();
        mine.zones.truncate(1);
        mine.draft = false;
        let saved = save(&dir, "koth_product_final", &mine).unwrap();
        assert_eq!((saved.origin.as_str(), saved.zones.len(), saved.draft), ("yours", 1, false));
        assert_eq!(reset(&dir, "koth_product_final").unwrap().zones.len(), seed.zones.len());
        let _ = std::fs::remove_dir_all(dir);
    }
}
