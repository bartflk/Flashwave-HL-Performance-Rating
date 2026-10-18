import { useSyncExternalStore } from "react";
import { k } from "./i18n";

/**
 * The colour theme (Q10, function's ask; more and your own, Flashy).
 *
 * A theme is the set of colour roles index.css declares (`--bg`, `--panel`,
 * `--accent-hi` …) — no component knows themes exist. The first four are
 * hand-tuned in index.css and switched with `data-theme` on `<html>`. Every
 * other theme, built in or your own, is four base colours -- background,
 * panels, text, accent -- from which the rest of the roles are worked out
 * here and set on `<html>` directly.
 *
 * The choice is per machine, not per account, so it lives in localStorage
 * rather than the database: it is a preference about this screen, and it has
 * to be readable before the first paint.
 */

export type Base = { bg: string; panel: string; text: string; accent: string };

export type Theme = {
  id: string;
  name: string;
  hint?: string;
  base: Base;
  /** Hand-tuned in index.css under `[data-theme]`. */
  css?: boolean;
  /** A second colour, for the logo's lower wave (Miku's pink hair ties);
   *  the accent when left out. */
  second?: string;
  /** Made in the app; editable and removable. */
  custom?: boolean;
};

export const BUILT_IN: Theme[] = [
  { id: "gravel", name: "Gravel", hint: k("TF2's browns and the game's orange"), css: true, base: { bg: "#1b1714", panel: "#25201c", text: "#f2e6d9", accent: "#e0763a" } },
  { id: "dustbowl", name: "Dustbowl", hint: k("hotter and sandier, the desert maps"), css: true, base: { bg: "#1d1712", panel: "#2a211a", text: "#f6e9d7", accent: "#f0a032" } },
  { id: "coldfront", name: "Coldfront", hint: k("slate and ice, the winter maps"), css: true, base: { bg: "#14181d", panel: "#1c222a", text: "#e4ecf4", accent: "#58a6d8" } },
  { id: "swiftwater", name: "Swiftwater", hint: k("mossy green, the one that is neither"), css: true, base: { bg: "#151914", panel: "#1e241c", text: "#e9f0e4", accent: "#7fb861" } },
  { id: "badlands", name: "Badlands", hint: k("red rock and dust"), base: { bg: "#1b1412", panel: "#2a1e1a", text: "#f3e3da", accent: "#d9583b" } },
  { id: "upward", name: "Upward", hint: k("dusk over the cliffs, violet"), base: { bg: "#16131b", panel: "#221d2a", text: "#ece6f3", accent: "#a77ce0" } },
  { id: "process", name: "Process", hint: k("teal and steel, the factory maps"), base: { bg: "#111919", panel: "#1a2525", text: "#e1f0ee", accent: "#3fb8a6" } },
  { id: "red", name: "RED", hint: k("Reliable Excavation Demolition"), base: { bg: "#1a1212", panel: "#2a1a1a", text: "#f4e3e1", accent: "#d4453f" } },
  { id: "blu", name: "BLU", hint: k("Builders League United"), base: { bg: "#10151c", panel: "#192230", text: "#e1eaf4", accent: "#5b93cc" } },
  { id: "mannco", name: "Mann Co.", hint: k("gold and walnut, the store"), base: { bg: "#15130f", panel: "#221f18", text: "#f1ead8", accent: "#e3b23c" } },
  { id: "miku", name: "Hatsune Miku", hint: k("teal twin-tails and a dash of pink, just for fun"), base: { bg: "#0d1a1c", panel: "#14272a", text: "#e3faf7", accent: "#39c5bb" }, second: "#e12885" },
];

export const DEFAULT_THEME = "gravel";

const KEY = "hl.theme";
const CUSTOM_KEY = "hl.themes.custom";

// ---- colour arithmetic ------------------------------------------------------

type Rgb = [number, number, number];

function rgb(hex: string): Rgb {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16) || 0) as Rgb;
}

function hex([r, g, b]: Rgb): string {
  return "#" + [r, g, b].map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0")).join("");
}

/** `a` moved `t` of the way towards `b`. */
function mix(a: string, b: string, t: number): string {
  const [x, y] = [rgb(a), rgb(b)];
  return hex([0, 1, 2].map((i) => x[i] + (y[i] - x[i]) * t) as Rgb);
}

const triple = (c: string) => rgb(c).join(", ");

