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
use tokio::sync::Mutex;
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

pub struct Throttled {
    client: reqwest::Client,
    gap: Duration,
    next_slot: Mutex<Instant>,
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
        Ok(Throttled { client, gap, next_slot: Mutex::new(Instant::now()), validated: Default::default() })
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

    /// The request itself. `interactive` gives up at once on a connect error
    /// (offline, DNS failing, the host down): a sync can wait out a blip, but
    /// a screen would sit on a spinner for the 13 s of backoff only to show
    /// its offline state anyway. Other failures retry either way.
    async fn fetch(&self, url: &str, interactive: bool) -> Result<Option<Vec<u8>>> {
        let mut last_err = None;
        let mut wait = Duration::ZERO;
        for attempt in 0..=RETRY_DELAYS.len() {
            if attempt > 0 {
                tokio::time::sleep(wait.max(RETRY_DELAYS[attempt - 1])).await;
            }
            self.wait_turn().await;

            match self.conditional(url).send().await {
                Ok(resp) => {
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
                        let body = resp.bytes().await.map(|b| b.to_vec()).with_context(|| format!("reading body of {url}"))?;
                        self.keep(url, etag, last_modified, &body);
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
                    // A rate limit says how long to back off; honour it, within reason.
                    wait = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()?.parse::<u64>().ok())
                        .map(|s| Duration::from_secs(s.min(90)))
                        .unwrap_or(Duration::ZERO);
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
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<u64> {
    use tokio::io::AsyncWriteExt;

    let mut resp = client.get(url).send().await.with_context(|| format!("requesting {url}"))?;
    if !resp.status().is_success() {
        bail!("{url}: HTTP {}", resp.status());
    }
    let total = resp.content_length();
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = dest.with_extension("dem.part");
    let mut file = tokio::fs::File::create(&part)
        .await
        .with_context(|| format!("creating {}", part.display()))?;

    let mut done: u64 = 0;
    let result: Result<()> = async {
        while let Some(chunk) = resp.chunk().await.context("reading download")? {
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
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
