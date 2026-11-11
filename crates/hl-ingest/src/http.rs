//! A polite HTTP client: one request per interval, with retry on transient
//! failure, and conditional requests where the server supports them.
//!
//! Neither trends.tf nor logs.tf publishes a rate limit (ETF2L does: 60 a
//! minute). Behaving as if they do is the price of using free community
//! services, and the fastest way to lose access is to not.

use anyhow::{bail, Context, Result};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::Instant;

/// Who is asking, and where to find out more: a server admin seeing these
/// requests can tell what they are and who to ask about them, rather than
/// blocking an address.
const USER_AGENT: &str = concat!(
    "Flashwave.tf/",
    env!("CARGO_PKG_VERSION"),
    " (TF2 Highlander rating app; +https://github.com/bartflk/Flashwave-HL-Performance-Rating)"
);

/// A URL with its query string built and encoded by the URL parser, never by
/// hand: `url("https://logs.tf/api/v1/log", [("player", id), ("limit", "1")])`.
/// `base` is a literal in our code, so a parse failure is a bug.
pub fn url<I, K, V>(base: &str, query: I) -> String
where
    I: IntoIterator,
    I::Item: Borrow<(K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    reqwest::Url::parse_with_params(base, query).expect("a valid base URL").into()
}

/// What a conditional request needs: the validators a server sent with a
/// body, and the body, so a 304 can be answered from it.
struct Validated {
    etag: Option<String>,
    last_modified: Option<String>,
    body: Vec<u8>,
    used: u64,
}

/// Bodies larger than this are not kept for revalidation (a logs.tf search is
/// megabytes); the polled and paged answers this is for are a few kB.
const VALIDATED_MAX_BODY: usize = 1 << 20;
/// And no more than this in all, per client.
const VALIDATED_MAX_TOTAL: usize = 8 << 20;

/// Backoff between attempts. Three tries total.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(3), Duration::from_secs(10)];

/// How long a host is left alone after it answers 429 or 503, when it did
/// not say for how long (or said less): its bans lift on their own, and
/// asking during one is what keeps them going.
const REST_MIN: Duration = Duration::from_secs(10 * 60);
/// And the longest we believe a `Retry-After` for: a day.
const REST_MAX: Duration = Duration::from_secs(24 * 3600);
/// A `Retry-After` up to this is waited out within the retries; a longer one
/// is a rest, not a pause.
const RETRY_WAIT_MAX: Duration = Duration::from_secs(90);

/// The most a plain answer may be (JSON, HTML, a zipped raw log).
pub const BODY_MAX: usize = 50 << 20;

/// Requests at once to one host: the gap sets the pace, this keeps a slow
/// answer from piling up behind it (a sync and a screen together).
const IN_FLIGHT: usize = 2;

pub struct Throttled {
    client: reqwest::Client,
    gap: Duration,
    next_slot: Mutex<Instant>,
    in_flight: Semaphore,
    /// Until when this host is left alone after a 429 or 503.
    rest_until: std::sync::Mutex<Option<Instant>>,
    /// By URL, for this run only.
    validated: std::sync::Mutex<(HashMap<String, Validated>, u64)>,
}

