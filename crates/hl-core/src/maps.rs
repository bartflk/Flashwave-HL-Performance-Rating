//! Map names.
//!
//! A map is uploaded under whatever version the server ran: `pl_upward_f12`,
//! `pl_upward_rc7`, `koth_product_final`. For anything a player thinks of as
//! "my upward games" those are one map, so the version comes off.

/// A map without its prefix or its version: `pl_upward_f12` is `upward`, and
/// `koth_product_final` is `product`.
pub fn map_base(map: &str) -> String {
    let m = map.to_ascii_lowercase();
    let m = ["pl_", "koth_", "cp_", "ctf_", "plr_", "arena_", "tc_", "mvm_"]
        .iter()
        .find_map(|p| m.strip_prefix(p))
        .unwrap_or(&m)
        .to_string();
    match m.rfind('_') {
        Some(i) if is_version(&m[i + 1..]) => m[..i].to_string(),
        _ => m,
    }
}

/// Whether a trailing word is a version rather than part of the name:
/// `f12`, `rc10`, `final`, `b5b`, `pro`. `steel` and `product` are not.
fn is_version(s: &str) -> bool {
    let (alpha, rest) = s.split_at(s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len()));
    matches!(alpha, "final" | "rc" | "b" | "f" | "v" | "a" | "pro")
        && rest.chars().next().is_none_or(|c| c.is_ascii_digit())
        && rest.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
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
