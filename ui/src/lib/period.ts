import { useSyncExternalStore } from "react";

/** A stretch of time to look at: everything, one season, or two dates. */
export type Period =
  | { kind: "all" }
  | { kind: "season"; key: string; name: string; from: number; to: number }
  | { kind: "custom"; from: number | null; to: number | null };

const KEY = "hl.period";
let current: Period = load();
const listeners = new Set<() => void>();

/**
 * A stored period, if it is one, otherwise "all time".
 *
 * This used to be `JSON.parse(raw) as Period` and nothing else, which is a
 * promise rather than a check. The period is saved across restarts, so a
 * value that breaks a render breaks *every* render, including after a
 * reload — a tester hit exactly that and reported "F5 doesn't fix it".
 * Anything that is not a well-formed period is thrown away here.
 */
function load(): Period {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { kind: "all" };
    const p = JSON.parse(raw) as Period;
    if (valid(p)) return p;
    localStorage.removeItem(KEY);
  } catch {
    // Storage can be blocked, or hold something that is not JSON at all.
  }
  return { kind: "all" };
}

/**
 * A timestamp we can build a Date from. `new Date(NaN).toISOString()` throws.
 *
 * A function declaration on purpose: `load()` runs while this module is
 * still evaluating, and a `const` here would be in the temporal dead zone —
 * the ReferenceError lands in load's own `catch` and silently resets
 * everyone's period to "all time", which is a worse bug than the one this
 * validation was added to fix.
 */
function time(v: unknown): v is number {
  return typeof v === "number" && Number.isFinite(v);
}

function valid(p: unknown): p is Period {
  if (!p || typeof p !== "object") return false;
  const k = (p as Period).kind;
  if (k === "all") return true;
  if (k === "season") {
    const s = p as Extract<Period, { kind: "season" }>;
    return typeof s.key === "string" && typeof s.name === "string" && time(s.from) && time(s.to);
  }
  if (k === "custom") {
    const c = p as Extract<Period, { kind: "custom" }>;
    return (c.from === null || time(c.from)) && (c.to === null || time(c.to));
  }
  return false;
}

export function setPeriod(p: Period) {
  // Two dates the wrong way round select nothing at all, which looks like
  // the app is broken rather than like a typo. Read them as a range.
  if (p.kind === "custom" && time(p.from) && time(p.to) && p.from > p.to) {
    p = { ...p, from: p.to, to: p.from };
  }
  if (!valid(p)) p = { kind: "all" };
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
  if (p.kind === "all") return { from: null, to: null };
  return { from: time(p.from) ? p.from : null, to: time(p.to) ? p.to : null };
}
