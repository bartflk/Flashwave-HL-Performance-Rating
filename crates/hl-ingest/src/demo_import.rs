//! A match from a demo alone (Q18, beowulf): the server had no logs.tf
//! config, so no log exists, and the demo is all there is.
//!
//! `hl_demos::synth` turns the demo into a server log and a logs.tf summary;
//! this stores them under a log id of its own, so the match is an ordinary
//! one to every page and pass. The id is negative -- logs.tf's are positive,
//! so the two can never meet -- and made from the file's name, so importing
//! the same demo twice replaces the first rather than adding a second.
//!
//! The demo has to live where the app looks for demos: a folder scan drops
//! any demo row whose file it did not find. One picked from elsewhere is
//! copied into `tf/demos`.

use crate::sync::{store_log_json, Imported};
use anyhow::{bail, Context, Result};
use hl_core::SteamId;
use hl_db::{Db, TimelineRow};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// How imported logs are linked to their demo: kept through every rescan.
pub const METHOD: &str = "import";
/// How a demo dropped onto a match page is linked to it: also kept.
pub const MANUAL: &str = "manual";

/// The log id an imported demo is first tried under.
pub fn log_id_for(file_name: &str) -> i64 {
    log_id_attempt(file_name, 0)
}

/// The `attempt`th id a demo's name hashes to. Attempt 0 is the id every
/// import so far was stored under, so those rows keep theirs; a later attempt
/// hashes the name with the attempt number, to probe past a file whose name
/// collides with another's (audit SY-10).
fn log_id_attempt(file_name: &str, attempt: u32) -> i64 {
    let name = file_name.to_ascii_lowercase();
    let name = if attempt == 0 { name } else { format!("{name}#{attempt}") };
    let h = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3));
    -((h % 1_000_000_000) as i64) - 1
}

/// The title an imported match is given.
fn import_title(file_name: &str) -> String {
    format!("{} (from the demo)", file_name.trim_end_matches(".dem"))
}

/// The id this demo is stored under: its name's hash, unless a different demo
/// already holds that id. The id is only 30 bits of the name, so two names
/// can collide, and the same name in two folders is two demos: either used to
/// overwrite the other's match silently. The same file again finds its own id.
async fn free_log_id(db: &Db, file_name: &str, path: &Path) -> Result<i64> {
    let same_file = |a: &Path| a == path || a.canonicalize().ok().zip(path.canonicalize().ok()).is_some_and(|(x, y)| x == y);
    let title = import_title(file_name);
    for attempt in 0..64 {
        let id = log_id_attempt(file_name, attempt);
        let Some(json) = db.raw_log(id).await? else { return Ok(id) };
        let demos = db.demos_for_log(id).await?;
        // Its own match from an earlier import, whose demo is linked or (the
        // link lost) which carries this name's title.
        let titled = serde_json::from_str::<serde_json::Value>(&json).ok().is_some_and(|v| v.pointer("/info/title").and_then(|t| t.as_str()) == Some(title.as_str()));
        if demos.iter().any(|d| same_file(Path::new(&d.path))) || (demos.is_empty() && titled) {
            return Ok(id);
        }
    }
    bail!("no free log id was found for {file_name}")
}

/// When the demo started, as a server writes its local time into a log:
/// from the name an STV server or Demo Support gives the file, else from
/// the file's own time less its length.
pub fn wall_start(file_name: &str, mtime_unix: i64, duration_s: i64) -> i64 {
    // `match-20260823-1956-pl_upward_f12.dem`
    let stv = file_name.split('-').collect::<Vec<_>>();
    if stv.len() >= 3 && stv[0].eq_ignore_ascii_case("match") && stv[1].len() == 8 && stv[2].len() == 4 {
        let s = format!("{} {}", stv[1], stv[2]);
        if let Ok(t) = chrono::NaiveDateTime::parse_from_str(&format!("{s}00"), "%Y%m%d %H%M%S") {
            return local_unix(t);
        }
    }
    if let Some(s) = hl_demos::scan::filename_time(file_name) {
        if let Ok(t) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
            return local_unix(t);
        }
    }
    mtime_unix - duration_s
}

