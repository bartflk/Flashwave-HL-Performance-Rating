//! The rating model: per-class components, scored as percentiles.
//!
//! A performance is one player's time on their main class in one match. Each
//! component (impact kills, sniper duel, deaths, ...) is compared against every
//! other performance on that class in the stored history, giving a percentile.
//! The rating is the weighted average of those percentiles, put on the scale
//! HLTV uses: 1.00 is an average game and a standard deviation is 0.25.
//!
//! Which components, and in what proportions, is one model per class and
//! lives in `weights.default.toml` — every one of the nine fitted against
//! who won and cross-validated (Q8). The file explains each of them.
//!
//! Only a player's **main class** is rated. `classkills` is recorded per
//! player, not per class played, so a flexer's kills cannot be split across
//! classes honestly. Deaths and damage are the main class's, over its minutes;
//! every whole-player total is read over the whole game (see `extract`).

use crate::impact::Impact;
use crate::weights::Weights;
use hl_core::matchdata::{LogFlags, PlayerLine};
use hl_core::TfClass;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// v2: Sniper reweighted against match results (DPM up, the duel and
/// headshot share down), with opening duels and untraded kills added.
/// v3: Sniper deaths in context: untraded deaths, deaths to flankers.
/// v4: Sniper Fight KAST.
/// v5: Sniper kills valued by the situation: a clean-up counts for less.
/// v6: the score is an HLTV-style rating around 1.00, not a 0-100 percentile.
/// v7: a model per class (Q8), each fitted against who won and
/// cross-validated, in place of six classes sharing one generic model.
/// v9: seven models refitted on the league sample (Q39): ~4,650 matchups a
/// class, 3,900 of them ETF2L officials.
/// v10: the Spy reweighted with the Spy mains: medic picks up, caps and
/// backstabs out.
/// v11: the cap's respawn cost out of every class; Medic ubers, Heavy kills
/// in context and Spy deaths out (the class-by-class review).
/// v12: round impact is half of every model; deaths count half as much
/// outside the Medic's, their weight going to kills and damage (Flashy).
/// v13: deaths on payload and attack/defend maps weighed apart; a Sniper's
/// there count a third of a KOTH death (measured: under a kill each, a KOTH
/// death about 1.4).
/// v14: the audit's rating fixes (RA-1 to RA-15): a player who switched class
/// is rated on whole-game numbers over the whole game; the Medic's model
/// carries less round impact and less dying; every class (and every game with
/// or without a server log) is put on the 1.00 scale by its own mean and
/// spread; Season MVP counts the stages once. Stored ratings are re-rated.
pub const MODEL_VERSION: &str = "v14";

/// How far one standard deviation moves the rating.
///
/// HLTV's own rating counts "how many standard deviations the player is above
/// or below average" and centres that on 1.00. The only free choice is how
/// wide a deviation is, and it is picked to make the numbers mean what they
/// mean over there: at 0.25 the best of this database's 54 regulars averages
/// 1.24 over a career, against ZywOo's 1.27 and s1mple's 1.23, and a single
/// game runs from about 0.60 at the fifth percentile to 1.64 at its best.
pub const RATING_SPREAD: f64 = 0.25;

/// Where the middle of the pool is, and how spread out it is.
///
/// Built from every rated performance once the pool exists, and stored, so
/// that a game rated on its own months later lands on the same scale as the
/// games it is listed beside.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub mean: f64,
    pub sd: f64,
}

impl Rating {
    /// The same rating with its score on the 1.00 scale.
    pub fn scaled(mut self, scale: &Scale) -> Rating {
        self.score = scale.rating(self.score);
        self
    }

    /// The same, on the scale of this rating's own class and kind of log.
    pub fn scaled_for(self, scales: &Scales) -> Rating {
        let scale = *scales.of_rating(&self);
        self.scaled(&scale)
    }

    /// Whether the server log's round impact is among the parts: a game with
    /// it is rated on a different mix, and spreads differently.
    pub fn has_round_impact(&self) -> bool {
        self.parts.iter().any(|p| p.component == Component::RoundImpact)
    }
}

/// A scale per class, and per kind of game, over the pooled one (RA-3).
///
/// One mean and spread for all nine classes favoured the classes whose models
/// have few correlated parts (a Sniper's percentiles spread wider than a
/// Medic's), and a game with no server log, rated on half a model, spreads
/// wider still. Each class's own scale puts a typical game of every class at
/// 1.00 and a deviation at the same distance; a class with too few games to
/// measure borrows the next wider scale.
#[derive(Debug, Clone, Default)]
pub struct Scales {
    pub pooled: Scale,
    /// `(class, with a server log)`; `None` for the class's games of both kinds.
    by: HashMap<(TfClass, Option<bool>), Scale>,
}

/// Fewest games a class's scale is measured from. Below it the next wider
/// scale is used: a spread drawn from a few dozen games moves too far on one.
pub const MIN_SCALE_POOL: usize = 200;

