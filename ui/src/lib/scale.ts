import { useSyncExternalStore } from "react";
import { inTauri } from "../api/client";

/**
 * How big the whole interface is drawn (Flashy: for 2K and 4K screens).
 *
 * The app window's own zoom, the way a browser's Ctrl + zooms a page: text,
 * spacing, the map and every chart grow together and stay sharp. In the app
 * it is the webview's zoom; in a plain browser (the mock), CSS `zoom` on the
 * page. Like the theme, it is a preference about this screen, so it lives in
 * localStorage and is applied before the first paint.
 */

export const SCALES = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;
export const DEFAULT_SCALE = 1;

const KEY = "hl.uiScale";

function valid(v: number): boolean {
  return Number.isFinite(v) && v >= SCALES[0] && v <= SCALES[SCALES.length - 1];
}

export function storedScale(): number {
  try {
    const v = Number(localStorage.getItem(KEY));
    return valid(v) ? v : DEFAULT_SCALE;
  } catch {
    return DEFAULT_SCALE;
  }
}

let current = storedScale();
const listeners = new Set<() => void>();

/** The layout is built for 900 CSS px and up (the window's minimum width). */
const MIN_LAYOUT_PX = 900;

/** What is drawn now, and whether it is the webview's zoom (which shrinks
 *  `innerWidth`) or the page's own `zoom` (which does not). */
let applied = 1;
let byWebview = inTauri;

/**
 * The scale to draw: the chosen one, but never so large that the window is
 * narrower than the layout wants in CSS px. At 150% a 900 px window is 600
 * CSS px wide, and the header and tables break; so the zoom stops where the
 * window still fits 900. Below 100% nothing is capped.
 */
function effectiveScale(): number {
  if (current <= 1) return current;
  const width = byWebview ? window.innerWidth * applied : window.innerWidth;
  const cap = Math.floor((width / MIN_LAYOUT_PX) * 100) / 100;
  return Math.max(1, Math.min(current, cap));
}

async function apply() {
  const scale = effectiveScale();
  if (scale === applied) return;
  if (inTauri && byWebview) {
    try {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      await getCurrentWebview().setZoom(scale);
      applied = scale;
      return;
    } catch {
      // An older webview without zoom: the page's own zoom below instead.
      byWebview = false;
    }
  }
  applied = scale;
  document.documentElement.style.setProperty("zoom", String(scale));
}

/** Applies the stored scale; called once before the first render. */
export function initScale() {
  void apply();
  // A window made narrower (or wider) moves the cap.
  window.addEventListener("resize", () => {
    if (current > 1) void apply();
  });
}

export function setScale(scale: number) {
  const s = Math.round(scale * 100) / 100;
  if (!valid(s)) return;
  current = s;
  try {
    localStorage.setItem(KEY, String(s));
  } catch {
    // Storage can be blocked; the scale then lasts until the app closes.
  }
  void apply();
  listeners.forEach((f) => f());
}

/** One step up or down the list, from wherever the scale is now. */
export function stepScale(dir: 1 | -1) {
  const next = dir === 1 ? SCALES.find((s) => s > current + 0.001) : [...SCALES].reverse().find((s) => s < current - 0.001);
  if (next !== undefined) setScale(next);
}

export function useScale(): number {
  return useSyncExternalStore(
    (f) => {
      listeners.add(f);
      return () => listeners.delete(f);
    },
    () => current,
  );
}

/** Ctrl + and Ctrl − step the scale, Ctrl 0 resets it, as in a browser. */
export function watchScaleKeys(): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    if (e.key === "=" || e.key === "+") stepScale(1);
    else if (e.key === "-" || e.key === "_") stepScale(-1);
    else if (e.key === "0") setScale(DEFAULT_SCALE);
    else return;
    e.preventDefault();
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}
