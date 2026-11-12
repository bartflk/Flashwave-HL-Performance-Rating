// What an RGL match page does with its logs' numbers, apart from drawing them.
import type { RglMapStats, RglStatPlayer } from "../api/types";

type Side = RglMapStats["sides"][number];

/** A map's two sides with the home team's first. The backend says which RGL
 *  team played which colour when the rosters made it clear; when they did not,
 *  the colours keep their order (RED, BLU). */
export function sidesInOrder(map: RglMapStats, homeId: number | undefined): [Side, Side] {
  const [a, b] = map.sides;
  const swap = homeId !== undefined && (b.teamId === homeId || (a.teamId !== null && a.teamId !== homeId && b.teamId === null));
  return swap ? [b, a] : [a, b];
}

/** The players of one side, in the table's order. */
export function playersOf(map: RglMapStats, side: Side["side"]): RglStatPlayer[] {
  return map.players.filter((p) => p.side === side);
}

/** The classes a map's players mostly played, Scout to Spy, each once. */
export function classesIn(map: RglMapStats): string[] {
  const seen: string[] = [];
  for (const p of map.players) if (p.class && !seen.includes(p.class)) seen.push(p.class);
  return seen;
}

/** The two players of a class, one from each side (the first side's first).
 *  A side that fielded two of the class (a stand-in who played a while) is
 *  represented by whoever played it longest. */
export function matchup(map: RglMapStats, cls: string, first: Side["side"]): [RglStatPlayer | null, RglStatPlayer | null] {
  const pick = (side: Side["side"]) =>
    map.players
      .filter((p) => p.class === cls && p.side === side)
      .sort((x, y) => y.minutes - x.minutes)[0] ?? null;
  const second = first === "Red" ? "Blue" : "Red";
  return [pick(first), pick(second)];
}

/** A player's rating over the match: the average of their maps' ratings,
 *  each weighted by the minutes played. Keyed by steam id; a player with no
 *  rated map is left out. */
export function matchRatings(maps: RglMapStats[]): Map<string, number> {
  const sums = new Map<string, { score: number; minutes: number }>();
  for (const m of maps) {
    for (const p of m.players) {
      if (p.rating === null) continue;
      const w = Math.max(p.minutes, 0.01);
      const s = sums.get(p.steamId) ?? { score: 0, minutes: 0 };
      s.score += p.rating * w;
      s.minutes += w;
      sums.set(p.steamId, s);
    }
  }
  return new Map([...sums].map(([id, s]) => [id, s.score / s.minutes]));
}

/** Which of two numbers is ahead (lower is better for deaths): 0 left, 1 right, -1 a tie. */
export function ahead(left: number | null, right: number | null, lowerBetter = false): -1 | 0 | 1 {
  if (left === null || right === null || left === right) return -1;
  return (left < right) === lowerBetter ? 0 : 1;
}
