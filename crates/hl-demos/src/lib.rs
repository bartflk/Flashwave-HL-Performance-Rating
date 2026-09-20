//! Demos: reading them, placing them on the real clock, and linking them to logs.
//!
//! v1 is index-only — the 1072-byte header, the file's timestamps and the Demo
//! Support sidecar. No packet parsing; that is v2.

pub mod header;
pub mod link;
pub mod parse;
pub mod scan;

pub use header::DemoHeader;
pub use link::{link, map_base, tick_for, DemoSpan, Link, LogClock};
pub use scan::{scan, DemoFile, SidecarEvent};