impl Throttled {
    pub fn new(gap: Duration) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(45))
            .gzip(true)
            .build()
            .context("building HTTP client")?;
        Ok(Throttled {
            client,
            gap,
            next_slot: Mutex::new(Instant::now()),
            in_flight: Semaphore::new(IN_FLIGHT),
            rest_until: Default::default(),
            validated: Default::default(),
        })
    }

    /// Wait for our slot. Holding the lock while sleeping serializes callers,
    /// which is the point.
    async fn wait_turn(&self) {
        let mut next = self.next_slot.lock().await;
        let now = Instant::now();
        if *next > now {
            tokio::time::sleep_until(*next).await;
        }
        *next = Instant::now() + self.gap;
    }

    /// GET a URL and return the body as text, retrying transient failures.
    pub async fn get_text(&self, url: &str) -> Result<String> {
        self.get_text_opt(url).await?.with_context(|| format!("{url}: HTTP 404 Not Found"))
    }

    /// As [`get_text`](Self::get_text), but a 404 is `None` rather than an error.
    pub async fn get_text_opt(&self, url: &str) -> Result<Option<String>> {
        text(url, self.fetch(url, false).await?)
    }

    /// As [`get_text`](Self::get_text), for a screen waiting on the answer:
    /// a connection that cannot be made at all is not retried.
    pub async fn get_text_interactive(&self, url: &str) -> Result<String> {
        self.get_text_opt_interactive(url).await?.with_context(|| format!("{url}: HTTP 404 Not Found"))
    }

    /// As [`get_text_opt`](Self::get_text_opt), for a screen waiting on the
    /// answer: a connection that cannot be made at all is not retried.
    pub async fn get_text_opt_interactive(&self, url: &str) -> Result<Option<String>> {
        text(url, self.fetch(url, true).await?)
    }

    /// GET a URL as raw bytes; a 404 is `None`. Same throttle and retries.
    pub async fn get_bytes_opt(&self, url: &str) -> Result<Option<Vec<u8>>> {
        self.fetch(url, false).await
    }

    /// As [`get_bytes_opt`](Self::get_bytes_opt), refusing more than `max`
    /// bytes: for a picture from a page's own HTML, which is not ours to trust.
    pub async fn get_bytes_capped(&self, url: &str, max: usize) -> Result<Option<Vec<u8>>> {
        self.send(url, false, true, max, || self.conditional(url)).await
    }

    /// How long this host still rests after a refusal, if it does.
    pub fn rest_left(&self) -> Option<Duration> {
        let until = (*self.rest_until.lock().unwrap())?;
        until.checked_duration_since(Instant::now())
    }

    /// Leave this host alone for `how_long` (never shortening a rest already
    /// running), and say so.
    fn rest_for(&self, url: &str, how_long: Duration) {
        let until = Instant::now() + how_long;
        let mut held = self.rest_until.lock().unwrap();
        if held.is_none_or(|h| h < until) {
            *held = Some(until);
        }
        tracing::warn!(url, minutes = how_long.as_secs() / 60, "the server is refusing requests; leaving it alone");
    }

    /// The request itself. `interactive` gives up at once on a connect error
    /// (offline, DNS failing, the host down): a sync can wait out a blip, but
    /// a screen would sit on a spinner for the 13 s of backoff only to show
    /// its offline state anyway. Other failures retry either way.
    async fn fetch(&self, url: &str, interactive: bool) -> Result<Option<Vec<u8>>> {
        self.send(url, interactive, true, BODY_MAX, || self.conditional(url)).await
    }

    /// POST a JSON body and return the answer as text; a 404 is `None`. Same
    /// throttle and retries, for a screen waiting on it. RGL's search and
    /// its paged match list are POSTs.
    pub async fn post_json_opt_interactive(&self, url: &str, body: &serde_json::Value) -> Result<Option<String>> {
        let body = serde_json::to_vec(body).context("writing a request body")?;
        let post = || self.client.post(url).header(reqwest::header::CONTENT_TYPE, "application/json").body(body.clone());
        text(url, self.send(url, true, false, BODY_MAX, post).await?)
    }

    /// A server's own count of what is left (`RateLimit-Remaining`, the IETF
    /// draft RGL sends, or the older `X-RateLimit-*`): when it says none,
    /// the next request waits for its window to reset, whatever our pace.
    async fn heed_limit(&self, headers: &reqwest::header::HeaderMap) {
        let num = |names: [&str; 2]| names.iter().find_map(|n| headers.get(*n)?.to_str().ok()?.trim().parse::<u64>().ok());
        if num(["ratelimit-remaining", "x-ratelimit-remaining"]) != Some(0) {
            return;
        }
        // Seconds to the reset; a value past a day is an epoch time, not a delay.
        let reset = num(["ratelimit-reset", "x-ratelimit-reset"]).unwrap_or(1);
        let reset = if reset > 86_400 {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            reset.saturating_sub(now)
        } else {
            reset
        };
        let mut next = self.next_slot.lock().await;
        *next = (*next).max(Instant::now() + Duration::from_secs(reset.clamp(1, 90)));
    }

    async fn send(&self, url: &str, interactive: bool, validate: bool, max_body: usize, request: impl Fn() -> reqwest::RequestBuilder) -> Result<Option<Vec<u8>>> {
        let mut last_err = None;
        let mut wait = Duration::ZERO;
        for attempt in 0..=RETRY_DELAYS.len() {
            if attempt > 0 {
                tokio::time::sleep(wait.max(RETRY_DELAYS[attempt - 1])).await;
            }
            // A host that said "not now" is not asked again until it is over,
            // by anything: a sync, a screen, a second pass.
            if let Some(left) = self.rest_left() {
                return Err(anyhow::Error::new(Refused { status: reqwest::StatusCode::TOO_MANY_REQUESTS })
                    .context(format!("{url}: the server asked us to wait; {} more minutes", left.as_secs() / 60 + 1)));
            }
            let _slot = self.in_flight.acquire().await.expect("the semaphore is never closed");
            self.wait_turn().await;

            match request().send().await {
                Ok(resp) => {
                    self.heed_limit(resp.headers()).await;
                    let status = resp.status();
                    if status == reqwest::StatusCode::NOT_MODIFIED {
                        if let Some(body) = self.revalidated(url) {
                            return Ok(Some(body));
                        }
                        bail!("{url}: HTTP 304 for a body no longer held");
                    }
                    if status.is_success() {
                        let header = |name| resp.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
                        let etag = header(reqwest::header::ETAG).map(|e| closed_etag(&e));
                        let last_modified = header(reqwest::header::LAST_MODIFIED);
                        let body = read_capped(resp, max_body).await.with_context(|| format!("reading body of {url}"))?;
                        if validate {
                            self.keep(url, etag, last_modified, &body);
                        }
                        return Ok(Some(body));
                    }
                    if status.as_u16() == 404 {
                        return Ok(None);
                    }
                    // Turned away: asking again, now or in ten seconds, only
                    // keeps the ban going.
                    if status.as_u16() == 403 {
                        return Err(anyhow::Error::new(Refused { status }).context(url.to_string()));
                    }
                    // 400s other than rate limiting will not improve on retry.
                    if status.is_client_error() && status.as_u16() != 429 {
                        bail!("{url}: HTTP {status}");
                    }
                    // A rate limit or an overloaded server says how long to
                    // back off: seconds or a date. Up to 90 s is waited out
                    // here; longer, or once the retries are spent, the host
                    // rests for as long as it asked, and at least ten minutes.
                    let asked = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| retry_after(v, chrono::Utc::now()));
                    let pushed_back = status.as_u16() == 429 || status.as_u16() == 503;
                    if pushed_back && (attempt == RETRY_DELAYS.len() || asked.is_some_and(|a| a > RETRY_WAIT_MAX)) {
                        self.rest_for(url, asked.unwrap_or_default().clamp(REST_MIN, REST_MAX));
                        return Err(anyhow::Error::new(Refused { status }).context(url.to_string()));
                    }
                    wait = asked.map_or(Duration::ZERO, |a| a.min(RETRY_WAIT_MAX));
                    tracing::warn!(url, %status, attempt, "transient HTTP failure");
                    last_err = Some(if status.as_u16() == 429 {
                        anyhow::Error::new(Refused { status }).context(url.to_string())
                    } else {
                        anyhow::anyhow!("{url}: HTTP {status}")
                    });
                }
                Err(e) => {
                    tracing::warn!(url, error = %e, attempt, "request failed");
                    let connect = e.is_connect();
                    last_err = Some(anyhow::Error::new(e).context(format!("requesting {url}")));
                    if interactive && connect {
                        break;
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("{url}: failed")))
    }
}

