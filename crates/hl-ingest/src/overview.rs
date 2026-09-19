//! Map overview images under the kill map, where one is on this machine.
//!
//! Images live in `<app data>/overviews/<map>.png`, named by map base
//! (`upward.png` for `pl_upward_f12`). They are not part of the app: the ones
//! used here are more.tf's renders, saved locally with the owner's say-so and
//! never committed. A map with no image keeps the outline drawn from kills.
//!
//! **Placement.** Each image is square and covers `1024 × scale` game units,
//! centred on `(x + 910·scale, y − 512·scale)`: the transform more.tf uses for
//! the same images, per map. Checked on this account by projecting stored
//! kill positions onto the images: 94-99% land on the drawn map, and firing
//! positions sit on balconies and cliff edges where Snipers stand.

use anyhow::{Context, Result};
use base64::Engine;
use hl_demos::map_base;
use serde::Serialize;
use std::path::Path;

/// `(map base, scale, x, y)`, from more.tf's map table.
const PLACEMENT: &[(&str, f64, f64, f64)] = &[
    ("gullywash", 9.3, -8464.0, 4761.0),
    ("snakewater", 11.0, -9484.0, 5840.0),
    ("sultry", 10.5, -9589.0, 5360.0),
    ("process", 10.0, -9102.0, 5120.0),
    ("sunshine", 9.0, -13824.0, 9855.0),
    ("metalworks", 11.0, -9852.0, 4760.0),
    ("villa", 10.5, -9557.0, 5376.0),
    ("proworks", 11.0, -9852.0, 4760.0),
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub map_base: String,
    /// Game units at the image's left edge and top edge (game y points up).
    pub min_x: f64,
    pub max_y: f64,
    /// Game units the square image spans on each side.
    pub size: f64,
    /// The image itself, as a data URL the page can draw.
    pub image: String,
}

/// Where a map's image sits in game units: `(min_x, max_y, size)`.
pub fn placement(map: &str) -> Option<(f64, f64, f64)> {
    let base = map_base(map);
    let &(_, scale, x, y) = PLACEMENT.iter().find(|p| p.0 == base)?;
    let size = 1024.0 * scale;
    let (cx, cy) = (x + 910.0 * scale, y - 512.0 * scale);
    Some((cx - size / 2.0, cy + size / 2.0, size))
}

/// The overview for a map, when both its placement and its image are known.
pub fn load(dir: &Path, map: &str) -> Result<Option<Overview>> {
    let Some((min_x, max_y, size)) = placement(map) else { return Ok(None) };
    let base = map_base(map);
    let path = dir.join(format!("{base}.png"));
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let image = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
    Ok(Some(Overview { map_base: base, min_x, max_y, size, image }))
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
            assert!((mx - ox).abs() < 0.01 && (my - oy).abs() < 0.01, "{gx},{gy}: {mx},{my} vs {ox},{oy}");
        }
    }

    #[test]
    fn versions_share_an_image_and_unknown_maps_have_none() {
        assert_eq!(placement("koth_proot_b5b"), placement("koth_proot_final"));
        assert!(placement("koth_lakeside_final").is_none());
    }
}
