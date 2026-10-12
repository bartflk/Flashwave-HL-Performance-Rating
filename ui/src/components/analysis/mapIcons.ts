import { useSyncExternalStore } from "react";

/**
 * TF2's own icons on the kill map (Q72): the Engineer HUD's building icons
 * and the kill feed's skull, Valve's art via the Official TF2 Wiki. They ship
 * as white cut-outs in `ui/public/icons` and are tinted here, so a building
 * takes its team's colour and a death the theme's.
 */
export type IconName = "sentry" | "dispenser" | "entrance" | "exit" | "teleporter" | "skull";

const FILES: Record<IconName, string> = {
  sentry: "/icons/sentry.png",
  dispenser: "/icons/dispenser.png",
  entrance: "/icons/tele_entrance.png",
  exit: "/icons/tele_exit.png",
  teleporter: "/icons/teleporter.png",
  skull: "/icons/skull.png",
};

export type MapIcons = Partial<Record<IconName, HTMLImageElement>>;

// Loaded once for the whole app; null until every file has answered. One
// that fails to load is left out, and its marker falls back to a shape.
let icons: MapIcons | null = null;
let started = false;
const listeners = new Set<() => void>();

function load() {
  if (started) return;
  started = true;
  const got: MapIcons = {};
  const names = Object.keys(FILES) as IconName[];
  let left = names.length;
  const done = () => {
    if (--left > 0) return;
    icons = got;
    listeners.forEach((f) => f());
  };
  for (const name of names) {
    const el = new Image();
    el.onload = () => {
      got[name] = el;
      done();
    };
    el.onerror = done;
    el.src = FILES[name];
  }
}

/** The icons, or null while they load; a canvas redraws when they arrive. */
export function useMapIcons(): MapIcons | null {
  load();
  return useSyncExternalStore(
    (f) => {
      listeners.add(f);
      return () => listeners.delete(f);
    },
    () => icons,
  );
}

const RIM = "rgba(20, 16, 12, 0.85)";
const PAD = 2;
const sprites = new Map<string, HTMLCanvasElement>();

/** The cut-out filled with one colour. */
function filled(img: HTMLImageElement, w: number, h: number, colour: string): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const g = c.getContext("2d")!;
  g.drawImage(img, 0, 0, w, h);
  g.globalCompositeOperation = "source-in";
  g.fillStyle = colour;
  g.fillRect(0, 0, w, h);
  return c;
}

/** The icon in one colour with a dark rim, so it reads on a light map and a
 *  dark one alike. Cached per colour, size and pixel density. */
function sprite(img: HTMLImageElement, name: IconName, colour: string, h: number, density: number): HTMLCanvasElement {
  const key = `${name}|${colour}|${h}|${density}`;
  const hit = sprites.get(key);
  if (hit) return hit;
  const ih = Math.round(h * density);
  const iw = Math.round((ih * img.width) / img.height);
  const pad = Math.ceil(PAD * density);
  const c = document.createElement("canvas");
  c.width = iw + pad * 2;
  c.height = ih + pad * 2;
  const g = c.getContext("2d")!;
  const rim = filled(img, iw, ih, RIM);
  const step = 1.25 * density;
  for (const [dx, dy] of [[-1, -1], [0, -1], [1, -1], [-1, 0], [1, 0], [-1, 1], [0, 1], [1, 1]]) {
    g.drawImage(rim, pad + dx * step, pad + dy * step);
  }
  g.drawImage(filled(img, iw, ih, colour), pad, pad);
  sprites.set(key, c);
  return c;
}

/** Draws one icon centred on (x, y), `h` pixels tall; false when that icon is
 *  not loaded, so the caller can draw its own shape instead. */
export function drawIcon(
  g: CanvasRenderingContext2D,
  all: MapIcons | null,
  name: IconName,
  [x, y]: [number, number],
  h: number,
  colour: string,
  alpha: number,
): boolean {
  const img = all?.[name];
  if (!img) return false;
  // Drawn at the canvas's own resolution, zoom and screen density included.
  const density = Math.max(1, Math.round(Math.abs(g.getTransform().a) * 2) / 2);
  const s = sprite(img, name, colour, h, density);
  const w = s.width / density;
  const hh = s.height / density;
  g.save();
  g.globalAlpha = alpha;
  g.drawImage(s, x - w / 2, y - hh / 2, w, hh);
  g.restore();
  return true;
}