/// How long a `Retry-After` asks for: a number of seconds, or an HTTP date
/// (`Wed, 21 Oct 2026 07:28:00 GMT`), which is the time from `now`.
fn retry_after(value: &str, now: chrono::DateTime<chrono::Utc>) -> Option<Duration> {
    let value = value.trim();
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let at = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    Some(at.signed_duration_since(now).to_std().unwrap_or_default())
}

/// A response's body, stopping with an error at `max` bytes: whatever a
/// server says it is sending (or does not say), a hostile or broken one
/// cannot fill the memory.
async fn read_capped(mut resp: reqwest::Response, max: usize) -> Result<Vec<u8>> {
    if resp.content_length().is_some_and(|n| n > max as u64) {
        bail!("the answer is larger than the {} MB this app will read", max >> 20);
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if body.len() + chunk.len() > max {
            bail!("the answer is larger than the {} MB this app will read", max >> 20);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

impl Throttled {
    /// A GET, carrying the validators of the copy held, if one is: the server
    /// then answers 304 with no body when nothing changed. trends.tf does,
    /// which makes asking it again (the "is my game up" poll, a second sync)
    /// almost free for it.
    fn conditional(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.get(url);
        let held = self.validated.lock().unwrap();
        if let Some(v) = held.0.get(url) {
            if let Some(etag) = &v.etag {
                req = req.header(reqwest::header::IF_NONE_MATCH, etag);
            }
            if let Some(at) = &v.last_modified {
                req = req.header(reqwest::header::IF_MODIFIED_SINCE, at);
            }
        }
        req
    }

    /// The copy held for `url`, after a 304 said it is current.
    fn revalidated(&self, url: &str) -> Option<Vec<u8>> {
        let mut held = self.validated.lock().unwrap();
        held.1 += 1;
        let tick = held.1;
        let v = held.0.get_mut(url)?;
        v.used = tick;
        Some(v.body.clone())
    }

    /// Hold a body for revalidation, when the server gave a validator and it
    /// is small; the least recently used go first past the total.
    fn keep(&self, url: &str, etag: Option<String>, last_modified: Option<String>, body: &[u8]) {
        let mut held = self.validated.lock().unwrap();
        let (map, tick) = &mut *held;
        if (etag.is_none() && last_modified.is_none()) || body.len() > VALIDATED_MAX_BODY {
            map.remove(url);
            return;
        }
        *tick += 1;
        map.insert(url.to_string(), Validated { etag, last_modified, body: body.to_vec(), used: *tick });
        while map.values().map(|v| v.body.len()).sum::<usize>() > VALIDATED_MAX_TOTAL {
            let Some(oldest) = map.iter().min_by_key(|(_, v)| v.used).map(|(k, _)| k.clone()) else { break };
            map.remove(&oldest);
        }
    }
}

/// trends.tf sends its ETags without the closing quote (`W/"831f…`), and
/// then does not recognise them sent back; closed, it answers 304.
fn closed_etag(etag: &str) -> String {
    let opened = etag.starts_with('"') || etag.starts_with("W/\"");
    if opened && (etag.len() < 2 || !etag.ends_with('"') || etag == "W/\"") {
        format!("{etag}\"")
    } else {
        etag.to_string()
    }
}

fn text(url: &str, body: Option<Vec<u8>>) -> Result<Option<String>> {
    body.map(|b| String::from_utf8(b).with_context(|| format!("{url}: body is not UTF-8"))).transpose()
}

/// Whether `url` is on one of `hosts` or a subdomain of one, whatever the scheme.
pub fn is_on(url: &str, hosts: &[&str]) -> bool {
    let Some(host) = reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase)) else { return false };
    hosts.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// Whether `url` is https on `host` or a subdomain of it: the only places a
/// file named by a remote answer is fetched from. A link in JSON or HTML is
/// whatever its author wrote, a LAN address or a plain-http server included.
pub fn is_https_on(url: &str, hosts: &[&str]) -> bool {
    url.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://")) && is_on(url, hosts)
}

/// A client for large files: no overall timeout (a big demo can legitimately
/// take minutes), but a read timeout so a stalled transfer still fails.
pub fn download_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()
        .context("building download client")
}

/// Stream `url` to `dest`, reporting `(bytes so far, total if known)`.
///
/// Written to `dest.part` and renamed only once complete, so an interrupted
/// download never leaves a truncated file that looks like a finished demo.
pub async fn download_to(
    client: &reqwest::Client,
    url: &str,
    dest: &std::path::Path,
    max: u64,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<u64> {
    use tokio::io::AsyncWriteExt;

    let mut resp = client.get(url).send().await.with_context(|| format!("requesting {url}"))?;
    if !resp.status().is_success() {
        bail!("{url}: HTTP {}", resp.status());
    }
    let total = resp.content_length();
    if total.is_some_and(|t| t > max) {
        bail!("{url}: the file is larger than the {} MB this app will download", max >> 20);
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    hl_demos::scan::mark_own_write(dest);
    let part = dest.with_extension("dem.part");
    let mut file = tokio::fs::File::create(&part)
        .await
        .with_context(|| format!("creating {}", part.display()))?;

    let mut done: u64 = 0;
    let result: Result<()> = async {
        while let Some(chunk) = resp.chunk().await.context("reading download")? {
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            // A server may not say how big it is, or lie; the count is ours.
            if done > max {
                bail!("{url}: the file is larger than the {} MB this app will download", max >> 20);
            }
            progress(done, total);
        }
        file.flush().await?;
        Ok(())
    }
    .await;

    drop(file);
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e);
    }
    if let Some(t) = total {
        if done != t {
            let _ = tokio::fs::remove_file(&part).await;
            bail!("download incomplete: {done} of {t} bytes");
        }
    }
    tokio::fs::rename(&part, dest)
        .await
        .with_context(|| format!("moving download into {}", dest.display()))?;
    Ok(done)
}

/// Whether a request never reached the server at all.
///
/// The difference matters. A log logs.tf answered about with a 404 or a
/// broken body has something wrong with it, and is worth marking so it is not
/// retried forever. A log we could not even connect about says nothing about
/// the log — only that the server is down or the network is — and marking
/// hundreds of them during an outage would park a whole history behind a
/// manual retry.
///
/// A server refusing us ([`Refused`]) counts too: it says nothing about the
/// log either.
pub fn unreachable(e: &anyhow::Error) -> bool {
    refused(e)
        || e.chain().any(|c| {
            c.downcast_ref::<reqwest::Error>()
                .is_some_and(|r| r.is_connect() || r.is_timeout() || r.is_request())
        })
}

/// A server that has turned this address away: a 403 on requests that
/// normally work, or a 429 that outlasted the retries. logs.tf does this for
/// a while after too many requests, and every further request only prolongs
/// it, so a pass that meets one stops asking at once.
#[derive(Debug)]
pub struct Refused {
    pub status: reqwest::StatusCode,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}: the server is refusing requests from this address for now", self.status)
    }
}

