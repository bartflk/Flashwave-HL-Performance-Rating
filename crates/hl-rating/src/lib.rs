//! Rating: class values, matchups, and the match detail view.
//!
//! Pure functions over [`hl_core::matchdata::NormalizedLog`]. No I/O beyond
//! reading an optional weights file.

pub mod detail;
pub mod model;
pub mod profile;
pub mod weights;

pub use detail::{build as build_detail, MatchDetail};
pub use model::{Baseline, Component, Performance, Rating, MODEL_VERSION};
pub use profile::{HistoryRow, Profile};
pub use weights::Weights;
