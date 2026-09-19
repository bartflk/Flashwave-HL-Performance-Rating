import { useSyncExternalStore } from "react";

/** A stretch of time to look at: everything, one season, or two dates. */
export type Period =
  | { kind: "all" }
  | { kind: "season"; key: string; name: string; from: number; to: number }
  | { kind: "custom"; from: number | null; to: number | null };

const KEY = "hl.period";
let current: Period = load();
const listeners = new Set<() => void>();

function load(): Period {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return JSON.parse(raw) as Period;
  } catch {
    // Storage can be blocked; the page works without it.
  }
  return { kind: "all" };
}

export function setPeriod(p: Period) {
  current = p;
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    // As above.
  }
  listeners.forEach((l) => l());
}

/** The period shared by the match list and the profile. */
export function usePeriod(): Period {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
}

/** Unix-second bounds, `null` where open. */
export function bounds(p: Period): { from: number | null; to: number | null } {
  return p.kind === "all" ? { from: null, to: null } : { from: p.from, to: p.to };
}
