import type { RglMatch, RglSeasonCard } from "../api/types";

/**
 * The pure parts of RGL's seasons as events (tf2esports' Events page): what
 * state a season is in, how its dates read, which seasons come first, and
 * how often each map of its pool has been played. Kept apart from the
 * components so they can be tested without React.
 */

export type SeasonStatus = "upcoming" | "ongoing" | "completed";

/** The dates the card knows; all in unix seconds. */
type Dates = Pick<RglSeasonCard, "over" | "matches" | "firstMatch" | "lastMatch">;

const WEEK = 7 * 24 * 3600;

/**
 * Where a season is: completed once every division is decided; upcoming
 * while nothing is scheduled, or its first match is still ahead; else
 * ongoing. `now` in unix seconds.
 */
export function seasonStatus(c: Dates, now: number): SeasonStatus {
  if (c.over) return "completed";
  if (c.matches === 0) return "upcoming";
  if (c.firstMatch !== null && c.firstMatch > now) return "upcoming";
  return "ongoing";
}

/** The league week a running season is in (week 1 from its first match), at least 1. */
export function weekNumber(first: number, now: number): number {
  return Math.max(1, Math.floor((now - first) / WEEK) + 1);
}

/** How a season's dates are said: the parts, formatted by the screen. */
export type SpanLabel =
  | { kind: "range"; from: number; to: number }
  | { kind: "starts"; at: number }
  | { kind: "ongoing"; week: number | null; since: number | null }
  | { kind: "none" };

export function spanLabel(c: Dates, now: number): SpanLabel {
  const status = seasonStatus(c, now);
  if (status === "completed") return c.firstMatch !== null && c.lastMatch !== null ? { kind: "range", from: c.firstMatch, to: c.lastMatch } : { kind: "none" };
  if (status === "upcoming") return c.firstMatch !== null ? { kind: "starts", at: c.firstMatch } : { kind: "none" };
  return { kind: "ongoing", week: c.firstMatch !== null ? weekNumber(c.firstMatch, now) : null, since: c.firstMatch };
}

/** tf2esports' order: ongoing first, then upcoming, then completed; newest first within each. */
const RANK: Record<SeasonStatus, number> = { ongoing: 0, upcoming: 1, completed: 2 };

/**
 * Seasons in the order the list shows them. A season whose card has not
 * been read yet keeps its place by id among the completed ones, so the list
 * does not jump about while the cards come in.
 */
export function orderSeasons<S extends { id: number }>(seasons: S[], status: (s: S) => SeasonStatus | undefined): S[] {
  return [...seasons].sort((a, b) => RANK[status(a) ?? "completed"] - RANK[status(b) ?? "completed"] || b.id - a.id);
}

export interface MapRow {
  map: string;
  /** Maps played so far (scores in, forfeits not counted). */
  played: number;
  /** Share of all maps played, 0 to 100. */
  share: number;
  homeWins: number;
  awayWins: number;
  /** Maps neither side won (equal scores). */
  draws: number;
  /** The home side's share of the maps won, 0 to 100; null before a map was won. */
  homeRate: number | null;
}

/**
 * How often each map was played and who won it, home or away, from a
 * season's matches. A map counts once a score is in; a forfeit has none and
 * is not a game. RGL's API does not say who picked a map, so the split is by
 * the side that was at home.
 */
export function mapStats(matches: RglMatch[]): MapRow[] {
  const by = new Map<string, Omit<MapRow, "share" | "homeRate">>();
  for (const m of matches) {
    if (m.isForfeit) continue;
    for (const x of m.maps) {
      if (x.homeScore === null || x.awayScore === null) continue;
      const r = by.get(x.mapName) ?? { map: x.mapName, played: 0, homeWins: 0, awayWins: 0, draws: 0 };
      by.set(x.mapName, r);
      r.played += 1;
      if (x.homeScore > x.awayScore) r.homeWins += 1;
      else if (x.homeScore < x.awayScore) r.awayWins += 1;
      else r.draws += 1;
    }
  }
  const total = [...by.values()].reduce((n, r) => n + r.played, 0);
  return [...by.values()]
    .map((r) => ({ ...r, share: total > 0 ? (r.played / total) * 100 : 0, homeRate: r.homeWins + r.awayWins > 0 ? (r.homeWins / (r.homeWins + r.awayWins)) * 100 : null }))
    .sort((a, b) => b.played - a.played || a.map.localeCompare(b.map));
}

/** How many of a season's matches have a result (a winner or a forfeit). */
export const playedCount = (matches: RglMatch[]) => matches.filter((m) => m.winner !== null || m.isForfeit).length;

/** The champion of each division, from the divisions' final ranks: the team ranked first. */
export function champions<D extends { name: string; teams: Array<{ finalRank: number | null }> }>(divisions: D[]): Array<{ division: D; team: D["teams"][number] }> {
  const out: Array<{ division: D; team: D["teams"][number] }> = [];
  for (const division of divisions) {
    const team = division.teams.find((tm) => tm.finalRank === 1);
    if (team) out.push({ division, team });
  }
  return out;
}
