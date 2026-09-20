import type { Analysis, FightStats, KillView } from "../../api/types";

/**
 * Fight counts for part of a match, recomputed from the kills in it.
 *
 * The stored counts (`analysis.fights`) cover the whole match, because the
 * fights pass reads the raw log once. For a round or one map of a combined
 * log, the kills carry everything a kill can say — who opened, who was traded,
 * who cleaned up — so those columns are counted again here.
 *
 * Three columns cannot be: Fight KAST needs who was alive for each fight,
 * forces need the damage on a Medic before a pop, and deaths around an uber
 * need the uber's own timing. They stay whole-match, and the table says so.
 */

const FLANK = new Set(["scout", "spy", "soldier"]);
const COMBO = new Set(["medic", "demoman", "heavy", "pyro"]);

/** The columns this file cannot recompute, left at zero. */
export type PartialFights = FightStats & { partial: true };

export function fightsInSlice(a: Analysis, rounds: Set<number>): Map<number, PartialFights> {
  const out = new Map<number, PartialFights>();
  const blank = (accountId: number): PartialFights => ({
    accountId,
    rounds: 0,
    kills: 0,
    deaths: 0,
    openingKills: 0,
    openingDeaths: 0,
    firstPicks: 0,
    firstDeaths: 0,
    tradedKills: 0,
    diedAfterKill: 0,
    tradeKills: 0,
    cleanupKills: 0,
    chargedPicks: 0,
    drops: 0,
    forces: 0,
    deathsBeforeUber: 0,
    deathsDuringUber: 0,
    deathsAfterUber: 0,
    tradedDeaths: 0,
    deathsToSniper: 0,
    deathsToFlank: 0,
    deathsToCombo: 0,
    stationaryDeaths: 0,
    fightsPresent: 0,
    fightsKast: 0,
    fightsKastEngaged: 0,
    partial: true,
  });
  const get = (id: number) => {
    const found = out.get(id) ?? blank(id);
    out.set(id, found);
    return found;
  };

  for (const k of a.kills as KillView[]) {
    if (!rounds.has(k.roundNum) || !k.tags) continue;
    const t = k.tags;
    const killer = get(k.killer);
    killer.kills += 1;
    killer.openingKills += t.opening ? 1 : 0;
    killer.firstPicks += t.firstOfRound ? 1 : 0;
    killer.tradedKills += t.traded ? 1 : 0;
    killer.diedAfterKill += t.diedAfter ? 1 : 0;
    killer.tradeKills += t.trade ? 1 : 0;
    killer.cleanupKills += t.cleanup ? 1 : 0;
    killer.chargedPicks += t.intoCharge ? 1 : 0;
    killer.drops += t.drop ? 1 : 0;

    const victim = get(k.victim);
    victim.deaths += 1;
    victim.openingDeaths += t.opening ? 1 : 0;
    victim.firstDeaths += t.firstOfRound ? 1 : 0;
    victim.tradedDeaths += t.deathTraded ? 1 : 0;
    victim.stationaryDeaths += t.stationary ? 1 : 0;
    if (k.killerClass === "sniper") victim.deathsToSniper += 1;
    else if (k.killerClass && FLANK.has(k.killerClass)) victim.deathsToFlank += 1;
    else if (k.killerClass && COMBO.has(k.killerClass)) victim.deathsToCombo += 1;
  }
  return out;
}
