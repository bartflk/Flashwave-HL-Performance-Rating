import { test } from "node:test";
import assert from "node:assert/strict";
import { blank, compare, sortRows } from "../src/lib/compare.ts";
import { compare as reexported } from "../src/lib/sort.tsx";

test("sort.tsx still exports compare", () => {
  assert.equal(reexported, compare);
});

test("compare: numbers ascending, equal is 0", () => {
  assert.ok(compare(1, 2) < 0);
  assert.ok(compare(2, 1) > 0);
  assert.equal(compare(3, 3), 0);
  assert.ok(compare(-5, 0.5) < 0);
});

test("compare: blanks are last and equal to each other", () => {
  for (const b of [null, undefined, "", NaN]) {
    assert.equal(compare(b, 5), 1);
    assert.equal(compare(5, b), -1);
    assert.equal(compare(b, "a"), 1);
  }
  assert.equal(compare(null, undefined), 0);
  assert.equal(compare("", NaN), 0);
  // Zero is a value, not a blank.
  assert.equal(blank(0), false);
  assert.ok(compare(0, 1) < 0);
});

test("compare: infinities of the same sign are equal, not NaN", () => {
  assert.equal(compare(Infinity, Infinity), 0);
  assert.equal(compare(-Infinity, -Infinity), 0);
  assert.ok(compare(-Infinity, Infinity) < 0);
});

test("compare: text is case-insensitive and numeric-aware", () => {
  assert.equal(compare("abc", "ABC"), 0);
  assert.ok(compare("Map 2", "Map 10") < 0);
  assert.notEqual(compare("á", "b"), NaN); // non-ASCII does not throw
});

test("sortRows: descending puts biggest first, blanks still last", () => {
  const rows = [{ v: 2 }, { v: null }, { v: 9 }, { v: NaN }, { v: 5 }];
  const desc = sortRows(rows, (r) => r.v, "desc").map((r) => r.v);
  assert.deepEqual(desc.slice(0, 3), [9, 5, 2]);
  assert.ok(desc.slice(3).every((v) => v === null || Number.isNaN(v)));
  const asc = sortRows(rows, (r) => r.v, "asc").map((r) => r.v);
  assert.deepEqual(asc.slice(0, 3), [2, 5, 9]);
  assert.ok(asc.slice(3).every((v) => v === null || Number.isNaN(v)));
});

test("sortRows: ties keep their order, input is not changed", () => {
  const rows = [
    { id: 1, v: "x" },
    { id: 2, v: "x" },
    { id: 3, v: "a" },
    { id: 4, v: "x" },
  ];
  const copy = structuredClone(rows);
  assert.deepEqual(sortRows(rows, (r) => r.v, "desc").map((r) => r.id), [1, 2, 4, 3]);
  assert.deepEqual(sortRows(rows, (r) => r.v, "asc").map((r) => r.id), [3, 1, 2, 4]);
  assert.deepEqual(rows, copy);
});

test("sortRows: empty and one row", () => {
  assert.deepEqual(sortRows([], () => 1, "asc"), []);
  assert.deepEqual(sortRows([{ a: 1 }], (r) => r.a, "desc"), [{ a: 1 }]);
});