/// A wall-clock time read off a file name, as the Unix time it was on this
/// machine's clock. Recorders and servers write their local time, not UTC:
/// read as UTC it put a match an hour or two off in Europe, which moved it in
/// the list and out of its ETF2L and demos.tf windows (audit SY-9).
fn local_unix(t: chrono::NaiveDateTime) -> i64 {
    use chrono::TimeZone;
    chrono::Local.from_local_datetime(&t).earliest().map_or_else(|| t.and_utc().timestamp(), |l| l.timestamp())
}

/// Where the demo is kept: as it is inside the TF folder's demo folders,
/// else copied into `tf/demos`.
fn place(tf: &Path, file: &Path) -> Result<PathBuf> {
    let dirs = [tf.to_path_buf(), tf.join("demos"), tf.join("demos").join("stv")];
    let parent = file.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(x, y)| x == y);
    if dirs.iter().any(|d| same(d, &parent)) {
        return Ok(file.to_path_buf());
    }
    let name = file.file_name().context("no file name")?;
    let dest = tf.join("demos").join(name);
    let size = std::fs::metadata(file)?.len();
    if std::fs::metadata(&dest).map(|m| m.len()) .ok() != Some(size) {
        std::fs::create_dir_all(tf.join("demos"))?;
        hl_demos::scan::mark_own_write(&dest);
        std::fs::copy(file, &dest).with_context(|| format!("copying the demo to {}", dest.display()))?;
    }
    Ok(dest)
}

/// A server log zipped the way logs.tf serves one.
pub(crate) fn zip_text(text: &str) -> Result<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        z.start_file("log.log", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated))?;
        z.write_all(text.as_bytes())?;
        z.finish()?;
    }
    Ok(buf.into_inner())
}

/// The demo already belongs to a log that exists: open that match instead.
#[derive(Debug)]
pub struct AlreadyLogged {
    pub log_id: i64,
    pub share: f64,
}

impl std::fmt::Display for AlreadyLogged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "this demo already belongs to log {} ({:.0}% of it), which is in your matches", self.log_id, self.share * 100.0)
    }
}

impl std::error::Error for AlreadyLogged {}

/// What an import did.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoImported {
    #[serde(flatten)]
    pub log: Imported,
    pub rounds: usize,
    pub kills: usize,
    /// Where the demo is kept now.
    pub path: String,
}

