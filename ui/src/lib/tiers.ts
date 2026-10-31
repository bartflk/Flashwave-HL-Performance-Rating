import { useSyncExternalStore } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { StatScales } from "../api/types";

/**
 * Colours for how a number stands on its class (Flashy: "a colour-coded
 * damage per minute, kind of like osu has colours for difficulties"): osu!'s
 * difficulty spectrum, blue through yellow and red to navy, then black,
 * laid over the percentiles of the class's games -- black for the top 1%.
 * Red to green is the default. The scale comes from the games held, officials counting fully
 * and scrims and pugs less (stat_scale.rs).
 */

/** osu!'s difficulty spectrum as its site draws it now, by star rating
 *  (Flashy, 31 Oct 2026: "the osu colours are more like this", over the
 *  muted ones of before): blue, cyan, green, yellow, orange, red, pink,
 *  purple, indigo, navy, black. */
const OSU: [number, string][] = [
  [0.1, "#4290fb"],
  [1.25, "#4fc0ff"],
  [2, "#4fffd5"],
  [2.5, "#7cff4f"],
  [3.3, "#f6f05c"],
  [4.2, "#ff8068"],
  [4.9, "#ff4e6f"],
  [5.8, "#c645b8"],
  [6.7, "#6563de"],
  [7.7, "#18158e"],
  [9, "#000000"],
];
/** Past the top 1%: osu!'s black, with gold text. */
const TOP = "#000000";

/** A percentile as a star rating: the average game near yellow (3.3), the
 *  top tenth in the purples and navy, the top 1% at the edge of black. */
function starsOf(pct: number): number {
  return 0.1 + 7.6 * Math.pow(Math.min(Math.max(pct, 0), 100) / 100, 1.4);
}

/**
 * Which colours the numbers wear (Flashy: "an option to have a red to green
 * scale instead"): osu!'s pills, red-to-green text over a faint bar like
 * Leetify's, or none. A preference about this screen, kept in localStorage
 * like the theme.
 */
export type Palette = "osu" | "redgreen" | "off";
const PALETTE_KEY = "hl.statColours";
let palette: Palette = (() => {
  try {
    const v = localStorage.getItem(PALETTE_KEY);
    // Red to green unless osu!'s were chosen (Flashy: "make the red to
    // green the default").
    return v === "osu" || v === "off" ? v : "redgreen";
  } catch {
    return "redgreen";
  }
})();
const paletteListeners = new Set<() => void>();
export function setPalette(p: Palette) {
  palette = p;
  try {
    localStorage.setItem(PALETTE_KEY, p);
  } catch {
    // Storage can be blocked; the choice then lasts until the app closes.
  }
  paletteListeners.forEach((f) => f());
}
export function usePalette(): Palette {
  return useSyncExternalStore(
    (f) => {
      paletteListeners.add(f);
      return () => paletteListeners.delete(f);
    },
    () => palette,
  );
}

/** Red to green: red the bottom, amber and yellow below average, a plain
 *  light grey around it, green above, the brightest green at the top. */
const RG: [number, string][] = [
  [0, "#e5534b"],
  [20, "#e8873a"],
  [40, "#e3c04d"],
  [55, "#d4d7dd"],
  [75, "#8fd48a"],
  [100, "#3fcf6e"],
];

/** The red-to-green colour at a percentile. */
export function redGreen(pct: number): string {
  for (let i = 1; i < RG.length; i++) {
    if (pct <= RG[i][0]) return mix(RG[i - 1][1], RG[i][1], (pct - RG[i - 1][0]) / (RG[i][0] - RG[i - 1][0]));
  }
  return RG[RG.length - 1][1];
}

export function useStatScales() {
  return useQuery({ queryKey: ["stat_scales"], queryFn: () => api.getStatScales(), staleTime: Infinity });
}

/** Where `v` stands among the class's games, 0-100 (99+ past the top 1%). */
export function percentileOf(v: number, cuts: number[], pcts: number[]): number | null {
  if (cuts.length !== pcts.length || cuts.length === 0) return null;
  if (v <= cuts[0]) return 0;
  for (let i = 1; i < cuts.length; i++) {
    if (v < cuts[i]) {
      const span = cuts[i] - cuts[i - 1];
      return pcts[i - 1] + (span > 0 ? ((v - cuts[i - 1]) / span) * (pcts[i] - pcts[i - 1]) : 0);
    }
  }
  return 100;
}

function mix(a: string, b: string, t: number): string {
  const p = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const [x, y] = [p(a), p(b)];
  return `#${x.map((c, i) => Math.round(c + (y[i] - c) * t).toString(16).padStart(2, "0")).join("")}`;
}

/** The colour at a percentile: its star rating on osu!'s spectrum. `_pcts`,
 *  the percentiles the scale is given at, is kept for the callers. */
export function tierColour(pct: number, _pcts: number[]): string {
  if (pct >= 100) return TOP;
  const s = starsOf(pct);
  if (s <= OSU[0][0]) return OSU[0][1];
  for (let i = 1; i < OSU.length; i++) {
    if (s < OSU[i][0]) return mix(OSU[i - 1][1], OSU[i][1], (s - OSU[i - 1][0]) / (OSU[i][0] - OSU[i - 1][0]));
  }
  return OSU[OSU.length - 1][1];
}

/** As osu! writes on its pills: dark on the light colours, its yellow from
 *  6.5 stars on, gold on the black at the top. */
export function tierText(bg: string, pct: number): string {
  if (bg === TOP) return "#f6d05c";
  return starsOf(pct) >= 6.5 ? "#f6f05c" : "rgba(0, 0, 0, 0.78)";
}

/**
 * A stat's colour and where it stands, or null where the class has no scale.
 * With a map, among the class's games on that map where it has a scale of
 * its own (Flashy: "300 dpm on steel on sniper is crazy vs 300 dpm on
 * koth_product"), else among all the class's games.
 */
export type Tier = NonNullable<ReturnType<typeof tierOf>>;

export function tierOf(scales: StatScales | undefined, cls: string | null | undefined, stat: "dpm" | "accuracy", v: number | null, map?: string | null) {
  if (!scales || !cls || v === null) return null;
  const onMap = map ? scales.maps?.[cls]?.[map] : undefined;
  const mapCuts = onMap?.[stat];
  const byMap = !!mapCuts && mapCuts.length > 0;
  const c = byMap ? onMap : scales.classes[cls];
  const cuts = c?.[stat];
  // A scale from a handful of games says little: heavies and pyros rarely
  // have logs.tf's accuracy stats. A map's scale already has 200 or more.
  const games = stat === "dpm" ? c?.dpmGames : c?.accuracyGames;
  if (!cuts || cuts.length === 0 || (games ?? 0) < (byMap ? 200 : 300)) return null;
  const pct = percentileOf(v, cuts, scales.percentiles);
  if (pct === null) return null;
  const bg = tierColour(pct, scales.percentiles);
  return {
    pct,
    bg,
    fg: tierText(bg, pct),
    map: byMap ? map! : null,
    // What the scale was measured on, for the hover card (Flashy: "out of
    // how many games, how many players").
    stat,
    cuts,
    pcts: scales.percentiles,
    games: games ?? 0,
    officials: stat === "dpm" ? c?.dpmOfficials : c?.accuracyOfficials,
    players: stat === "dpm" ? c?.dpmPlayers : c?.accuracyPlayers,
    // A map asked for that has too few games for a scale of its own.
    missedMap: !byMap && map ? map : null,
    // Made from the logs.tf archive: its span, for the card's note.
    archive: c?.archive && scales.archive ? scales.archive : null,
  };
}
