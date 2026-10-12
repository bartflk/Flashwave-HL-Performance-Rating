//! Clients for trends.tf, logs.tf, more.tf, demos.tf and ETF2L.
//!
//! Each returns both a typed view and the verbatim JSON, because the verbatim
//! JSON is what gets stored: parsing rules change, source rows don't.

use crate::http::{download_client, download_to, refused, url, Refused, Throttled};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long logs.tf is left alone after it refuses us. Its bans lift on
/// their own; asking during one is what keeps them going.
const LOGSTF_REST: Duration = Duration::from_secs(10 * 60);

/// One row of the trends.tf log index.
#[derive(Debug, Clone, Deserialize)]
pub struct TrendsRow {
    pub logid: i64,
    pub title: Option<String>,
    pub map: Option<String>,
    pub time: Option<i64>,
    pub duration: Option<i64>,
    pub format: Option<String>,
    pub league: Option<String>,
    pub matchid: Option<i64>,
    pub demoid: Option<i64>,
    pub duplicate_of: Option<Vec<i64>>,
    pub updated: Option<i64>,
}

/// One row of the logs.tf search list.
#[derive(Debug, Clone, Deserialize)]
pub struct LogsTfRow {
    pub id: i64,
    pub title: Option<String>,
    pub map: Option<String>,
    pub date: Option<i64>,
    pub players: Option<i64>,
}

/// Rows per page asked of ETF2L's paged lists: within what its docs call
/// reasonable, and a fifth of the requests of its default 20.
pub const ETF2L_PAGE: &str = "100";

pub struct Sources {
    trends: Throttled,
    /// drops.tf, by Icewind: logs.tf's logs, the JSON and the raw server
    /// logs, served about an hour after upload and without logs.tf's limits.
    /// Asked first for both (Flashy, October 2026).
    drops: Throttled,
    logstf: Throttled,
    /// When logs.tf last turned us away, if it did within [`LOGSTF_REST`].
    logstf_refused_at: Mutex<Option<Instant>>,
    moretf: Throttled,
    demostf: Throttled,
    etf2l: Throttled,
    /// Steam profiles and avatar images, for the owner's picture.
    web: Throttled,
    downloads: reqwest::Client,
}

/// A demo's metadata on demos.tf.
#[derive(Debug, Clone, Deserialize)]
pub struct DemosTfMeta {
    pub id: i64,
    /// Where the file itself lives.
    pub url: String,
    /// e.g. `match-20260823-1956-pl_upward_f12.dem`
    pub name: String,
    pub map: Option<String>,
    /// Seconds.
    pub duration: Option<i64>,
    /// Upload time, unix seconds UTC: moments after the recording ended.
    pub time: Option<i64>,
    /// Where demos.tf keeps the file: `freezer`, `fridge`, ... or `deleted`
    /// when the file is gone (Q62: every demo before 14 October 2019).
    #[serde(default)]
    pub backend: Option<String>,
}

impl DemosTfMeta {
    /// demos.tf still has the file: not marked deleted, and a link to it.
    pub fn available(&self) -> bool {
        self.backend.as_deref() != Some("deleted") && !self.url.is_empty()
    }
}

impl Sources {
    pub fn new() -> Result<Self> {
        Ok(Sources {
            trends: Throttled::new(Duration::from_millis(1000))?,
            // No published limit. Measured (7 Oct 2026): 0.14 s a log, 0.2 s
            // a raw log, and 4 at once no slower and never refused. Four a
            // second, one at a time, is still a light guest on one person's
            // server.
            drops: Throttled::new(Duration::from_millis(250))?,
            // logs.tf stopped answering twice after a few hundred requests at
            // one a second (~750 raw logs, then ~300 part logs from a second
            // address), so it gets a slower pace; bulk jobs are also capped
            // per sync (`BULK_PER_SYNC`).
            logstf: Throttled::new(Duration::from_millis(2000))?,
            logstf_refused_at: Mutex::new(None),
            // A single volunteer's server: as slow as logs.tf.
            moretf: Throttled::new(Duration::from_millis(2000))?,
            demostf: Throttled::new(Duration::from_millis(1000))?,
            // ETF2L does publish a limit: 60 requests a minute. Stay under it.
            etf2l: Throttled::new(Duration::from_millis(1500))?,
            web: Throttled::new(Duration::from_millis(1000))?,
            downloads: download_client()?,
        })
    }

