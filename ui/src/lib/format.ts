import type { Team } from "../api/types";

/** `pl_swiftwater_final1` -> mode `pl`, name `swiftwater`. */
export function splitMap(map: string | null): { mode: string | null; name: string | null } {
  if (!map) return { mode: null, name: null };
  const m = /^(koth|pl|cp|ctf|plr|arena|tc)_(.+)$/i.exec(map);
  if (!m) return { mode: null, name: map };
  const name = m[2].replace(/_(final\d*|rc\d+[a-z]?|b\d+[a-z]?|f\d+|v\d+|a\d+|pro\d*)$/i, "");
  return { mode: m[1].toLowerCase(), name };
}

export function formatDate(unix: number | null, withYear = false): string {
  if (unix === null) return "—";
  const d = new Date(unix * 1000);
  const sameYear = d.getFullYear() === new Date().getFullYear();
  return d.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    ...(sameYear && !withYear ? {} : { year: "numeric" }),
  });
}

/** 288 -> `4:48`. */
export function clock(seconds: number | null): string {
  if (seconds === null || seconds < 0) return "—";
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/** 2816 -> `47 min`. */
export function minutes(seconds: number): string {
  return `${Math.round(seconds / 60)} min`;
}

export function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** TF2's own spelling in the UI. */
export function teamLabel(t: Team): string {
  return t === "Red" ? "RED" : "BLU";
}

/**
 * A rating, HLTV style: 1.00 is an average game against the players you
 * face, 1.40 a strong one, 0.60 a poor one. Always two decimals — a rating
 * that reads "1.2" next to one that reads "1.24" looks like two scales.
 */
export function rating(n: number | null | undefined): string {
  return n === null || n === undefined ? "—" : n.toFixed(2);
}

/** The band a rating is drawn across, so 1.00 lands exactly in the middle. */
export const RATING_BAND = { min: 0.4, max: 1.6 };

/** A rating as a position on that band, 0 to 100, for a bar or a track. */
export function ratingPercent(n: number): number {
  const { min, max } = RATING_BAND;
  return Math.max(0, Math.min(100, ((n - min) / (max - min)) * 100));
}

export function signed(n: number, digits = 1): string {
  const s = n.toFixed(digits);
  return n > 0 ? `+${s}` : s;
}
