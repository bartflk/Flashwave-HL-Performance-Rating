import type { RglMatch } from "../api/types";
import { t } from "./i18n";

/**
 * The pure parts of the RGL season page: what counts as decided, which week
 * a match belongs to, and each division's table worked out from its matches
 * (RGL gives only a final rank). Kept apart from RglPage.tsx so they can be
 * tested without React.
 */

/** A match with a result: a winner, or a forfeit (a double forfeit has no winner). */
export const decided = (m: RglMatch) => m.winner !== null || m.isForfeit;

/** "Week 1 - Vigil" -> "Week 1". */
export const weekOf = (m: RglMatch) => (m.matchName ?? "").split(" - ")[0].trim() || t("Other");

/**
 * The week to open on: the first with a match not yet decided, else the
 * last. A double forfeit has no winner but is decided; counting it as
 * undecided would keep the page on that week for the rest of the season.
 */
export function openWeek(matches: RglMatch[]): string | undefined {
  return liveWeek(matches) ?? matches.map(weekOf).at(-1);
}

/** The first week with a match not yet decided; none once the season is played. */
export function liveWeek(matches: RglMatch[]): string | undefined {
  const weeks = [...new Set(matches.map(weekOf))];
  return weeks.find((w) => matches.some((m) => weekOf(m) === w && !decided(m)));
}

export type Standing = { id: number; name: string; tag: string | null; played: number; won: number; lost: number; for: number; against: number };

/** Each division's table from its matches: wins, then losses, then rounds. */
export function standings(matches: RglMatch[]): Map<string, Standing[]> {
  const by = new Map<string, Map<number, Standing>>();
  for (const m of matches) {
    if (m.winner === null || m.teams.length < 2) continue;
    const div = m.divisionName ?? "";
    const rows = by.get(div) ?? new Map<number, Standing>();
    by.set(div, rows);
    for (const tm of m.teams) {
      const r = rows.get(tm.teamId) ?? { id: tm.teamId, name: tm.teamName, tag: tm.teamTag, played: 0, won: 0, lost: 0, for: 0, against: 0 };
      rows.set(tm.teamId, r);
      r.played += 1;
      if (m.winner === tm.teamId) r.won += 1;
      else r.lost += 1;
      for (const x of m.maps) {
        r.for += (tm.isHome ? x.homeScore : x.awayScore) ?? 0;
        r.against += (tm.isHome ? x.awayScore : x.homeScore) ?? 0;
      }
    }
  }
  const out = new Map<string, Standing[]>();
  for (const [div, rows] of by)
    out.set(
      div,
      [...rows.values()].sort((a, b) => b.won - a.won || a.lost - b.lost || b.for - b.against - (a.for - a.against) || a.name.localeCompare(b.name)),
    );
  return out;
}
