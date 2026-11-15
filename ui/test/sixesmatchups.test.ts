import { test } from "node:test";
import assert from "node:assert/strict";
import { roleRows } from "../src/lib/sixesmatchups.ts";
import type { MatchDetail, SixesRatingRow, StoredRole } from "../src/api/types.ts";

const player = (accountId: number, team: "Red" | "Blue", timeS = 1800, isMe = false) => ({ accountId, name: `p${accountId}`, team, timeS, isMe });
const d = {
  leftTeam: "Blue",
  players: [player(1, "Blue", 1800, true), player(2, "Blue"), player(3, "Blue", 600), player(11, "Red"), player(12, "Red")],
} as unknown as MatchDetail;
const roles: StoredRole[] = [
  { accountId: 1, role: "medic" },
  { accountId: 2, role: "pocket" },
  { accountId: 3, role: "pocket" },
  { accountId: 11, role: "medic" },
  { accountId: 12, role: "pocket" },
] as StoredRole[];
const r = (accountId: number, role: string, rating: number): SixesRatingRow => ({ logId: 1, accountId, role, rating, score: 50, minutes: 30, parts: "[]" });

test("players are paired role against role, the longer of a swap playing it", () => {
  const rows = roleRows(d, roles, [r(1, "medic", 1.2), r(11, "medic", 1.0), r(2, "pocket", 0.9), r(12, "pocket", 0.91)]);
  assert.deepEqual(rows.map((x) => x.role), ["pocket", "medic"], "team-sheet order, roles nobody played left out");
  const pocket = rows[0];
  assert.equal(pocket.left?.accountId, 2, "30 minutes beats 10");
  assert.equal(pocket.winner, "even", "0.01 apart is even");
  const medic = rows[1];
  assert.equal(medic.winner, "left");
  assert.ok(medic.decisive && medic.involvesMe);
});

test("a role with a rating missing on one side has no winner", () => {
  const rows = roleRows(d, roles, [r(1, "medic", 1.2)]);
  assert.equal(rows.find((x) => x.role === "medic")?.winner, null);
});
