import { test } from "node:test";
import assert from "node:assert/strict";
import { changeOf, divisionsOf, inDivision, isMine, rowOf } from "../src/lib/rglrank.ts";
import type { RglRankRow } from "../src/api/types.ts";

const row = (rank: number, id: number, division: string | null, ids = [id]): RglRankRow => ({ rank, id, ids, name: `T${id}`, tag: null, division, tier: null, points: 1500 - rank, change: null, pointsChange: 0, matches: 5, players: [] });
const rows = [row(1, 10, "Invite"), row(2, 20, "Invite", [20, 8]), row(3, 30, "Advanced"), row(4, 40, null)];

test("divisions come best first, once each, without a missing one", () => {
  assert.deepEqual(divisionsOf(rows), ["Invite", "Advanced"]);
});

test("a division filter keeps the table's ranks", () => {
  assert.deepEqual(inDivision(rows, "Advanced").map((r) => r.rank), [3]);
  assert.equal(inDivision(rows, null).length, 4);
});

test("a team is found by any of its ids", () => {
  assert.equal(rowOf(rows, 8)?.id, 20);
  assert.equal(rowOf(rows, 99), null);
  assert.equal(rowOf(undefined, 10), null);
  assert.equal(isMine(rows[1], new Set([8])), true);
  assert.equal(isMine(rows[0], new Set([8])), false);
});

test("the week's change reads as up, down, level or unknown", () => {
  assert.deepEqual(changeOf(2), { kind: "up", places: 2 });
  assert.deepEqual(changeOf(-3), { kind: "down", places: 3 });
  assert.deepEqual(changeOf(0), { kind: "same", places: 0 });
  assert.deepEqual(changeOf(null), { kind: "none", places: 0 });
});
