/**
 * The pure part of table sorting (see sort.tsx for the hook and the header
 * cell): how two cells compare, and how a column puts its rows in order.
 * Kept apart so it can be tested without React.
 */

export type SortValue = number | string | null | undefined;
export type Dir = "asc" | "desc";

/** Nothing in the cell: no value, an empty string, or a number that is not one. */
export function blank(v: SortValue): boolean {
  return v === null || v === undefined || v === "" || (typeof v === "number" && Number.isNaN(v));
}

/** Ascending order, blanks last. */
export function compare(x: SortValue, y: SortValue): number {
  if (blank(x) && blank(y)) return 0;
  if (blank(x)) return 1;
  if (blank(y)) return -1;
  if (typeof x === "number" && typeof y === "number") {
    // `x - y` is NaN for two infinities of the same sign, and a comparator
    // that answers NaN leaves the sort in an order nobody chose.
    return x === y ? 0 : x < y ? -1 : 1;
  }
  return String(x).localeCompare(String(y), undefined, { numeric: true, sensitivity: "base" });
}

/**
 * The rows in the order of one column, in `dir`. Blanks stay at the bottom
 * whichever way it runs, and rows that tie keep the order they came in.
 */
export function sortRows<R>(rows: R[], get: (r: R) => SortValue, dir: Dir): R[] {
  const sign = dir === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    const x = get(a);
    const y = get(b);
    const c = compare(x, y);
    // Blanks are put last by `compare` already; flip only the rest.
    return blank(x) || blank(y) ? c : c * sign;
  });
}
