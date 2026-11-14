import { useSyncExternalStore } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { StoredFormat } from "../api/types";

/**
 * Which format a tab shows: Highlander or 6v6 (the 6v6 plan, P0).
 *
 * Each tab that has both (Matches, Profile, ETF2L) has its own choice, made
 * from the menu that opens when you hover the tab (boSe., question 2: "you
 * hover matches and it allows you to choose between 6s and Highlander").
 * Clicking the tab opens the format it showed last. Formats are never mixed
 * on one page.
 *
 * Only the formats chosen in setup are offered, and none of this shows in a
 * build with 6v6 switched off (question 15: 6v6 ships with its rating), so
 * a Highlander player sees no change.
 */
export type GameFormat = "hl" | "6s";
export type FormatTab = "matches" | "profile" | "etf2l";
export const FORMAT_TABS: readonly FormatTab[] = ["matches", "profile", "etf2l"];

export const toUi = (f: StoredFormat): GameFormat => (f === "sixes" ? "6s" : "hl");
export const toStored = (f: GameFormat): StoredFormat => (f === "6s" ? "sixes" : "highlander");

export interface FormatsPlayed {
  /** In setup's order, Highlander first; never empty. */
  formats: GameFormat[];
  /** Whether this build has 6v6 at all. */
  sixesEnabled: boolean;
  /** More than one format to choose from: the tabs show their menus. */
  multi: boolean;
}

const HL_ONLY: FormatsPlayed = { formats: ["hl"], sixesEnabled: false, multi: false };

export function useFormatsPlayed(): FormatsPlayed {
  const q = useQuery({ queryKey: ["formats_played"], queryFn: api.formatsPlayed, staleTime: Infinity });
  if (!q.data) return HL_ONLY;
  const formats = q.data.sixesEnabled ? q.data.formats.map(toUi) : ["hl" as const];
  const list: GameFormat[] = formats.length ? formats : ["hl"];
  return { formats: list, sixesEnabled: q.data.sixesEnabled, multi: list.length > 1 };
}

const KEY = "tab-format";
let chosen: Partial<Record<FormatTab, GameFormat>> = read();
let listeners: Array<() => void> = [];

function read(): Partial<Record<FormatTab, GameFormat>> {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return v && typeof v === "object" ? v : {};
  } catch {
    return {};
  }
}

export function setTabFormat(tab: FormatTab, f: GameFormat) {
  chosen = { ...chosen, [tab]: f };
  try {
    localStorage.setItem(KEY, JSON.stringify(chosen));
  } catch {
    // Private window or blocked storage: the choice still holds this run.
  }
  listeners.forEach((l) => l());
}

function subscribe(l: () => void) {
  listeners.push(l);
  return () => {
    listeners = listeners.filter((x) => x !== l);
  };
}

/** What a tab shows: its last choice when that format is still played, else the first played. */
export function tabFormat(tab: FormatTab, played: FormatsPlayed, store = chosen): GameFormat {
  const c = store[tab];
  return c && played.formats.includes(c) ? c : played.formats[0];
}

export function useTabFormat(tab: FormatTab): GameFormat {
  const played = useFormatsPlayed();
  const store = useSyncExternalStore(subscribe, () => chosen);
  return tabFormat(tab, played, store);
}

/** Every tab's format at once, for the header. */
export function useTabFormats(): Record<FormatTab, GameFormat> {
  const played = useFormatsPlayed();
  const store = useSyncExternalStore(subscribe, () => chosen);
  return { matches: tabFormat("matches", played, store), profile: tabFormat("profile", played, store), etf2l: tabFormat("etf2l", played, store) };
}

/** A map's mode from its name: 5CP and KOTH are 6v6's two. */
export function mapMode(map: string): "5cp" | "koth" | "other" {
  if (map.startsWith("cp_")) return "5cp";
  if (map.startsWith("koth_")) return "koth";
  return "other";
}

/**
 * A 6v6 5CP match's midfights: each round starts at the middle point, and
 * the team that capped first won it (logs.tf's `firstcap`). Null for any
 * other match, or a log that did not record it (the 6v6 plan, P1).
 */
export function midfights(d: { format: string | null; map: string | null; rounds: Array<{ firstcap: "Red" | "Blue" | null }> }): { Red: number; Blue: number } | null {
  if (d.format !== "sixes" || mapMode(d.map ?? "") !== "5cp") return null;
  const out = { Red: 0, Blue: 0 };
  for (const r of d.rounds) if (r.firstcap) out[r.firstcap] += 1;
  return out.Red + out.Blue > 0 ? out : null;
}

/** Maps neither won nor lost: draws (a 5CP map level at the time limit). */
export function drawn(r: { mapsPlayed: number; mapsWon: number; mapsLost: number }): number {
  return Math.max(0, r.mapsPlayed - r.mapsWon - r.mapsLost);
}
