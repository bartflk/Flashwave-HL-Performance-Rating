//! Getting match data in: trends.tf for the index, logs.tf for the detail.
//!
//! The pure parts — [`normalize`], [`classify`] and [`dedupe`] — have no I/O
//! and carry the correctness burden. [`sync`] is the orchestration around them.

pub mod classify;
pub mod dedupe;
mod http;
pub mod normalize;
pub mod sources;
pub mod sync;

pub use sources::Sources;
pub use sync::{reprocess, sync, Progress, SyncOptions, SyncSummary};

pub use detail::match_detail;
mod detail;
pub mod rating;
pub mod context;
pub mod rawlog;
pub mod kills;
pub mod analysis;
pub mod mapview;
pub mod mapres;
pub mod maps;
pub mod etf2l;
pub mod teammates;
pub mod demos;
pub use demos::{fetch_stv, index_demos, DemoIndexSummary, StvFetched};
pub use rating::{load_profile, rate_all, rated_classes, RateSummary};
