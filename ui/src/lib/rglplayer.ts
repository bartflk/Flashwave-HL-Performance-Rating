import type { RglFormMatch, RglMedal, RglPlayerSeason } from "../api/types";

/**
 * The pure parts of the RGL player page: the career's totals, its years,
 * and what a result was to the player's team. Kept apart from Players.tsx so
 * they can be tested without React.
 */

/** RGL's divisions, best first, the order "highest division reached" goes by. */
export const DIVISION_ORDER = ["Invite", "Advanced", "Main", "Intermediate", "Amateur", "Newcomer"];

/** A division's place on the ladder; one RGL does not list ranks last. */
export const divisionRank = (name: string | null): number => {
  const i = DIVISION_ORDER.findIndex((d) => d.toLowerCase() === (name ?? "").toLowerCase());
  return i < 0 ? DIVISION_ORDER.length : i;
};

export interface Career {
  seasons: number;
  teams: number;
  wins: number;
  losses: number;
  /** Win share of decided matches, 0-100, null with none. */
  winPct: number | null;
  firstYear: number | null;
  lastYear: number | null;
  /** The best division they played in, with its season's tier on the ETF2L ladder. */
  highest: { division: string; tier: string | null } | null;
  titles: number;
  podiums: number;
}

const yearOf = (iso: string | null): number | null => {
  if (!iso) return null;
  const y = new Date(iso).getFullYear();
  return Number.isNaN(y) ? null : y;
};

/** The year of a stint, from its start (else the end it has). */
export const stintYear = (s: RglPlayerSeason): number | null => yearOf(s.startedAt) ?? yearOf(s.leftAt);

export function career(seasons: RglPlayerSeason[], medals: RglMedal[]): Career {
  const wins = seasons.reduce((n, s) => n + s.wins, 0);
  const losses = seasons.reduce((n, s) => n + s.losses, 0);
  const years = seasons.map(stintYear).filter((y): y is number => y !== null);
  let best: RglPlayerSeason | null = null;
  for (const s of seasons) {
    if (s.division && (best === null || divisionRank(s.division) < divisionRank(best.division))) best = s;
  }
  return {
    seasons: new Set(seasons.map((s) => s.seasonId)).size,
    teams: new Set(seasons.map((s) => s.teamId)).size,
    wins,
    losses,
    winPct: wins + losses > 0 ? (wins / (wins + losses)) * 100 : null,
    firstYear: years.length ? Math.min(...years) : null,
    lastYear: years.length ? Math.max(...years) : null,
    highest: best?.division ? { division: best.division, tier: best.tier } : null,
    titles: medals.filter((m) => m.place === 1).length,
    podiums: medals.length,
  };
}

/** How many of each place: [gold, silver, bronze]. */
export const medalCounts = (medals: RglMedal[]): [number, number, number] => [1, 2, 3].map((p) => medals.filter((m) => m.place === p).length) as [number, number, number];

export interface YearRow {
  year: number | null;
  stints: RglPlayerSeason[];
  wins: number;
  losses: number;
}

/** Their stints grouped by the year they began, newest year first, a stint
 *  with no date last; within a year, newest season first. */
export function byYear(seasons: RglPlayerSeason[]): YearRow[] {
  const by = new Map<number | null, RglPlayerSeason[]>();
  for (const s of seasons) {
    const y = stintYear(s);
    by.set(y, [...(by.get(y) ?? []), s]);
  }
  return [...by.entries()]
    .sort((a, b) => (a[0] === null ? 1 : b[0] === null ? -1 : b[0] - a[0]))
    .map(([year, stints]) => ({
      year,
      stints: [...stints].sort((a, b) => b.seasonId - a.seasonId || (b.startedAt ?? "").localeCompare(a.startedAt ?? "")),
      wins: stints.reduce((n, s) => n + s.wins, 0),
      losses: stints.reduce((n, s) => n + s.losses, 0),
    }));
}

/** What a result was to the player's team: won, lost, or neither (a
 *  forfeit with no winner named). */
export function resultFor(f: RglFormMatch): "W" | "L" | null {
  if (f.match.winner === null) return null;
  return f.match.winner === f.teamId ? "W" : "L";
}

/** The newest `n` results' letters, oldest first so the squares read left to right. */
export function formLine(form: RglFormMatch[], n = 10): Array<"W" | "L" | null> {
  return form.slice(0, n).map(resultFor).reverse();
}
