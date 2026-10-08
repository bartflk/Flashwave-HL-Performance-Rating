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
    match m.as_str() {
        "viaduct" => "pro_viaduct".into(),
        "proplant_fence" | "proplant_v8_nnb" => "proplant".into(),
        "proside" | "lakeside_r" | "lakeside_r2" => "lakeside".into(),
        "swiftwater_ugc" => "swiftwater".into(),
        "problitz" => "barnblitz".into(),
        "ashville_rc1_nb7" => "ashville".into(),
        "badwater_snowy2" => "badwater".into(),
        "gravelpit_x" => "gravelpit".into(),
        "eruption_b10_test2" | "eruption_b10_test_2" => "eruption".into(),
        "vigil_rc8_test4" => "vigil".into(),
        "biohazard_cal" => "biohazard".into(),
        _ if m.starts_with("cornwater_") => "cornwater".into(),
        _ if m.starts_with("millstone_ugc_") => "millstone".into(),
        _ => m,
    }
}

/// A map as people name it: the gamemode kept, the version gone.
/// `pl_upward_f12` and `pl_upward_rc7` are both `pl_upward`; versions are
/// small fixes, not a different map to win or lose on.
pub fn map_name(map: &str) -> String {
    let m = map.to_ascii_lowercase();
    let base = map_base(&m);
    match PREFIXES.iter().find(|p| m.starts_with(*p)) {
        Some(p) => format!("{p}{base}"),
        None => base,
    }
}

/// Gamemode prefixes, longest first so `koth_` is tried before `k`-anything.
/// `tow_` and the rest are here because a Highlander season occasionally
/// runs something that is not payload or king of the hill.
const PREFIXES: &[&str] = &[
    "pl_", "koth_", "cp_", "ctf_", "plr_", "arena_", "tc_", "mvm_", "tow_", "pass_", "sd_", "pd_",
    "vsh_", "rd_", "trade_", "jump_", "dm_",
];

/// Whether a trailing word is a version rather than part of the name:
/// `f12`, `rc10`, `final`, `b5b`, `pro`. `steel` and `product` are not.
fn is_version(s: &str) -> bool {
    let (alpha, rest) = s.split_at(s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len()));
    // `rcx` and `finalx` have no digit at all and are still versions —
    // `koth_product_rcx` was reading as a map called `product_rcx`, which is
    // why it had no overview image while every other Product did.
    let known = matches!(alpha, "final" | "rc" | "b" | "f" | "v" | "a" | "pro" | "rcx" | "finalx")
        || (alpha == "r" && rest.starts_with(|c: char| c.is_ascii_digit()));
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
        // Viaduct's pool entry uses the pro_viaduct identity.
        assert_eq!(map_base("koth_viaduct_rc4"), "pro_viaduct");
        // Real names that end in a word from the version list.
        assert_eq!(map_base("cp_process_final"), "process");
        assert_eq!(map_base("pl_upward"), "upward");
    }

    use super::*;

    #[test]
    fn a_map_name_keeps_its_gamemode() {
        assert_eq!(map_name("pl_upward_f12"), "pl_upward");
        assert_eq!(map_name("pl_upward_f10"), "pl_upward");
        assert_eq!(map_name("koth_ashville_final1"), "koth_ashville");
        assert_eq!(map_name("koth_product_final"), "koth_product");
        assert_eq!(map_name("tow_tetsudo_b10c"), "tow_tetsudo");
        assert_eq!(map_name("koth_cascade"), "koth_cascade");
        assert_eq!(map_name("PL_Vigil_RC10"), "pl_vigil");
        assert_eq!(map_name("dm_airfusion_final"), "dm_airfusion");
        assert_eq!(map_name("koth_viaduct"), "koth_pro_viaduct");
        // Two different maps stay two.
        assert_ne!(map_name("koth_proot_b5b"), map_name("koth_proplant_v8"));
    }

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
        assert_eq!(map_base("koth_ultiduo_r"), "ultiduo_r");
        assert_eq!(map_base("koth_ultiduo_r_b7"), "ultiduo_r");
    }

    #[test]
    fn related_map_names_share_their_canonical_base() {
        for (variant, canonical) in [
            ("viaduct", "pro_viaduct"),
            ("pro_viaduct", "pro_viaduct"),
            ("proplant_fence", "proplant"),
            ("proplant_v8_nnb", "proplant"),
            ("proside", "lakeside"),
            ("tow_tetsudo_b10a", "tetsudo"),
            ("swiftwater_ugc", "swiftwater"),
            ("problitz", "barnblitz"),
            ("lakeside_r2", "lakeside"),
            ("lakeside_r", "lakeside"),
            ("ashville_rc1_nb7", "ashville"),
            ("prowater", "prowater"),
            ("gravelpit_x", "gravelpit"),
            ("millstone_ugc_4", "millstone"),
            ("millstone_ugc_7", "millstone"),
            ("cornwater_b7c_fix", "cornwater"),
            ("cornwater", "cornwater"),
            ("eruption_b10_test2", "eruption"),
            ("eruption_b10_test_2", "eruption"),
            ("vigil_rc8_test4", "vigil"),
            ("proworks", "proworks"),
            ("badwater_snowy2", "badwater"),
            ("biohazard_cal", "biohazard"),
            ("caverns_r1", "caverns"),
            ("lockdown_r6", "lockdown"),
            ("tigcrik_r2", "tigcrik"),
        ] {
            assert_eq!(map_base(variant), canonical, "{variant}");
        }
        assert_eq!(map_base("koth_prowater_rc2"), "prowater");
        assert_ne!(map_base("prowater"), map_base("badwater"));
        assert_ne!(map_base("cornwater_b7c_fix"), map_base("badwater"));
        assert_ne!(map_base("proworks"), map_base("metalworks"));
        assert_eq!(map_base("dm_airfusion_final"), "airfusion");
        assert_eq!(map_base("dm_biohazard_cal"), "biohazard");
        assert_eq!(map_base("dm_caverns_r1"), "caverns");
        assert_eq!(map_base("dm_lostvillage_two_final"), "lostvillage_two");
    }
}
