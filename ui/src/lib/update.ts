import { useSyncExternalStore } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { inTauri } from "../api/client";
import { noteError } from "./problems";

/**
 * Checking for a new version, and installing it.
 *
 * The app ships unsigned, so every manual update means downloading an
 * installer and talking Windows out of blocking it. Doing it in-app avoids
 * that entirely: the updater fetches the installer itself, checks it
 * against a signature made with our own key, and runs it.
 *
 * **That signature is not code signing.** It is a minisign keypair Tauri
 * generated, and it answers a different question: not "is this publisher
 * trusted by Microsoft" but "was this build made by whoever holds the
 * private key". Nobody can push a malicious update to installed clients
 * without that key, even if they take over the GitHub release page.
 *
 * The manifest lives at the `releases/latest/download/latest.json` URL,
 * which GitHub always points at the newest release — so publishing a
 * release *is* publishing the update.
 */

export type UpdateState =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "available"; version: string; notes: string; update: Update }
  | { state: "downloading"; version: string; got: number; total: number | null }
  | { state: "ready"; version: string }
  | { state: "failed"; message: string };

let status: UpdateState = { state: "idle" };
let listeners: Array<() => void> = [];
let checked = false;

function set(next: UpdateState) {
  status = next;
  listeners.forEach((l) => l());
}

/**
 * Look once, quietly.
 *
 * A failure here is written to the problem list but never shown: an app
 * that cannot reach GitHub on startup is not broken, and a person who did
 * not ask about updates should not be told about a network error.
 */
export async function checkForUpdate(manual = false) {
  if (!inTauri) return;
  if (checked && !manual) return;
  checked = true;
  set({ state: "checking" });
  try {
    const update = await check();
    if (!update) {
      set({ state: "idle" });
      return;
    }
    set({
      state: "available",
      version: update.version,
      notes: update.body ?? "",
      update,
    });
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    noteError({ what: "checking for an update", message });
    set(manual ? { state: "failed", message } : { state: "idle" });
  }
}

/** Download and install, then offer to restart. */
export async function installUpdate() {
  if (status.state !== "available") return;
  const { update, version } = status;
  set({ state: "downloading", version, got: 0, total: null });
  try {
    let got = 0;
    let total: number | null = null;
    await update.downloadAndInstall((e) => {
      if (e.event === "Started") total = e.data.contentLength ?? null;
      else if (e.event === "Progress") {
        got += e.data.chunkLength;
        set({ state: "downloading", version, got, total });
      } else if (e.event === "Finished") set({ state: "ready", version });
    });
    set({ state: "ready", version });
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    noteError({ what: `installing ${version}`, message });
    set({ state: "failed", message });
  }
}

/**
 * Restart into the new version.
 *
 * The database is the reason this is a button rather than automatic: the
 * app holds a lock on it while the window is open, and the installer
 * replacing files under a running process is exactly the kind of thing
 * that corrupted it before. Closing properly first is the safe order.
 */
export async function restartNow() {
  try {
    await relaunch();
  } catch (e) {
    noteError({ what: "restarting", message: e instanceof Error ? e.message : String(e) });
  }
}

export function dismissUpdate() {
  set({ state: "idle" });
}

export function useUpdate(): UpdateState {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        listeners = listeners.filter((x) => x !== l);
      };
    },
    () => status,
  );
}
