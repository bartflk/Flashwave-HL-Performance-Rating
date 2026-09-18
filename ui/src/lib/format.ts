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

export function signed(n: number, digits = 1): string {
  const s = n.toFixed(digits);
  return n > 0 ? `+${s}` : s;
}
