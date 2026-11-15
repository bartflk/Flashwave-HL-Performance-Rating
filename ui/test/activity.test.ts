import { test } from "node:test";
import assert from "node:assert/strict";
import { appendNew } from "../src/lib/activity.ts";
import type { ActivityLine } from "../src/api/types.ts";

const line = (seq: number): ActivityLine => ({ seq, at: seq * 1000, level: "info", source: "test", text: `line ${seq}` });

test("a line already held is not added again", () => {
  const have = [line(1), line(2), line(3)];
  assert.deepEqual(appendNew(have, [line(2), line(3), line(4)]).map((l) => l.seq), [1, 2, 3, 4]);
});

test("an answer with nothing new leaves the lines as they were", () => {
  const have = [line(1), line(2)];
  assert.equal(appendNew(have, [line(1), line(2)]), have);
});

test("after the view is cleared every line is new", () => {
  assert.deepEqual(appendNew([], [line(5), line(6)]).map((l) => l.seq), [5, 6]);
});