impl Scales {
    /// Measure every scale from `(class, has a server log, score)`; `None`
    /// when even the pooled scale cannot be measured.
    pub fn of(samples: &[(TfClass, bool, f64)]) -> Option<Scales> {
        let all: Vec<f64> = samples.iter().map(|s| s.2).collect();
        let pooled = Scale::of(&all)?;
        let mut groups: HashMap<(TfClass, Option<bool>), Vec<f64>> = HashMap::new();
        for &(class, log, score) in samples {
            groups.entry((class, None)).or_default().push(score);
            groups.entry((class, Some(log))).or_default().push(score);
        }
        let by = groups
            .into_iter()
            .filter(|(_, v)| v.len() >= MIN_SCALE_POOL)
            .filter_map(|(k, v)| Some((k, Scale::of(&v)?)))
            .collect();
        Some(Scales { pooled, by })
    }

    /// The scale for a game of `class`, with or without a server log.
    pub fn for_game(&self, class: TfClass, with_log: bool) -> &Scale {
        self.by.get(&(class, Some(with_log))).or_else(|| self.by.get(&(class, None))).unwrap_or(&self.pooled)
    }

    pub fn of_rating(&self, r: &Rating) -> &Scale {
        self.for_game(r.class, r.has_round_impact())
    }

    /// The measured scales for storing: `(class, log, mean, sd)`, `log` being
    /// `None` for the class's games of both kinds.
    pub fn stored(&self) -> Vec<(TfClass, Option<bool>, f64, f64)> {
        let mut v: Vec<_> = self.by.iter().map(|(&(c, l), s)| (c, l, s.mean, s.sd)).collect();
        v.sort_by_key(|(c, l, _, _)| (c.as_str(), *l));
        v
    }

    pub fn from_stored(pooled: Scale, rows: impl IntoIterator<Item = (TfClass, Option<bool>, f64, f64)>) -> Scales {
        Scales { pooled, by: rows.into_iter().map(|(c, l, mean, sd)| ((c, l), Scale { mean, sd })).collect() }
    }
}

impl Default for Scale {
    /// The shape a pool of weighted percentiles has before one is measured:
    /// centred halfway, a fifth of the range either side. Used only where
    /// there is no stored pool yet — a database on its first run, or a test.
    fn default() -> Self {
        Scale { mean: 50.0, sd: 20.0 }
    }
}

impl Scale {
    /// Measure the pool. `scores` are weighted percentiles, 0 to 100.
    pub fn of(scores: &[f64]) -> Option<Scale> {
        if scores.len() < 2 {
            return None;
        }
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let var = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / scores.len() as f64;
        let sd = var.sqrt();
        (sd > f64::EPSILON).then_some(Scale { mean, sd })
    }

    /// What one percentile point of a component is worth as a rating.
    pub fn per_percentile(&self) -> f64 {
        RATING_SPREAD / self.sd
    }

