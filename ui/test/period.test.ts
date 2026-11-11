import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { bounds, setPeriod, type Period } from "../src/lib/period.ts";

const stored = () => JSON.parse(localStorage.getItem("hl.period") ?? "null");

beforeEach(() => localStorage.clear());

test("bounds: all time is open both ends", () => {
  assert.deepEqual(bounds({ kind: "all" }), { from: null, to: null });
});

test("bounds: a season gives its dates, a custom range may be half open", () => {
  assert.deepEqual(bounds({ kind: "season", key: "s1", name: "S1", from: 10, to: 20 }), { from: 10, to: 20 });
  assert.deepEqual(bounds({ kind: "custom", from: 5, to: null }), { from: 5, to: null });
  assert.deepEqual(bounds({ kind: "custom", from: null, to: 7 }), { from: null, to: 7 });
  // Unix second 0 is a real bound, not "open".
  assert.deepEqual(bounds({ kind: "custom", from: 0, to: 0 }), { from: 0, to: 0 });
});

test("bounds: a NaN date reads as open instead of breaking a query", () => {
  assert.deepEqual(bounds({ kind: "custom", from: NaN, to: Infinity }), { from: null, to: null });
});

test("setPeriod: dates the wrong way round are swapped", () => {
  setPeriod({ kind: "custom", from: 100, to: 50 });
  assert.deepEqual(stored(), { kind: "custom", from: 50, to: 100 });
});

test("setPeriod: a malformed period becomes all time", () => {
  setPeriod({ kind: "season", key: "s", name: "S", from: NaN, to: 1 });
  assert.deepEqual(stored(), { kind: "all" });
  setPeriod({ kind: "nonsense" } as unknown as Period);
  assert.deepEqual(stored(), { kind: "all" });
  setPeriod(null as unknown as Period);
  assert.deepEqual(stored(), { kind: "all" });
});

test("setPeriod: a good season is kept as it is", () => {
  const p: Period = { kind: "season", key: "etf2l-12", name: "Season 12", from: 1, to: 2 };
  setPeriod(p);
  assert.deepEqual(stored(), p);
});

test("setPeriod: an equal range stays", () => {
  setPeriod({ kind: "custom", from: 5, to: 5 });
  assert.deepEqual(stored(), { kind: "custom", from: 5, to: 5 });
});
