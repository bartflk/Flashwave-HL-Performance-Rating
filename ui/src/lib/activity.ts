import { useSyncExternalStore } from "react";
import { api } from "../api/client";
import type { ActivityLine, ActivitySite } from "../api/types";

/**
 * The activity feed in the window (Flashy: "a waterfall 3/4 lines high that
 * fades out, also logging all these things + api statuses and other debug
 * options"): the backend's lines (hl_ingest::activity), asked for every
 * POLL_MS while something on screen shows them -- the sync card's waterfall
 * or Settings › Activity log -- and not at all otherwise.
 */

const POLL_MS = 800;
/** Lines kept here; the backend keeps 5,000, the log file everything. */
const KEEP = 3_000;

export interface Activity {
  lines: ActivityLine[];
  sites: ActivitySite[];
  verbose: boolean;
}

let state: Activity = { lines: [], sites: [], verbose: false };
let last = 0;
let paused = false;
let timer: number | null = null;
const listeners = new Set<() => void>();

function set(next: Activity) {
  state = next;
  listeners.forEach((l) => l());
}

async function poll() {
  try {
    const v = await api.getActivity(last);
    if (v.lines.length > 0) last = v.lines[v.lines.length - 1].seq;
    if (paused) {
      set({ ...state, sites: v.sites, verbose: v.verbose });
    } else {
      set({ lines: [...state.lines, ...v.lines].slice(-KEEP), sites: v.sites, verbose: v.verbose });
    }
  } catch {
    // Quiet: the next tick asks again.
  }
}

function subscribe(l: () => void) {
  listeners.add(l);
  if (timer === null) {
    void poll();
    timer = window.setInterval(() => void poll(), POLL_MS);
  }
  return () => {
    listeners.delete(l);
    if (listeners.size === 0 && timer !== null) {
      window.clearInterval(timer);
      timer = null;
    }
  };
}

export function useActivity(): Activity {
  return useSyncExternalStore(subscribe, () => state);
}

/** Stop adding lines to the view (the sites keep updating); the feed keeps them. */
export function setActivityPaused(on: boolean) {
  paused = on;
  // Picks up what came in while paused from the backend's own buffer.
  if (!on) last = state.lines.length > 0 ? state.lines[state.lines.length - 1].seq : 0;
}

export function activityPaused(): boolean {
  return paused;
}

/** Empty the view; the backend and the log file keep everything. */
export function clearActivity() {
  set({ ...state, lines: [] });
}

export async function setVerbose(on: boolean) {
  const v = await api.setActivityVerbose(on);
  set({ ...state, verbose: v });
}

/** A line as text, for copying: time, level, source, text. */
export function lineText(l: ActivityLine): string {
  const d = new Date(l.at);
  const time = `${d.toLocaleDateString(undefined, { year: "numeric", month: "2-digit", day: "2-digit" })} ${d.toLocaleTimeString(undefined, { hour12: false })}`;
  return `${time}  ${l.level.toUpperCase().padEnd(5)}  ${l.source}: ${l.text}`;
}
