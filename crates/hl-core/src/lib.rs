//! Domain types shared by every crate in the workspace.
//!
//! Deliberately free of database, network and Tauri dependencies so it can be
//! used from the GUI, the CLI harness and unit tests alike. (`tfpath` touches
//! the filesystem; that is the one exception, and it only reads.)

pub mod config;
pub mod error;
pub mod maps;
pub mod matchdata;
pub mod steamid;
pub mod tfclass;
pub mod tfpath;

pub use config::AppConfig;
pub use error::{Error, Result};
pub use maps::map_base;
pub use steamid::SteamId;
pub use tfclass::TfClass;
pub use tfpath::TfPathInfo;
