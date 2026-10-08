//! Map overview images under the kill map.
//!
//! The app ships more.tf's renders of the Highlander pool (`overviews/` in
//! the repository, used with more.tf's permission), named by map base
//! (`upward.png` for `pl_upward_f12`). An image in `<app data>/overviews/`
//! of the same name wins, so a player can put a better one in. A map with
//! no image keeps the outline drawn from kills.
//!
//! Additional map overview images and rectangular-map boundary data are from
//! demos.tf.
//!
//! The Maps section in Settings (Q31) lets a player put their own image in
//! for any map, and line it up over the kills where the table below has no
//! placement for it; both are kept beside the built-in ones and win.
//!
//! PNGs in the repository's `overviews/` folder are embedded at build time.
//! `BUILT_IN` maps their filenames to the canonical map bases used by the app.
//!
//! **Placement.** Square legacy images use more.tf's transform. New
//! rectangular images use demos.tf's supplied world bounds. Alloy does not
//! work from this dataset.

use anyhow::{Context, Result};
use base64::Engine;
use hl_demos::map_base;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `(map base, scale, x, y)`, from more.tf's map table.
const PLACEMENT: &[(&str, f64, f64, f64)] = &[
    ("gullywash", 9.3, -8464.0, 4761.0),
    ("snakewater", 11.0, -9484.0, 5840.0),
    ("sultry", 10.5, -9589.0, 5360.0),
    ("process", 10.0, -9102.0, 5120.0),
    ("sunshine", 9.0, -13824.0, 9855.0),
    ("metalworks", 11.0, -9852.0, 4760.0),
    ("villa", 10.5, -9557.0, 5376.0),
    ("bagel", 9.0, -8192.0, 4608.0),
    ("ashville", 8.0, -7322.0, 4101.0),
    ("product", 7.0, -7907.0, 3584.0),
    ("proplant", 9.0, -8192.0, 4608.0),
    ("proot", 7.75, -7054.0, 3968.0),
    ("cascade", 8.25, -7512.0, 4226.0),
    ("swiftwater", 8.0, -4381.0, 2726.0),
    ("vigil", 7.5, -5802.0, 4940.0),
    ("upward", 5.5, -4956.0, 2216.0),
    ("steel", 8.0, -6740.0, 3196.0),
];

/// `(map base, min x, min y, max x, max y)` world bounds for rectangular images.
const BOUNDARIES: &[(&str, f64, f64, f64, f64)] = &[
    ("badwater", -2427.0, -3340.0, 3785.0, 2922.0),
    ("badlands", -4285.0, -4898.0, 2577.0, 4858.0),
    ("ballin_sky", -1024.0, -1504.0, 1024.0, 1504.0),
    ("snakewater", -5671.0, -2649.0, 6687.0, 2961.0),
    ("ultiduo_baloo", -1678.0, -1964.0, 1678.0, 1964.0),
    ("sunshine", -8798.0, 173.0, -2502.0, 10279.0),
    ("metalworks", -3034.0, -6699.0, 3374.0, 4939.0),
    ("lakeside", -4617.0, -2160.0, 4491.0, 1114.0),
    ("granary", -3069.0, -6539.0, 7.0, 6263.0),
    ("reckoner", -3800.0, -4921.0, 3794.0, 4879.0),
    ("borneo", -3470.0, -8160.0, 3168.0, 3786.0),
    ("ultiduo_grove", -2099.0, -1793.0, 2099.0, 1793.0),
    ("ultiduo_r", -1337.0, -1313.0, 1337.0, 1313.0),
    ("warmtic", -1337.0, -4437.0, 2099.0, 4547.0),
    ("millstone", -3349.0, -1776.0, 2709.0, 4840.0),
    ("vanguard", -5585.0, -1896.0, 5585.0, 1896.0),
    ("coalplant", -1513.0, -4089.0, 1527.0, 4083.0),
    ("ramjam", -3464.0, -1865.0, 3464.0, 1851.0),
    ("airfusion", -855.0, -2533.0, 2315.0, 337.0),
    ("biohazard", 422.0, 366.0, 2486.0, 1942.0),
    ("caverns", -1979.0, -1476.0, 1091.0, 1884.0),
    ("ethic", -1549.0, -1537.0, 273.0, -19.0),
    ("killbox_kbh_2p", -1510.0, -1017.0, 1010.0, 1017.0),
    ("lockdown", -4670.0, 1257.0, -2382.0, 6343.0),
    ("lostarena", -867.0, -1421.0, 1277.0, 1041.0),
    ("lostvillage", -513.0, -1830.0, 2163.0, 2000.0),
    ("tigcrik", -1608.0, -402.0, 264.0, 1268.0),
    ("kalinka", -4117.0, -5036.0, 4117.0, 4740.0),
    ("cardinal", -6716.0, -1782.0, 6716.0, 1782.0),
    ("airfield", -3102.0, -2094.0, 3102.0, 2094.0),
    ("clearcut", -4432.0, -1283.0, 4438.0, 1255.0),
    ("villa", -5988.0, -2466.0, 5988.0, 2466.0),
    ("gravelpit", -5359.0, -328.0, 1795.0, 5610.0),
];

