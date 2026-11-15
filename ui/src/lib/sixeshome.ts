import type { SixesMatch, SixesRow } from "../api/types";

/** Which officials the 6v6 Home lists. */
export type When = "today" | "tomorrow" | "week";

/** The day a time falls on, in local time, counted from `now`'s day. */
export function dayOffset(time: number, now: Date = new Date()): number {
  const d = new Date(time * 1000);
  const a = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const b = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  return Math.round((a - b) / 86_400_000);
}

/**
 * The officials for `when`, and the ones whose day has passed unplayed
 * (audit F7): "Today" used to hold every match due on any earlier day, so
 * late in a season it filled with weeks-old ones never reported.
 */
export function fixturesFor(upcoming: SixesMatch[], when: When, now: Date = new Date()): { shown: SixesMatch[]; overdue: SixesMatch[] } {
  const dated = upcoming.filter((m) => m.time !== null && !m.played);
  const day = (m: SixesMatch) => dayOffset(m.time!, now);
  const shown = dated.filter((m) => (when === "today" ? day(m) === 0 : when === "tomorrow" ? day(m) === 1 : day(m) >= 0 && day(m) <= 7));
  const overdue = dated.filter((m) => day(m) < 0).sort((a, b) => b.time! - a.time!);
  return { shown, overdue };
}

/** The list opened first: today's when there are any, else the week's. */
export function firstWhen(upcoming: SixesMatch[], now: Date = new Date()): When {
  return upcoming.some((m) => m.time !== null && !m.played && dayOffset(m.time, now) === 0) ? "today" : "week";
}

/**
 * A division's leader (audit F2): the top of ETF2L's table, which comes in
 * place order with its tie-breaks applied, skipping a team that dropped out.
 * Sorting by points named the wrong team on a tie, or a dropped one.
 */
export function leaderOf(teams: SixesRow[]): SixesRow | null {
  return teams.find((t) => !t.dropped) ?? null;
}
