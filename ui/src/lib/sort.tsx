import { useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { StatTip } from "../components/StatTip";
import type { Entry, GlossaryKey } from "./glossary";
import { compare, sortRows, type Dir, type SortValue } from "./compare";

export { compare };
export type { Dir, SortValue };

/**
 * Every table sorts by its columns (Q79, Flashy: "all the tables in the app
 * to be clickable where you can sort by per columns").
 *
 * A table names its columns and how to read each from a row; `useSort`
 * keeps the choice -- per table, across visits -- and hands back the rows in
 * that order; `SortTh` is the header cell that sets it. A number column
 * sorts biggest first on the first click, a text column A to Z; a second
 * click reverses it, and rows with nothing in the column stay at the bottom
 * either way.
 */

export type SortState = { key: string; dir: Dir } | null;

export interface Sorter {
  sort: SortState;
  toggle: (key: string) => void;
}

const KEY = (id: string) => `hl.sort.${id}`;

function read(id: string): SortState {
  try {
    const v = JSON.parse(localStorage.getItem(KEY(id)) ?? "null");
    return v && typeof v.key === "string" && (v.dir === "asc" || v.dir === "desc") ? v : null;
  } catch {
    return null;
  }
}

function write(id: string, s: SortState) {
  try {
    if (s) localStorage.setItem(KEY(id), JSON.stringify(s));
    else localStorage.removeItem(KEY(id));
  } catch {
    // Blocked storage: the sort still applies until the page closes.
  }
}

/**
 * `id` names the table for remembering its sort; `columns` reads each
 * sortable column from a row; `initial` is the order before any click
 * (`null`: as given).
 */
export function useSort<R>(id: string, rows: R[], columns: Record<string, (r: R) => SortValue>, initial: SortState = null): Sorter & { sorted: R[] } {
  const [sort, setSort] = useState<SortState>(() => read(id) ?? initial);
  const cols = useRef(columns);
  cols.current = columns;
  const sorted = useMemo(() => {
    const get = sort ? cols.current[sort.key] : undefined;
    if (!sort || !get) return rows;
    return sortRows(rows, get, sort.dir);
  }, [rows, sort]);
  const toggle = (key: string) => {
    setSort((s) => {
      let next: SortState;
      if (s?.key === key) next = { key, dir: s.dir === "asc" ? "desc" : "asc" };
      else {
        // Numbers start biggest first, text A to Z.
        const first = rows.map((r) => cols.current[key]?.(r)).find((v) => v !== null && v !== undefined && v !== "");
        next = { key, dir: typeof first === "number" ? "desc" : "asc" };
      }
      write(id, next);
      return next;
    });
  };
  return { sorted, sort, toggle };
}

/** A header cell that sorts its table by `k`. */
export function SortTh({ k, s, children, className, title, tip }: { k: string; s: Sorter; children: ReactNode; className?: string; title?: string; tip?: GlossaryKey | Entry }) {
  const active = s.sort?.key === k;
  const dir = active ? s.sort!.dir : null;
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      s.toggle(k);
    }
  };
  return (
    <th
      className={`${className ?? ""} sortable${active ? " sorted" : ""}`}
      aria-sort={dir === "asc" ? "ascending" : dir === "desc" ? "descending" : "none"}
      tabIndex={0}
      onClick={() => s.toggle(k)}
      onKeyDown={onKey}
      title={title}
    >
      {tip ? (
        <StatTip {...(typeof tip === "string" ? { k: tip } : { entry: tip })} inline>
          {children}
        </StatTip>
      ) : (
        children
      )}
      <span className="sort-arrow" aria-hidden>
        {dir === "asc" ? "▲" : dir === "desc" ? "▼" : "↕"}
      </span>
    </th>
  );
}