/// Separate map identities that use another map's built-in image by default.
const DEFAULT_IMAGE_FALLBACKS: &[(&str, &str)] = &[
    ("pro_viaduct", "viaduct"),
    ("prowater", "badwater"),
    ("cornwater", "badwater"),
    ("proworks", "metalworks"),
];

fn default_image_base(base: &str) -> &str {
    DEFAULT_IMAGE_FALLBACKS
        .iter()
        .find_map(|(map, image)| (*map == base).then_some(*image))
        .unwrap_or(base)
}

/// The built-in images, keyed by their canonical map base.
/// Built-in images: original more.tf renders and additional images from demos.tf.
const BUILT_IN: &[(&str, &[u8])] = &[
    ("aim_arena_reloaded", include_bytes!("../../../overviews/aim_arena_reloaded.png")),
    ("airfield", include_bytes!("../../../overviews/airfield.png")),
    ("airfusion", include_bytes!("../../../overviews/airfusion.png")),
    ("ashville", include_bytes!("../../../overviews/ashville.png")),
    ("badlands", include_bytes!("../../../overviews/badlands.png")),
    ("badwater", include_bytes!("../../../overviews/badwater.png")),
    ("bagel", include_bytes!("../../../overviews/bagel.png")),
    ("ballin_sky", include_bytes!("../../../overviews/ballin_sky.png")),
    ("biohazard", include_bytes!("../../../overviews/biohazard.png")),
    ("borneo", include_bytes!("../../../overviews/borneo.png")),
    ("cardinal", include_bytes!("../../../overviews/cardinal.png")),
    ("cascade", include_bytes!("../../../overviews/cascade.png")),
    ("caverns", include_bytes!("../../../overviews/caverns.png")),
    ("clearcut", include_bytes!("../../../overviews/clearcut.png")),
    ("coalplant", include_bytes!("../../../overviews/coalplant.png")),
    ("ethic", include_bytes!("../../../overviews/ethic.png")),
    ("granary", include_bytes!("../../../overviews/granary.png")),
    ("gravelpit", include_bytes!("../../../overviews/gravelpit.png")),
    ("gullywash", include_bytes!("../../../overviews/gullywash.png")),
    ("kalinka", include_bytes!("../../../overviews/kalinka.png")),
    ("killbox_kbh_2p", include_bytes!("../../../overviews/killbox_kbh_2p.png")),
    ("lakeside", include_bytes!("../../../overviews/lakeside.png")),
    ("lockdown", include_bytes!("../../../overviews/lockdown.png")),
    ("lostarena", include_bytes!("../../../overviews/lostarena.png")),
    ("lostvillage", include_bytes!("../../../overviews/lostvillage.png")),
    ("lostvillage_two", include_bytes!("../../../overviews/lostvillage_two.png")),
    ("metalworks", include_bytes!("../../../overviews/metalworks.png")),
    ("millstone", include_bytes!("../../../overviews/millstone.png")),
    ("process", include_bytes!("../../../overviews/process.png")),
    ("product", include_bytes!("../../../overviews/product.png")),
    ("proot", include_bytes!("../../../overviews/proot.png")),
    ("proplant", include_bytes!("../../../overviews/proplant.png")),
    ("ramjam", include_bytes!("../../../overviews/ramjam.png")),
    ("reckoner", include_bytes!("../../../overviews/reckoner.png")),
    ("resident_cu", include_bytes!("../../../overviews/resident_cu.png")),
    ("snakewater", include_bytes!("../../../overviews/snakewater.png")),
    ("steel", include_bytes!("../../../overviews/steel.png")),
    ("sultry", include_bytes!("../../../overviews/sultry.png")),
    ("sunshine", include_bytes!("../../../overviews/sunshine.png")),
    ("swiftwater", include_bytes!("../../../overviews/swiftwater.png")),
    ("tigcrik", include_bytes!("../../../overviews/tigcrik.png")),
    ("ultiduo_baloo", include_bytes!("../../../overviews/ultiduo_baloo.png")),
    ("ultiduo_grove", include_bytes!("../../../overviews/ultiduo_grove.png")),
    ("ultiduo_r", include_bytes!("../../../overviews/ultiduo_r.png")),
    ("upward", include_bytes!("../../../overviews/upward.png")),
    ("vanguard", include_bytes!("../../../overviews/vanguard.png")),
    ("viaduct", include_bytes!("../../../overviews/viaduct.png")),
    ("vigil", include_bytes!("../../../overviews/vigil.png")),
    ("villa", include_bytes!("../../../overviews/villa.png")),
    ("warmtic", include_bytes!("../../../overviews/warmtic.png")),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub map_base: String,
    /// Game units at the image's left edge and top edge (game y points up).
    pub min_x: f64,
    pub max_y: f64,
    /// Game units the image spans across.
    pub size: f64,
    /// Rendered height over width. Built-in rectangular images use the ratio
    /// of their supplied bounds; imported images keep their pixel ratio.
    pub aspect: f64,
    /// The image itself, as a data URL the page can draw.
    pub image: String,
}

