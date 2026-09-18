//! Rating: class values, matchups, and the match detail view.
//!
//! Pure functions over [`hl_core::matchdata::NormalizedLog`]. No I/O beyond
//! reading an optional weights file.

pub mod detail;
pub mod value;
pub mod weights;

pub use detail::{build as build_detail, MatchDetail};
pub use weights::Weights;