/** Relative luminance, for the editor's warnings. */
export function luminance(c: string): number {
  const [r, g, b] = rgb(c).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

/** Every role index.css's themes set, worked out from four base colours. */
export function derive({ bg, panel, text, accent }: Base): Record<string, string> {
  const white = "#ffffff";
  const black = "#000000";
  const muted = mix(text, bg, 0.38);
  return {
    "--bg": bg,
    "--panel": panel,
    "--panel-2": mix(panel, text, 0.06),
    "--border": mix(panel, text, 0.13),
    "--text": text,
    "--muted": muted,
    "--accent": accent,
    "--accent-dim": mix(accent, bg, 0.45),
    "--accent-2": mix(accent, bg, 0.06),
    "--accent-hi": mix(accent, white, 0.06),
    "--accent-lo": mix(accent, black, 0.06),
    "--accent-edge": mix(accent, black, 0.18),
    "--accent-hi-hover": mix(accent, white, 0.16),
    "--accent-lo-hover": mix(accent, white, 0.04),
    "--accent-edge-hover": mix(accent, black, 0.1),
    "--accent-text": mix(accent, white, 0.22),
    "--accent-soft": mix(accent, text, 0.45),
    "--text-hi": mix(text, white, 0.4),
    "--line": mix(panel, text, 0.2),
    "--line-2": mix(panel, text, 0.26),
    "--faint": mix(text, bg, 0.55),
    "--rule": mix(bg, text, 0.05),
    "--row-alt": mix(bg, text, 0.03),
    "--sunken": mix(panel, bg, 0.3),
    "--raised": mix(panel, text, 0.08),
    "--accent-rgb": triple(accent),
    "--text-rgb": triple(text),
    "--muted-rgb": triple(muted),
  };
}

const ROLES = Object.keys(derive(BUILT_IN[0].base));

// ---- the stored choice and your own themes -----------------------------------

function readCustom(): Theme[] {
  try {
    const list = JSON.parse(localStorage.getItem(CUSTOM_KEY) ?? "[]") as Theme[];
    return Array.isArray(list) ? list.filter((t) => t && t.id && t.base).map((t) => ({ ...t, custom: true, css: false })) : [];
  } catch {
    return [];
  }
}

function writeCustom(list: Theme[]) {
  try {
    localStorage.setItem(CUSTOM_KEY, JSON.stringify(list.map(({ id, name, base }) => ({ id, name, base }))));
  } catch {
    // Blocked storage: the theme lasts until the app closes.
  }
}

let custom: Theme[] = [];
let all: Theme[] = BUILT_IN;
let current = DEFAULT_THEME;
let version = 0;
const listeners = new Set<() => void>();

function changed() {
  all = [...BUILT_IN, ...custom];
  version++;
  listeners.forEach((l) => l());
}

export function themeById(id: string): Theme {
  return all.find((t) => t.id === id) ?? BUILT_IN[0];
}

/** Put a theme, or a palette being edited, on the document. */
function paint(theme: Theme | null, preview?: Base) {
  const root = document.documentElement;
  for (const role of ROLES) root.style.removeProperty(role);
  root.style.removeProperty("--second");
  root.removeAttribute("data-theme");
  if (preview) {
    for (const [role, value] of Object.entries(derive(preview))) root.style.setProperty(role, value);
    return;
  }
  if (!theme) return;
  if (theme.css) {
    if (theme.id !== DEFAULT_THEME) root.setAttribute("data-theme", theme.id);
    return;
  }
  for (const [role, value] of Object.entries(derive(theme.base))) root.style.setProperty(role, value);
  if (theme.second) root.style.setProperty("--second", theme.second);
}

/** Called once from main.tsx before React renders, so the window never shows
 *  the default palette for a frame and then repaints. */
export function initTheme() {
  custom = readCustom();
  all = [...BUILT_IN, ...custom];
  try {
    current = localStorage.getItem(KEY) ?? DEFAULT_THEME;
  } catch {
    current = DEFAULT_THEME;
  }
  if (!all.some((t) => t.id === current)) current = DEFAULT_THEME;
  paint(themeById(current));
}

export function setTheme(id: string) {
  current = id;
  paint(themeById(id));
  try {
    localStorage.setItem(KEY, id);
  } catch {
    // Blocked storage: the choice still applies for this session.
  }
  changed();
}

/** Show a palette while it is being edited; null puts the chosen theme back. */
export function previewTheme(base: Base | null) {
  if (base) paint(null, base);
  else paint(themeById(current));
}

/** Save one of your own (new or edited) and switch to it. */
export function saveCustomTheme(name: string, base: Base, id?: string): string {
  const theme: Theme = { id: id ?? `custom-${Date.now().toString(36)}`, name: name.trim() || "My theme", base, custom: true };
  custom = id ? custom.map((t) => (t.id === id ? theme : t)) : [...custom, theme];
  writeCustom(custom);
  changed();
  setTheme(theme.id);
  return theme.id;
}

export function deleteCustomTheme(id: string) {
  custom = custom.filter((t) => t.id !== id);
  writeCustom(custom);
  if (current === id) setTheme(DEFAULT_THEME);
  else changed();
}

/** A theme as a line of text to share, and back. */
export function themeCode(t: Pick<Theme, "name" | "base">): string {
  const bytes = new TextEncoder().encode(JSON.stringify({ name: t.name, ...t.base }));
  return `hltheme:${btoa(String.fromCharCode(...bytes))}`;
}

export function readThemeCode(code: string): { name: string; base: Base } | null {
  try {
    const raw = code.trim().replace(/^hltheme:/, "");
    const v = JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(raw), (c) => c.charCodeAt(0))));
    const ok = (c: unknown) => typeof c === "string" && /^#[0-9a-f]{6}$/i.test(c);
    if (!ok(v.bg) || !ok(v.panel) || !ok(v.text) || !ok(v.accent)) return null;
    return { name: String(v.name ?? "Shared theme").slice(0, 40), base: { bg: v.bg, panel: v.panel, text: v.text, accent: v.accent } };
  } catch {
    return null;
  }
}

export function useTheme(): string {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
}

/** Every theme, yours included; re-renders when one is added or changed. */
export function useThemes(): Theme[] {
  useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => version,
  );
  return all;
}