/// Where an image sits, in game units, as the player lines it up.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub min_x: f64,
    pub max_y: f64,
    pub size: f64,
}

/// Image files a player may put in: the types a webview draws.
const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];
/// Larger than any render seen, small enough that a wrong file is refused.
const MAX_BYTES: usize = 25 * 1024 * 1024;

/// Where a map's image sits in game units. Square images use more.tf's
/// transform; rectangular images use demos.tf's supplied boundary data.
pub fn placement(map: &str) -> Option<(f64, f64, f64)> {
    let base = map_base(map);
    let image_base = default_image_base(&base);
    let bytes = BUILT_IN.iter().find(|image| image.0 == image_base)?.1;
    let (_, width, height) = image_info(bytes)?;

    if width == height {
        let &(_, scale, x, y) = PLACEMENT.iter().find(|p| p.0 == image_base)?;
        let size = 1024.0 * scale;
        let (cx, cy) = (x + 910.0 * scale, y - 512.0 * scale);
        return Some((cx - size / 2.0, cy + size / 2.0, size));
    }

    let &(_, min_x, min_y, max_x, max_y) = BOUNDARIES.iter().find(|b| b.0 == image_base)?;
    let size = max_x - min_x;
    if size <= 0.0 || max_y <= min_y {
        return None;
    }
    Some((min_x, max_y, size))
}

fn boundary_aspect(base: &str) -> Option<f64> {
    let image_base = default_image_base(base);
    let &(_, min_x, min_y, max_x, max_y) = BOUNDARIES.iter().find(|b| b.0 == image_base)?;
    let width = max_x - min_x;
    let height = max_y - min_y;
    (width > 0.0 && height > 0.0).then_some(height / width)
}

fn placement_path(dir: &Path, base: &str) -> PathBuf {
    dir.join(format!("{base}.placement.json"))
}

/// The player's own image file for a base, if there is one.
fn user_image(dir: &Path, base: &str) -> Option<PathBuf> {
    EXTENSIONS
        .iter()
        .map(|e| dir.join(format!("{base}.{e}")))
        .find(|p| p.exists())
}