impl std::error::Error for Refused {}

/// A log the source says does not exist: HTTP 404 or 410. drops.tf answers
/// "not found" with nothing and the app then asks logs.tf, so this from
/// logs.tf means neither has it -- deleted, not busy (Q77).
pub fn not_found(e: &anyhow::Error) -> bool {
    let text = format!("{e:#}");
    text.contains("HTTP 404") || text.contains("HTTP 410")
}

pub fn refused(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<Refused>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn a_deleted_log_is_told_from_a_busy_server() {
        // Q77: what the 23 in Flashy's list said, against what an outage says.
        assert!(not_found(&anyhow::anyhow!("https://logs.tf/api/v1/log/4089847: HTTP 404 Not Found")));
        assert!(not_found(&anyhow::anyhow!("HTTP 410 Gone")));
        assert!(!not_found(&anyhow::anyhow!("https://logs.tf/api/v1/log/1: HTTP 502 Bad Gateway")));
        assert!(!not_found(&anyhow::anyhow!("operation timed out")));
    }

    #[test]
    fn retry_after_is_read_as_seconds_or_as_a_date() {
        let now = chrono::DateTime::parse_from_rfc2822("Wed, 21 Oct 2026 07:00:00 GMT").unwrap().with_timezone(&chrono::Utc);
        assert_eq!(retry_after("120", now), Some(Duration::from_secs(120)));
        assert_eq!(retry_after(" 3600 ", now), Some(Duration::from_secs(3600)));
        assert_eq!(retry_after("Wed, 21 Oct 2026 07:28:00 GMT", now), Some(Duration::from_secs(28 * 60)));
        // A date already past is no wait at all; nonsense is not an answer.
        assert_eq!(retry_after("Wed, 21 Oct 2026 06:00:00 GMT", now), Some(Duration::ZERO));
        assert_eq!(retry_after("soon", now), None);
    }

    #[test]
    fn a_file_is_fetched_only_over_https_from_a_listed_host() {
        let demos = ["demos.tf"];
        assert!(is_https_on("https://freezer.demos.tf/ab/cd/x.dem", &demos));
        assert!(is_https_on("https://demos.tf/x.dem", &demos));
        assert!(!is_https_on("http://freezer.demos.tf/x.dem", &demos));
        assert!(!is_https_on("https://evildemos.tf/x.dem", &demos));
        assert!(!is_https_on("https://demos.tf.evil.example/x.dem", &demos));
        assert!(!is_https_on("https://192.168.1.5/x.dem", &demos));
        assert!(!is_https_on("not a url", &demos));
    }

    /// One 429 with a long Retry-After: the host rests, and the next call
    /// fails at once without reaching the server (which accepts only one).
    #[tokio::test]
    async fn a_host_that_says_wait_is_left_alone() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = conn.read(&mut buf).unwrap();
                request.extend_from_slice(&buf[..n]);
            }
            conn.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 3600\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let client = Throttled::new(Duration::ZERO).unwrap();
        let at = format!("http://127.0.0.1:{port}/x");
        let first = client.get_text(&at).await.unwrap_err();
        assert!(refused(&first));
        assert!(client.rest_left().unwrap() > Duration::from_secs(3500));
        let started = std::time::Instant::now();
        let second = client.get_text(&at).await.unwrap_err();
        assert!(refused(&second));
        assert!(started.elapsed() < Duration::from_secs(1));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn an_answer_past_the_cap_is_refused() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = conn.read(&mut buf).unwrap();
                request.extend_from_slice(&buf[..n]);
            }
            conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n01234567890123456789").unwrap();
        });
        let client = Throttled::new(Duration::ZERO).unwrap();
        let at = format!("http://127.0.0.1:{port}/x");
        assert!(client.get_bytes_capped(&at, 10).await.is_err());
        server.join().unwrap();
    }

    #[test]
    fn a_query_is_encoded_by_the_parser() {
        assert_eq!(
            url("https://etf2l.org/wp-json/wp/v2/posts", [("search", "Highlander Fall & Winter #2"), ("_fields", "title,content")]),
            "https://etf2l.org/wp-json/wp/v2/posts?search=Highlander+Fall+%26+Winter+%232&_fields=title%2Ccontent"
        );
        assert_eq!(url("https://api.demos.tf/demos", [("players[]", "76561198099396919")]), "https://api.demos.tf/demos?players%5B%5D=76561198099396919");
    }

    #[test]
    fn an_unclosed_etag_is_closed() {
        assert_eq!(closed_etag("W/\"831f"), "W/\"831f\"");
        assert_eq!(closed_etag("W/\"831f\""), "W/\"831f\"");
        assert_eq!(closed_etag("\"abc\""), "\"abc\"");
        assert_eq!(closed_etag("\"abc"), "\"abc\"");
    }

    /// A server saying no requests are left this window (RGL's
    /// `RateLimit-*`): the next one waits for the reset, though our own pace
    /// would let it go at once.
    #[tokio::test]
    async fn a_spent_rate_limit_is_waited_out() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut conn, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = conn.read(&mut buf).unwrap();
                    request.extend_from_slice(&buf[..n]);
                }
                let reply = "HTTP/1.1 200 OK\r\nRateLimit-Limit: 5\r\nRateLimit-Remaining: 0\r\nRateLimit-Reset: 1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]";
                conn.write_all(reply.as_bytes()).unwrap();
            }
        });
        let client = Throttled::new(Duration::ZERO).unwrap();
        let at = format!("http://127.0.0.1:{port}/v0/regions");
        client.get_text(&at).await.unwrap();
        let started = std::time::Instant::now();
        client.get_text(&at).await.unwrap();
        assert!(started.elapsed() >= Duration::from_millis(900), "{:?}", started.elapsed());
        server.join().unwrap();
    }

    /// A one-off server on localhost: the first request gets a body with an
    /// ETag, the second must ask with it and gets a bare 304.
    #[tokio::test]
    async fn a_304_is_answered_from_the_copy_held() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut asked = Vec::new();
            for reply in [
                "HTTP/1.1 200 OK\r\nETag: W/\"v1\"\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
                "HTTP/1.1 304 Not Modified\r\nETag: W/\"v1\"\r\nConnection: close\r\n\r\n",
            ] {
                let (mut conn, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = conn.read(&mut buf).unwrap();
                    request.extend_from_slice(&buf[..n]);
                }
                asked.push(String::from_utf8(request).unwrap().to_ascii_lowercase());
                conn.write_all(reply.as_bytes()).unwrap();
            }
            asked
        });

        let client = Throttled::new(Duration::ZERO).unwrap();
        let at = format!("http://127.0.0.1:{port}/api/v1/logs?limit=1");
        assert_eq!(client.get_text(&at).await.unwrap(), "hello");
        assert_eq!(client.get_text(&at).await.unwrap(), "hello", "the 304 answered from the copy");
        let asked = server.join().unwrap();
        assert!(!asked[0].contains("if-none-match"));
        assert!(asked[1].contains("if-none-match: w/\"v1\""), "{}", asked[1]);
        assert!(asked[0].contains("user-agent: flashwave.tf/"));
    }
}