/// Import `file` as a match of its own. `progress` names each step.
pub async fn import(db: &Db, w: &hl_rating::Weights, tf: &Path, file: &Path, me: Option<SteamId>, mut progress: impl FnMut(&'static str)) -> Result<DemoImported> {
    progress("Copying the demo");
    let path = place(tf, file)?;
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let header = hl_demos::DemoHeader::parse(&bytes)?;
    drop(bytes);
    if header.map.is_empty() {
        bail!("{file_name} does not look like a TF2 demo");
    }
    let rate = header.tick_rate().unwrap_or(66.67);
    let is_stv = header.server.is_empty() || header.recorder.eq_ignore_ascii_case("SourceTV") || header.recorder.to_ascii_lowercase().contains("sourcetv");
    let owner = if is_stv { String::new() } else { me.map(|m| m.to_steamid3()).unwrap_or_default() };

    // The demo's row comes from the folder scan like any other's. If the
    // scan links it to a log that exists, this is not a match without one.
    crate::index_demos(db, tf).await?;
    let here = path.canonicalize()?;
    let demo_id = db
        .demos_named(&file_name)
        .await?
        .into_iter()
        .find(|(_, p)| Path::new(p).canonicalize().ok().as_ref() == Some(&here))
        .map(|(id, _)| id)
        .context("the demo was not found by the folder scan")?;
    let log_id = free_log_id(db, &file_name, &path).await?;
    // Any link to a logs.tf log (positive ids) means one exists: a demo
    // that is half of a combined log covers a small share of it, and is no
    // less that log's.
    if let Some((real, share)) = db.links_of_demo(demo_id).await?.into_iter().find(|(l, _)| *l > 0) {
        return Err(AlreadyLogged { log_id: real, share }.into());
    }

    progress("Reading the demo");
    let read_path = path.clone();
    let (stored, tl) = tokio::task::spawn_blocking(move || -> Result<_> {
        let (_, stored) = hl_demos::aim::pass_recording(&read_path, &owner, rate, Some(hl_demos::timeline::DEFAULT_STRIDE), &mut |_| {})?;
        let stored = stored.context("the demo gave no timeline")?;
        let tl = hl_demos::timeline::Timeline::decode(&stored)?;
        Ok((stored, tl))
    })
    .await??;
    if tl.people.len() < 4 {
        bail!("{file_name} has {} players in it: not a match", tl.people.len());
    }

    progress("Building the match from it");
    let mtime = std::fs::metadata(&path)?.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    let start = wall_start(&file_name, mtime, tl.seconds(tl.end()) as i64);
    let title = import_title(&file_name);
    let synth = hl_demos::synth::synthesize(&tl, &header.map, start, &title);
    if synth.kills == 0 {
        bail!("{file_name} has no kills in a live round: nothing to rate");
    }

    let json = serde_json::to_string(&synth.json)?;
    let log = store_log_json(db, w, log_id, &json).await?;
    // The zip and its kills in one write (audit SY-1).
    crate::kills::store(db, log_id, &zip_text(&synth.text)?, crate::rawlog::parse(&synth.text)).await?;

    progress("Linking the demo");
    // Made by hand: an imported log has no upload time to be placed by.
    let path_s = path.to_string_lossy().into_owned();
    db.add_demo_link(demo_id, log_id, METHOD, 1.0).await?;
    // The log was written on the demo's own clock, so the demo starts
    // exactly where the log does: what the aim pass needs to line the two up.
    db.set_demo_start(demo_id, start as f64).await?;
    // And the log is placed on that clock now, not at the next scan.
    crate::index_demos(db, tf).await?;
    db.put_timeline(
        demo_id,
        &TimelineRow {
            version: stored.version,
            tick_rate: stored.tick_rate,
            stride: i64::from(stored.stride),
            head: stored.head.clone(),
            samples: stored.samples.clone(),
            changes: stored.changes.clone(),
            objects: stored.objects.clone(),
            events: stored.events.clone(),
            raw_bytes: stored.raw_bytes as i64,
            stored_bytes: stored.stored_bytes() as i64,
        },
    )
    .await?;

    Ok(DemoImported { log, rounds: synth.rounds, kills: synth.kills, path: path_s })
}

/// The passes a sync would run for a new log, for the one just imported:
/// each round's map, the fights pass, aim from the demo, then the rating
/// (which every sync ends with, since new matches move the baselines).
pub async fn derive(db: &Db, w: &hl_rating::Weights, me: Option<SteamId>, mut progress: impl FnMut(&'static str)) -> Result<()> {
    progress("Working out each round's map");
    crate::maps::resolve_all(db).await?;
    progress("Reading the fights");
    crate::fights::derive_all(db, false, |_, _| {}).await?;
    if let Some(me) = me {
        progress("Reading aim from the demo");
        if let Err(e) = crate::aim::derive_all(db, me, false, |_, _, _, _| {}).await {
            tracing::warn!(error = %format!("{e:#}"), "reading aim from an imported demo failed");
        }
    }
    progress("Rating");
    crate::rate_all(db, me, w, |_| {}).await?;
    Ok(())
}

/// What linking a demo to a match found.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoLinked {
    pub demo_id: i64,
    pub file_name: String,
    pub stv: bool,
    /// The log's kills the demo holds, found at one offset.
    pub kills_matched: usize,
    pub log_kills: usize,
    /// Players in both.
    pub players_shared: usize,
    pub path: String,
}

/// The demo belongs to another match, or no match at all.
#[derive(Debug)]
pub struct WrongDemo(pub String);

impl std::fmt::Display for WrongDemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for WrongDemo {}

/// The maps a log's map field names: one, or a combined log's several
/// ("product + proot", "pl_vigil_rc10, koth_proot_b5b").
fn map_names(field: &str) -> Vec<String> {
    field.split(['+', ',', '/', '&']).map(str::trim).filter(|m| !m.is_empty()).map(str::to_string).collect()
}

/// The shift from demo seconds to the log's own clock that lines up the
/// most kills, by the same killer on the same victim: `(shift, matched)`.
/// Each log kill counts once.
pub fn best_shift(demo: &[(f64, u32, u32)], log: &[(i64, u32, u32)]) -> Option<(i64, usize)> {
    let mut votes: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for &(s, killer, victim) in demo {
        for &(at, k, v) in log {
            if k == killer && v == victim {
                *votes.entry((at as f64 - s).round() as i64).or_default() += 1;
            }
        }
    }
    // A shift and its neighbours: the two clocks tick in whole seconds, so
    // the true shift splits its votes across adjacent values.
    let score = |c: i64| (c - 1..=c + 1).map(|x| votes.get(&x).copied().unwrap_or(0)).sum::<usize>();
    let best = votes.keys().copied().max_by_key(|c| (score(*c), votes[c]))?;
    let matched = log
        .iter()
        .filter(|&&(at, k, v)| demo.iter().any(|&(s, dk, dv)| dk == k && dv == v && ((at as f64 - s) - best as f64).abs() <= 2.0))
        .count();
    Some((best, matched))
}

/// Link a demo the owner picked (dropped on a match page) to that match.
///
/// Checked before anything is stored: the demo's map is the log's, it
/// shares most of its players, and at least a handful of its kills line up
/// with the log's at one shift -- which is also what places the demo on the
/// log's clock, so aim and every demo panel line up exactly.
pub async fn link_to_log(db: &Db, tf: &Path, file: &Path, log_id: i64, me: Option<SteamId>, progress: impl FnMut(&'static str)) -> Result<DemoLinked> {
    link_to_log_by(db, tf, file, log_id, me, MANUAL, progress).await
}

/// [`link_to_log`], recording how the demo was found (`method`): by hand,
/// or from ETF2L's match page (Q48).
pub async fn link_to_log_by(
    db: &Db,
    tf: &Path,
    file: &Path,
    log_id: i64,
    me: Option<SteamId>,
    method: &str,
    mut progress: impl FnMut(&'static str),
) -> Result<DemoLinked> {
    progress("Copying the demo");
    let path = place(tf, file)?;
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let header = hl_demos::DemoHeader::parse(&bytes)?;
    drop(bytes);
    let json: serde_json::Value = serde_json::from_str(&db.raw_log(log_id).await?.context("this match is not stored")?)?;
    let log_map = json.pointer("/info/map").and_then(|m| m.as_str()).unwrap_or_default().to_string();
    // A combined log's map is its title, "product + proot" (Q51, ivg): the
    // demo may be on any of its maps, as its rounds and parts name them.
    let mut maps = map_names(&log_map);
    maps.extend(db.segments(log_id).await?.into_iter().filter_map(|s| s.map));
    maps.extend(db.parts_of(log_id).await?.into_iter().filter_map(|p| p.map));
    let bases: std::collections::HashSet<String> = maps.iter().map(|m| hl_demos::map_base(m)).filter(|m| !m.is_empty()).collect();
    if !bases.is_empty() && !bases.contains(&hl_demos::map_base(&header.map)) {
        return Err(WrongDemo(format!("{file_name} is on {}, and this match is on {log_map}.", header.map)).into());
    }
    let rate = header.tick_rate().unwrap_or(66.67);
    let is_stv = header.server.is_empty() || header.recorder.to_ascii_lowercase().contains("sourcetv");
    let owner = if is_stv { String::new() } else { me.map(|m| m.to_steamid3()).unwrap_or_default() };

    progress("Reading the demo");
    let read_path = path.clone();
    let (stored, tl) = tokio::task::spawn_blocking(move || -> Result<_> {
        let (_, stored) = hl_demos::aim::pass_recording(&read_path, &owner, rate, Some(hl_demos::timeline::DEFAULT_STRIDE), &mut |_| {})?;
        let stored = stored.context("the demo gave no timeline")?;
        let tl = hl_demos::timeline::Timeline::decode(&stored)?;
        Ok((stored, tl))
    })
    .await??;

    progress("Checking it is this match");
    let log_players: std::collections::HashSet<u32> = json
        .get("players")
        .and_then(|p| p.as_object())
        .map(|o| o.keys().filter_map(|k| SteamId::parse(k).ok()).map(|s| s.account_id()).collect())
        .unwrap_or_default();
    let demo_players: std::collections::HashSet<u32> = tl.people.iter().filter_map(|p| crate::aim::account_of(&p.steamid)).collect();
    let shared = log_players.intersection(&demo_players).count();
    if shared < 6 {
        return Err(WrongDemo(format!("{file_name} has {shared} of this match's players in it: it is from another match.")).into());
    }
    let demo_kills: Vec<(f64, u32, u32)> = tl
        .events
        .iter()
        .filter_map(|(t, e)| {
            let hl_demos::timeline::GameEvent::PlayerDeath(d) = e else { return None };
            let victim = crate::aim::account_of(&tl.people[tl.slot_of_user(d.user_id)?].steamid)?;
            let killer = crate::aim::account_of(&tl.people[tl.slot_of_user(d.attacker)?].steamid)?;
            (killer != victim).then_some((tl.seconds(*t), killer, victim))
        })
        .collect();
    let log_kills: Vec<(i64, u32, u32)> = db.kills_for_log(log_id).await?.iter().map(|k| (k.at_raw, k.killer, k.victim)).collect();
    if log_kills.is_empty() {
        return Err(WrongDemo("This match's server log is not downloaded, so there are no kills to line the demo up with: sync once more, then drop it again.".into()).into());
    }
    let Some((shift, matched)) = best_shift(&demo_kills, &log_kills).filter(|(_, m)| *m >= 5) else {
        return Err(WrongDemo(format!("{file_name} has this match's players, but none of its kills line up with the log's: a different match between the same teams?")).into());
    };

    progress("Linking the demo");
    crate::index_demos(db, tf).await?;
    let here = path.canonicalize()?;
    let demo_id = db
        .demos_named(&file_name)
        .await?
        .into_iter()
        .find(|(_, p)| Path::new(p).canonicalize().ok().as_ref() == Some(&here))
        .map(|(id, _)| id)
        .context("the demo was not found by the folder scan")?;
    // The log's raw clock is the server's; the demo starts `shift` seconds
    // into it, and the log clock's offset turns that into UTC.
    let clock = db.log_clock_offset(log_id).await?.unwrap_or(0);
    db.set_demo_start(demo_id, (shift + clock) as f64).await?;
    let share = (matched as f64 / log_kills.len().max(1) as f64).min(1.0);
    db.add_demo_link(demo_id, log_id, method, share).await?;
    // The demo's header names its map: resolve again now, so a match whose
    // log never said which map it was on is drawn at once (Q30).
    if let Err(e) = crate::maps::resolve_all(db).await {
        tracing::warn!(log_id, error = %format!("{e:#}"), "round maps not resolved again after linking a demo");
    }
    db.put_timeline(
        demo_id,
        &TimelineRow {
            version: stored.version,
            tick_rate: stored.tick_rate,
            stride: i64::from(stored.stride),
            head: stored.head.clone(),
            samples: stored.samples.clone(),
            changes: stored.changes.clone(),
            objects: stored.objects.clone(),
            events: stored.events.clone(),
            raw_bytes: stored.raw_bytes as i64,
            stored_bytes: stored.stored_bytes() as i64,
        },
    )
    .await?;
    if let Some(me) = me {
        progress("Reading aim from the demo");
        if let Err(e) = crate::aim::derive_log(db, me, log_id).await {
            tracing::warn!(log_id, error = %format!("{e:#}"), "reading aim from a linked demo failed");
        }
    }
    Ok(DemoLinked {
        demo_id,
        file_name,
        stv: is_stv,
        kills_matched: matched,
        log_kills: log_kills.len(),
        players_shared: shared,
        path: path.to_string_lossy().into_owned(),
    })
}

/// The demos.tf id in a link or a bare number: `https://demos.tf/990239`,
/// `demos.tf/990239`, `990239`.
pub fn demostf_id(text: &str) -> Option<i64> {
    let t = text.trim().trim_end_matches('/');
    let last = t.rsplit('/').next()?;
    let id: i64 = last.split(['?', '#']).next()?.parse().ok()?;
    let host_ok = !t.contains('/') || t.contains("demos.tf/");
    (id > 0 && host_ok).then_some(id)
}

/// Add a demo by its demos.tf link (Q53, Emiel): downloaded into the STV
/// folder, then checked and linked like a dropped one. The way round a
/// lookup that missed it.
pub async fn link_demostf(
    db: &Db,
    sources: &crate::Sources,
    tf: &Path,
    log_id: i64,
    demos_tf_id: i64,
    me: Option<SteamId>,
    progress: impl FnMut(u64, Option<u64>),
) -> Result<DemoLinked> {
    let meta = sources.demostf_meta(demos_tf_id).await?;
    if !meta.available() {
        return Err(WrongDemo(format!("demos.tf no longer has the file of demo {demos_tf_id}: its demos from before 14 October 2019 are gone.")).into());
    }
    let safe: String = meta.name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    let dir = tf.join(hl_demos::scan::STV_DIR);
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(&safe);
    sources.download_demostf(&meta.url, &dest, progress).await?;
    crate::kills::ensure_one(db, sources, log_id).await?;
    let linked = match link_to_log_by(db, tf, &dest, log_id, me, "demos.tf", |_| {}).await {
        Ok(l) => l,
        Err(e) => {
            // Not this match's: the download is not kept.
            let _ = std::fs::remove_file(&dest);
            return Err(e);
        }
    };
    db.set_demo_demos_tf_id(linked.demo_id, demos_tf_id).await?;
    Ok(linked)
}

/// What reading a match again did.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReRead {
    /// Demos read again from their files.
    pub demos: usize,
    /// Linked demos whose file is gone (deleted to save space, or moved):
    /// what was read from them before is kept as it was.
    pub missing: usize,
    /// The server log's fights were derived again.
    pub fights: bool,
    /// Aim, deaths and routes were derived again.
    pub aim: bool,
}

/// Read one match again with this version of the app: every linked demo
/// from its file (its timeline: spychecks, ping, reflects, the cart and
/// positions all read from that), then aim, deaths and routes from the new
/// timelines, then the fights from the stored server log. For after an
/// update that reads more, or reads better, than the one that first did.
pub async fn reread_log(db: &Db, me: Option<SteamId>, log_id: i64, mut progress: impl FnMut(&'static str)) -> Result<ReRead> {
    let mut out = ReRead::default();
    for demo in db.demos_for_log(log_id).await? {
        let path = PathBuf::from(&demo.path);
        if demo.deleted || !path.is_file() {
            out.missing += 1;
            continue;
        }
        progress("Reading the demo");
        // The owner's own demo follows the owner; an STV follows nobody.
        let owner = if demo.kind == "stv" { String::new() } else { me.map(|m| m.to_steamid3()).unwrap_or_default() };
        let rate = demo.tick_rate.unwrap_or(66.67);
        let stored = tokio::task::spawn_blocking(move || -> Result<_> {
            let (_, stored) = hl_demos::aim::pass_recording(&path, &owner, rate, Some(hl_demos::timeline::DEFAULT_STRIDE), &mut |_| {})?;
            stored.context("the demo gave no timeline")
        })
        .await??;
        db.put_timeline(demo.demo_id, &timeline_row(&stored)).await?;
        out.demos += 1;
    }
    if let Some(me) = me.filter(|_| out.demos > 0) {
        progress("Reading aim from the demo");
        crate::aim::derive_log(db, me, log_id).await?;
        out.aim = true;
    }
    progress("Reading the server log");
    out.fights = crate::fights::derive_log(db, log_id).await?;
    Ok(out)
}

fn timeline_row(stored: &hl_demos::timeline::Stored) -> TimelineRow {
    TimelineRow {
        version: stored.version,
        tick_rate: stored.tick_rate,
        stride: i64::from(stored.stride),
        head: stored.head.clone(),
        samples: stored.samples.clone(),
        changes: stored.changes.clone(),
        objects: stored.objects.clone(),
        events: stored.events.clone(),
        raw_bytes: stored.raw_bytes as i64,
        stored_bytes: stored.stored_bytes() as i64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kills_line_up_at_one_shift() {
        // The demo started 1000 s into the log's clock; one kill a second
        // off, and one that is in neither.
        let demo = [(10.0, 1, 2), (40.0, 3, 4), (95.0, 2, 1), (300.0, 9, 9)];
        let log = [(1010, 1, 2), (1041, 3, 4), (1095, 2, 1), (1500, 5, 6)];
        assert_eq!(best_shift(&demo, &log), Some((1000, 3)));
        assert_eq!(best_shift(&[], &log), None);
    }

    #[test]
    fn a_demos_tf_link_gives_its_id() {
        assert_eq!(demostf_id("https://demos.tf/990239"), Some(990239));
        assert_eq!(demostf_id(" demos.tf/990239/ "), Some(990239));
        assert_eq!(demostf_id("990239"), Some(990239));
        assert_eq!(demostf_id("https://logs.tf/3426234"), None);
        assert_eq!(demostf_id("vigil"), None);
    }

    #[test]
    fn a_combined_log_names_each_of_its_maps() {
        assert_eq!(map_names("product + proot"), ["product", "proot"]);
        assert_eq!(map_names("koth_product_final"), ["koth_product_final"]);
        assert!(map_names("").is_empty());
        let demo = hl_demos::map_base("koth_product_final");
        assert!(map_names("product + proot").iter().any(|m| hl_demos::map_base(m) == demo));
    }

    #[test]
    fn the_same_file_is_the_same_log() {
        let a = log_id_for("match-20260823-1956-pl_upward_f12.dem");
        assert!(a < 0);
        assert_eq!(a, log_id_for("MATCH-20260823-1956-pl_upward_f12.dem"));
        assert_ne!(a, log_id_for("match-20260823-1957-pl_upward_f12.dem"));
    }

    #[test]
    fn a_name_that_collides_is_probed_past_and_the_same_demo_keeps_its_id() {
        let name = "match-20260823-1956-pl_upward_f12.dem";
        // Attempt 0 is the id every earlier import used; the rest differ.
        assert_eq!(log_id_attempt(name, 0), log_id_for(name));
        let ids: std::collections::HashSet<i64> = (0..8).map(|a| log_id_attempt(name, a)).collect();
        assert_eq!(ids.len(), 8);
        assert!(ids.iter().all(|i| *i < 0));
    }

    #[tokio::test]
    async fn two_demos_with_one_id_do_not_overwrite_each_other() {
        let db = Db::connect_in_memory().await.unwrap();
        let name = "match-20260823-1956-pl_upward_f12.dem";
        let (a, b) = (std::path::Path::new("C:/demos/a/x.dem"), std::path::Path::new("C:/demos/b/x.dem"));
        // Nothing stored: the first attempt.
        assert_eq!(free_log_id(&db, name, a).await.unwrap(), log_id_attempt(name, 0));
        // A match from demo A holds that id, with the title the import gives it.
        let json = serde_json::json!({"info": {"title": import_title(name)}}).to_string();
        db.store_raw_log(log_id_attempt(name, 0), &json).await.unwrap();
        db.upsert_demo(&hl_db::DemoRow {
            path: "C:/demos/a/x.dem", file_name: name, playdemo_arg: "x", kind: "stv", size_bytes: 1, mtime: 1, map: "pl_upward", server: "",
            recorder: "", playback_s: 1.0, ticks: 1, tick_rate: Some(66.0), start_utc: None, filename_time: None, demos_tf_id: None, events: Vec::new(),
        })
        .await
        .unwrap();
        let demo_a = db.demos_named(name).await.unwrap()[0].0;
        db.add_demo_link(demo_a, log_id_attempt(name, 0), METHOD, 1.0).await.unwrap();
        // The same file again finds its own match; another folder's file with the
        // same name gets the next id instead of replacing it.
        assert_eq!(free_log_id(&db, name, a).await.unwrap(), log_id_attempt(name, 0));
        assert_eq!(free_log_id(&db, name, b).await.unwrap(), log_id_attempt(name, 1));
    }

    #[test]
    fn the_start_comes_from_the_name_when_it_has_one() {
        // The name's time is the machine's local time, whatever zone that is.
        use chrono::TimeZone;
        let local = |y, mo, d, h, mi, s| chrono::Local.with_ymd_and_hms(y, mo, d, h, mi, s).earliest().unwrap().timestamp();
        assert_eq!(wall_start("match-20260823-1956-pl_upward_f12.dem", 0, 0), local(2026, 8, 23, 19, 56, 0));
        assert_eq!(wall_start("flashwav2026-09-17_21-00-24.dem", 0, 0), local(2026, 9, 17, 21, 0, 24));
        assert_eq!(wall_start("my_demo.dem", 10_000, 600), 9_400);
    }
}
