import { useSyncExternalStore } from "react";
import { api } from "../api/client";
import { noteError } from "./problems";
import { startSync } from "./sync";

/**
 * A demo appearing means a match has just been played.
 *
 * The backend watches the TF2 folders and says when a recording is finished.
 * That is the earliest this app can know you played, and it is the moment
 * the page should be worth alt-tabbing to — so the sync starts itself, and
 * a card says what is happening rather than leaving the window to change on
 * its own for no visible reason.
 *
 * logs.tf is not instant. An upload can take a minute or two after the
 * server stops, so the first sync often finds nothing and a second one is
 * tried a little later before giving up.
 */

export interface DemoSeen {
  fileName: string;
  at: number;
  /** Syncs attempted for this demo so far. */
  tries: number;
  state: "syncing" | "waiting" | "gaveup";
}

/** How long after a demo to try again when the log is not up yet. */
const RETRY_MS = 90_000;
const MAX_TRIES = 3;

let seen: DemoSeen | null = null;
let listeners: Array<() => void> = [];
let started = false;
let timer: number | null = null;

function set(next: DemoSeen | null) {
  seen = next;
  listeners.forEach((l) => l());
}

export function watchDemos() {
  if (started) return;
  started = true;
  void api
    .onNewDemo((d) => {
      set({ fileName: d.fileName, at: Date.now(), tries: 1, state: "syncing" });
      void startSync(false);
      schedule();
    })
    .catch((e) => noteError({ what: "watching the demos folder", message: String(e) }));
}

/**
 * Try again while logs.tf catches up.
 *
 * There is no way to ask "is my match uploaded yet" other than syncing, so
 * this is a few polite attempts rather than a loop.
 */
function schedule() {
  if (timer !== null) window.clearTimeout(timer);
  timer = window.setTimeout(() => {
    timer = null;
    if (!seen) return;
    if (seen.tries >= MAX_TRIES) {
      set({ ...seen, state: "gaveup" });
      return;
    }
    set({ ...seen, tries: seen.tries + 1, state: "syncing" });
    void startSync(false);
    schedule();
  }, RETRY_MS);
}

/** The card is dismissed, or a new demo replaces it. */
export function dismissDemoSeen() {
  if (timer !== null) window.clearTimeout(timer);
  timer = null;
  set(null);
}

export function useDemoSeen(): DemoSeen | null {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        listeners = listeners.filter((x) => x !== l);
      };
    },
    () => seen,
  );
}
