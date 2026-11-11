import { test } from "node:test";
import assert from "node:assert/strict";
import { decided, standings, weekOf } from "../src/lib/rglstandings.ts";
import type { RglMatch } from "../src/api/types.ts";

let id = 0;
function match(o: {
  week?: string | null;
  div?: string | null;
  home: [number, string];
  away: [number, string];
  winner: number | null;
  forfeit?: boolean;
  maps?: Array<[number | null, number | null]>;
}): RglMatch {
  return {
    matchId: ++id,
    matchDate: null,
    matchName: o.week === undefined ? "Week 1 - Vigil" : o.week,
    divisionName: o.div === undefined ? "Advanced" : o.div,
    isForfeit: o.forfeit ?? false,
    winner: o.winner,
    teams: [
      { teamId: o.home[0], teamName: o.home[1], teamTag: null, isHome: true },
      { teamId: o.away[0], teamName: o.away[1], teamTag: null, isHome: false },
    ],
    maps: (o.maps ?? []).map(([h, a], i) => ({ mapName: `m${i}`, homeScore: h, awayScore: a })),
  };
}

test("weekOf: the part before ' - ', or Other", () => {
  assert.equal(weekOf(match({ week: "Week 3 - Product", home: [1, "A"], away: [2, "B"], winner: null })), "Week 3");
  assert.equal(weekOf(match({ week: "Playoffs", home: [1, "A"], away: [2, "B"], winner: null })), "Playoffs");
  assert.equal(weekOf(match({ week: null, home: [1, "A"], away: [2, "B"], winner: null })), "Other");
  assert.equal(weekOf(match({ week: "  - x", home: [1, "A"], away: [2, "B"], winner: null })), "Other");
});

test("standings: wins, losses and rounds for and against", () => {
  const t = standings([
    match({ home: [1, "A"], away: [2, "B"], winner: 1, maps: [[5, 2], [3, 1]] }),
    match({ home: [2, "B"], away: [1, "A"], winner: 2, maps: [[5, 0]] }),
  ]).get("Advanced")!;
  const a = t.find((r) => r.id === 1)!;
  const b = t.find((r) => r.id === 2)!;
  assert.deepEqual([a.played, a.won, a.lost, a.for, a.against], [2, 1, 1, 8, 8]);
  assert.deepEqual([b.played, b.won, b.lost, b.for, b.against], [2, 1, 1, 8, 8]);
});

test("standings: ties on wins and losses are split by round difference, then name", () => {
  const t = standings([
    match({ home: [1, "Zed"], away: [3, "Cee"], winner: 1, maps: [[5, 4]] }),
    match({ home: [2, "Abe"], away: [4, "Dee"], winner: 2, maps: [[5, 0]] }),
    match({ home: [5, "Bob"], away: [6, "Fay"], winner: 5, maps: [[5, 4]] }),
  ]).get("Advanced")!;
  // Winners: Abe (+5), then Bob and Zed (+1 each, by name), then losers.
  assert.deepEqual(t.slice(0, 3).map((r) => r.name), ["Abe", "Bob", "Zed"]);
});

test("standings: forfeit with a winner counts as a win and a loss with no rounds", () => {
  const t = standings([match({ home: [1, "A"], away: [2, "B"], winner: 2, forfeit: true })]).get("Advanced")!;
  assert.deepEqual(t.map((r) => [r.name, r.won, r.lost, r.for, r.against]), [["B", 1, 0, 0, 0], ["A", 0, 1, 0, 0]]);
});

test("standings: a double forfeit is a loss for both, a match not played yet counts for nobody", () => {
  const t = standings([
    match({ home: [1, "A"], away: [2, "B"], winner: null, forfeit: true }),
    match({ home: [1, "A"], away: [2, "B"], winner: null }),
  ]).get("Advanced")!;
  assert.deepEqual(t.map((r) => [r.played, r.won, r.lost]), [[1, 0, 1], [1, 0, 1]]);
});

test("standings: playoffs do not count in a season that has weeks, a Bye Week is no game", () => {
  const t = standings([
    match({ week: "Week 1 - Vigil", home: [1, "A"], away: [2, "B"], winner: 1 }),
    match({ week: "Semi Final", home: [1, "A"], away: [2, "B"], winner: 2 }),
    match({ week: "Week 2 - Vigil", home: [1, "A"], away: [3, "Bye Week - Advanced"], winner: 1 }),
  ]).get("Advanced")!;
  assert.deepEqual(t.map((r) => [r.id, r.played, r.won]), [[1, 1, 1], [2, 1, 0]]);
});

test("standings: a match with fewer than two teams (a bye) is skipped", () => {
  const m = match({ home: [1, "A"], away: [2, "B"], winner: 1 });
  m.teams = [m.teams[0]];
  assert.equal(standings([m]).size, 0);
  assert.equal(standings([]).size, 0);
});

test("standings: null round scores count as zero, never NaN", () => {
  const t = standings([match({ home: [1, "A"], away: [2, "B"], winner: 1, maps: [[null, null], [5, null]] })]).get("Advanced")!;
  assert.deepEqual([t[0].for, t[0].against, t[1].for, t[1].against], [5, 0, 0, 5]);
});

test("standings: divisions are kept apart, groups 2A and 2B included; a division-less match is '' ", () => {
  const t = standings([
    match({ div: "Group 2A", home: [1, "A"], away: [2, "B"], winner: 1 }),
    match({ div: "Group 2B", home: [1, "A"], away: [3, "C"], winner: 3 }),
    match({ div: null, home: [8, "X"], away: [9, "Y"], winner: 8 }),
  ]);
  assert.deepEqual([...t.keys()].sort(), ["", "Group 2A", "Group 2B"]);
  assert.equal(t.get("Group 2A")!.length, 2);
  // Team 1 plays in both groups with a separate record in each.
  assert.equal(t.get("Group 2A")!.find((r) => r.id === 1)!.won, 1);
  assert.equal(t.get("Group 2B")!.find((r) => r.id === 1)!.lost, 1);
});

test("standings: a team renamed mid-season is one row, under the name it first had", () => {
  const t = standings([
    match({ home: [1, "Old name"], away: [2, "B"], winner: 1 }),
    match({ home: [1, "New name"], away: [3, "C"], winner: 1 }),
  ]).get("Advanced")!;
  const rows = t.filter((r) => r.id === 1);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].played, 2);
});

test("decided: a winner or a forfeit", () => {
  assert.equal(decided(match({ home: [1, "A"], away: [2, "B"], winner: 1 })), true);
  assert.equal(decided(match({ home: [1, "A"], away: [2, "B"], winner: null, forfeit: true })), true);
  assert.equal(decided(match({ home: [1, "A"], away: [2, "B"], winner: null })), false);
});

