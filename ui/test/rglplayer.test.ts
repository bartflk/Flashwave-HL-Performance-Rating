import { test } from "node:test";
import assert from "node:assert/strict";
import { byYear, career, divisionRank, formLine, medalCounts, resultFor } from "../src/lib/rglplayer.ts";
import type { RglFormMatch, RglMedal, RglPlayerSeason } from "../src/api/types.ts";

const stint = (o: Partial<RglPlayerSeason> & { seasonId: number }): RglPlayerSeason => ({
  seasonName: `HL Season ${o.seasonId}`,
  division: "Main",
  tier: "Mid",
  teamId: o.seasonId * 10,
  teamName: "Team",
  teamTag: null,
  startedAt: "2025-03-01T00:00:00Z",
  leftAt: null,
  wins: 5,
  losses: 2,
  ...o,
});
const medal = (place: number): RglMedal => ({ seasonId: 1, seasonName: "S", teamId: place, teamName: "T", division: "Main", place });

test("the career totals, span and highest division", () => {
  const seasons = [
    stint({ seasonId: 24, division: "Invite", tier: "Premiership", startedAt: "2026-05-03T00:00:00Z", wins: 9, losses: 1, teamId: 1 }),
    stint({ seasonId: 23, division: "Advanced", tier: "High", startedAt: "2025-10-03T00:00:00Z", wins: 7, losses: 4, teamId: 1 }),
    stint({ seasonId: 20, division: "Main", startedAt: "2024-03-01T00:00:00Z", wins: 4, losses: 5, teamId: 2 }),
  ];
  const c = career(seasons, [medal(1), medal(2)]);
  assert.deepEqual([c.seasons, c.teams, c.wins, c.losses], [3, 2, 20, 10]);
  assert.equal(Math.round(c.winPct ?? 0), 67);
  assert.deepEqual([c.firstYear, c.lastYear], [2024, 2026]);
  assert.deepEqual(c.highest, { division: "Invite", tier: "Premiership" });
  assert.deepEqual([c.titles, c.podiums], [1, 2]);
});

test("an empty career has no percentage, span or division", () => {
  const c = career([], []);
  assert.equal(c.winPct, null);
  assert.equal(c.firstYear, null);
  assert.equal(c.highest, null);
});

test("divisions rank from Invite down and unknown ones last", () => {
  assert.ok(divisionRank("Invite") < divisionRank("Advanced"));
  assert.ok(divisionRank("Newcomer") < divisionRank("Something else"));
  assert.equal(divisionRank("advanced"), 1);
});

test("medals are counted by place", () => {
  assert.deepEqual(medalCounts([medal(1), medal(1), medal(3)]), [2, 0, 1]);
});

test("stints group by the year they began, newest first, undated last", () => {
  const rows = byYear([
    stint({ seasonId: 20, startedAt: "2024-03-01T00:00:00Z" }),
    stint({ seasonId: 24, startedAt: "2026-05-01T00:00:00Z" }),
    stint({ seasonId: 25, startedAt: "2026-09-01T00:00:00Z" }),
    stint({ seasonId: 5, startedAt: null, leftAt: null }),
  ]);
  assert.deepEqual(rows.map((r) => r.year), [2026, 2024, null]);
  assert.deepEqual(rows[0].stints.map((s) => s.seasonId), [25, 24]);
  assert.equal(rows[0].wins, 10);
});

const form = (teamId: number, winner: number | null): RglFormMatch => ({
  teamId,
  match: { matchId: 1, matchDate: null, matchName: null, divisionName: null, isForfeit: false, winner, teams: [], maps: [] },
});

test("a result is a win or a loss for the player's own team", () => {
  assert.equal(resultFor(form(7, 7)), "W");
  assert.equal(resultFor(form(7, 8)), "L");
  assert.equal(resultFor(form(7, null)), null);
});

test("the form line is the newest results, oldest first", () => {
  // Newest first in: W, L, L.
  assert.deepEqual(formLine([form(7, 7), form(7, 8), form(7, 8)], 2), ["L", "W"]);
});
