import type { RglRankRow } from "../api/types";

/**
 * The pure parts of the RGL power ranking page: its divisions, the rows
 * under a division filter, a team's row among them, and how a week's change
 * reads. Kept apart from the components so they can be tested without React.
 */

/** The divisions in the table, best first (the rows come best first). */
export function divisionsOf(rows: RglRankRow[]): string[] {
  const out: string[] = [];
  for (const r of rows) if (r.division && !out.includes(r.division)) out.push(r.division);
  return out;
}

/** The rows of one division, or all of them; ranks stay the table's. */
export function inDivision(rows: RglRankRow[], division: string | null): RglRankRow[] {
  return division === null ? rows : rows.filter((r) => r.division === division);
}

/** A team's row, whichever of its seasons' ids is known. */
export function rowOf(rows: RglRankRow[] | undefined, teamId: number): RglRankRow | null {
  return rows?.find((r) => r.id === teamId || r.ids.includes(teamId)) ?? null;
}

/** Whether any of these team ids is the row's team. */
export function isMine(row: RglRankRow, mine: ReadonlySet<number>): boolean {
  return mine.has(row.id) || row.ids.some((i) => mine.has(i));
}

/** The "+/-" cell: places gained or lost since a week ago, a dash for none or unknown. */
export function changeOf(change: number | null): { kind: "up" | "down" | "same" | "none"; places: number } {
  if (change === null) return { kind: "none", places: 0 };
  if (change > 0) return { kind: "up", places: change };
  if (change < 0) return { kind: "down", places: -change };
  return { kind: "same", places: 0 };
}