    /// A weighted percentile as a rating around 1.00.
    ///
    /// Floored at zero: a rating is a multiple of average output, and there is
    /// no such thing as less than none of it. Nothing caps the top, the same
    /// way nothing caps a 2.00 game on HLTV.
    pub fn rating(&self, score: f64) -> f64 {
        round2((1.0 + (score - self.mean) / self.sd * RATING_SPREAD).max(0.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    ImpactKills,
    ImpactAssists,
    MedicPicks,
    Duel,
    HeadshotShare,
    Backstabs,
    Heal,
    Ubers,
    Drops,
    Caps,
    Deaths,
    Dpm,
    /// Opening duels: first kills of fights got, less those died to.
    Opening,
    /// Share of kills not traded straight back.
    Untraded,
    /// Deaths the team did not trade within 3 s (PLAN §12 step 1).
    UntradedDeaths,
    /// Deaths to a Scout, Spy or Soldier.
    FlankDeaths,
    /// Deaths near a spot already killed from twice in the same life.
    StationaryDeaths,
    /// Share of fights with a kill, assist, survival or traded death (PLAN §12 step 2).
    FightKast,
    /// The same, with survival counted only when the player fired in the fight.
    FightKastEngaged,
    /// Impact kills, each also scaled by its situation (PLAN §12 step 3).
    SituationKills,
    /// Win chance the player's kills added up, in points (PLAN §12 step 5).
    /// Where `SituationKills` scales a kill's value by its state, this *is*
    /// the state's value, measured and unnormalised: a kill at even numbers
    /// is worth 0.19 and a clean-up at four up 0.05.
    FightSwing,
    /// Captures weighted by the enemies alive to stop them (Q17, ivg).
    CapsContested,
    /// Captures weighted by the capper's own team dead at the time (Q25,
    /// ivg). Lower is taken as better, which is ivg's reading of the
    /// respawn rule; `hl validate` says whether the data agrees.
    CapsMatesDead,
    /// Fight swing with each kill shared among whoever damaged the victim in
    /// the five seconds before (Q6b), not given whole to the last shot.
    FightSwingShared,
    /// Q25 reopened (§18b, boSe and ivg): seconds of missed respawn waves
    /// the player's caps cost their own dead. Lower is taken as better;
    /// `hl situation --spawn-delay` found long delays go with losing.
    CapsSpawnDelay,
    /// Round impact (v12): the round-win chance the player's kills added and
    /// their deaths gave away, in rounds per 10 minutes (round_impact.rs).
    RoundImpact,
    /// Deaths on a payload or attack/defend map (v13). `Deaths` is the same
    /// count on the symmetric maps (KOTH, 5CP): a class can weigh the two
    /// apart, as the Sniper's measured cost of a death does (Flashy: "on
    /// attack you should be dying and taking risks").
    DeathsAttackDefend,
}

impl Component {
    pub const ALL: [Component; 27] = [
        Component::ImpactKills,
        Component::ImpactAssists,
        Component::MedicPicks,
        Component::Duel,
        Component::HeadshotShare,
        Component::Backstabs,
        Component::Heal,
        Component::Ubers,
        Component::Drops,
        Component::Caps,
        Component::Deaths,
        Component::Dpm,
        Component::Opening,
        Component::Untraded,
        Component::UntradedDeaths,
        Component::FlankDeaths,
        Component::StationaryDeaths,
        Component::FightKast,
        Component::FightKastEngaged,
        Component::SituationKills,
        Component::FightSwing,
        Component::CapsContested,
        Component::CapsMatesDead,
        Component::FightSwingShared,
        Component::CapsSpawnDelay,
        Component::RoundImpact,
        Component::DeathsAttackDefend,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Component::ImpactKills => "impact_kills",
            Component::ImpactAssists => "impact_assists",
            Component::MedicPicks => "medic_picks",
            Component::Duel => "duel",
            Component::HeadshotShare => "headshot_share",
            Component::Backstabs => "backstabs",
            Component::Heal => "heal",
            Component::Ubers => "ubers",
            Component::Drops => "drops",
            Component::Caps => "caps",
            Component::Deaths => "deaths",
            Component::Dpm => "dpm",
            Component::Opening => "opening",
            Component::Untraded => "untraded",
            Component::UntradedDeaths => "untraded_deaths",
            Component::FlankDeaths => "flank_deaths",
            Component::StationaryDeaths => "stationary_deaths",
            Component::FightKast => "fight_kast",
            Component::FightKastEngaged => "fight_kast_engaged",
            Component::SituationKills => "situation_kills",
            Component::FightSwing => "fight_swing",
            Component::CapsContested => "caps_contested",
            Component::CapsMatesDead => "caps_mates_dead",
            Component::FightSwingShared => "fight_swing_shared",
            Component::CapsSpawnDelay => "caps_spawn_delay",
            Component::RoundImpact => "round_impact",
            Component::DeathsAttackDefend => "deaths_attack_defend",
        }
    }

    pub fn parse(s: &str) -> Option<Component> {
        Component::ALL.into_iter().find(|c| c.key() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Component::ImpactKills => "Impact kills",
            Component::ImpactAssists => "Impact assists",
            Component::MedicPicks => "Medic picks",
            Component::Duel => "Sniper duel",
            Component::HeadshotShare => "Headshot share",
            Component::Backstabs => "Backstabs",
            Component::Heal => "Healing",
            Component::Ubers => "Ubers",
            Component::Drops => "Drops",
            Component::Caps => "Caps",
            Component::Deaths => "Deaths",
            Component::Dpm => "Damage / min",
            Component::Opening => "Opening duels",
            Component::Untraded => "Kills not traded",
            Component::UntradedDeaths => "Untraded deaths",
            Component::FlankDeaths => "Deaths to flankers",
            Component::StationaryDeaths => "Stationary deaths",
            Component::FightKast => "Fight KAST",
            Component::FightKastEngaged => "Fight KAST, engaged",
            Component::SituationKills => "Kills in context",
            Component::FightSwing => "Fight swing",
            Component::CapsContested => "Caps into a defence",
            Component::CapsMatesDead => "Caps with your team dead",
            Component::FightSwingShared => "Fight swing, shared",
            Component::CapsSpawnDelay => "Spawns delayed by your caps",
            Component::RoundImpact => "Round impact",
            Component::DeathsAttackDefend => "Deaths (attack/defend maps)",
        }
    }

    /// The unit the raw value is shown in.
    pub fn unit(self) -> &'static str {
        match self {
            Component::HeadshotShare | Component::Untraded => "% of kills",
            Component::FightKast | Component::FightKastEngaged => "% of fights",
            Component::Heal | Component::Dpm => "per min",
            Component::Opening => "net per 10 min",
            Component::FightSwing => "win % per 10 min",
            Component::CapsContested => "defenders per 10 min",
            Component::CapsMatesDead => "teammates dead per 10 min",
            Component::FightSwingShared => "win % per 10 min",
            Component::CapsSpawnDelay => "seconds per 10 min",
            Component::RoundImpact => "rounds won per 10 min",
            _ => "per 10 min",
        }
    }

    /// Fewer deaths and fewer drops are better; everything else, more is.
    pub fn higher_is_better(self) -> bool {
        !matches!(
            self,
            Component::Deaths
                | Component::DeathsAttackDefend
                | Component::Drops
                | Component::UntradedDeaths
                | Component::FlankDeaths
                | Component::StationaryDeaths
                | Component::CapsMatesDead
                | Component::CapsSpawnDelay
        )
    }
}

/// One rateable performance and its raw component values.
#[derive(Debug, Clone)]
pub struct Performance {
    pub account_id: u32,
    pub class: TfClass,
    pub minutes: f64,
    pub values: Vec<(Component, f64)>,
    /// The map this was played on, without its version — `upward`. `None`
    /// for a log whose rounds span several maps, which cannot belong to one
    /// pool; those are measured against every map together.
    pub map: Option<String>,
}

/// Raw component values for `player` on their main class, or `None` when they
/// did not play it long enough to rate.
///
/// `impact` is the player's kills valued one by one from the raw log (victim
/// class, map, side). Without it, impact falls back to `classkills` at the
/// general values.
pub fn extract(
    player: &PlayerLine,
    flags: &LogFlags,
    w: &Weights,
    impact: Option<&Impact>,
    map: Option<&str>,
) -> Option<Performance> {
    let class = player.main_class()?;
    let line = player.classes.iter().find(|c| c.class == class)?;
    let minutes = line.time_s as f64 / 60.0;
    if minutes < w.general.min_minutes {
        return None;
    }
    // Two clocks (RA-1). What logs.tf and the server log count per class --
    // deaths and damage -- is read over the main class's minutes. Everything
    // else is a whole-player total (kills, assists, heals, ubers, caps, the
    // fights pass, round impact) and is read over the whole game, so twelve
    // minutes of Spy and ten of Sniper are not rated as twenty-two minutes of
    // kills in twelve.
    let per10 = 10.0 / minutes;
    let total_minutes = (player.total_time() as f64 / 60.0).max(minutes);
    let all10 = 10.0 / total_minutes;
    let s = &player.stats;
    let vs = |c: TfClass| player.vs.iter().find(|v| v.other_class == c);

    // Headshots are the whole player's, so they are shared over all their kills.
    let kills_all: i64 = player.classes.iter().map(|c| c.kills).sum();
    let mut values = Vec::new();
    for (component, _) in w.model_for(class) {
        let v = match component {
            Component::ImpactKills => Some(
                impact.map(|i| i.kills).unwrap_or_else(|| {
                    player.vs.iter().map(|v| v.kills as f64 * w.victim(v.other_class)).sum::<f64>()
                }) * all10,
            ),
            Component::ImpactAssists => Some(
                impact.map(|i| i.assists).unwrap_or_else(|| {
                    player.vs.iter().map(|v| v.assists as f64 * w.victim(v.other_class)).sum::<f64>()
                }) * w.general.assist_share
                    * all10,
            ),
            Component::MedicPicks => Some(vs(TfClass::Medic).map_or(0, |v| v.kills) as f64 * all10),
            Component::Duel => {
                let v = vs(TfClass::Sniper);
                let diff = v.map_or(0, |v| v.kills) - v.map_or(0, |v| v.deaths);
                Some(diff as f64 * all10)
            }
            // Undefined without kills, and meaningless where the log did not
            // record headshots: skipped rather than scored as zero.
            Component::HeadshotShare => {
                (flags.hs && kills_all > 0).then(|| s.headshots as f64 / kills_all as f64 * 100.0)
            }
            Component::Backstabs => flags.bs.then_some(s.backstabs as f64 * all10),
            Component::Heal => Some(s.heal as f64 / total_minutes),
            Component::Ubers => Some(s.ubers as f64 * all10),
            Component::Drops => Some(s.drops as f64 * all10),
            Component::Caps => flags.cp.then_some(s.cpc as f64 * all10),
            // v13: one count, read under one name or the other by the map's
            // mode, so a class can weigh payload deaths apart from KOTH ones.
            Component::Deaths => (!flags.attack_defend).then_some(line.deaths as f64 * per10),
            Component::DeathsAttackDefend => flags.attack_defend.then_some(line.deaths as f64 * per10),
            Component::Dpm => Some(line.dmg as f64 / minutes),
            // Both need the fights pass, so a log without a raw log skips
            // them and the other weights take up the slack.
            Component::Opening => {
                impact.and_then(|i| i.fights).map(|f| (f64::from(f.opening_kills) - f64::from(f.opening_deaths)) * all10)
            }
            Component::Untraded => impact
                .and_then(|i| i.fights)
                .filter(|f| f.kills > 0)
                .map(|f| (1.0 - f64::from(f.traded_kills) / f64::from(f.kills)) * 100.0),
            Component::UntradedDeaths => impact
                .and_then(|i| i.fights)
                .map(|f| f64::from(f.deaths.saturating_sub(f.traded_deaths)) * all10),
            Component::FlankDeaths => impact.and_then(|i| i.fights).map(|f| f64::from(f.flank_deaths) * all10),
            Component::StationaryDeaths => impact.and_then(|i| i.fights).map(|f| f64::from(f.stationary_deaths) * all10),
            Component::FightKast => impact
                .and_then(|i| i.fights)
                .filter(|f| f.fights_present > 0)
                .map(|f| f64::from(f.fights_kast) / f64::from(f.fights_present) * 100.0),
            Component::FightKastEngaged => impact
                .and_then(|i| i.fights)
                .filter(|f| f.fights_present > 0)
                .map(|f| f64::from(f.fights_kast_engaged) / f64::from(f.fights_present) * 100.0),
            // Without a raw log there is no situation: plain impact kills.
            Component::SituationKills => Some(
                impact.map(|i| i.kills_situation).unwrap_or_else(|| {
                    player.vs.iter().map(|v| v.kills as f64 * w.victim(v.other_class)).sum::<f64>()
                }) * all10,
            ),
            // Measured from the raw log or not at all: without one there is
            // no state to value a kill in, so the component is missing and
            // the other weights take up the slack.
            Component::FightSwing => {
                impact.and_then(|i| i.swing).map(|s| s * 100.0 * all10)
            }
            // Both only exist where the raw log was read, and only mean
            // anything on a map with points, like `Caps` itself.
            Component::CapsContested => flags
                .cp
                .then(|| impact.and_then(|i| i.fights))
                .flatten()
                .map(|f| f64::from(f.caps_contested) * all10),
            Component::CapsMatesDead => flags
                .cp
                .then(|| impact.and_then(|i| i.fights))
                .flatten()
                .map(|f| f64::from(f.caps_mates_dead) * all10),
            // KOTH and 5CP only. On payload and attack/defend a capture
            // moves the attackers' spawn up, so the wait it adds to their
            // dead is the mode working, not a cost (Flashy): left out, like
            // on a map with no points.
            Component::CapsSpawnDelay => (flags.cp && !flags.attack_defend)
                .then(|| impact.and_then(|i| i.fights))
                .flatten()
                .map(|f| f64::from(f.caps_spawn_delay) * all10),
            Component::FightSwingShared => impact.and_then(|i| i.swing_shared).map(|s| s * 100.0 * all10),
            // Needs the server log: without one the other half of the model
            // carries the rating, as for every server-log part.
            Component::RoundImpact => impact.and_then(|i| i.round).map(|r| r * all10),
        };
        if let Some(v) = v {
            values.push((*component, v));
        }
    }

    Some(Performance {
        account_id: player.id.account_id(),
        class,
        minutes,
        values,
        map: map.map(str::to_string),
    })
}

/// Sorted component values per class: the pool every rating is measured against.
#[derive(Debug, Clone, Default)]
pub struct Baseline {
    by: HashMap<(TfClass, Component), Vec<f64>>,
    /// The same, per map. A Sniper game on Vigil averages 0.90 and on Product
    /// 1.10 — three quarters of a deviation of difference that is the map's,
    /// not the player's. Measured September 2026 over 13,568 performances.
    by_map: HashMap<(TfClass, Component, String), Vec<f64>>,
}

/// A per-map pool smaller than this is not a pool, it is a handful of games,
/// and a percentile drawn from it moves too far on one result. Measured: at
/// 100 every map of the current ETF2L pool qualifies for every class
/// (product 217, vigil 215, upward 194, ashville 116, steel 115, proot 105)
/// and the older maps fall back, which is the right way round.
pub const MIN_MAP_POOL: usize = 100;

impl Baseline {
    /// Build from performances, leaving out `exclude` — the owner — so they are
    /// measured against the players they face rather than against themselves.
    pub fn build<'a>(perfs: impl IntoIterator<Item = &'a Performance>, exclude: Option<u32>) -> Self {
        let mut by: HashMap<(TfClass, Component), Vec<f64>> = HashMap::new();
        let mut by_map: HashMap<(TfClass, Component, String), Vec<f64>> = HashMap::new();
        for p in perfs {
            if Some(p.account_id) == exclude {
                continue;
            }
            for &(c, v) in &p.values {
                if !v.is_finite() {
                    continue;
                }
                by.entry((p.class, c)).or_default().push(v);
                // Every game is in the general pool as well as its map's, so
                // the fallback is a real pool and not the leftovers.
                if let Some(m) = &p.map {
                    by_map.entry((p.class, c, m.clone())).or_default().push(v);
                }
            }
        }
        for vals in by.values_mut() {
            vals.sort_by(f64::total_cmp);
        }
        by_map.retain(|_, v| v.len() >= MIN_MAP_POOL);
        for vals in by_map.values_mut() {
            vals.sort_by(f64::total_cmp);
        }
        Baseline { by, by_map }
    }

