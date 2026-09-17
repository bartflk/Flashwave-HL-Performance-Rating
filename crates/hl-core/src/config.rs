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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// The player this app is about. Everything syncs and rates relative to them.
    pub steamid: Option<SteamId>,
    /// Absolute path to the `tf` directory of the TF2 install.
    pub tf_path: Option<String>,
}

impl AppConfig {
    /// Whether first-run setup is complete. The UI gates the rest of the app on this.
    pub fn is_ready(&self) -> bool {
        self.steamid.is_some() && self.tf_path.is_some()
    }
}
