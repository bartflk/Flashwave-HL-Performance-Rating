import { test } from "node:test";
import assert from "node:assert/strict";
import { champions, mapStats, orderSeasons, playedCount, seasonStatus, spanLabel, weekNumber } from "../src/lib/rglseasons.ts";
import type { RglMatch } from "../src/api/types.ts";

const DAY = 86400;
const NOW = 1_800_000_000;
const c = (o: Partial<{ over: boolean; matches: number; firstMatch: number | null; lastMatch: number | null }> = {}) => ({ over: false, matches: 10, firstMatch: NOW - 10 * DAY, lastMatch: NOW + 40 * DAY, ...o });

test("seasonStatus: completed, upcoming (nothing scheduled or first match ahead), else ongoing", () => {
  assert.equal(seasonStatus(c({ over: true }), NOW), "completed");
  assert.equal(seasonStatus(c({ matches: 0, firstMatch: null }), NOW), "upcoming");
  assert.equal(seasonStatus(c({ firstMatch: NOW + 3 * DAY }), NOW), "upcoming");
  assert.equal(seasonStatus(c(), NOW), "ongoing");
  // Matches scheduled but the dates unread: it has started as far as we can tell.
  assert.equal(seasonStatus(c({ firstMatch: null }), NOW), "ongoing");
});

test("weekNumber counts from the first match and is never below 1", () => {
  assert.equal(weekNumber(NOW, NOW), 1);
  assert.equal(weekNumber(NOW - 7 * DAY, NOW), 2);
  assert.equal(weekNumber(NOW - 20 * DAY, NOW), 3);
  assert.equal(weekNumber(NOW + 5 * DAY, NOW), 1);
});

test("spanLabel: a range when completed, starts when upcoming, week when ongoing", () => {
  assert.deepEqual(spanLabel(c({ over: true }), NOW), { kind: "range", from: NOW - 10 * DAY, to: NOW + 40 * DAY });
  assert.deepEqual(spanLabel(c({ over: true, firstMatch: null }), NOW), { kind: "none" });
  assert.deepEqual(spanLabel(c({ firstMatch: NOW + DAY }), NOW), { kind: "starts", at: NOW + DAY });
  assert.deepEqual(spanLabel(c({ matches: 0, firstMatch: null }), NOW), { kind: "none" });
  assert.deepEqual(spanLabel(c(), NOW), { kind: "ongoing", week: 2, since: NOW - 10 * DAY });
});

test("orderSeasons: ongoing, upcoming, completed; newest first within; unread cards sit with the completed", () => {
  const seasons = [{ id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }, { id: 5 }];
  const status = { 1: "completed", 2: "ongoing", 3: "upcoming", 4: "completed" } as const;
  const out = orderSeasons(seasons, (s) => status[s.id as 1 | 2 | 3 | 4]);
  assert.deepEqual(out.map((s) => s.id), [2, 3, 5, 4, 1]);
  assert.deepEqual(seasons.map((s) => s.id), [1, 2, 3, 4, 5]);
});

let id = 0;
function match(maps: Array<[string, number | null, number | null]>, o: { forfeit?: boolean; winner?: number | null } = {}): RglMatch {
  return {
    matchId: ++id,
    matchDate: null,
    matchName: null,
    isForfeit: o.forfeit ?? false,
    winner: o.winner === undefined ? 1 : o.winner,
    teams: [
      { teamId: 1, teamName: "A", teamTag: null, isHome: true },
      { teamId: 2, teamName: "B", teamTag: null, isHome: false },
    ],
    maps: maps.map(([mapName, homeScore, awayScore]) => ({ mapName, homeScore, awayScore })),
  };
}

test("mapStats counts maps with scores, splits home and away wins, most played first", () => {
  const rows = mapStats([
    match([["pl_upward", 2, 0], ["koth_product", 1, 3]]),
    match([["pl_upward", 0, 2]]),
    match([["pl_upward", 1, 1]]),
    match([["pl_swiftwater", null, null]], { winner: null }),
    match([["pl_badwater", 0, 0]], { forfeit: true }),
  ]);
  assert.deepEqual(rows.map((r) => r.map), ["pl_upward", "koth_product"]);
  const up = rows[0];
  assert.deepEqual([up.played, up.homeWins, up.awayWins, up.draws], [3, 1, 1, 1]);
  assert.equal(up.homeRate, 50);
  assert.equal(Math.round(up.share), 75);
  assert.equal(rows[1].homeRate, 0);
  assert.deepEqual(mapStats([]), []);
});

test("a map nobody won has no home rate", () => {
  assert.equal(mapStats([match([["pl_upward", 1, 1]])])[0].homeRate, null);
});

test("playedCount: a winner or a forfeit is played", () => {
  assert.equal(playedCount([match([]), match([], { winner: null }), match([], { forfeit: true, winner: null })]), 2);
});

test("champions: the team ranked first in each division, none before it is decided", () => {
  const divisions = [
    { name: "Invite", teams: [{ finalRank: 2 }, { finalRank: 1 }] },
    { name: "Main", teams: [{ finalRank: null }, { finalRank: null }] },
    { name: "Newcomer", teams: [{ finalRank: 1 }] },
  ];
  assert.deepEqual(champions(divisions).map((x) => x.division.name), ["Invite", "Newcomer"]);
});