    /// Page through the trends.tf index for one player. `updated_since` makes
    /// the sync incremental: only rows added or changed since then come back.
    pub async fn trends_index(
        &self,
        steamid64: &str,
        updated_since: Option<i64>,
        mut on_page: impl FnMut(usize),
    ) -> Result<Vec<(TrendsRow, String)>> {
        let mut query = vec![("steamid64", steamid64.to_string()), ("limit", "100".to_string())];
        if let Some(t) = updated_since {
            query.push(("updated_since", t.to_string()));
        }
        let mut next = url("https://trends.tf/api/v1/logs", &query);

        let mut rows = Vec::new();
        // Hard cap as a guard against a pagination loop, well above any real history.
        for _ in 0..500 {
            let body = self.trends.get_text(&next).await?;
            let page: Value = serde_json::from_str(&body).context("parsing trends.tf page")?;

            for raw in page.get("logs").and_then(Value::as_array).into_iter().flatten() {
                let row: TrendsRow =
                    serde_json::from_value(raw.clone()).context("parsing trends.tf row")?;
                rows.push((row, raw.to_string()));
            }
            on_page(rows.len());

            // trends.tf builds the next page's path itself.
            match page.get("next_page").and_then(Value::as_str) {
                Some(path) if !path.is_empty() => next = format!("https://trends.tf{path}"),
                _ => return Ok(rows),
            }
        }
        anyhow::bail!("trends.tf pagination did not terminate")
    }

    /// One page of trends.tf's ETF2L Highlander logs, newest first: rows and
    /// the path of the next page. For the league sample, which needs every
    /// official's logs, not only the owner's.
    pub async fn trends_league_page(&self, path: Option<&str>) -> Result<(Vec<TrendsRow>, Option<String>)> {
        let first = || url("https://trends.tf/api/v1/logs", [("league", "etf2l"), ("format", "highlander"), ("limit", "100")]);
        let body = self.trends.get_text(&path.map_or_else(first, |p| format!("https://trends.tf{p}"))).await?;
        let page: Value = serde_json::from_str(&body).context("parsing trends.tf league page")?;
        let rows = page
            .get("logs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|raw| serde_json::from_value::<TrendsRow>(raw.clone()).context("parsing trends.tf row"))
            .collect::<Result<Vec<_>>>()?;
        let next = page.get("next_page").and_then(Value::as_str).filter(|n| !n.is_empty()).map(str::to_string);
        Ok((rows, next))
    }

    /// Every log logs.tf knows for this player, in one request. Covers the logs
    /// trends.tf never indexed — mostly 2014-2019 on this account.
    pub async fn logstf_search(&self, steamid64: &str) -> Result<Vec<(LogsTfRow, String)>> {
        // 10000 is logs.tf's documented maximum: a whole history in one request.
        let search = url("https://logs.tf/api/v1/log", [("player", steamid64), ("limit", "10000")]);
        let body = self.logstf_gated(self.logstf.get_text(&search)).await?;
        let page: Value = serde_json::from_str(&body).context("parsing logs.tf search")?;
        page.get("logs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|raw| {
                let row: LogsTfRow =
                    serde_json::from_value(raw.clone()).context("parsing logs.tf search row")?;
                Ok((row, raw.to_string()))
            })
            .collect()
    }

    /// The raw server log behind a logs.tf page, as the zip logs.tf serves;
    /// `None` when logs.tf has none.
    pub async fn logstf_rawlog(&self, log_id: i64) -> Result<Option<Vec<u8>>> {
        self.logstf_gated(self.logstf.get_bytes_opt(&format!("https://logs.tf/logs/log_{log_id}.log.zip"))).await
    }

