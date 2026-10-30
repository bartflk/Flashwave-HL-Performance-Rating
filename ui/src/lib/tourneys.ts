import { useSyncExternalStore } from "react";

/**
 * Whether the match list shows tournament dividers (Flashy: "would be nice
 * to have like a separator with a tournament name ... maybe make this a
 * setting"). On unless turned off in Settings; kept in localStorage like the
 * stat colours.
 */
const KEY = "hl.tournamentDividers";
let on: boolean = (() => {
  try {
    return localStorage.getItem(KEY) !== "0";
  } catch {
    return true;
  }
})();
const listeners = new Set<() => void>();

export function setTournamentDividers(v: boolean) {
  on = v;
  try {
    localStorage.setItem(KEY, v ? "1" : "0");
  } catch {
    // Blocked storage: the choice lasts until the app closes.
  }
  listeners.forEach((f) => f());
}

export function useTournamentDividers(): boolean {
  return useSyncExternalStore(
    (f) => {
      listeners.add(f);
      return () => listeners.delete(f);
    },
    () => on,
  );
}
