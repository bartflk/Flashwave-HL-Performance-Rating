import type { Analysis, AnalysisPlayer, Jump, RoundSpan } from "../../api/types";
import { copy } from "../../lib/toast";
import { clock } from "../../lib/format";

/**
 * Colour roles shared by every analysis view, so one meaning keeps one colour:
 * blue is damage and kills you dealt, orange is what was done to you.
 * Validated as a pair on the dark surface (CVD ΔE 19.7, normal 24.2); every
 * view also separates them by shape or side, never by colour alone.
 */
export const KILL = "#5791c8";
export const DEATH = "#d6763a";

/** TF2's own class order, as the scoreboard lists them. */
export const CLASS_ORDER = ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"];

export const CLASS_SHORT: Record<string, string> = {
  scout: "Scout",
  soldier: "Soldier",
  pyro: "Pyro",
  demoman: "Demo",
  heavy: "Heavy",
  engineer: "Engie",
  medic: "Medic",
  sniper: "Sniper",
  spy: "Spy",
};

/** "R2 3:41": the round and the time into it. */
export function roundClock(t: number, rounds: RoundSpan[]): string {
  const r = rounds.find((x) => t >= x.startS && t <= x.endS) ?? rounds[rounds.length - 1];
  return r ? `R${r.roundNum} ${clock(Math.max(0, t - r.startS))}` : clock(t);
}

export function jumpTo(j: Jump, what: string) {
  void copy(`demo_gototick ${j.tick}`, what);
}

export function playerMap(a: Analysis): Map<number, AnalysisPlayer> {
  return new Map(a.players.map((p) => [p.accountId, p]));
}

/** Inside the chosen round, or every round when none is chosen. */
export function inRound(roundNum: number, round: number | null): boolean {
  return round === null || roundNum === round;
}