    /// Full JSON for one log, verbatim.
    pub async fn logstf_log(&self, log_id: i64) -> Result<String> {
        let body = self.logstf_gated(self.logstf.get_text(&format!("https://logs.tf/api/v1/log/{log_id}"))).await?;
        // Validate before storing: a truncated or HTML error body must not
        // become a "raw source" that reprocess later chokes on.
        let v: Value = serde_json::from_str(&body)
            .with_context(|| format!("log {log_id}: response is not JSON"))?;
        if v.get("players").is_none() {
            anyhow::bail!("log {log_id}: response has no players");
        }
        Ok(body)
    }

    /// One log's JSON, from drops.tf first and logs.tf when drops.tf does
    /// not have it -- a log less than about an hour old, or one it lost.
    /// Says which answered: `"drops.tf"` or `"logs.tf"`. drops.tf serves
    /// logs.tf's JSON as it is (checked field by field, October 2026), so
    /// both are stored alike.
    pub async fn log_json(&self, log_id: i64) -> Result<(String, &'static str)> {
        match self.drops_log(log_id).await {
            Ok(Some(body)) => return Ok((body, DROPS)),
            Ok(None) => {}
            Err(e) => tracing::info!(log_id, error = %format!("{e:#}"), "drops.tf would not give the log; asking logs.tf"),
        }
        Ok((self.logstf_log(log_id).await?, "logs.tf"))
    }

    /// One log's JSON from drops.tf; `None` when it does not have it.
    pub async fn drops_log(&self, log_id: i64) -> Result<Option<String>> {
        let Some(body) = self.drops.get_text_opt(&format!("https://drops.tf/api/log/{log_id}")).await? else { return Ok(None) };
        // Like logs.tf's: a cut-off or error body is not a log.
        let v: Value = serde_json::from_str(&body).with_context(|| format!("drops.tf log {log_id}: response is not JSON"))?;
        Ok(v.get("players").is_some().then_some(body))
    }

    /// The raw server log behind a log, zipped the way logs.tf serves it:
    /// drops.tf's plain copy first (byte for byte logs.tf's, October 2026),
    /// then logs.tf's zip. `None` when neither has one.
    pub async fn rawlog_zip(&self, log_id: i64) -> Result<Option<Vec<u8>>> {
        match self.drops_rawlog(log_id).await {
            Ok(Some(zip)) => return Ok(Some(zip)),
            Ok(None) => {}
            Err(e) => tracing::info!(log_id, error = %format!("{e:#}"), "drops.tf would not give the raw log; asking logs.tf"),
        }
        self.logstf_rawlog(log_id).await
    }

    /// The raw server log from drops.tf only, zipped like logs.tf's; `None`
    /// when it does not have it.
    pub async fn drops_rawlog(&self, log_id: i64) -> Result<Option<Vec<u8>>> {
        match self.drops.get_bytes_opt(&drops_rawlog_url(log_id)).await? {
            Some(text) if !text.is_empty() => Ok(Some(zip_log(log_id, &text)?)),
            _ => Ok(None),
        }
    }

    /// The newest log this player is in: one small request, for "is the game
    /// I just played up yet" every few seconds after a match (ivg, Flashy).
    /// logs.tf first, as the upload lands there; trends.tf, which lists it a
    /// minute or so later, while logs.tf is refusing us.
    pub async fn newest_log(&self, steamid64: &str) -> Result<Option<(i64, &'static str)>> {
        let first = |body: &str, key: &str| -> Result<Option<i64>> {
            let v: Value = serde_json::from_str(body).context("parsing the newest log")?;
            Ok(v.get("logs").and_then(|l| l.get(0)).and_then(|l| l.get(key)).and_then(Value::as_i64))
        };
        if !self.logstf_resting() {
            let newest = url("https://logs.tf/api/v1/log", [("player", steamid64), ("limit", "1")]);
            match self.logstf_gated(self.logstf.get_text_interactive(&newest)).await {
                Ok(body) => return Ok(first(&body, "id")?.map(|id| (id, "logs.tf"))),
                Err(e) => tracing::info!(error = %format!("{e:#}"), "logs.tf would not say; asking trends.tf"),
            }
        }
        let newest = url("https://trends.tf/api/v1/logs", [("steamid64", steamid64), ("limit", "1")]);
        let body = self.trends.get_text_interactive(&newest).await?;
        Ok(first(&body, "logid")?.map(|id| (id, "trends.tf")))
    }

