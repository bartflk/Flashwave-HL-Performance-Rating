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
pub use sync::{import_log, parse_log_id, reprocess, sync, Imported, Progress, SyncOptions, SyncSummary};

pub use detail::{match_detail, view_of, MatchView};
mod detail;
pub mod rating;
pub mod context;
pub mod rawlog;
pub mod state;
pub mod statecheck;
pub mod fights;
pub mod situation;
pub mod teamfights;
pub mod seasons;
pub mod owner;
pub mod parts;
pub mod validate;
pub mod kills;

/// At most this many bulk requests to logs.tf in one sync (raw logs, parts).
/// A backlog drains over several syncs instead of tripping its limit. 100
/// still earned a 403 (Flashy, September 2026), so it is 30: the newest
/// matches first, the backlog over a few more syncs.
pub const BULK_PER_SYNC: usize = 30;

/// Raw server logs fetched per sync. drops.tf serves them (logs.tf only
/// for what it has not got), at about 0.45 s each: 300 is a couple of
/// minutes, where logs.tf's pace held a sync to [`BULK_PER_SYNC`].
pub const RAW_PER_SYNC: usize = 300;
pub mod aim;
pub mod backup;
pub mod lock;
pub mod restore;
pub mod analysis;
pub mod mapview;
pub mod overview;
pub mod mapres;
pub mod maps;
pub mod etf2l;
pub mod teammates;
pub mod demos;
pub mod demostf;
pub mod spy;
pub mod demostats;
pub mod spawns;
pub mod cart;
pub mod demo_import;
pub mod leagues;
pub mod callouts;
pub mod snapshot;
pub mod mapsettings;
pub mod moretf;
pub mod league_sample;
pub mod league_rating;
pub mod catalogue;
pub mod trends_career;
pub mod transfers;
pub mod team_info;
pub mod fixtures;
pub mod news;
pub mod league_match;
pub mod recruitment;
pub mod match_page;
pub mod etf2l_demos;
pub mod buildings;
pub mod lives;
pub mod mass;
pub use demos::{extra_demo_dirs, fetch_stv, index_demos, set_extra_demo_dirs, DemoIndexSummary, StvFetched};
pub use rating::{load_profile, rate_all, rated_classes, RateSummary};
