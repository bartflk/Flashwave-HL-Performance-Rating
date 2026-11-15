import { test } from "node:test";
import assert from "node:assert/strict";
import { firstWhen, fixturesFor, leaderOf } from "../src/lib/sixeshome.ts";
import type { SixesMatch, SixesRow } from "../src/api/types.ts";

const now = new Date(2026, 10, 15, 12, 0, 0);
const at = (days: number) => Math.floor(new Date(2026, 10, 15 + days, 20, 0, 0).getTime() / 1000);
const team = { id: 1, name: "A", avatar: null };
const m = (id: number, days: number, played = false): SixesMatch => ({ id, competitionId: 1, system: "roundRobin", division: null, tier: null, week: null, round: null, time: at(days), clan1: team, clan2: team, r1: null, r2: null, defaultWin: false, maps: [], played });
const row = (id: number, score: number, dropped = false): SixesRow => ({ id, name: `T${id}`, country: null, mapsPlayed: 0, mapsWon: 0, mapsLost: 0, gcWon: 0, gcLost: 0, penalty: 0, score, byes: 0, seededPoints: 0, dropped });

test("today holds today's officials only; earlier unplayed ones are apart", () => {
  const { shown, overdue } = fixturesFor([m(1, -20), m(2, -1), m(3, 0), m(4, 1)], "today", now);
  assert.deepEqual(shown.map((x) => x.id), [3]);
  assert.deepEqual(overdue.map((x) => x.id), [2, 1], "most recent first");
});

test("the week is today to seven days on, not the past", () => {
  assert.deepEqual(fixturesFor([m(1, -3), m(2, 0), m(3, 7), m(4, 8)], "week", now).shown.map((x) => x.id), [2, 3]);
});

test("the page opens on today only when something is on today", () => {
  assert.equal(firstWhen([m(1, -2), m(2, 3)], now), "week", "an old unplayed match does not make it today");
  assert.equal(firstWhen([m(1, 0)], now), "today");
});

test("the leader is the table's top team that has not dropped, not the most points", () => {
  assert.equal(leaderOf([row(1, 10), row(2, 12)])?.id, 1, "ETF2L's order already has the tie-breaks");
  assert.equal(leaderOf([row(1, 14, true), row(2, 12)])?.id, 2);
  assert.equal(leaderOf([row(1, 0, true)]), null);
});