    pub fn from_parts(parts: impl IntoIterator<Item = (TfClass, Component, Option<String>, Vec<f64>)>) -> Self {
        let mut by = HashMap::new();
        let mut by_map = HashMap::new();
        for (class, c, map, mut vals) in parts {
            vals.sort_by(f64::total_cmp);
            match map {
                Some(m) => {
                    by_map.insert((class, c, m), vals);
                }
                None => {
                    by.insert((class, c), vals);
                }
            }
        }
        Baseline { by, by_map }
    }

    /// Every stored pool: `None` for the general one, `Some(map)` for a map's.
    pub fn parts(&self) -> impl Iterator<Item = (TfClass, Component, Option<&str>, &[f64])> {
        self.by
            .iter()
            .map(|((class, c), v)| (*class, *c, None, v.as_slice()))
            .chain(
                self.by_map
                    .iter()
                    .map(|((class, c, m), v)| (*class, *c, Some(m.as_str()), v.as_slice())),
            )
    }

    /// Which pool a rating on this map and class is measured against, and how
    /// big it is: `("vigil", 215)`, or `(None, 1502)` when the map has too few
    /// games of its own.
    pub fn pool_for(&self, class: TfClass, map: Option<&str>) -> (Option<&str>, usize) {
        if let Some(m) = map {
            let n = self
                .by_map
                .iter()
                .filter(|((c, _, mm), _)| *c == class && mm == m)
                .map(|(_, v)| v.len())
                .max()
                .unwrap_or(0);
            if n >= MIN_MAP_POOL {
                // The key is owned by the map; hand back the stored spelling.
                let key = self.by_map.keys().find(|(c, _, mm)| *c == class && mm == m);
                return (key.map(|(_, _, mm)| mm.as_str()), n);
            }
        }
        (None, self.pool_size(class))
    }