    /// Logs every one of these players is in, newest first, as logs.tf lists
    /// them: `(id, title, map, date, players)`.
    pub async fn logstf_with_players(&self, steamid64s: &[String], limit: u32) -> Result<Vec<(i64, String, String, i64, i64)>> {
        let players = steamid64s.join(",");
        let limit = limit.to_string();
        let search = url("https://logs.tf/api/v1/log", [("player", players.as_str()), ("limit", limit.as_str())]);
        let body = self.logstf_gated(self.logstf.get_text(&search)).await?;
        let v: Value = serde_json::from_str(&body).context("parsing logs.tf's search")?;
        Ok(v["logs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| {
                Some((
                    l["id"].as_i64()?,
                    l["title"].as_str().unwrap_or("").to_string(),
                    l["map"].as_str().unwrap_or("").to_string(),
                    l["date"].as_i64()?,
                    l["players"].as_i64().unwrap_or(0),
                ))
            })
            .collect())
    }

    /// The player's most recent logs, newest first: `(log id, Highlander)`.
    /// logs.tf counts a log's players (16 to 22 is a Highlander match, with
    /// its stand-ins and swaps); trends.tf, asked while logs.tf refuses us,
    /// names the format.
    pub async fn recent_logs(&self, steamid64: &str, limit: u32) -> Result<(Vec<(i64, bool)>, &'static str)> {
        if !self.logstf_resting() {
            let limit = limit.to_string();
            let recent = url("https://logs.tf/api/v1/log", [("player", steamid64), ("limit", limit.as_str())]);
            match self.logstf_gated(self.logstf.get_text_interactive(&recent)).await {
                Ok(body) => {
                    let v: Value = serde_json::from_str(&body).context("parsing logs.tf's recent logs")?;
                    let rows = v["logs"].as_array().into_iter().flatten();
                    return Ok((rows.filter_map(|l| Some((l["id"].as_i64()?, l["players"].as_i64().is_some_and(|n| (16..=22).contains(&n))))).collect(), "logs.tf"));
                }
                Err(e) => tracing::info!(error = %format!("{e:#}"), "logs.tf would not list recent logs; asking trends.tf"),
            }
        }
        let limit = limit.to_string();
        let recent = url("https://trends.tf/api/v1/logs", [("steamid64", steamid64), ("limit", limit.as_str())]);
        let body = self.trends.get_text_interactive(&recent).await?;
        let v: Value = serde_json::from_str(&body).context("parsing trends.tf's recent logs")?;
        let rows = v["logs"].as_array().into_iter().flatten();
        Ok((rows.filter_map(|l| Some((l["logid"].as_i64()?, l["format"].as_str() == Some("highlander")))).collect(), "trends.tf"))
    }

    /// Seconds left of logs.tf's rest, if it is resting.
    pub fn logstf_rest_left(&self) -> Option<u64> {
        let at = (*self.logstf_refused_at.lock().unwrap())?;
        LOGSTF_REST.checked_sub(at.elapsed()).map(|d| d.as_secs())
    }

    /// Whether logs.tf turned us away within the last [`LOGSTF_REST`]: the
    /// sync then goes to more.tf for new logs without asking logs.tf first.
    pub fn logstf_resting(&self) -> bool {
        self.logstf_refused_at.lock().unwrap().is_some_and(|at| at.elapsed() < LOGSTF_REST)
    }

    /// A logs.tf request, unless logs.tf refused us recently; a refusal
    /// starts the rest. One place, so every pass that talks to logs.tf --
    /// the sync, raw logs, part logs, round maps -- stops together.
    async fn logstf_gated<T>(&self, request: impl std::future::Future<Output = Result<T>>) -> Result<T> {
        if self.logstf_resting() {
            return Err(anyhow::Error::new(Refused { status: reqwest::StatusCode::FORBIDDEN })
                .context("logs.tf refused us a few minutes ago; not asking again yet"));
        }
        let result = request.await;
        if let Err(e) = &result {
            if refused(e) {
                tracing::warn!("logs.tf is refusing requests; leaving it alone for {} minutes", LOGSTF_REST.as_secs() / 60);
                *self.logstf_refused_at.lock().unwrap() = Some(Instant::now());
            }
        }
        result
    }

    /// more.tf's own parse of a logs.tf log: `None` when more.tf does not
    /// have it. The stand-in for logs.tf while logs.tf refuses us
    /// (see `moretf.rs`).
    pub async fn moretf_log(&self, log_id: i64) -> Result<Option<String>> {
        let Some(body) = self.moretf.get_text_opt(&format!("https://more.tf/api/log/{log_id}")).await? else {
            return Ok(None);
        };
        // A cut-off body is not a log (seen once, 177 kB of a 359 kB answer).
        let v: Value = serde_json::from_str(&body).with_context(|| format!("more.tf log {log_id}: response is not JSON"))?;
        if v.get("success").and_then(Value::as_bool) != Some(true) || v.get("players").is_none() {
            return Ok(None);
        }
        Ok(Some(body))
    }
}

