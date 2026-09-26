//! Map names.
//!
//! A map is uploaded under whatever version the server ran: `pl_upward_f12`,
//! `pl_upward_rc7`, `koth_product_final`. For anything a player thinks of as
//! "my upward games" those are one map, so the version comes off.

/// A map without its prefix or its version: `pl_upward_f12` is `upward`, and
/// `koth_product_final` is `product`.
pub fn map_base(map: &str) -> String {
    let m = map.to_ascii_lowercase();
    let mut m = PREFIXES
        .iter()
        .find_map(|p| m.strip_prefix(p))
        .unwrap_or(&m)
        .to_string();
    // More than one version segment can stack up: `pl_badwater_pro_v12` is
    // a version of a version. Strip until nothing at the end is one.
    while let Some(i) = m.rfind('_') {
        if !is_version(&m[i + 1..]) {
            break;
        }
        m.truncate(i);
    }
    m
}

/// Gamemode prefixes, longest first so `koth_` is tried before `k`-anything.
/// `tow_` and the rest are here because a Highlander season occasionally
/// runs something that is not payload or king of the hill.
const PREFIXES: &[&str] = &[
    "pl_", "koth_", "cp_", "ctf_", "plr_", "arena_", "tc_", "mvm_", "tow_", "pass_", "sd_", "pd_",
    "vsh_", "rd_", "trade_", "jump_",
];

/// Whether a trailing word is a version rather than part of the name:
/// `f12`, `rc10`, `final`, `b5b`, `pro`. `steel` and `product` are not.
fn is_version(s: &str) -> bool {
    let (alpha, rest) = s.split_at(s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len()));
    // `rcx` and `finalx` have no digit at all and are still versions —
    // `koth_product_rcx` was reading as a map called `product_rcx`, which is
    // why it had no overview image while every other Product did.
    let known = matches!(alpha, "final" | "rc" | "b" | "f" | "v" | "a" | "pro" | "rcx" | "finalx");
    known
        && rest.chars().all(|c| c.is_ascii_alphanumeric())
        && (rest.is_empty()
            || rest.starts_with(|c: char| c.is_ascii_digit())
            || matches!(alpha, "rcx" | "finalx"))
}

#[cfg(test)]
mod tests {
    /// Cases found by auditing the maps actually in the database, September
    /// 2026: 21 of 50 had no overview image, and three of those were this
    /// function's fault rather than a missing picture.
    #[test]
    fn version_suffixes_that_used_to_be_missed() {
        // No digit after the letters, and still a version.
        assert_eq!(map_base("koth_product_rcx"), "product");
        // A version of a version.
        assert_eq!(map_base("pl_badwater_pro_v12"), "badwater");
        assert_eq!(map_base("pl_badwater_pro_v9"), "badwater");
        // A gamemode prefix the list did not know.
        assert_eq!(map_base("tow_tetsudo_b10a"), "tetsudo");
        // And the ordinary ones still work.
        assert_eq!(map_base("pl_upward_f12"), "upward");
        assert_eq!(map_base("koth_ashville_final1"), "ashville");
        assert_eq!(map_base("cp_steel_f12"), "steel");
    }

    #[test]
    fn a_name_that_merely_looks_like_a_version_survives() {
        // `pro_viaduct` is a map whose name begins with the word, not a
        // version of `viaduct`; stripping from the front is not our job.
        assert_eq!(map_base("koth_pro_viaduct_rc4"), "pro_viaduct");
        // Real names that end in a word from the version list.
        assert_eq!(map_base("cp_process_final"), "process");
        assert_eq!(map_base("pl_upward"), "upward");
    }

    use super::*;

    #[test]
    fn a_version_comes_off_and_a_name_does_not() {
        assert_eq!(map_base("pl_upward_f12"), "upward");
        assert_eq!(map_base("pl_upward_rc7"), "upward");
        assert_eq!(map_base("koth_product_final"), "product");
        assert_eq!(map_base("koth_proot_b5b"), "proot");
        assert_eq!(map_base("cp_steel_f12"), "steel");
        // Not versions: the last word is part of the name.
        assert_eq!(map_base("pl_swiftwater"), "swiftwater");
        assert_eq!(map_base("koth_ashville_final1"), "ashville");
        assert_eq!(map_base("cp_gullywash"), "gullywash");
    }
}