/// A placement the player saved, else the built-in one.
pub fn placement_for(dir: &Path, base: &str) -> Option<Placement> {
    if let Some(p) = std::fs::read_to_string(placement_path(dir, base))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
    {
        return Some(p);
    }
    placement(base).map(|(min_x, max_y, size)| Placement { min_x, max_y, size })
}

/// `(mime, width, height)` from an image's first bytes: PNG, JPEG or WebP.
/// None for anything else, which is how a wrong file is refused.
pub fn image_info(b: &[u8]) -> Option<(&'static str, u32, u32)> {
    let be32 = |i: usize| {
        b.get(i..i + 4)
            .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    };
    let be16 = |i: usize| {
        b.get(i..i + 2)
            .map(|s| u32::from(u16::from_be_bytes([s[0], s[1]])))
    };
    let le16 = |i: usize| {
        b.get(i..i + 2)
            .map(|s| u32::from(u16::from_le_bytes([s[0], s[1]])))
    };
    let le24 = |i: usize| {
        b.get(i..i + 3)
            .map(|s| u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16)
    };
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(("image/png", be32(16)?, be32(20)?));
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        // Walk the segments to a start-of-frame marker, which holds the size.
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                return None;
            }
            let marker = b[i + 1];
            let len = be16(i + 2)? as usize;
            if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                return Some(("image/jpeg", be16(i + 7)?, be16(i + 5)?));
            }
            i += 2 + len;
        }
        return None;
    }
    if b.get(0..4) == Some(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        return match b.get(12..16)? {
            b"VP8 " => Some(("image/webp", le16(26)? & 0x3FFF, le16(28)? & 0x3FFF)),
            b"VP8L" => {
                let v = u32::from_le_bytes([*b.get(21)?, *b.get(22)?, *b.get(23)?, *b.get(24)?]);
                Some(("image/webp", (v & 0x3FFF) + 1, ((v >> 14) & 0x3FFF) + 1))
            }
            b"VP8X" => Some(("image/webp", le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        };
    }
    None
}

/// A map's image as a data URL with its aspect (height over width): the
/// player's own first, then the built-in one. Whether or not it has a
/// placement, so the aligner can show it.
pub fn image(dir: &Path, map: &str) -> Result<Option<(String, f64)>> {
    let base = map_base(map);
    let own_image = user_image(dir, &base);
    let bytes = match &own_image {
        Some(path) => std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
        None => match BUILT_IN.iter().find(|i| i.0 == default_image_base(&base)) {
            Some(&(_, b)) => b.to_vec(),
            None => return Ok(None),
        },
    };
    let Some((mime, w, h)) = image_info(&bytes) else { return Ok(None) };
    let pixel_aspect = if w == 0 { 1.0 } else { f64::from(h) / f64::from(w) };
    let aspect = if own_image.is_none() { boundary_aspect(&base).unwrap_or(pixel_aspect) } else { pixel_aspect };
    Ok(Some((format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)), aspect)))
}

/// The overview for a map, when both its placement and its image are known:
/// the player's own image and placement in `dir` first, then the built-in.
pub fn load(dir: &Path, map: &str) -> Result<Option<Overview>> {
    let base = map_base(map);
    let Some(Placement { min_x, max_y, size }) = placement_for(dir, &base) else { return Ok(None) };
    let Some((image, aspect)) = image(dir, &base)? else { return Ok(None) };
    Ok(Some(Overview { map_base: base, min_x, max_y, size, aspect, image }))
}

/// Where a map's image and placement come from: "yours", "built in" or
/// "none". What the Maps section in Settings lists.
pub fn origins(dir: &Path, base: &str) -> (&'static str, &'static str) {
    let base = map_base(base);
    let built_in = BUILT_IN.iter().find(|i| i.0 == default_image_base(&base)).map(|i| i.1);
    // A copy of the built-in image, byte for byte, is the built-in image:
    // installs from before the images shipped kept them in this folder.
    let own = user_image(dir, &base).filter(|p| built_in.is_none_or(|b| std::fs::read(p).map_or(true, |mine| mine != b)));
    let image = if own.is_some() {
        "yours"
    } else if built_in.is_some() {
        "built in"
    } else {
        "none"
    };
    let placed = if placement_path(dir, &base).exists() { "yours" } else if placement(&base).is_some() { "built in" } else { "none" };
    (image, placed)
}

