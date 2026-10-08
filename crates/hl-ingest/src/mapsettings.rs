//! The Maps section in Settings (Q31, Flashy): every map the app knows, with
//! where its image, its placement and its callouts come from, and how many
//! of the player's matches were on it.
//!
//! A map is "known" if anything names it: the player's matches, the shapes
//! and images the app ships, the callout seeds, or a file the player put in.

use crate::mapres::GeometryFile;
use crate::{callouts, overview};
use anyhow::Result;
use hl_db::Db;
use hl_demos::map_base;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapRow {
    pub base: String,
    /// The version played most, or the one the app ships a shape for.
    pub name: String,
    /// The player's matches with a round on this map.
    pub matches: i64,
    /// "yours", "built in" or "none", for the image and where it sits.
    pub image: String,
    pub placement: String,
    /// "yours", "built in" or "none".
    pub callouts: String,
    pub zones: usize,
    /// Callouts known by name and not yet drawn.
    pub unplaced: usize,
    pub draft: bool,
    /// Who drew the callouts, where the file says.
    pub callouts_author: Option<String>,
    /// The last callout import can still be taken back.
    pub callouts_undo: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapsOverview {
    pub maps: Vec<MapRow>,
    /// Matches with at least one round on no known map.
    pub unknown_matches: i64,
}

/// `data` is the app's data folder; images live in `data/overviews`.
pub async fn list(db: &Db, data: &Path) -> Result<MapsOverview> {
    let images = data.join("overviews");
    let shipped = GeometryFile::built_in();

    // Matches per base, and the version played most.
    let mut matches: HashMap<String, i64> = HashMap::new();
    let mut names: HashMap<String, (i64, String)> = HashMap::new();
    for (map, logs) in db.map_log_counts().await? {
        let base = map_base(&map);
        *matches.entry(base.clone()).or_default() += logs;
        let e = names.entry(base).or_insert((0, map.clone()));
        if logs > e.0 {
            *e = (logs, map);
        }
    }

    let mut bases: BTreeSet<String> = matches.keys().cloned().collect();
    bases.extend(shipped.maps.keys().cloned());
    bases.extend(overview::built_in_bases().map(str::to_string));
    bases.extend(overview::user_bases(&images));
    bases.extend(callouts::built_in_bases().map(str::to_string));
    bases.extend(callouts::user_bases(data));

    let mut maps = Vec::with_capacity(bases.len());
    for base in bases {
        let (image, placement) = overview::origins(&images, &base);
        // A callout file that cannot be read still lists the map.
        let c = callouts::load(data, &base).unwrap_or_default();
        let name = names
            .get(&base)
            .map(|(_, n)| n.clone())
            .or_else(|| shipped.maps.get(&base).map(|m| m.name.clone()))
            .unwrap_or_else(|| base.clone());
        maps.push(MapRow {
            matches: matches.get(&base).copied().unwrap_or(0),
            image: image.into(),
            placement: placement.into(),
            callouts: if c.origin.is_empty() { "none".into() } else { c.origin },
            zones: c.zones.len(),
            unplaced: c.names.len(),
            draft: c.draft,
            callouts_author: c.author.clone(),
            callouts_undo: callouts::has_undo(data, &base),
            name,
            base,
        });
    }
    // Most played first; maps never played after, by name.
    maps.sort_by(|a, b| b.matches.cmp(&a.matches).then_with(|| a.base.cmp(&b.base)));
    Ok(MapsOverview { maps, unknown_matches: db.unknown_map_logs().await? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_new_install_lists_every_shipped_map() {
        let db = Db::connect_in_memory().await.unwrap();
        let data = std::env::temp_dir().join(format!("hl-mapsettings-{}", std::process::id()));
        let o = list(&db, &data).await.unwrap();
        let vigil = o.maps.iter().find(|m| m.base == "vigil").expect("vigil is listed");
        assert_eq!((vigil.image.as_str(), vigil.placement.as_str(), vigil.matches), ("built in", "built in", 0));
        assert_eq!(vigil.name, "pl_vigil_rc10", "the shipped shape's name");
        assert_eq!(vigil.callouts, "built in");
        let lakeside = o
            .maps
            .iter()
            .find(|m| m.base == "lakeside")
            .expect("lakeside is listed");
        assert_eq!(lakeside.image, "built in");
        assert_eq!(lakeside.placement, "built in");
        for (base, placement) in [
            ("prowater", "built in"),
            ("cornwater", "built in"),
            ("proworks", "built in"),
            ("pro_viaduct", "none"),
        ] {
            let map = o
                .maps
                .iter()
                .find(|m| m.base == base)
                .unwrap_or_else(|| panic!("{base} is listed separately"));
            assert_eq!(map.image, "built in", "{base}");
            assert_eq!(map.placement, placement, "{base}");
        }
        assert!(
            !o.maps.iter().any(|m| m.base == "alloy"),
            "Alloy is not shipped until its image is redone"
        );
        assert_eq!(o.unknown_matches, 0);
    }
}
