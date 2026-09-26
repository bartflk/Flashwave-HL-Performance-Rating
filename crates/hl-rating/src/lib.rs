//! Rating: class values, matchups, and the match detail view.
//!
//! Pure functions over [`hl_core::matchdata::NormalizedLog`]. No I/O beyond
//! reading an optional weights file.

pub mod detail;
pub mod impact;
pub mod model;
pub mod profile;
pub mod weights;

pub use detail::{build as build_detail, DemoView, EventRow, Jump, MatchDetail, PartView};
pub use model::{Baseline, Component, Performance, Rating, MODEL_VERSION};
pub use profile::{HistoryRow, OppositionBand, Profile};
pub use impact::{FightCounts, Impact, KillCtx};
pub use weights::Weights;
