import { test } from "node:test";
import assert from "node:assert/strict";
import { ahead, classesIn, matchRatings, matchup, sidesInOrder } from "../src/lib/rglstats.ts";
import type { RglMapStats, RglSideTotals, RglStatPlayer } from "../src/api/types.ts";

const side = (s: "Red" | "Blue", teamId: number | null): RglSideTotals => ({ side: s, teamId, score: 0, kills: 0, deaths: 0, damage: 0, heals: 0, ubers: 0, drops: 0 });
const player = (steamId: string, s: "Red" | "Blue", cls: string, minutes: number, rating: number | null): RglStatPlayer => ({
  steamId, name: steamId, side: s, teamId: null, class: cls, minutes, kills: 0, assists: 0, deaths: 0, damage: 0, dpm: null, dtpm: null, dpmVs: null, dpmVsBasis: null, headshots: 0, heals: 0, ubers: 0, drops: 0, rating,
});
const map = (players: RglStatPlayer[], red: number | null, blue: number | null): RglMapStats => ({
  logId: 1, map: "pl_x", minutes: 20, sides: [side("Red", red), side("Blue", blue)], players,
  awards: { mvp: null, medicMvp: null, topFragger: null, mostHeadshots: null }, rated: true,
});

test("the home team's side comes first", () => {
  assert.equal(sidesInOrder(map([], 10, 20), 10)[0].side, "Red");
  assert.equal(sidesInOrder(map([], 20, 10), 10)[0].side, "Blue");
  // Only one colour known: the other is the home team's if this one is not.
  assert.equal(sidesInOrder(map([], 20, null), 10)[0].side, "Blue");
  // Nothing known: the colours keep their order.
  assert.equal(sidesInOrder(map([], null, null), 10)[0].side, "Red");
  assert.equal(sidesInOrder(map([], 10, 20), undefined)[0].side, "Red");
});

test("a player's match rating weighs each map by the minutes played", () => {
  const a = map([player("p", "Red", "soldier", 30, 1.5), player("q", "Blue", "scout", 20, null)], null, null);
  const b = map([player("p", "Red", "soldier", 10, 0.9)], null, null);
  const r = matchRatings([a, b]);
  assert.ok(Math.abs(r.get("p")! - (1.5 * 30 + 0.9 * 10) / 40) < 1e-9);
  assert.equal(r.has("q"), false, "unrated: left out");
});

test("a class's matchup is one player a side, the longest played", () => {
  const m = map([player("r1", "Red", "medic", 5, null), player("r2", "Red", "medic", 15, null), player("b1", "Blue", "medic", 20, null), player("b2", "Blue", "scout", 20, null)], null, null);
  const [l, r] = matchup(m, "medic", "Red");
  assert.equal(l?.steamId, "r2");
  assert.equal(r?.steamId, "b1");
  assert.equal(matchup(m, "scout", "Red")[0], null);
  assert.deepEqual(classesIn(m), ["medic", "scout"]);
});

test("which of two numbers is ahead", () => {
  assert.equal(ahead(5, 3), 0);
  assert.equal(ahead(3, 5), 1);
  assert.equal(ahead(3, 5, true), 0);
  assert.equal(ahead(4, 4), -1);
  assert.equal(ahead(null, 4), -1);
});
