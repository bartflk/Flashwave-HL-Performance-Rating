//! SteamID handling.
//!
//! This exists in exactly one place on purpose. logs.tf keys players by
//! SteamID3 (`[U:1:12345]`), RGL and demos.tf use SteamID64, and players paste
//! whatever their profile shows them. Every conversion in the app goes through
//! here so the formats can never drift apart.

use crate::error::{Error, Result};
use std::fmt;

/// Offset between a Steam account id and its 64-bit form (individual accounts).
const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

/// A Steam account, stored canonically as its 32-bit account id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SteamId(u32);

impl SteamId {
    pub fn from_account_id(id: u32) -> Self {
        SteamId(id)
    }

    pub fn account_id(self) -> u32 {
        self.0
    }

    pub fn as_u64(self) -> u64 {
        STEAMID64_BASE + self.0 as u64
    }

    /// `76561198000000000` — the form used by RGL, demos.tf and the logs.tf
    /// search endpoint.
    pub fn to_steamid64(self) -> String {
        self.as_u64().to_string()
    }

    /// `[U:1:12345]` — the form logs.tf uses as its player map key.
    pub fn to_steamid3(self) -> String {
        format!("[U:1:{}]", self.0)
    }

    /// `STEAM_0:1:12345` — the legacy form, still what some sites display.
    pub fn to_steamid2(self) -> String {
        format!("STEAM_0:{}:{}", self.0 % 2, self.0 / 2)
    }

    pub fn from_u64(v: u64) -> Result<Self> {
        if v < STEAMID64_BASE {
            return Err(Error::InvalidSteamId(format!(
                "{v} is below the individual-account range"
            )));
        }
        let account = v - STEAMID64_BASE;
        u32::try_from(account)
            .map(SteamId)
            .map_err(|_| Error::InvalidSteamId(format!("{v} is out of range")))
    }

    /// Accepts every form a user might realistically paste: SteamID64, SteamID3
    /// with or without brackets, SteamID2, or a community profile URL.
    pub fn parse(input: &str) -> Result<Self> {
        let s = input.trim();
        if s.is_empty() {
            return Err(Error::InvalidSteamId("empty input".into()));
        }

        // Profile URL -> take the trailing path segment and re-parse it.
        if let Some(rest) = s.rsplit_once("/profiles/") {
            let id = rest.1.trim_end_matches('/');
            return Self::parse(id);
        }

        // STEAM_X:Y:Z
        if let Some(rest) = s.strip_prefix("STEAM_").or_else(|| s.strip_prefix("steam_")) {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() != 3 {
                return Err(Error::InvalidSteamId(format!("malformed SteamID2 `{s}`")));
            }
            let y: u32 = parts[1]
                .parse()
                .map_err(|_| Error::InvalidSteamId(format!("malformed SteamID2 `{s}`")))?;
            let z: u32 = parts[2]
                .parse()
                .map_err(|_| Error::InvalidSteamId(format!("malformed SteamID2 `{s}`")))?;
            if y > 1 {
                return Err(Error::InvalidSteamId(format!("malformed SteamID2 `{s}`")));
            }
            return z
                .checked_mul(2)
                .and_then(|v| v.checked_add(y))
                .map(SteamId)
                .ok_or_else(|| Error::InvalidSteamId(format!("SteamID2 `{s}` overflows")));
        }

        // [U:1:12345] or U:1:12345
        let trimmed = s.trim_start_matches('[').trim_end_matches(']');
        if let Some(rest) = trimmed.strip_prefix("U:1:").or_else(|| trimmed.strip_prefix("u:1:")) {
            return rest
                .parse::<u32>()
                .map(SteamId)
                .map_err(|_| Error::InvalidSteamId(format!("malformed SteamID3 `{s}`")));
        }

        // Bare number: SteamID64 if it is large enough, otherwise an account id.
        if let Ok(v) = s.parse::<u64>() {
            return if v >= STEAMID64_BASE {
                Self::from_u64(v)
            } else {
                u32::try_from(v)
                    .map(SteamId)
                    .map_err(|_| Error::InvalidSteamId(format!("`{s}` is out of range")))
            };
        }

        Err(Error::InvalidSteamId(format!("unrecognised format `{s}`")))
    }
}

impl fmt::Display for SteamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_steamid64())
    }
}

impl serde::Serialize for SteamId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        // As a string: SteamID64 exceeds JavaScript's safe integer range.
        s.serialize_str(&self.to_steamid64())
    }
}

impl<'de> serde::Deserialize<'de> for SteamId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        // Fully qualified: the `Deserialize` trait is not in scope here.
        let raw = <String as serde::Deserialize>::deserialize(d)?;
        SteamId::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // b4nny, as a well-known public account id.
    const ACCOUNT: u32 = 2006;

    #[test]
    fn round_trips_through_every_format() {
        let id = SteamId::from_account_id(ACCOUNT);
        assert_eq!(SteamId::parse(&id.to_steamid64()).unwrap(), id);
        assert_eq!(SteamId::parse(&id.to_steamid3()).unwrap(), id);
        assert_eq!(SteamId::parse(&id.to_steamid2()).unwrap(), id);
    }

    #[test]
    fn known_conversion_is_exact() {
        let id = SteamId::parse("76561197960265729").unwrap();
        assert_eq!(id.account_id(), 1);
        assert_eq!(id.to_steamid3(), "[U:1:1]");
        assert_eq!(id.to_steamid2(), "STEAM_0:1:0");
    }

    #[test]
    fn accepts_unbracketed_steamid3_and_profile_urls() {
        let a = SteamId::parse("U:1:2006").unwrap();
        let b = SteamId::parse("[U:1:2006]").unwrap();
        let c = SteamId::parse("https://steamcommunity.com/profiles/76561197960267734/").unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn rejects_junk() {
        assert!(SteamId::parse("").is_err());
        assert!(SteamId::parse("not-an-id").is_err());
        assert!(SteamId::parse("STEAM_0:9:5").is_err());
        assert!(SteamId::parse("123").is_ok()); // bare account id is allowed
    }
}
