import { useSyncExternalStore } from "react";

/**
 * The colour theme (Q10, function's ask).
 *
 * A theme is a `data-theme` on `<html>` and a list of variable overrides in
 * index.css — no component knows themes exist, which is the whole point of
 * having spent the effort turning the stylesheets' literals into roles.
 *
 * The choice is per machine, not per account, so it lives in localStorage
 * rather than the database: it is a preference about this screen, and it has
 * to be readable before the first paint.
 */

export const THEMES = [
  { id: "gravel", name: "Gravel", hint: "TF2's browns and the game's orange" },
  { id: "dustbowl", name: "Dustbowl", hint: "hotter and sandier, the desert maps" },
  { id: "coldfront", name: "Coldfront", hint: "slate and ice, the winter maps" },
  { id: "swiftwater", name: "Swiftwater", hint: "mossy green, the one that is neither" },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

export const DEFAULT_THEME: ThemeId = "gravel";

const KEY = "hl.theme";

function isTheme(v: string | null): v is ThemeId {
  return THEMES.some((t) => t.id === v);
}

/** What is stored, or the default. Storage can be blocked; that is not fatal. */
export function storedTheme(): ThemeId {
  try {
    const v = localStorage.getItem(KEY);
    return isTheme(v) ? v : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

/**
 * Put the theme on the document.
 *
 * Called once from main.tsx before React renders, so the window never shows
 * the default palette for a frame and then repaints.
 */
export function applyTheme(id: ThemeId) {
  const root = document.documentElement;
  if (id === DEFAULT_THEME) {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", id);
  }
}

let current: ThemeId = DEFAULT_THEME;
let listeners: Array<() => void> = [];

export function initTheme() {
  current = storedTheme();
  applyTheme(current);
}

export function setTheme(id: ThemeId) {
  current = id;
  applyTheme(id);
  try {
    localStorage.setItem(KEY, id);
  } catch {
    // Blocked storage: the choice still applies for this session.
  }
  listeners.forEach((l) => l());
}

export function useTheme(): ThemeId {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        listeners = listeners.filter((x) => x !== l);
      };
    },
    () => current,
  );
}
