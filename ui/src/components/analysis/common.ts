import type { Analysis, AnalysisPlayer, Jump, RoundSpan } from "../../api/types";
import { copy } from "../../lib/toast";
import { clock } from "../../lib/format";

/**
 * Colour roles shared by every analysis view, so one meaning keeps one colour:
 * blue is damage and kills you dealt, orange is what was done to you.
 * Validated as a pair on the dark surface (CVD ΔE 19.7, normal 24.2); every
 * view also separates them by shape or side, never by colour alone.
 *
 * These two are deliberately the same in every theme (Q10). They are a
 * validated pair carrying a meaning, like RED and BLU — a theme may repaint
 * the app around them, not them.
 */
/// Both are CSS variables so a theme moves them (Q10). SVG and inline
/// styles take `var(...)` perfectly well; the only place that cannot is a
/// canvas, which reads the resolved value through `themeColour` below.
export const KILL = "var(--kill)";
export const DEATH = "var(--death)";

/**
 * The value a CSS variable currently resolves to.
 *
 * Canvas takes a colour string and cannot parse `var(--kill)`, so the kill
 * map asks for the resolved colour instead. Read at draw time, not at
 * module load: the theme can change while the page is open.
 */
export function themeColour(name: string, fallback: string): string {
  if (typeof window === "undefined") return fallback;
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

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

/**
 * What the filter row selects: one map of a combined log, one round, or all
 * of it. Every view draws only this slice.
 */
export interface Slice {
  /** The rounds in the slice; null means every round. */
  rounds: Set<number> | null;
  /** Game-time span of the slice. */
  startS: number;
  endS: number;
  /** A single round is selected. */
  oneRound: boolean;
  /** The slice's map; null when it spans several maps. */
  map: string | null;
  /** The match covers more than one map. */
  multiMap: boolean;
}

export function inSlice(roundNum: number, s: Slice): boolean {
  return s.rounds === null || s.rounds.has(roundNum);
}

/**
 * What the slice covers, in words, for a panel to put in a sentence.
 *
 * There are three cases and the views used to know about two. Picking one
 * map of a combined log sets `rounds` to that map's rounds, which reads as
 * "a filter is on" and was being described as "in this round" — wrong for a
 * seven-round map, and wrong in a way that makes the numbers look wrong
 * rather than the label. Anything saying what it is showing says it here.
 */
export function sliceLabel(s: Slice): string {
  if (s.rounds === null) return "over the whole match";
  if (s.oneRound) return "in this round";
  return s.map ? `on ${s.map}` : "in these rounds";
}

/** The same, as a noun for "counted from ...". */
export function sliceScope(s: Slice): string {
  if (s.rounds === null) return "the whole match";
  if (s.oneRound) return "this round";
  return s.map ? `this map` : "these rounds";
}
