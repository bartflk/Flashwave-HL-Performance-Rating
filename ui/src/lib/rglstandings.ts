import type { RglMatch } from "../api/types";
import { t } from "./i18n";

/**
 * The pure parts of the RGL season page: what counts as decided, which week
 * a match belongs to, and each division's table worked out from its matches.
 * Kept apart from RglPage.tsx so they can be tested without React.
 */

/** A match with a result: a winner, or a forfeit. */
export const decided = (m: RglMatch) => m.winner !== null || m.isForfeit;

/** "Week 1 - Vigil" is week "Week 1"; a playoff round keeps its own name. */
export const weekOf = (m: RglMatch) => (m.matchName ?? "").split(" - ")[0].trim() || t("Other");

export type Standing = { id: number; name: string; tag: string | null; played: number; won: number; lost: number; for: number; against: number };

/** A regular-season week ("Week 3 - Vigil"); playoffs are rounds of their own. */
const isWeek = (m: RglMatch) => /^week\b/i.test((m.matchName ?? "").trim());
/** A pairing with RGL's placeholder for a team that has the week off. */
const isBye = (m: RglMatch) => m.teams.some((x) => x.teamName.startsWith("Bye Week"));

/**
 * Each division's table from its matches: wins, then losses, then rounds.
 * Only the regular season counts (playoffs would add games to a table they
 * are not part of), a Bye Week is no game, and a forfeit with no winner named
 * is a loss for both. RGL ranks by points, which its API does not give, so
 * the table is by wins and losses and says so.
 */
export function standings(matches: RglMatch[]): Map<string, Standing[]> {
  const by = new Map<string, Map<number, Standing>>();
  // A season whose matches carry no week names at all is counted whole.
  const weeks = matches.some(isWeek);
  for (const m of matches) {
    if (m.teams.length < 2 || isBye(m) || (weeks && !isWeek(m))) continue;
    if (!decided(m)) continue;
    const div = m.divisionName ?? "";
    const rows = by.get(div) ?? new Map<number, Standing>();
    by.set(div, rows);
    for (const tm of m.teams) {
      const r = rows.get(tm.teamId) ?? { id: tm.teamId, name: tm.teamName, tag: tm.teamTag, played: 0, won: 0, lost: 0, for: 0, against: 0 };
      rows.set(tm.teamId, r);
      r.played += 1;
      if (m.winner === tm.teamId) r.won += 1;
      else r.lost += 1;
      if (m.isForfeit) continue;
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
