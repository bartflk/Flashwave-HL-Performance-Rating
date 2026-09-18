//! A polite HTTP client: one request per interval, with retry on transient
//! failure.
//!
//! None of trends.tf, logs.tf or ETF2L publishes a rate limit. Behaving as if
//! they do is the price of using free community services, and the fastest way
//! to lose access is to not.

use anyhow::{bail, Context, Result};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

const USER_AGENT: &str = concat!(
    "hl-rating/",
    env!("CARGO_PKG_VERSION"),
    " (local Highlander performance tool)"
);

/// Backoff between attempts. Three tries total.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(3), Duration::from_secs(10)];

pub struct Throttled {
    client: reqwest::Client,
    gap: Duration,
    next_slot: Mutex<Instant>,
}

impl Throttled {
    pub fn new(gap: Duration) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(45))
            .gzip(true)
            .build()
            .context("building HTTP client")?;
        Ok(Throttled { client, gap, next_slot: Mutex::new(Instant::now()) })
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
        let mut last_err = None;
        for attempt in 0..=RETRY_DELAYS.len() {
            if attempt > 0 {
                tokio::time::sleep(RETRY_DELAYS[attempt - 1]).await;
            }
            self.wait_turn().await;

            match self.client.get(url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return resp.text().await.with_context(|| format!("reading body of {url}"));
                    }
                    // 404 and friends will not improve on retry.
                    if status.is_client_error() && status.as_u16() != 429 {
                        bail!("{url}: HTTP {status}");
                    }
                    tracing::warn!(url, %status, attempt, "transient HTTP failure");
                    last_err = Some(anyhow::anyhow!("{url}: HTTP {status}"));
                }
                Err(e) => {
                    tracing::warn!(url, error = %e, attempt, "request failed");
                    last_err = Some(anyhow::Error::new(e).context(format!("requesting {url}")));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("{url}: failed")))
    }
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
