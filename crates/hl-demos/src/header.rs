//! The Source engine demo header: a fixed 1072-byte prefix.
//!
//! ```text
//! offset  size  field
//!      0     8  "HL2DEMO\0"
//!      8     4  demo protocol          (i32 LE)
//!     12     4  network protocol       (i32 LE)
//!     16   260  server name            (NUL-padded)
//!    276   260  client name            (the recorder, for a POV demo)
//!    536   260  map name
//!    796   260  game directory
//!   1056     4  playback time, seconds (f32 LE)
//!   1060     4  ticks                  (i32 LE)
//!   1064     4  frames                 (i32 LE)
//!   1068     4  sign-on length         (i32 LE)
//! ```
//!
//! Enough to index a demo in microseconds without parsing a single packet.

use anyhow::{bail, Result};
use serde::Serialize;

pub const HEADER_LEN: usize = 1072;
const MAGIC: &[u8; 8] = b"HL2DEMO\0";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoHeader {
    pub demo_protocol: i32,
    pub network_protocol: i32,
    pub server: String,
    pub recorder: String,
    pub map: String,
    pub game_dir: String,
    pub playback_s: f32,
    pub ticks: i32,
    pub frames: i32,
}

impl DemoHeader {
    /// Ticks per second. Zero-duration headers (a recording that was never
    /// closed cleanly) have no usable rate.
    pub fn tick_rate(&self) -> Option<f64> {
        (self.playback_s > 0.0 && self.ticks > 0).then(|| self.ticks as f64 / self.playback_s as f64)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_LEN {
            bail!("demo header truncated: {} of {HEADER_LEN} bytes", bytes.len());
        }
        if &bytes[..8] != MAGIC {
            bail!("not a Source demo (bad magic)");
        }
        let i32_at = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().expect("4 bytes"));
        let text = |o: usize| {
            let field = &bytes[o..o + 260];
            let end = field.iter().position(|&b| b == 0).unwrap_or(260);
            // Latin-1 rather than UTF-8: server and player names are raw bytes.
            field[..end].iter().map(|&b| b as char).collect::<String>()
        };
        Ok(DemoHeader {
            demo_protocol: i32_at(8),
            network_protocol: i32_at(12),
            server: text(16),
            recorder: text(276),
            map: text(536),
            game_dir: text(796),
            playback_s: f32::from_le_bytes(bytes[1056..1060].try_into().expect("4 bytes")),
            ticks: i32_at(1060),
            frames: i32_at(1064),
        })
    }
}
