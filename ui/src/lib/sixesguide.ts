import { useSyncExternalStore } from "react";
import type { ComponentGuide } from "../api/types";
import type { GameFormat } from "./gameformat";
import { goTo } from "./goto";
import { t } from "./i18n";

/**
 * What each 6v6 component means, for "How ratings work" in 6v6. The backend
 * says which components a role is rated on and their share
 * (hl_ingest::sixes_guide); the words live here so they can be translated.
 */

type Group = ComponentGuide["group"];
export interface SixesComponentText {
  label: string;
  unit: string;
  group: Group;
  description: string;
}

export function sixesComponent(key: string): SixesComponentText {
  const c = (label: string, unit: string, group: Group, description: string) => ({ label, unit, group, description });
  const per10 = t("per 10 min");
  switch (key) {
    case "round_impact":
      return c(t("Round impact"), per10, "teamplay", t("How much each of your kills and deaths changed your team's chance of winning the round, measured from who won the rounds of thousands of 6v6 officials."));
    case "impact_kills":
      return c(t("Kills"), per10, "fragging", t("Your kills. Once kill values are measured, each is worth what killing that class does for winning the round: a Medic counts for more than a Scout."));
    case "impact_assists":
      return c(t("Assists"), per10, "teamplay", t("Kills you helped a teammate get, with damage or healing."));
    case "deaths":
      return c(t("Deaths"), per10, "survival", t("How often you died."));
    case "untraded_deaths":
      return c(t("Untraded deaths"), per10, "survival", t("Deaths no teammate answered with a kill soon after: dying for nothing."));
    case "dpm":
      return c(t("Damage per minute"), t("dmg/min"), "fragging", t("Damage dealt per minute."));
    case "pressure":
      return c(t("Pressure"), t("dmg/min"), "fragging", t("Damage per minute on the off-class: how much the other team had to respect it."));
    case "dmg_ratio":
      return c(t("Damage ratio"), t("dealt per taken"), "fragging", t("Damage dealt for every point of damage taken."));
    case "medic_picks":
      return c(t("Medic picks"), per10, "fragging", t("Enemy Medics you killed."));
    case "opening":
      return c(t("Opening duels"), per10, "fragging", t("First kills of a fight: winning the duel that starts it."));
    case "space":
      return c(t("Space created"), t("of the team's damage taken"), "teamplay", t("The share of your team's damage you took, the Medic left out: going in first and drawing fire so others can play."));
    case "forces":
      return c(t("Forces"), per10, "teamplay", t("Making the other team use their Über or fall back. How to count it is still being agreed with the reviewers."));
    case "protect":
      return c(t("Protecting the Medic"), per10, "teamplay", t("Staying close to your Medic and killing the players who go for them."));
    case "backcaps":
      return c(t("Backcaps"), per10, "objective", t("Points taken behind the other team while they push."));
    case "caps_contested":
      return c(t("Caps contested"), per10, "objective", t("Capping or blocking a point, counted for the Medic only while he is übered or the last one alive: otherwise he should never be on it."));
    case "double_flank_deaths":
      return c(t("Both flanks down"), per10, "survival", t("Deaths with the team's other flank dying within 10 seconds. One flank down is fine while the other holds and gives the combo vision; both at once leaves the side open. Counted on 5CP after the middle point and on Product."));
    case "uber_picks":
      return c(t("Kills in an über"), per10, "speciality", t("Kills made during an über the pocket Scout took, the Medic's charge on him: picks and flashes."));
    case "deaths_band":
      return c(t("Deaths against the Medic's"), t("outside 1 to 8 more"), "survival", t("A pocket Scout dies more than his Medic, often saving him: 1 to 8 more deaths per 30 minutes is normal. Fewer, or far more, is marked down."));
    case "denies":
      return c(t("Denies"), per10, "objective", t("Kills that stopped the other team taking a point."));
    case "heal":
      return c(t("Healing per minute"), t("heal/min"), "medic", t("Healing given per minute."));
    case "ubers_vs":
      return c(t("Übers against theirs"), t("more than theirs"), "medic", t("Your Übers minus the other Medic's."));
    case "deaths_vs":
      return c(t("Deaths against theirs"), t("more than theirs"), "medic", t("Your deaths minus the other Medic's."));
    case "adv_lost":
      return c(t("Advantages lost"), t("per game"), "medic", t("Times your Über advantage over the other Medic was lost, as logs.tf counts it."));
    case "traps":
      return c(t("Sticky kills"), per10, "speciality", t("Kills with stickybombs: traps and detonations."));
    case "sentry":
      return c(t("Sentry kills"), per10, "speciality", t("Kills by your sentry."));
    case "eng_deaths":
      return c(t("Engineer deaths"), per10, "survival", t("How often you died as Engineer."));
    case "building_life":
      return c(t("Building lifetime"), t("per game"), "speciality", t("How long your buildings stayed up."));
    case "build_speed":
      return c(t("Building speed"), t("per game"), "speciality", t("How quickly your buildings went up after you switched."));
    case "survival":
      return c(t("Survival"), t("per game"), "survival", t("How long you stayed alive on the class."));
    case "headshot_share":
      return c(t("Headshot share"), t("of kills"), "speciality", t("The share of your kills that were headshots."));
    case "reflects":
      return c(t("Reflect kills"), per10, "speciality", t("Kills with reflected projectiles."));
    case "spychecks":
      return c(t("Spychecks"), per10, "speciality", t("Spies found and hit before they got a stab."));
    case "backstabs":
      return c(t("Backstabs"), per10, "speciality", t("Kills with a backstab."));
    case "saps":
      return c(t("Saps"), per10, "speciality", t("Buildings sapped."));
    default:
      return c(key, "", "teamplay", "");
  }
}

/* Which format "How ratings work" shows: the one it was opened for. */
let shown: GameFormat = "hl";
const listeners = new Set<() => void>();

export function setGuideFormat(f: GameFormat) {
  shown = f;
  listeners.forEach((l) => l());
}

/** "How ratings work", on one format's models. */
export function openRatingGuide(f: GameFormat) {
  setGuideFormat(f);
  goTo("rating");
}

export function useGuideFormat(): GameFormat {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => shown,
  );
}
