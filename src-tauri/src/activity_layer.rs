//! Every line the backend logs, into the activity feed as well as the
//! console and the log file (`hl_ingest::activity`; Settings › Activity
//! log and the sync card's waterfall). Debug lines only while the verbose
//! switch is on; trace lines never.

use std::fmt::Write as _;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

/// The longest a line is kept: a slow query's statement runs to kilobytes.
const MAX_TEXT: usize = 300;

/// Fields that only make a line long: the SQL of a slow statement.
const SKIP: [&str; 3] = ["db.statement", "summary", "rows_affected"];

pub struct ActivityLayer;

#[derive(Default)]
struct Text {
    message: String,
    fields: String,
}

impl Visit for Text {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let name = field.name();
        if name == "message" {
            let _ = write!(self.message, "{value:?}");
        } else if !SKIP.contains(&name) {
            let _ = write!(self.fields, " {name}={value:?}");
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let name = field.name();
        if name == "message" {
            self.message.push_str(value);
        } else if name == "url" {
            // A URL as the feed shows requests: site and path, no query values.
            let (host, path) = hl_ingest::activity::host_path(value);
            let _ = write!(self.fields, " url={host}{path}");
        } else if !SKIP.contains(&name) {
            let _ = write!(self.fields, " {name}={value}");
        }
    }
}

/// The part of the app a log line comes from, from its module path.
pub fn source_of(target: &str) -> String {
    if target.starts_with("sqlx") || target.starts_with("hl_db") {
        return "database".to_string();
    }
    let mut parts = target.split("::");
    let krate = parts.next().unwrap_or(target);
    match (krate, parts.next()) {
        ("hl_ingest" | "hl_app_lib" | "hl_demos" | "hl_rating", Some(module)) => module.to_string(),
        ("hl_app_lib", None) => "app".to_string(),
        (k, _) => k.trim_start_matches("hl_").to_string(),
    }
}

impl<S: Subscriber> Layer<S> for ActivityLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = *event.metadata().level();
        let name = match level {
            Level::ERROR => "error",
            Level::WARN => "warn",
            Level::INFO => "info",
            Level::DEBUG if hl_ingest::activity::verbose() => "debug",
            _ => return,
        };
        let mut text = Text::default();
        event.record(&mut text);
        let mut line = format!("{}{}", text.message, text.fields);
        if line.len() > MAX_TEXT {
            let cut = (0..=MAX_TEXT).rev().find(|i| line.is_char_boundary(*i)).unwrap_or(0);
            line.truncate(cut);
            line.push('…');
        }
        hl_ingest::activity::push(name, &source_of(event.metadata().target()), line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_s_source_is_the_module_it_came_from() {
        assert_eq!(source_of("hl_ingest::league"), "league");
        assert_eq!(source_of("hl_ingest::sixes_sample::run"), "sixes_sample");
        assert_eq!(source_of("sqlx::query"), "database");
        assert_eq!(source_of("hl_db"), "database");
        assert_eq!(source_of("hl_app_lib"), "app");
        assert_eq!(source_of("hl_app_lib::league"), "league");
    }
}
