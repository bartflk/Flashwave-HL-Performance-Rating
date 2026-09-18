//! Clients for trends.tf and logs.tf.
//!
//! Each returns both a typed view and the verbatim JSON, because the verbatim
//! JSON is what gets stored: parsing rules change, source rows don't.

use crate::http::Throttled;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

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

pub struct Sources {
    trends: Throttled,
    logstf: Throttled,
}

impl Sources {
    pub fn new() -> Result<Self> {
        Ok(Sources {
            trends: Throttled::new(Duration::from_millis(1000))?,
            logstf: Throttled::new(Duration::from_millis(1000))?,
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
        let mut path = format!("/api/v1/logs?steamid64={steamid64}&limit=100");
        if let Some(t) = updated_since {
            path.push_str(&format!("&updated_since={t}"));
        }

        let mut rows = Vec::new();
        // Hard cap as a guard against a pagination loop, well above any real history.
        for _ in 0..500 {
            let body = self.trends.get_text(&format!("https://trends.tf{path}")).await?;
            let page: Value = serde_json::from_str(&body).context("parsing trends.tf page")?;

            for raw in page.get("logs").and_then(Value::as_array).into_iter().flatten() {
                let row: TrendsRow =
                    serde_json::from_value(raw.clone()).context("parsing trends.tf row")?;
                rows.push((row, raw.to_string()));
            }
            on_page(rows.len());

            match page.get("next_page").and_then(Value::as_str) {
                Some(next) if !next.is_empty() => path = next.to_string(),
                _ => return Ok(rows),
            }
        }
        anyhow::bail!("trends.tf pagination did not terminate")
    }

    /// Every log logs.tf knows for this player, in one request. Covers the logs
    /// trends.tf never indexed — mostly 2014-2019 on this account.
    pub async fn logstf_search(&self, steamid64: &str) -> Result<Vec<(LogsTfRow, String)>> {
        let url = format!("https://logs.tf/api/v1/log?player={steamid64}&limit=10000");
        let body = self.logstf.get_text(&url).await?;
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

    /// Full JSON for one log, verbatim.
    pub async fn logstf_log(&self, log_id: i64) -> Result<String> {
        let body = self
            .logstf
            .get_text(&format!("https://logs.tf/api/v1/log/{log_id}"))
            .await?;
        // Validate before storing: a truncated or HTML error body must not
        // become a "raw source" that reprocess later chokes on.
        let v: Value = serde_json::from_str(&body)
            .with_context(|| format!("log {log_id}: response is not JSON"))?;
        if v.get("players").is_none() {
            anyhow::bail!("log {log_id}: response has no players");
        }
        Ok(body)
    }
}
