import { test } from "node:test";
import assert from "node:assert/strict";
import { blank, compare } from "../src/lib/sort.tsx";


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