    pub fn is_empty(&self) -> bool {
        self.by.is_empty()
    }

    /// Size of the pool for a class (its largest component).
    pub fn pool_size(&self, class: TfClass) -> usize {
        self.by
            .iter()
            .filter(|((c, _), _)| *c == class)
            .map(|(_, v)| v.len())
            .max()
            .unwrap_or(0)
    }

    /// Mid-rank percentile in 0..=1: the share of the pool below `value`,
    /// counting ties as half. `None` when there is no pool to compare against.
    /// Where `value` sits in the pool for this class, component and map.
    ///
    /// The map's own pool when it has one — `by_map` only ever holds pools
    /// that cleared [`MIN_MAP_POOL`] — and every map together otherwise.
    pub fn percentile(
        &self,
        class: TfClass,
        c: Component,
        map: Option<&str>,
        value: f64,
    ) -> Option<f64> {
        let vals = map
            .and_then(|m| self.by_map.get(&(class, c, m.to_string())))
            .or_else(|| self.by.get(&(class, c)))
            .filter(|v| !v.is_empty())?;
        let below = vals.partition_point(|x| *x < value);
        let not_above = vals.partition_point(|x| *x <= value);
        Some((below as f64 + (not_above - below) as f64 / 2.0) / vals.len() as f64)
    }
}

