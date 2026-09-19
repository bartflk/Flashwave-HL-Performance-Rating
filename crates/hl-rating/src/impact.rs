//! Impact from individual kills, valued in context.
//!
//! Without a raw log, impact comes from `classkills`: totals per victim class,
//! each at its general value. With one, every kill is valued on its own, by
//! the victim's class, the map, and whether the victim was defending. On a
//! symmetric map with no per-map values the two agree exactly: the raw log
//! reproduces `classkills` kill for kill.

use crate::weights::Weights;
use hl_core::matchdata::Team;
use hl_core::TfClass;
use std::collections::HashMap;

/// A kill line, reduced to what valuing it needs.
#[derive(Debug, Clone, Copy)]
pub struct KillCtx {
    pub killer: u32,
    pub assister: Option<u32>,
    pub victim_class: Option<TfClass>,
    /// The victim's colour at that moment.
    pub victim_team: Option<Team>,
    /// Counts as a kill (inside a round, not a feign death).
    pub counts: bool,
    /// Its assist counts (inside a round; logs.tf credits feign-death assists).
    pub assist_counts: bool,
}

/// Summed kill values for one player, before any per-minute scaling.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Impact {
    pub kills: f64,
    pub assists: f64,
}

/// A victim whose class the log never named is valued like a Scout, the
/// reference class.
const UNKNOWN_VICTIM: f64 = 1.0;

/// On attack/defence maps RED defends: BLU pushes the cart or takes the points.
/// Stopwatch swaps the colours between halves, and each kill carries the
/// colour of that moment, so this holds in both halves.
pub fn defending(team: Option<Team>) -> bool {
    team == Some(Team::Red)
}

pub fn value(k: &KillCtx, map: Option<&str>, w: &Weights) -> f64 {
    match k.victim_class {
        Some(c) => w.victim_in(c, map, defending(k.victim_team)),
        None => UNKNOWN_VICTIM,
    }
}

/// Per player: the value of their kills and of their assists. Each kill comes
/// with the map it happened on: in a log combined from several maps, the
/// round's map, not the log's name.
pub fn impacts<'a>(kills: impl IntoIterator<Item = (KillCtx, Option<&'a str>)>, w: &Weights) -> HashMap<u32, Impact> {
    let mut out: HashMap<u32, Impact> = HashMap::new();
    for (k, map) in kills {
        let k = &k;
        let v = value(k, map, w);
        if k.counts {
            out.entry(k.killer).or_default().kills += v;
        }
        if let (true, Some(a)) = (k.assist_counts, k.assister) {
            out.entry(a).or_default().assists += v;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kill(killer: u32, class: TfClass, team: Team) -> KillCtx {
        KillCtx {
            killer,
            assister: Some(9),
            victim_class: Some(class),
            victim_team: Some(team),
            counts: true,
            assist_counts: true,
        }
    }

    #[test]
    fn a_defending_engineer_is_worth_more_on_payload_only() {
        let w = Weights::default_weights();
        let ks = [kill(1, TfClass::Engineer, Team::Red), kill(1, TfClass::Engineer, Team::Blue)];
        let pl = impacts(ks.iter().map(|k| (*k, Some("pl_upward_f12"))), &w)[&1].kills;
        let koth = impacts(ks.iter().map(|k| (*k, Some("koth_product_final"))), &w)[&1].kills;
        let general = w.victim(TfClass::Engineer);
        assert!(w.victim_in(TfClass::Engineer, Some("pl_upward_f12"), true) > general);
        assert_eq!(koth, 2.0 * general);
        assert!(pl > koth);
    }

    #[test]
    fn each_kill_is_valued_on_its_own_map() {
        // One log, two maps: the defending Engineer counts extra on Vigil only.
        let w = Weights::default_weights();
        let k = kill(1, TfClass::Engineer, Team::Red);
        let i = impacts([(k, Some("pl_vigil_rc10")), (k, Some("koth_proot_b5b"))], &w);
        let general = w.victim(TfClass::Engineer);
        assert_eq!(i[&1].kills, w.victim_in(TfClass::Engineer, Some("pl_vigil_rc10"), true) + general);
    }

    #[test]
    fn feign_deaths_count_for_the_assister_only() {
        let w = Weights::default_weights();
        let mut k = kill(1, TfClass::Spy, Team::Blue);
        k.counts = false;
        let i = impacts([(k, None)], &w);
        assert!(!i.contains_key(&1));
        assert_eq!(i[&9].assists, w.victim(TfClass::Spy));
    }
}
