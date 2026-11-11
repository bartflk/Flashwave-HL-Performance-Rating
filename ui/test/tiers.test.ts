import { test } from "node:test";
import assert from "node:assert/strict";
import { percentileOf, redGreen, tierColour, tierOf, tierText } from "../src/lib/tiers.ts";
import type { StatScales } from "../src/api/types.ts";

const HEX = /^#[0-9a-f]{6}$/;

test("percentileOf: below, between, at and above the cuts", () => {
  const cuts = [100, 200, 300];
  const pcts = [0, 50, 100];
  assert.equal(percentileOf(50, cuts, pcts), 0);
  assert.equal(percentileOf(100, cuts, pcts), 0);
  assert.equal(percentileOf(150, cuts, pcts), 25);
  assert.equal(percentileOf(200, cuts, pcts), 50);
  assert.equal(percentileOf(250, cuts, pcts), 75);
  assert.equal(percentileOf(300, cuts, pcts), 100);
  assert.equal(percentileOf(1e9, cuts, pcts), 100);
});

test("percentileOf: no scale, or one that does not line up, is null", () => {
  assert.equal(percentileOf(1, [], []), null);
  assert.equal(percentileOf(1, [1, 2], [0]), null);
});

test("percentileOf: a flat stretch of cuts does not divide by zero", () => {
  const p = percentileOf(5, [5, 5, 5, 10], [0, 40, 60, 100]);
  assert.ok(p !== null && Number.isFinite(p));
  assert.ok(Number.isFinite(percentileOf(5, [0, 5, 5, 10], [0, 40, 60, 100])!));
});

test("percentileOf: a single cut", () => {
  assert.equal(percentileOf(1, [5], [50]), 0);
  assert.equal(percentileOf(9, [5], [50]), 100);
});

test("redGreen: always a valid colour, ends are the end stops", () => {
  assert.equal(redGreen(0), "#e5534b");
  assert.equal(redGreen(100), "#3fcf6e");
  for (const p of [0, 1, 19.9, 20, 55, 74, 99.9, 100, 250, -10, -1e9, Infinity]) assert.match(redGreen(p), HEX, `pct ${p}`);
});

test("redGreen: out-of-range percentiles clamp to the end stops", () => {
  assert.equal(redGreen(-10), redGreen(0));
  assert.equal(redGreen(1000), redGreen(100));
});

test("tierColour: valid colours across the range, black at the top", () => {
  for (const p of [0, 0.5, 10, 50, 90, 99, 99.9]) assert.match(tierColour(p, []), HEX, `pct ${p}`);
  assert.equal(tierColour(100, []), "#000000");
  assert.equal(tierColour(-5, []), tierColour(0, []));
});

test("tierColour: a NaN percentile does not become the black of the top 1%", () => {
  assert.notEqual(tierColour(NaN, []), "#000000");
});

test("tierText: gold on black, dark on light", () => {
  assert.equal(tierText("#000000", 100), "#f6d05c");
  assert.equal(tierText(tierColour(10, []), 10), "rgba(0, 0, 0, 0.78)");
  assert.equal(tierText(tierColour(99.5, []), 99.5), "#f6f05c");
});

const scales: StatScales = {
  percentiles: [0, 50, 100],
  classes: { Scout: { dpm: [100, 200, 300], dpmGames: 500, accuracy: [0.1, 0.2, 0.3], accuracyGames: 299 } },
  maps: { Scout: { koth_product: { dpm: [150, 250, 350], dpmGames: 250 }, pl_small: { dpm: [150, 250, 350], dpmGames: 20 } } },
} as unknown as StatScales;

test("tierOf: nothing to colour without scales, a class or a value", () => {
  assert.equal(tierOf(undefined, "Scout", "dpm", 200), null);
  assert.equal(tierOf(scales, null, "dpm", 200), null);
  assert.equal(tierOf(scales, "Scout", "dpm", null), null);
  assert.equal(tierOf(scales, "Medic", "dpm", 200), null);
});

test("tierOf: a value of 0 is a value, not 'nothing'", () => {
  const t = tierOf(scales, "Scout", "dpm", 0);
  assert.ok(t);
  assert.equal(t!.pct, 0);
});

test("tierOf: too few games gives no scale", () => {
  assert.equal(tierOf(scales, "Scout", "accuracy", 0.2), null);
});

test("tierOf: a map with enough games has its own scale, one without falls back and says so", () => {
  const onMap = tierOf(scales, "Scout", "dpm", 250, "koth_product");
  assert.equal(onMap!.map, "koth_product");
  assert.equal(onMap!.pct, 50);
  const unknown = tierOf(scales, "Scout", "dpm", 200, "cp_nowhere");
  assert.equal(unknown!.map, null);
  assert.equal(unknown!.missedMap, "cp_nowhere");
  assert.equal(unknown!.pct, 50);
});