impl Sources {
    /// GET a path on the ETF2L v2 API; `None` when it does not exist.
    /// A page as text; `None` when it does not exist.
    /// A player's trends.tf page, ETF2L Highlander officials only (Q37):
    /// HTML, as it has no JSON API. `None` when trends.tf has no such player.
    pub async fn trends_player_page(&self, steamid64: &str) -> Result<Option<String>> {
        self.trends.get_text_opt_interactive(&crate::trends_career::page_url(steamid64)).await
    }

    pub async fn fetch_text(&self, url: &str) -> Result<Option<String>> {
        self.web.get_text_opt(url).await
    }

    /// As [`fetch_text`](Self::fetch_text), for a screen waiting on it: no
    /// retries when the connection cannot be made.
    pub async fn fetch_text_interactive(&self, url: &str) -> Result<Option<String>> {
        self.web.get_text_opt_interactive(url).await
    }

    /// A small file (an avatar image); `None` when it does not exist.
    pub async fn fetch_bytes(&self, url: &str) -> Result<Option<Vec<u8>>> {
        self.web.get_bytes_opt(url).await
    }

    pub async fn etf2l_get(&self, path: &str) -> Result<Option<String>> {
        let [first, second] = etf2l_hosts(unix_now());
        match self.etf2l.get_text_opt(&format!("{first}{path}")).await {
            Ok(Some(body)) => Ok(Some(body)),
            first_try => etf2l_fallback(first_try, self.etf2l.get_text_opt(&format!("{second}{path}")).await),
        }
    }

    /// One page of a paged ETF2L list, [`ETF2L_PAGE`] rows at a time rather
    /// than its default 20.
    pub async fn etf2l_page(&self, path: &str, page: i64) -> Result<Option<String>> {
        let page = page.to_string();
        let query = [("page", page.as_str()), ("limit", ETF2L_PAGE)];
        let [first, second] = etf2l_hosts(unix_now());
        match self.etf2l.get_text_opt(&url(&format!("{first}{path}"), query)).await {
            Ok(Some(body)) => Ok(Some(body)),
            first_try => etf2l_fallback(first_try, self.etf2l.get_text_opt(&url(&format!("{second}{path}"), query)).await),
        }
    }

    /// As [`etf2l_get`](Self::etf2l_get), for a screen waiting on it: no
    /// retries when the connection cannot be made.
    pub async fn etf2l_get_interactive(&self, path: &str) -> Result<Option<String>> {
        let [first, second] = etf2l_hosts(unix_now());
        match self.etf2l.get_text_opt_interactive(&format!("{first}{path}")).await {
            Ok(Some(body)) => Ok(Some(body)),
            first_try => etf2l_fallback(first_try, self.etf2l.get_text_opt_interactive(&format!("{second}{path}")).await),
        }
    }

