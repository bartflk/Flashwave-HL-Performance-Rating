import { useSyncExternalStore } from "react";
import type { QueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import type { DailyCheck } from "../api/types";

/**
 * The once-a-day ETF2L check (Flashy: "a check 1 time per day when you
 * start up or whenever this info is checked, and if its correct in the
 * app"): ETF2L's seasons, results and final standings read again, at startup
 * and when the ETF2L tab opens. The backend skips it when it ran in the last
 * day or a sync is reading ETF2L anyway; a sync's end asks again. A card
 * shows only when a medal changed.
 */

let found: DailyCheck | null = null;
let running = false;
const listeners: Array<() => void> = [];

function set(v: DailyCheck | null) {
  found = v;
  for (const l of listeners) l();
}

export async function checkEtf2lDaily(qc: QueryClient, force = false) {
  if (running) return;
  running = true;
  try {
    const r = await api.etf2lDailyCheck(force);
    if (!r.ran) return;
    // Medals, tables and results on every page that shows them: once a
    // day, so everything is simply asked again.
    void qc.invalidateQueries();
    if (r.changed.length > 0) set(r);
  } catch {
    // Quiet: the next start, or the ETF2L tab, tries again.
  } finally {
    running = false;
  }
}

export function dismissEtf2lCheck() {
  set(null);
}

export function useEtf2lCheck(): DailyCheck | null {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => listeners.splice(listeners.indexOf(l), 1);
    },
    () => found,
  );
}
