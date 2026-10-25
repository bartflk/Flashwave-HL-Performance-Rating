import { useSyncExternalStore } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { StatScales } from "../api/types";

/**
 * Colours for how a number stands on its class (Flashy: "a colour-coded
 * damage per minute, kind of like osu has colours for difficulties"): osu!'s
 * difficulty order, muted slate and teal at the bottom through olive,
 * amber, red and magenta to violet, then black, laid over the percentiles of the class's games -- the deepest for
 * the top 1%. The scale comes from the games held, officials counting fully
 * and scrims and pugs less (stat_scale.rs).
 */

/** One colour per percentile the scale is given at: osu!'s order, blue,
 *  green, yellow, red, purple, but quiet at the bottom and loud at the top
 *  (Flashy: "the lower DPM colours are very bright, they should use muted
 *  colours"). An ordinary game sits back; only a standout one catches the eye. */
const STOPS = ["#323a46", "#36414f", "#34505e", "#365e55", "#58663a", "#8f7a2c", "#bf6a2a", "#d8484e", "#c23ba6", "#7a4cf0"];
/** Past the top 1%: osu!'s black, with gold text. */
const TOP = "#000000";

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
    return v === "redgreen" || v === "off" ? v : "osu";
  } catch {
    return "osu";
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

/** The colour at a percentile, blended between the stops around it. */
export function tierColour(pct: number, pcts: number[]): string {
  if (pct >= 100) return TOP;
  for (let i = 1; i < pcts.length; i++) {
    if (pct < pcts[i]) return mix(STOPS[i - 1], STOPS[i], (pct - pcts[i - 1]) / (pcts[i] - pcts[i - 1]));
  }
  return STOPS[STOPS.length - 1];
}

/** Gold on black at the top; white on the loud colours; a soft grey on the
 *  quiet ones, so the bottom of the scale reads as quiet too. */
export function tierText(bg: string, pct: number): string {
  if (bg === TOP) return "#f6d05c";
  return pct < 55 ? "#c4c9d2" : "#fff";
}

/**
 * A stat's colour and where it stands, or null where the class has no scale.
 * With a map, among the class's games on that map where it has a scale of
 * its own (Flashy: "300 dpm on steel on sniper is crazy vs 300 dpm on
 * koth_product"), else among all the class's games.
 */
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
  return { pct, bg, fg: tierText(bg, pct), map: byMap ? map! : null };
}