/// A rating with its working shown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rating {
    pub class: TfClass,
    /// An HLTV-style rating: 1.00 is the pool's average game, 1.40 a strong
    /// one, 0.60 a poor one. Straight out of [`rate`] this is still the
    /// weighted average of the component percentiles, 0 to 100; [`Scale`]
    /// turns it into a rating once the pool is known.
    pub score: f64,
    pub minutes: f64,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub component: Component,
    pub label: String,
    pub unit: String,
    pub raw: f64,
    /// 0-100, already flipped for lower-is-better components, so higher is
    /// always better here.
    pub percentile: f64,
    /// Share of the rating, after renormalising over the components present.
    pub weight: f64,
}

pub fn rate(perf: &Performance, baseline: &Baseline, w: &Weights) -> Option<Rating> {
    let weights: HashMap<Component, f64> = w.model_for(perf.class).iter().copied().collect();

    let mut parts = Vec::new();
    // The unrounded percentile of each part, for the score: only what is
    // shown is rounded (RA-7). Rounded weights summed to 0.99 for a Scout or
    // a Spy and took a hundredth off every game.
    let mut exact = Vec::new();
    for &(c, raw) in &perf.values {
        // A value that is not a number has no place in a pool and none in a
        // rating: left to `percentile` it would count as the lowest of all,
        // which for a lower-is-better stat is the best.
        if !raw.is_finite() {
            continue;
        }
        let Some(&weight) = weights.get(&c).filter(|w| **w > 0.0) else { continue };
        let Some(p) = baseline.percentile(perf.class, c, perf.map.as_deref(), raw) else { continue };
        let p = if c.higher_is_better() { p } else { 1.0 - p };
        exact.push(p * 100.0);
        parts.push(Part {
            component: c,
            label: c.label().to_string(),
            unit: c.unit().to_string(),
            raw: round2(raw),
            percentile: round1(p * 100.0),
            weight,
        });
    }

    let total: f64 = parts.iter().map(|p| p.weight).sum();
    if total <= 0.0 {
        return None;
    }
    let score = parts.iter().zip(&exact).map(|(p, x)| x * p.weight / total).sum::<f64>();
    for p in &mut parts {
        p.weight = round2(p.weight / total);
    }

    Some(Rating {
        class: perf.class,
        score: round1(score),
        minutes: round1(perf.minutes),
        parts,
    })
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod scale_tests {
    use super::*;

    #[test]
    fn the_middle_of_the_pool_rates_one() {
        let pool: Vec<f64> = (0..=100).map(f64::from).collect();
        let s = Scale::of(&pool).unwrap();
        assert_eq!(s.rating(50.0), 1.00, "the average game is 1.00");
        assert!(s.rating(80.0) > 1.0 && s.rating(20.0) < 1.0);
        // Symmetric either side, because the mapping is a straight line.
        assert!(((s.rating(80.0) - 1.0) + (s.rating(20.0) - 1.0)).abs() < 0.001);
    }

    #[test]
    fn a_deviation_is_worth_what_it_says_it_is() {
        let s = Scale { mean: 50.0, sd: 20.0 };
        assert_eq!(s.rating(70.0), 1.0 + RATING_SPREAD, "one deviation up");
        assert_eq!(s.rating(30.0), 1.0 - RATING_SPREAD, "and down");
        assert_eq!(s.rating(110.0), round2(1.0 + 3.0 * RATING_SPREAD), "nothing caps the top");
    }

    #[test]
    fn a_rating_never_goes_below_nothing() {
        // A pool so tight that an ordinary game is ten deviations out.
        let s = Scale { mean: 50.0, sd: 1.0 };
        assert_eq!(s.rating(10.0), 0.0);
    }

    #[test]
    fn a_pool_that_cannot_be_measured_is_refused() {
        assert!(Scale::of(&[]).is_none());
        assert!(Scale::of(&[42.0]).is_none(), "one game says nothing about spread");
        assert!(Scale::of(&[7.0; 20]).is_none(), "nor does a pool with no spread at all");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perf(account_id: u32, v: f64) -> Performance {
        Performance {
            map: None,
            account_id,
            class: TfClass::Sniper,
            minutes: 30.0,
            // The Sniper model weighs untraded deaths, not all deaths (v3).
            values: vec![(Component::UntradedDeaths, v), (Component::ImpactKills, v)],
        }
    }

    #[test]
    fn percentile_is_mid_rank() {
        let pool: Vec<_> = (1..=4).map(|i| perf(i, i as f64)).collect();
        let b = Baseline::build(&pool, None);
        // 2.0 sits above 1 value and ties 1: (1 + 0.5) / 4.
        assert_eq!(b.percentile(TfClass::Sniper, Component::ImpactKills, None, 2.0), Some(0.375));
        assert_eq!(b.percentile(TfClass::Sniper, Component::ImpactKills, None, 0.0), Some(0.0));
        assert_eq!(b.percentile(TfClass::Sniper, Component::ImpactKills, None, 9.0), Some(1.0));
    }

    #[test]
    fn the_owner_is_left_out_of_the_pool() {
        let pool = vec![perf(1, 1.0), perf(2, 2.0), perf(99, 50.0)];
        let b = Baseline::build(&pool, Some(99));
        assert_eq!(b.pool_size(TfClass::Sniper), 2);
    }

    #[test]
    fn fewer_deaths_rate_higher() {
        let w = Weights::default_weights();
        let pool: Vec<_> = (1..=10).map(|i| perf(i, i as f64)).collect();
        let b = Baseline::build(&pool, None);
        let few = Performance { values: vec![(Component::UntradedDeaths, 1.0)], ..perf(0, 0.0) };
        let many = Performance { values: vec![(Component::UntradedDeaths, 10.0)], ..perf(0, 0.0) };
        let few = rate(&few, &b, &w).unwrap().score;
        let many = rate(&many, &b, &w).unwrap().score;
        assert!(few > many, "few deaths {few} should beat many deaths {many}");
    }

    #[test]
    fn missing_components_renormalise_instead_of_scoring_zero() {
        let w = Weights::default_weights();
        let pool: Vec<_> = (1..=10).map(|i| perf(i, i as f64)).collect();
        let b = Baseline::build(&pool, None);
        let r = rate(&perf(0, 10.0), &b, &w).unwrap();
        let total: f64 = r.parts.iter().map(|p| p.weight).sum();
        assert!((total - 1.0).abs() < 0.02, "weights renormalise to 1, got {total}");
    }

    use hl_core::matchdata::{ClassLine, PlayerStats, Team, VsLine};
    use hl_core::SteamId;

    fn class_line(class: TfClass, secs: i64, deaths: i64) -> ClassLine {
        ClassLine { class, time_s: secs, kills: 0, assists: 0, deaths, dmg: 0, shots: 0, hits: 0 }
    }

    fn player(classes: Vec<ClassLine>) -> PlayerLine {
        PlayerLine {
            id: SteamId::from_account_id(7),
            name: None,
            team: Team::Red,
            stats: PlayerStats::default(),
            classes,
            vs: vec![VsLine { other_class: TfClass::Scout, kills: 11, deaths: 0, assists: 0 }],
        }
    }

    fn value_of(p: &Performance, c: Component) -> f64 {
        p.values.iter().find(|(x, _)| *x == c).map(|(_, v)| *v).unwrap()
    }

    #[test]
    fn a_class_switchers_totals_are_read_over_the_whole_game() {
        // RA-1: 12 minutes of Soldier and 10 of Sniper, 11 Scout kills in all.
        // The kills are the whole game's, so they are per 22 minutes, not per
        // the Soldier's 12 (which rated them 1.8 times too high).
        let w = Weights::default_weights();
        let flags = LogFlags::default();
        let switcher = player(vec![class_line(TfClass::Soldier, 12 * 60, 6), class_line(TfClass::Sniper, 10 * 60, 3)]);
        let perf = extract(&switcher, &flags, &w, None, None).unwrap();
        assert_eq!(perf.class, TfClass::Soldier);
        let per10 = 11.0 * w.victim(TfClass::Scout) * 10.0 / 22.0;
        assert!((value_of(&perf, Component::ImpactKills) - per10).abs() < 1e-9);
        // Deaths are the class's own, over the class's minutes.
        assert!((value_of(&perf, Component::Deaths) - 6.0 * 10.0 / 12.0).abs() < 1e-9);
        // A player who stayed on the Soldier for all 22 minutes is rated alike.
        let stayer = extract(&player(vec![class_line(TfClass::Soldier, 22 * 60, 6)]), &flags, &w, None, None).unwrap();
        assert!((value_of(&stayer, Component::ImpactKills) - per10).abs() < 1e-9);
    }

    #[test]
    fn scores_come_from_unrounded_weights() {
        // RA-7: a Scout's displayed weights round to 0.99, which took a
        // hundredth off every Scout game. Every part at the top of its pool
        // must score exactly 100.
        let w = Weights::default_weights();
        let model: Vec<(Component, f64)> =
            w.model_for(TfClass::Scout).iter().copied().filter(|(c, _)| *c != Component::DeathsAttackDefend).collect();
        let at = |x: f64| model.iter().map(|(c, _)| (*c, x)).collect::<Vec<_>>();
        let pool: Vec<Performance> = (0..4).map(|i| Performance { class: TfClass::Scout, values: at(10.0 + i as f64), ..perf(i, 0.0) }).collect();
        let b = Baseline::build(&pool, None);
        // Better than the whole pool on every part: high where more is better, low where less is.
        let values = model.iter().map(|(c, _)| (*c, if c.higher_is_better() { 1e6 } else { -1e6 })).collect();
        let top = Performance { class: TfClass::Scout, values, ..perf(99, 0.0) };
        let r = rate(&top, &b, &w).unwrap();
        assert_eq!(r.score, 100.0);
        let shown: f64 = r.parts.iter().map(|p| p.weight).sum();
        assert!(shown < 1.0, "the displayed weights still round ({shown}); the score no longer depends on it");
    }

    #[test]
    fn every_class_is_scaled_by_its_own_spread() {
        // RA-3: Snipers' scores spread over 0..100, Medics' over 40..60. A
        // Sniper at 90 and a Medic at 55 are each one deviation up in their
        // own class and rate alike; on one shared scale the Medic never
        // leaves the middle.
        let mut samples = Vec::new();
        for i in 0..MIN_SCALE_POOL {
            let x = i as f64 / (MIN_SCALE_POOL - 1) as f64;
            samples.push((TfClass::Sniper, true, x * 100.0));
            samples.push((TfClass::Medic, true, 40.0 + x * 20.0));
        }
        let s = Scales::of(&samples).unwrap();
        let sniper = s.for_game(TfClass::Sniper, true);
        let medic = s.for_game(TfClass::Medic, true);
        assert!(sniper.sd > 2.0 * medic.sd);
        let (a, b) = (sniper.rating(50.0 + sniper.sd), medic.rating(50.0 + medic.sd));
        assert!((a - b).abs() < 0.011, "{a} vs {b}");
        assert!(s.pooled.rating(55.0) < 1.1, "the shared scale leaves a strong Medic game near average");
        // A class with too few games borrows the pooled scale.
        assert_eq!(s.for_game(TfClass::Heavy, true), &s.pooled);
        // And a game without a server log has no scale of its own here, so it
        // takes the class's.
        assert_eq!(s.for_game(TfClass::Sniper, false).sd, s.for_game(TfClass::Sniper, true).sd);
    }

    #[test]
    fn stored_scales_come_back_as_they_went_in() {
        let mut samples = Vec::new();
        for i in 0..MIN_SCALE_POOL {
            samples.push((TfClass::Pyro, i % 2 == 0, i as f64 % 90.0));
        }
        let s = Scales::of(&samples).unwrap();
        let back = Scales::from_stored(s.pooled, s.stored());
        for log in [true, false] {
            assert_eq!(back.for_game(TfClass::Pyro, log), s.for_game(TfClass::Pyro, log));
        }
    }
}
