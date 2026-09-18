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
