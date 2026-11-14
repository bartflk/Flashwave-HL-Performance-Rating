import { useSyncExternalStore } from "react";

/**
 * The one-time card that says 6v6 has arrived (Flashy: "a notification
 * after the new update and an easy way to change to 6's"). It shows in a
 * build with 6v6 until it is dismissed, once per machine.
 */

const KEY = "hl.sixesNotice.dismissed";
const listeners: Array<() => void> = [];

function read(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

let dismissed = read();

export function dismissSixesNotice() {
  dismissed = true;
  try {
    localStorage.setItem(KEY, "1");
  } catch {
    // Blocked storage: it stays away for this run.
  }
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.push(l);
  return () => {
    const i = listeners.indexOf(l);
    if (i >= 0) listeners.splice(i, 1);
  };
}

export function useSixesNoticeDismissed(): boolean {
  return useSyncExternalStore(subscribe, () => dismissed);
}
