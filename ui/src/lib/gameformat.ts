import { useSyncExternalStore } from "react";

/**
 * Which format the app is showing: Highlander or 6v6 (prototype; Flashy:
 * "integrate 6v6 as a gamemode ... paying respect to both gamemodes").
 *
 * One switch for the whole app, as a person plays one format at a time and
 * every page (matches, ratings, the league tabs) means a different thing in
 * each. Highlander stays the default: it is what the app was built for. The
 * choice is a per-viewer convenience, kept in localStorage.
 */
export type GameFormat = "hl" | "6s";

const KEY = "game-format";
let current: GameFormat = read();
let listeners: Array<() => void> = [];

function read(): GameFormat {
  try {
    return localStorage.getItem(KEY) === "6s" ? "6s" : "hl";
  } catch {
    return "hl";
  }
}

export function setGameFormat(f: GameFormat) {
  current = f;
  try {
    localStorage.setItem(KEY, f);
  } catch {
    // Private window or blocked storage: the switch still works this run.
  }
  listeners.forEach((l) => l());
}

export function useGameFormat(): [GameFormat, (f: GameFormat) => void] {
  const f = useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        listeners = listeners.filter((x) => x !== l);
      };
    },
    () => current,
  );
  return [f, setGameFormat];
}

/** A map's mode from its name: 5CP and KOTH are 6v6's two. */
export function mapMode(map: string): "5cp" | "koth" | "other" {
  if (map.startsWith("cp_")) return "5cp";
  if (map.startsWith("koth_")) return "koth";
  return "other";
}

/** Maps neither won nor lost: draws (a 5CP map level at the time limit). */
export function drawn(r: { mapsPlayed: number; mapsWon: number; mapsLost: number }): number {
  return Math.max(0, r.mapsPlayed - r.mapsWon - r.mapsLost);
}
