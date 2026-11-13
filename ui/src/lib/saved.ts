import { useState } from "react";

/**
 * `useState`, kept in localStorage under `hl.<key>` so it outlasts a tab
 * switch and a restart (Flashy: "i usually look for sniper ... i go to some
 * other tab and go back and i have to click them again"). A stored value
 * `ok` turns down -- an old shape, or storage that was edited -- is thrown
 * away for the default, so a bad value never breaks the page it sets.
 */
export function useSaved<T>(key: string, initial: T, ok: (v: unknown) => v is T): [T, (v: T | ((prev: T) => T)) => void] {
  const storeKey = `hl.${key}`;
  const [value, setValue] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(storeKey);
      if (raw !== null) {
        const v: unknown = JSON.parse(raw);
        if (ok(v)) return v;
        localStorage.removeItem(storeKey);
      }
    } catch {
      // Blocked or broken storage: the default.
    }
    return initial;
  });
  const set = (next: T | ((prev: T) => T)) =>
    setValue((prev) => {
      const v = typeof next === "function" ? (next as (prev: T) => T)(prev) : next;
      try {
        localStorage.setItem(storeKey, JSON.stringify(v));
      } catch {
        // Blocked storage: the choice holds for this session.
      }
      return v;
    });
  return [value, set];
}
