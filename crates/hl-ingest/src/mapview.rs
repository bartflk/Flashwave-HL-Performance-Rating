//! A map's outline, drawn from where people fought on it.
//!
//! No overview images: every kill in the stored raw logs records where both
//! players stood, and on a map with a few thousand kills those positions trace
//! the playable space. Binned top-down into a grid, the counts are the map's
//! silhouette, and the busiest cells are its chokes and sightlines.
//!
//! Versions share one outline (`pl_upward_f10` and `_f12` are both `upward`):
//! competitive map updates rarely move the geometry enough to matter at this
//! resolution.

use anyhow::Result;
use hl_core::SteamId;
use hl_db::Db;
use hl_demos::map_base;
use serde::Serialize;

/// Cells along the map's longer side.
const GRID: f64 = 180.0;
/// Positions outside these percentiles (per axis) are ignored when sizing the
/// grid, so one player stuck in a skybox does not shrink the map to a dot.
const TRIM: f64 = 0.004;
/// Fewer positions than this and there is no outline worth drawing.
const MIN_POINTS: usize = 400;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapView {
    pub map_base: String,
    /// Matches the outline was drawn from.
    pub games: usize,
    pub points: usize,
    /// Game units at the grid's left edge and top edge (game y points up).
    pub min_x: f64,
    pub max_y: f64,
    /// Game units per cell.
    pub cell: f64,
    pub width: usize,
    pub height: usize,
    /// Row-major, row 0 at the top: every kill's two positions.
    pub occupancy: Vec<u32>,
    /// The owner's position when they got a kill, over every match on this map.
    pub my_kills: Vec<u32>,
    /// The owner's position when they died.
    pub my_deaths: Vec<u32>,
    pub my_games: usize,
}

/// One kill's two positions, top-down.
#[derive(Debug, Clone, Copy)]
pub struct PosKill {
    pub log_id: i64,
    pub killer: u32,
    pub victim: u32,
    pub killer_xy: (i32, i32),
    pub victim_xy: (i32, i32),
}

/// `None` when there are too few kills on this map to draw it.
pub async fn load(db: &Db, map: &str, me: Option<SteamId>) -> Result<Option<MapView>> {
    let base = map_base(map);
    let maps: Vec<String> = db.kept_maps().await?.into_iter().filter(|m| map_base(m) == base).collect();
    if maps.is_empty() {
        return Ok(None);
    }
    let kills: Vec<PosKill> = db
        .kill_positions(&maps)
        .await?
        .into_iter()
        .map(|(log_id, killer, victim, kx, ky, vx, vy)| PosKill {
            log_id,
            killer,
            victim,
            killer_xy: (kx, ky),
            victim_xy: (vx, vy),
        })
        .collect();
    Ok(build(&base, &kills, me.map(|m| m.account_id())))
}

pub fn build(base: &str, kills: &[PosKill], me: Option<u32>) -> Option<MapView> {
    let pts: Vec<(i32, i32)> = kills.iter().flat_map(|k| [k.killer_xy, k.victim_xy]).collect();
    if pts.len() < MIN_POINTS {
        return None;
    }
    let (x0, x1) = trimmed(pts.iter().map(|p| p.0));
    let (y0, y1) = trimmed(pts.iter().map(|p| p.1));
    // A small margin so edge cells are not cut off.
    let pad = 0.03 * (x1 - x0).max(y1 - y0);
    let (x0, x1, y0, y1) = (x0 - pad, x1 + pad, y0 - pad, y1 + pad);
    let cell = ((x1 - x0).max(y1 - y0) / GRID).max(1.0);
    let width = ((x1 - x0) / cell).ceil() as usize;
    let height = ((y1 - y0) / cell).ceil() as usize;

    let index = |(x, y): (i32, i32)| -> Option<usize> {
        let cx = ((x as f64 - x0) / cell).floor();
        let cy = ((y1 - y as f64) / cell).floor();
        (cx >= 0.0 && cy >= 0.0 && (cx as usize) < width && (cy as usize) < height)
            .then(|| cy as usize * width + cx as usize)
    };
    let mut occupancy = vec![0u32; width * height];
    let mut my_kills = vec![0u32; width * height];
    let mut my_deaths = vec![0u32; width * height];
    let mut my_logs = std::collections::HashSet::new();
    for k in kills {
        for p in [k.killer_xy, k.victim_xy] {
            if let Some(i) = index(p) {
                occupancy[i] += 1;
            }
        }
        if Some(k.killer) == me {
            my_logs.insert(k.log_id);
            if let Some(i) = index(k.killer_xy) {
                my_kills[i] += 1;
            }
        }
        if Some(k.victim) == me {
            my_logs.insert(k.log_id);
            if let Some(i) = index(k.victim_xy) {
                my_deaths[i] += 1;
            }
        }
    }
    let games = kills.iter().map(|k| k.log_id).collect::<std::collections::HashSet<_>>().len();
    Some(MapView {
        map_base: base.to_string(),
        games,
        points: pts.len(),
        min_x: x0,
        max_y: y1,
        cell,
        width,
        height,
        occupancy,
        my_kills,
        my_deaths,
        my_games: my_logs.len(),
    })
}

fn trimmed(values: impl Iterator<Item = i32>) -> (f64, f64) {
    let mut v: Vec<i32> = values.collect();
    v.sort_unstable();
    let lo = v[((v.len() as f64 * TRIM) as usize).min(v.len() - 1)];
    let hi = v[((v.len() as f64 * (1.0 - TRIM)) as usize).min(v.len() - 1)];
    (lo as f64, hi as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(log_id: i64, killer: u32, victim: u32, kxy: (i32, i32), vxy: (i32, i32)) -> PosKill {
        PosKill { log_id, killer, victim, killer_xy: kxy, victim_xy: vxy }
    }

    #[test]
    fn too_few_kills_draw_nothing() {
        assert!(build("upward", &[k(1, 1, 2, (0, 0), (10, 10))], None).is_none());
    }

    #[test]
    fn grid_is_top_down_with_north_at_the_top() {
        // A 1000 x 500 unit area filled evenly, plus the owner's kills in the
        // north-west corner and deaths in the south-east.
        let mut ks = Vec::new();
        for i in 0..400i32 {
            let x = i * 37 % 1000;
            let y = i * 53 % 500;
            ks.push(k(i as i64 % 5, 90, 91, (x, y), (1000 - x, 500 - y)));
        }
        for _ in 0..20 {
            ks.push(k(9, 7, 91, (5, 495), (500, 250)));
            ks.push(k(9, 91, 7, (500, 250), (995, 5)));
        }
        let m = build("test", &ks, Some(7)).unwrap();
        assert!(m.width > m.height, "wider than tall, like the area");
        assert_eq!(m.my_games, 1);
        let at = |grid: &[u32]| {
            let i = grid.iter().position(|&n| n > 0).unwrap();
            (i % m.width, i / m.width)
        };
        let (kx, ky) = at(&m.my_kills);
        let (dx, dy) = at(&m.my_deaths);
        assert!(kx < m.width / 4 && ky < m.height / 4, "north-west kills land top left");
        assert!(dx > m.width * 3 / 4 && dy > m.height * 3 / 4, "south-east deaths land bottom right");
    }
}
