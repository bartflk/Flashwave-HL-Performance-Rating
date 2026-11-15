//! "How ratings work" for 6v6 (Flashy: "is 'how ratings work' page update
//! for 6's as well?"): every role's model as the 6v6 rating uses it, from
//! the same `weights.sixes.toml`, so a refit redraws the page.
//!
//! What each component means is written in the UI, where the translations
//! are read from; this says which components a role is rated on, their
//! share, and which of them are measured yet. A component not measured yet
//! is left out of a game's rating and the rest scaled up, so each one also
//! carries what it counts for today.

use crate::sixes_rating::{higher_is_better, weights};
use crate::sixes_round_win::{WinTable, SETTING};
use anyhow::Result;
use hl_db::Db;
use serde::Serialize;
use std::collections::BTreeMap;

/// Every component the 6v6 rating can fill from a log today
/// (`sixes_rating::perfs_valued`); round impact joins once the round-win
/// table is measured. A test holds this to what the rating fills.
pub const MEASURED: [&str; 21] = [
    "impact_kills",
    "impact_assists",
    "deaths",
    "dpm",
    "pressure",
    "traps",
    "sentry",
    "reflects",
    "eng_deaths",
    "headshot_share",
    "backstabs",
    "airshots",
    "dmg_ratio",
    "medic_picks",
    "deaths_vs_medic",
    "space",
    "heal",
    "adv_lost",
    "ubers_vs",
    "drops_vs",
    "deaths_vs",
];

/// The main roles in the order a team sheet lists them.
const ROLES: [&str; 6] = ["pocket", "roamer", "flank_scout", "pocket_scout", "demoman", "medic"];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GuideComponent {
    pub key: String,
    /// Its share of the role's model, 0 to 1.
    pub share: f64,
    /// What it counts for in a rating today: its share among the measured
    /// ones, 0 when it is not measured yet.
    pub counts: f64,
    pub measured: bool,
    pub higher_is_better: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GuideRole {
    /// "pocket", or "off:heavy" for an off-class.
    pub role: String,
    pub off: bool,
    /// Biggest share first.
    pub components: Vec<GuideComponent>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SixesGuide {
    pub model_version: String,
    /// A main role's game is rated from this many minutes, an off-class's
    /// from `off_min_minutes`.
    pub min_minutes: f64,
    pub off_min_minutes: f64,
    /// Whether the round-win table is measured: round impact counts, and
    /// kills are valued by who died.
    pub round_win: bool,
    /// What killing each class is worth, the average kill 1; empty until
    /// the round-win table is measured.
    pub victim_values: Vec<(String, f64)>,
    pub roles: Vec<GuideRole>,
}

pub async fn guide(db: &Db) -> Result<SixesGuide> {
    let table: Option<WinTable> = db.get_setting(SETTING).await?.and_then(|s| serde_json::from_str(&s).ok());
    Ok(guide_with(table.as_ref()))
}

fn guide_with(table: Option<&WinTable>) -> SixesGuide {
    let w = weights();
    let measured = |c: &str| MEASURED.contains(&c) || (c == "round_impact" && table.is_some());
    let role = |key: String, off: bool, t: &BTreeMap<String, f64>| {
        let used: Vec<(&String, f64)> = t.iter().map(|(c, s)| (c, *s)).filter(|(_, s)| *s > 0.0).collect();
        let total: f64 = used.iter().map(|(_, s)| s).sum();
        let live: f64 = used.iter().filter(|(c, _)| measured(c)).map(|(_, s)| s).sum();
        let mut components: Vec<GuideComponent> = used
            .iter()
            .map(|(c, s)| GuideComponent {
                key: c.to_string(),
                share: s / total,
                counts: if measured(c) && live > 0.0 { s / live } else { 0.0 },
                measured: measured(c),
                higher_is_better: higher_is_better(c),
            })
            .collect();
        components.sort_by(|a, b| b.share.total_cmp(&a.share).then_with(|| a.key.cmp(&b.key)));
        GuideRole { role: key, off, components }
    };
    let mut roles: Vec<GuideRole> = ROLES.iter().filter_map(|r| w.role.get(*r).map(|t| role(r.to_string(), false, t))).collect();
    // A role the weights file has and the list above does not still shows.
    roles.extend(w.role.iter().filter(|(r, _)| !ROLES.contains(&r.as_str())).map(|(r, t)| role(r.clone(), false, t)));
    roles.extend(w.off.iter().map(|(c, t)| role(format!("off:{c}"), true, t)));
    let mut victim_values: Vec<(String, f64)> = table.map(|t| t.victims.iter().map(|(c, v)| (c.clone(), *v)).collect()).unwrap_or_default();
    victim_values.sort_by(|a, b| b.1.total_cmp(&a.1));
    SixesGuide {
        model_version: w.model_version,
        min_minutes: crate::sixes_rating::MIN_MAIN_S as f64 / 60.0,
        off_min_minutes: crate::sixes_rating::MIN_OFF_S as f64 / 60.0,
        round_win: table.is_some(),
        victim_values,
        roles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_is_listed_with_its_shares_adding_up() {
        let g = guide_with(None);
        assert_eq!(g.roles.iter().filter(|r| !r.off).map(|r| r.role.as_str()).collect::<Vec<_>>(), ROLES.to_vec());
        assert!(g.roles.iter().any(|r| r.role == "off:heavy" && r.off));
        for r in &g.roles {
            let share: f64 = r.components.iter().map(|c| c.share).sum();
            let counts: f64 = r.components.iter().map(|c| c.counts).sum();
            assert!((share - 1.0).abs() < 1e-9, "{}: shares add to {share}", r.role);
            assert!(counts == 0.0 || (counts - 1.0).abs() < 1e-9, "{}: what counts adds to {counts}", r.role);
            assert!(r.components.windows(2).all(|w| w[0].share >= w[1].share), "{}: biggest first", r.role);
            assert!(r.components.iter().all(|c| c.share > 0.0), "{}: a weight of 0 is not in the model", r.role);
        }
    }

    #[test]
    fn round_impact_counts_only_once_the_round_win_table_is_measured() {
        let pocket = |g: &SixesGuide| g.roles.iter().find(|r| r.role == "pocket").unwrap().components.iter().find(|c| c.key == "round_impact").unwrap().clone();
        let before = pocket(&guide_with(None));
        assert!(!before.measured && before.counts == 0.0 && (before.share - 0.5).abs() < 1e-9);
        let table = WinTable { by_diff: vec![0.5; 11], rounds: 300, kills: 3000, victims: [("medic".to_string(), 1.8), ("scout".to_string(), 0.9)].into() };
        let g = guide_with(Some(&table));
        let after = pocket(&g);
        assert!(after.measured && after.counts > 0.0);
        assert_eq!(g.victim_values.first().map(|v| v.0.as_str()), Some("medic"), "most valuable first");
    }
}
