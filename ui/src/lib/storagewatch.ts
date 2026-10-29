import { useSyncExternalStore } from "react";
import { api } from "../api/client";

/**
 * A card when the app's copies pile up (Flashy: "a notification if you have
 * more than 2 backups and also notify you if you exceed 5 gb"): more than
 * two backups, or the database, its backups and what was left behind them
 * over 5 GB together. Looked at when the app starts and after every sync.
 * Dismissed, it stays away until there are more backups or another GB.
 */

const GB = 1024 ** 3;
const MAX_BACKUPS = 2;
const MAX_BYTES = 5 * GB;
const KEY = "hl.storageNotice.dismissed";

export interface StorageNotice {
  backups: number;
  backupsBytes: number;
  /** Database, backups and leftovers together. */
  total: number;
  tooMany: boolean;
  tooBig: boolean;
}

let found: StorageNotice | null = null;
const listeners: Array<() => void> = [];

function set(v: StorageNotice | null) {
  found = v;
  for (const l of listeners) l();
}

function dismissed(): { backups: number; gb: number } | null {
  try {
    const s = localStorage.getItem(KEY);
    return s ? (JSON.parse(s) as { backups: number; gb: number }) : null;
  } catch {
    return null;
  }
}

export async function checkStorage() {
  try {
    const o = await api.getStorageOverview();
    const total = o.database + o.backupsBytes + o.straysBytes;
    const backups = o.backups.length;
    const tooMany = backups > MAX_BACKUPS;
    const tooBig = total > MAX_BYTES;
    if (!tooMany && !tooBig) return set(null);
    const d = dismissed();
    if (d && backups <= d.backups && Math.floor(total / GB) <= d.gb) return set(null);
    set({ backups, backupsBytes: o.backupsBytes, total, tooMany, tooBig });
  } catch {
    // Quiet: the next start or sync looks again.
  }
}

export function dismissStorage() {
  if (found) {
    try {
      localStorage.setItem(KEY, JSON.stringify({ backups: found.backups, gb: Math.floor(found.total / GB) }));
    } catch {
      // Blocked storage: it may show again next start.
    }
  }
  set(null);
}

export function useStorageNotice(): StorageNotice | null {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => listeners.splice(listeners.indexOf(l), 1);
    },
    () => found,
  );
}

/** Gigabytes, one decimal: "5.3 GB". */
export function gb(bytes: number): string {
  return `${(bytes / GB).toFixed(1)} GB`;
}
