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
