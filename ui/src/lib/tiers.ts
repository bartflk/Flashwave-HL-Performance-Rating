import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { StatScales } from "../api/types";

/**
 * Colours for how a number stands on its class (Flashy: "a colour-coded
 * damage per minute, kind of like osu has colours for difficulties"): osu!'s
 * difficulty spectrum, blue through green, yellow, red and purple to near
 * black, laid over the percentiles of the class's games -- the deepest for
 * the top 1%. The scale comes from the games held, officials counting fully
 * and scrims and pugs less (stat_scale.rs).
 */

/** osu!'s star-rating colours, one per percentile the scale is given at. */
const STOPS = ["#4290fb", "#4fc0ff", "#4fffd5", "#7cff4f", "#f6f05c", "#ff8068", "#ff4e6f", "#c645b8", "#6563de", "#18158e"];
/** Past the top 1%: osu!'s black, with gold text. */
const TOP = "#000000";

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

/** Dark text on the light end of the spectrum, light on the dark end, gold on black. */
export function tierText(bg: string): string {
  if (bg === TOP) return "#f6d05c";
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(bg.slice(i, i + 2), 16) / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5 ? "#111" : "#fff";
}

/** A stat's colour and where it stands, or null where the class has no scale. */
export function tierOf(scales: StatScales | undefined, cls: string | null | undefined, stat: "dpm" | "accuracy", v: number | null) {
  if (!scales || !cls || v === null) return null;
  const c = scales.classes[cls];
  const cuts = c?.[stat];
  // A scale from a handful of games says little: heavies and pyros rarely
  // have logs.tf's accuracy stats.
  const games = stat === "dpm" ? c?.dpmGames : c?.accuracyGames;
  if (!cuts || cuts.length === 0 || (games ?? 0) < 300) return null;
  const pct = percentileOf(v, cuts, scales.percentiles);
  if (pct === null) return null;
  const bg = tierColour(pct, scales.percentiles);
  return { pct, bg, fg: tierText(bg) };
}