/// Every base with a built-in image or placement.
pub fn built_in_bases() -> impl Iterator<Item = &'static str> {
    BUILT_IN
        .iter()
        .filter(|image| map_base(image.0) == image.0)
        .map(|image| image.0)
        .chain(DEFAULT_IMAGE_FALLBACKS.iter().map(|(map, _)| *map))
        .chain(PLACEMENT.iter().map(|p| p.0))
}

/// Bases the player has put an image or a placement in for.
pub fn user_bases(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let base = name
                .strip_suffix(".placement.json")
                .map(str::to_string)
                .or_else(|| {
                    let (stem, ext) = name.rsplit_once('.')?;
                    EXTENSIONS
                        .contains(&ext.to_ascii_lowercase().as_str())
                        .then(|| stem.to_string())
                })?;
            Some(base)
        })
        .collect()
}

/// Put a player's image in for a map, replacing any image of theirs there
/// was. Refuses anything that is not a PNG, JPEG or WebP, or is too large.
pub fn import(dir: &Path, map: &str, from: &Path) -> Result<()> {
    let base = map_base(map);
    anyhow::ensure!(
        !base.is_empty() && base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "`{map}` is not a map"
    );
    let bytes = std::fs::read(from).with_context(|| format!("reading {}", from.display()))?;
    anyhow::ensure!(
        bytes.len() <= MAX_BYTES,
        "{} is larger than 25 MB",
        from.display()
    );
    let (mime, w, h) = image_info(&bytes)
        .with_context(|| format!("{} is not a PNG, JPEG or WebP image", from.display()))?;
    anyhow::ensure!(w >= 64 && h >= 64, "{} is only {w}×{h}", from.display());
    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        _ => "webp",
    };
    std::fs::create_dir_all(dir)?;
    // One image a map: a new one replaces the old whatever its type.
    for e in EXTENSIONS {
        let old = dir.join(format!("{base}.{e}"));
        if old.exists() {
            std::fs::remove_file(&old)?;
        }
    }
    let path = dir.join(format!("{base}.{ext}"));
    let tmp = dir.join(format!("{base}.{ext}.tmp"));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Save where the player lined the image up.
pub fn save_placement(dir: &Path, map: &str, p: Placement) -> Result<()> {
    anyhow::ensure!(
        p.min_x.is_finite() && p.max_y.is_finite() && p.size.is_finite() && p.size > 0.0,
        "not a placement"
    );
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        placement_path(dir, &map_base(map)),
        serde_json::to_string_pretty(&p)?,
    )?;
    Ok(())
}