    /// Every SourceTV demo demos.tf holds for one player, newest first.
    ///
    /// trends.tf links a demo to 70% of this account's logs and nothing to the
    /// other 30%, which left a third of matches with no way to reach their
    /// demo. demos.tf knows about them; it just has to be asked, and a demo
    /// carries its map and the second it started, which is enough to match it
    /// to a log.
    ///
    /// `before` pages backwards: pass the oldest `time` seen so far.
    pub async fn demostf_for_player(
        &self,
        steamid64: &str,
        before: Option<i64>,
    ) -> Result<Vec<DemosTfMeta>> {
        // `players[]` is PHP's array syntax, which demos.tf's API reads. It
        // has no page-size parameter; paging is by `before`.
        let mut query = vec![("players[]", steamid64.to_string())];
        if let Some(t) = before {
            query.push(("before", t.to_string()));
        }
        let body = self.demostf.get_text(&url("https://api.demos.tf/demos", &query)).await?;
        serde_json::from_str(&body).context("parsing the demos.tf demo list")
    }

    /// Who played what in a demos.tf demo, from its page: `(steamid64, class,
    /// team)` with demos.tf's own spellings (`heavyweapons`, `red`). The
    /// list endpoint has no players, so finding a player's demos on one class
    /// asks this once per demo (Q69).
    pub async fn demostf_players(&self, demo_id: i64) -> Result<Vec<(String, String, String)>> {
        let body = self.demostf.get_text(&format!("https://api.demos.tf/demos/{demo_id}")).await?;
        let v: Value = serde_json::from_str(&body).with_context(|| format!("parsing demos.tf demo {demo_id}"))?;
        Ok(v["players"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let id = p["steamid"].as_str().map(str::to_string).or_else(|| p["steamid"].as_i64().map(|n| n.to_string()))?;
                Some((id, p["class"].as_str().unwrap_or("").to_string(), p["team"].as_str().unwrap_or("").to_string()))
            })
            .collect())
    }

    pub async fn demostf_meta(&self, demo_id: i64) -> Result<DemosTfMeta> {
        let body = self.demostf.get_text(&format!("https://api.demos.tf/demos/{demo_id}")).await?;
        serde_json::from_str(&body).with_context(|| format!("parsing demos.tf metadata for {demo_id}"))
    }

    pub async fn download(
        &self,
        url: &str,
        dest: &std::path::Path,
        progress: impl FnMut(u64, Option<u64>),
    ) -> Result<u64> {
        download_to(&self.downloads, url, dest, progress).await
    }
}

/// drops.tf's name, as a log's source is stored and shown.
pub const DROPS: &str = "drops.tf";

/// Where drops.tf keeps a raw server log: by million, then by thousand,
/// each seven digits -- `4000000/4118000/log_4118933.log`.
pub fn drops_rawlog_url(log_id: i64) -> String {
    format!("https://drops.tf/logs/logs/{:07}/{:07}/log_{log_id}.log", log_id / 1_000_000 * 1_000_000, log_id / 1000 * 1000)
}

/// A raw log zipped the way logs.tf serves it, so it is stored and read
/// like logs.tf's.
fn zip_log(log_id: i64, text: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        z.start_file(format!("log_{log_id}.log"), zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated))?;
        z.write_all(text)?;
        z.finish()?;
    }
    Ok(buf.into_inner())
}

/// When ETF2L's API moves (its announcement, October 2026): v2 is served at
/// `api.etf2l.org` from 1 November 2026, after the 10:00-14:00 CET switch,
/// and `api-v2.etf2l.org` answers until the end of the year.
const ETF2L_MOVE: i64 = 1_793_538_000; // 2026-11-01 13:00 UTC
const ETF2L_NEW: &str = "https://api.etf2l.org";
const ETF2L_OLD: &str = "https://api-v2.etf2l.org";

