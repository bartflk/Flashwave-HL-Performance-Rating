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
 * logs.tf is not instant: the upload lands seconds to a minute or two after
 * the server stops. So the app waits ten seconds, then looks every five for
 * two minutes, and syncs the moment the log is there.
 */

export interface DemoSeen {
  fileName: string;
  at: number;
  /** Looks for a new log so far. */
  tries: number;
  /** waiting: before the first look; looking: every few seconds;
   *  syncing: a new log is up and being fetched; gaveup: none came. */
  state: "waiting" | "looking" | "syncing" | "gaveup";
}

/** How long after a demo to look the first time. Straight away was too
 *  soon: the log was not up yet (ivg, Flashy). */
const FIRST_LOOK_MS = 10_000;
/** Then every few seconds, one small request each, not a sync (Flashy). */
const LOOK_EVERY_MS = 5_000;
/** And no longer than this after the first look. */
const LOOK_FOR_MS = 120_000;

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
      set({ fileName: d.fileName, at: Date.now(), tries: 0, state: "waiting" });
      schedule(FIRST_LOOK_MS);
    })
    .catch((e) => noteError({ what: "watching the demos folder", message: String(e) }));
}

/**
 * Look for the log every few seconds until it is up, then sync once.
 *
 * Each look asks only for the newest log you are in (logs.tf, or trends.tf
 * while logs.tf is refusing us); a whole sync each time would be dozens of
 * requests to catch one log.
 */
function schedule(after: number) {
  if (timer !== null) window.clearTimeout(timer);
  timer = window.setTimeout(() => {
    timer = null;
    void look();
  }, after);
}

async function look() {
  const at = seen;
  if (!at || at.state === "syncing" || at.state === "gaveup") return;
  set({ ...at, tries: at.tries + 1, state: "looking" });
  let found = false;
  try {
    const newest = await api.newestLog();
    found = newest !== null && !newest.known;
  } catch (e) {
    noteError({ what: "looking for the new log", message: String(e) });
  }
  // A new demo may have replaced this one while we asked.
  if (seen?.fileName !== at.fileName) return;
  if (found) {
    // Ask first, then mark it syncing and start in the same breath: the card
    // clears when the sync is over, so it must not read "syncing" while the
    // sync store still shows the last one's end.
    let busy = false;
    try {
      busy = await api.syncBusy();
    } catch (e) {
      noteError({ what: "starting the sync for the new log", message: String(e) });
    }
    if (seen?.fileName !== at.fileName) return;
    set({ ...seen, state: "syncing" });
    if (!busy) void startSync(false);
    return;
  }
  if (Date.now() - seen.at >= FIRST_LOOK_MS + LOOK_FOR_MS) {
    set({ ...seen, state: "gaveup" });
    return;
  }
  schedule(LOOK_EVERY_MS);
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