/// Back to the built-in image and placement: the player's own files go.
pub fn remove(dir: &Path, map: &str) -> Result<()> {
    let base = map_base(map);
    for path in EXTENSIONS
        .iter()
        .map(|e| dir.join(format!("{base}.{e}")))
        .chain([placement_path(dir, &base)])
    {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A position more.tf would draw at (x%, y%) of the image lands at the
    /// same fraction of our placement.
    #[test]
    fn placement_matches_moretf_projection() {
        let (s, x, y) = (5.5, -4956.0, 2216.0);
        let moretf = |gx: f64, gy: f64| {
            (
                (gx - (x + 910.0 * s)) / s * 0.097656 + 50.0,
                (gy - (y - 512.0 * s)) / s * -0.097656 + 50.0,
            )
        };
        let (min_x, max_y, size) = placement("pl_upward_f12").unwrap();
        for (gx, gy) in [(0.0, 0.0), (1234.0, -2345.0), (-3000.0, 1500.0)] {
            let (mx, my) = moretf(gx, gy);
            let (ox, oy) = ((gx - min_x) / size * 100.0, (max_y - gy) / size * 100.0);
            assert!(
                (mx - ox).abs() < 0.01 && (my - oy).abs() < 0.01,
                "{gx},{gy}: {mx},{my} vs {ox},{oy}"
            );
        }
    }

    #[test]
    fn built_in_images_are_valid_and_selected() {
        let empty = std::env::temp_dir().join(format!("hl-overviews-none-{}", std::process::id()));
        let o = load(&empty, "pl_vigil_rc10")
            .unwrap()
            .expect("vigil ships an image");
        assert!(
            o.image.starts_with("data:image/png;base64,iVBORw0KGgo"),
            "a PNG"
        );
        for (base, bytes) in BUILT_IN {
            assert_eq!(
                image_info(bytes).map(|info| info.0),
                Some("image/png"),
                "{base}"
            );
        }
        assert!(BUILT_IN.iter().any(|(base, _)| *base == "metalworks"));
        assert!(BUILT_IN.iter().any(|(base, _)| *base == "badwater"));
        assert!(BUILT_IN.iter().any(|(base, _)| *base == "viaduct"));
        assert!(BUILT_IN.iter().any(|(base, _)| *base == "millstone"));
        assert!(!BUILT_IN.iter().any(|(base, _)| *base == "alloy"));
    }

    #[test]
    fn rectangular_images_use_supplied_boundaries_and_square_images_keep_legacy_placement() {
        assert_eq!(
            placement("pl_badwater_pro_v9"),
            Some((-2427.0, 2922.0, 6212.0))
        );
        assert_eq!(
            placement("koth_prowater_rc2"),
            Some((-2427.0, 2922.0, 6212.0))
        );
        assert_eq!(placement("koth_proworks"), placement("metalworks"));
        assert!(placement("cp_sultry_b8a").is_none());
        assert!(placement("alloy").is_none());

        let square_placement = placement("cp_gullywash_f6");
        assert_eq!(square_placement, placement("gullywash"));

        for &(base, _, _, _, _) in BOUNDARIES {
            assert!(placement(base).is_some(), "{base}");
        }

        let dir = Path::new("missing-overview-dir");
        assert_eq!(
            image(dir, "cornwater_b7c_fix").unwrap(),
            image(dir, "badwater").unwrap()
        );
        assert_eq!(
            image(dir, "prowater").unwrap(),
            image(dir, "badwater").unwrap()
        );
        assert_eq!(
            image(dir, "proworks").unwrap(),
            image(dir, "metalworks").unwrap()
        );
        assert_eq!(
            image(dir, "viaduct").unwrap(),
            image(dir, "pro_viaduct").unwrap()
        );
        let bases: Vec<_> = built_in_bases().collect();
        for base in ["pro_viaduct", "prowater", "cornwater", "proworks"] {
            assert!(bases.contains(&base), "{base} should be listed separately");
        }
        assert!(!bases.contains(&"alloy"));
        assert!(!bases.contains(&"viaduct"));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hl-overview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A PNG header of the given size: enough for `image_info`.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, 6, 0, 0, 0]);
        b
    }

    #[test]
    fn image_sizes_are_read_from_the_header() {
        assert_eq!(image_info(&png(1024, 768)), Some(("image/png", 1024, 768)));
        let (mime, w, h) = image_info(BUILT_IN[0].1).unwrap();
        assert_eq!((mime, w, h > 0), ("image/png", w, true));
        // A minimal JPEG: SOI, an APP0 segment, then SOF0 with 600×400.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01,
            0x90, 0x02, 0x58, 0x03, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(image_info(&jpeg), Some(("image/jpeg", 600, 400)));
        assert_eq!(image_info(b"not an image at all"), None);
    }

    #[test]
    fn a_players_image_and_placement_win_and_remove_goes_back() {
        let dir = scratch("import");
        let file = dir.join("mine.png");
        std::fs::write(&file, png(2048, 1024)).unwrap();
        import(&dir.join("overviews"), "pl_vigil_rc10", &file).unwrap();
        let o = dir.join("overviews");
        assert_eq!(origins(&o, "vigil"), ("yours", "built in"));
        let v = load(&o, "pl_vigil_rc10").unwrap().unwrap();
        assert!(
            (v.aspect - 0.5).abs() < 1e-9,
            "a wide image keeps its shape"
        );

        let p = Placement {
            min_x: -100.0,
            max_y: 200.0,
            size: 5000.0,
        };
        save_placement(&o, "vigil", p).unwrap();
        assert_eq!(placement_for(&o, "vigil"), Some(p));
        let mut bases = user_bases(&o);
        bases.sort();
        bases.dedup();
        assert_eq!(bases, ["vigil"]);

        remove(&o, "vigil").unwrap();
        assert_eq!(origins(&o, "vigil"), ("built in", "built in"));
        assert!(load(&o, "pl_vigil_rc10")
            .unwrap()
            .unwrap()
            .image
            .starts_with("data:image/png"));
    }

    #[test]
    fn a_copy_of_the_built_in_image_counts_as_built_in() {
        let dir = scratch("copy");
        std::fs::write(
            dir.join("vigil.png"),
            BUILT_IN.iter().find(|i| i.0 == "vigil").unwrap().1,
        )
        .unwrap();
        assert_eq!(origins(&dir, "vigil").0, "built in");
        std::fs::write(dir.join("vigil.png"), png(512, 512)).unwrap();
        assert_eq!(origins(&dir, "vigil").0, "yours");
    }

    #[test]
    fn a_default_image_fallback_keeps_its_own_custom_image_and_placement() {
        let dir = scratch("fallback");
        assert_eq!(origins(&dir, "cornwater"), ("built in", "built in"));
        assert_eq!(
            placement_for(&dir, "cornwater"),
            placement_for(&dir, "badwater")
        );

        std::fs::write(dir.join("cornwater.png"), png(2048, 1024)).unwrap();
        assert_eq!(origins(&dir, "cornwater"), ("yours", "built in"));
        let (_, aspect) = image(&dir, "cornwater_b7c_fix").unwrap().unwrap();
        assert!((aspect - 0.5).abs() < 1e-9);

        let custom = Placement {
            min_x: 10.0,
            max_y: 20.0,
            size: 30.0,
        };
        save_placement(&dir, "cornwater", custom).unwrap();
        assert_eq!(placement_for(&dir, "cornwater"), Some(custom));
        assert_ne!(
            placement_for(&dir, "cornwater"),
            placement_for(&dir, "badwater")
        );
    }

    #[test]
    fn a_file_that_is_not_an_image_is_refused() {
        let dir = scratch("refuse");
        let file = dir.join("notes.png");
        std::fs::write(&file, b"these are my callouts").unwrap();
        assert!(import(&dir, "vigil", &file).is_err());
        assert_eq!(origins(&dir, "vigil").0, "built in");
    }

    #[test]
    fn a_map_with_an_image_but_no_placement_has_no_overview_until_lined_up() {
        let dir = scratch("unplaced");
        let file = dir.join("lake.png");
        std::fs::write(&file, png(1024, 1024)).unwrap();
        import(&dir, "aim_arena_reloaded", &file).unwrap();
        assert_eq!(origins(&dir, "aim_arena_reloaded"), ("yours", "none"));
        assert!(load(&dir, "aim_arena_reloaded").unwrap().is_none());
        assert!(
            image(&dir, "aim_arena_reloaded").unwrap().is_some(),
            "the aligner can still show it"
        );
        save_placement(
            &dir,
            "aim_arena_reloaded",
            Placement {
                min_x: 0.0,
                max_y: 0.0,
                size: 4000.0,
            },
        )
        .unwrap();
        assert!(load(&dir, "aim_arena_reloaded").unwrap().is_some());
    }

    #[test]
    fn versions_share_an_image_and_unknown_maps_have_none() {
        assert_eq!(placement("koth_proot_b5b"), placement("koth_proot_final"));
        assert_eq!(
            placement("koth_lakeside_final"),
            Some((-4617.0, 1114.0, 9108.0))
        );
        assert!(placement("unknown_map").is_none());
    }
}
