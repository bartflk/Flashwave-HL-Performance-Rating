import { useSyncExternalStore } from "react";
import { api } from "../api/client";

/**
 * Demo downloads, tracked app-wide rather than by the panel that started
 * them.
 *
 * A SourceTV demo is a hundred megabytes and takes half a minute, and the
 * fetch runs in the backend whatever the window is showing — so the progress
 * has to outlive the match page it was started from. Anything that wants to
 * show it reads this store; the backend's events are listened to once.
 */

export interface Download {
  logId: number;
  /** What the match is, for the card: "Upward, 17 Sept". */
  label: string;
  bytes: number;
  total: number | null;
  state: "running" | "done" | "failed";
  error?: string;
  /** Set when it finished, so a card can be dismissed on its own terms. */
  finishedAt?: number;
}

let downloads: Download[] = [];
let listeners: Array<() => void> = [];
let started = false;

function emit() {
  // A new array each time: the store is compared by identity.
  downloads = [...downloads];
  listeners.forEach((l) => l());
}

function put(logId: number, patch: Partial<Download>, label?: string) {
  const at = downloads.findIndex((d) => d.logId === logId);
  if (at === -1) {
    downloads.push({ logId, label: label ?? `log ${logId}`, bytes: 0, total: null, state: "running", ...patch });
  } else {
    downloads[at] = { ...downloads[at], ...patch };
  }
  emit();
}

/** Start listening to the backend's download events. Safe to call twice. */
export function watchDownloads() {
  if (started) return;
  started = true;
  void api.onStv({
    onProgress: (p) => put(p.logId, { bytes: p.bytes, total: p.total, state: "running" }),
    onDone: (d) => put(d.logId, { state: "done", finishedAt: Date.now(), bytes: d.bytes }),
    onError: (e) => put(e.logId, { state: "failed", error: e.message, finishedAt: Date.now() }),
  });
}

/** Note a download about to start, so its card appears at once. */
export function beginDownload(logId: number, label: string) {
  put(logId, { bytes: 0, total: null, state: "running" }, label);
}

export function dismissDownload(logId: number) {
  downloads = downloads.filter((d) => d.logId !== logId);
  emit();
}

export function useDownloads(): Download[] {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        listeners = listeners.filter((x) => x !== l);
      };
    },
    () => downloads,
  );
}

/** Whether this match has a download in flight, for a panel's own button. */
export function useDownload(logId: number): Download | null {
  const all = useDownloads();
  return all.find((d) => d.logId === logId) ?? null;
}
