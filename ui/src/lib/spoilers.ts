import { useSyncExternalStore } from "react";

/**
 * The spoiler switch (tf2esports' "Show results"): off hides every score and
 * winner. A tiny store so any tab can read it; the RGL tab uses it first and
 * the ETF2L tab can adopt it later. Remembered per viewer; storage may be
 * blocked, then it lasts for the run only.
 */
const KEY = "show-results";

let shown = true;
try {
  shown = globalThis.localStorage?.getItem(KEY) !== "0";
} catch {
  // Blocked storage: results stay shown.
}

const listeners = new Set<() => void>();

export function setShowResults(on: boolean) {
  if (on === shown) return;
  shown = on;
  try {
    globalThis.localStorage?.setItem(KEY, on ? "1" : "0");
  } catch {
    // Not remembered; the switch still works for this run.
  }
  for (const l of listeners) l();
}

export const showResults = () => shown;

function subscribe(l: () => void) {
  listeners.add(l);
  return () => void listeners.delete(l);
}

export function useShowResults(): [boolean, (on: boolean) => void] {
  return [useSyncExternalStore(subscribe, showResults, showResults), setShowResults];
}
