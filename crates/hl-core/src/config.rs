//! Application configuration.
//!
//! Persisted as key/value rows in `app_config` rather than a struct-shaped blob,
//! so adding a setting never needs a migration.

use crate::steamid::SteamId;
use serde::{Deserialize, Serialize};

pub mod keys {
    pub const STEAMID: &str = "steamid";
    pub const TF_PATH: &str = "tf_path";
    pub const SCHEMA_NOTE: &str = "schema_note";
    /// Set to "1" to download every log ever played rather than the recent
    /// years plus officials. Asked for in Settings, never on by default.
    pub const ALL_HISTORY: &str = "fetch_all_history";
    /// Set to "1" to delete a downloaded STV demo once every match it
    /// covers has been read (PLAN Q23). Off by default, and deliberately:
    /// the first thing a new version of the aim pass wants is the demos it
    /// has already read, and deleting them means a match can never be
    /// improved by a later pass without fetching the file again.
    pub const AUTO_DELETE_DEMOS: &str = "auto_delete_demos";
    /// The formats the owner plays, chosen in setup: "highlander",
    /// "sixes" or both, comma-separated (see `format::formats_played`).
    /// Missing means Highlander alone, as before 6v6.
    pub const FORMATS_PLAYED: &str = "formats_played";
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// The player this app is about. Everything syncs and rates relative to them.
    pub steamid: Option<SteamId>,
    /// Absolute path to the `tf` directory of the TF2 install. Optional: it
    /// is only needed for demos, and everything else comes from logs.tf.
    pub tf_path: Option<String>,
}

impl AppConfig {
    /// Whether first-run setup is complete: the owner is known. The TF2
    /// folder can be skipped (a PC without TF2, or no interest in demos).
    pub fn is_ready(&self) -> bool {
        self.steamid.is_some()
    }
}