/// The ETF2L API's addresses in the order to ask them: the old one until
/// the move, the new one after, each the other's fallback.
pub fn etf2l_hosts(now: i64) -> [&'static str; 2] {
    if now >= ETF2L_MOVE {
        [ETF2L_NEW, ETF2L_OLD]
    } else {
        [ETF2L_OLD, ETF2L_NEW]
    }
}

/// The first address's answer unless it failed or had nothing, then the
/// second's; when both fail, the first's error.
fn etf2l_fallback(first: Result<Option<String>>, second: Result<Option<String>>) -> Result<Option<String>> {
    match (first, second) {
        (_, Ok(Some(body))) => Ok(Some(body)),
        (Ok(None), _) => Ok(None),
        (Err(e), Ok(None)) => Err(e),
        (Err(e), Err(_)) => Err(e),
        (Ok(Some(body)), _) => Ok(Some(body)),
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raw_log_is_found_by_million_then_thousand() {
        assert_eq!(drops_rawlog_url(4118933), "https://drops.tf/logs/logs/4000000/4118000/log_4118933.log");
        assert_eq!(drops_rawlog_url(3000000), "https://drops.tf/logs/logs/3000000/3000000/log_3000000.log");
        assert_eq!(drops_rawlog_url(700123), "https://drops.tf/logs/logs/0000000/0700000/log_700123.log");
    }

    #[test]
    fn a_raw_log_from_drops_tf_is_zipped_like_logs_tf() {
        let text = b"L 09/11/2026 - 19:05:35: \"a<3><[U:1:1]><Blue>\" changed role to \"soldier\"\n";
        let zip = zip_log(4118933, text).unwrap();
        assert_eq!(crate::rawlog::unzip(&zip).unwrap().as_bytes(), text);
    }

    #[test]
    fn etf2l_moves_to_its_new_address_on_1_november() {
        assert_eq!(etf2l_hosts(ETF2L_MOVE - 1), [ETF2L_OLD, ETF2L_NEW]);
        assert_eq!(etf2l_hosts(ETF2L_MOVE), [ETF2L_NEW, ETF2L_OLD]);
        // 1 November 2026, 13:00 UTC is 14:00 CET: the end of ETF2L's downtime.
        assert_eq!(chrono::DateTime::from_timestamp(ETF2L_MOVE, 0).unwrap().to_rfc3339(), "2026-11-01T13:00:00+00:00");
    }

    #[test]
    fn the_other_etf2l_address_answers_when_the_first_cannot() {
        let body = |s: &str| Ok(Some(s.to_string()));
        assert_eq!(etf2l_fallback(Err(anyhow::anyhow!("down")), body("b")).unwrap().as_deref(), Some("b"));
        assert_eq!(etf2l_fallback(Ok(None), body("b")).unwrap().as_deref(), Some("b"), "missing on one, there on the other");
        assert_eq!(etf2l_fallback(Ok(None), Ok(None)).unwrap(), None, "missing on both: missing");
        assert!(etf2l_fallback(Err(anyhow::anyhow!("down")), Err(anyhow::anyhow!("down too"))).is_err());
    }

    /// The real response for the S36 official against TWS.
    #[test]
    fn parses_demostf_metadata() {
        let json = r#"{"id":1497032,"url":"https://freezer.demos.tf/ff/e7/ffe7_match-20260823-1956-pl_upward_f12.dem",
            "name":"match-20260823-1956-pl_upward_f12.dem","server":"serveme.tf #1559951","duration":1157,
            "nick":"SourceTV Demo","map":"pl_upward_f12","time":1787516129,"red":"GOYDA","blue":"RED",
            "redScore":0,"blueScore":2,"playerCount":18,"players":[]}"#;
        let m: DemosTfMeta = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, 1497032);
        assert_eq!(m.duration, Some(1157));
        assert_eq!(m.time, Some(1787516129));
        assert!(m.url.ends_with(".dem"));
    }
}
