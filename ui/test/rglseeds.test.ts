import { test } from "node:test";
import assert from "node:assert/strict";
import { divisionRows, isPlaceholder } from "../src/lib/rglseeds.ts";
import type { RglTeam } from "../src/api/types.ts";

const team = (id: number, name: string, seeding: number | null, status: string | null = "Active", finalRank: number | null = null): RglTeam => ({ id, name, tag: null, finalRank, seeding, teamStatus: status });

test("dropped teams take no seed, and the seeds are numbered again over the rest", () => {
  // The Invite list Flashy sent: seeds 1 and 3 dropped out.
  const rows = divisionRows([
    team(1, "Locked In Syndrome", 1, "Dead"),
    team(2, "Team Fun", 2),
    team(3, "Team Name", 3, "Dead"),
    team(4, "emo bunny club", 4),
    team(5, "Monk Mode", 5),
    team(9, "Free Agent - Invite", null),
  ]);
  assert.deepEqual(
    rows.map((r) => [r.team.name, r.seed, r.out]),
    [
      ["Team Fun", 1, false],
      ["emo bunny club", 2, false],
      ["Monk Mode", 3, false],
      ["Locked In Syndrome", null, true],
      ["Team Name", null, true],
      ["Free Agent - Invite", null, false],
    ],
  );
});

test("a finished division keeps RGL's final order, a dropped team after it", () => {
  const rows = divisionRows([team(1, "C", null, "Active", 3), team(2, "Out", null, "Active", 99), team(3, "A", null, "Active", 1), team(4, "B", null, "Active", 2)]);
  assert.deepEqual(rows.map((r) => r.team.name), ["A", "B", "C", "Out"]);
  assert.ok(rows.every((r) => r.seed === null), "no seeds once placed");
});

test("free-agent slots are recognised", () => {
  assert.ok(isPlaceholder("Free Agent - Invite") && isPlaceholder("free agents"));
  assert.ok(!isPlaceholder("Freedom Fighters"));
});
