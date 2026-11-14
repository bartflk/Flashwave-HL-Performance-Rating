import { test } from "node:test";
import assert from "node:assert/strict";
import { drawn, mapMode, midfights, tabFormat, type FormatsPlayed } from "../src/lib/gameformat.ts";

const both: FormatsPlayed = { formats: ["hl", "6s"], sixesEnabled: true, multi: true };
const hlOnly: FormatsPlayed = { formats: ["hl"], sixesEnabled: false, multi: false };
const sixOnly: FormatsPlayed = { formats: ["6s"], sixesEnabled: true, multi: false };

test("a tab shows its last choice while that format is played, else the first played", () => {
  assert.equal(tabFormat("matches", both, { matches: "6s" }), "6s");
  assert.equal(tabFormat("profile", both, { matches: "6s" }), "hl", "each tab has its own choice");
  assert.equal(tabFormat("matches", hlOnly, { matches: "6s" }), "hl", "a Highlander player never lands on 6v6");
  assert.equal(tabFormat("etf2l", sixOnly, {}), "6s", "a 6v6-only player starts on 6v6");
});

test("midfights count who capped mid first, on 6v6 5CP only", () => {
  const rounds = [{ firstcap: "Red" as const }, { firstcap: "Blue" as const }, { firstcap: "Red" as const }, { firstcap: null }];
  assert.deepEqual(midfights({ format: "sixes", map: "cp_process_f12", rounds }), { Red: 2, Blue: 1 });
  assert.equal(midfights({ format: "sixes", map: "koth_product_final", rounds }), null, "KOTH has no midfight");
  assert.equal(midfights({ format: "highlander", map: "cp_process_f12", rounds }), null);
  assert.equal(midfights({ format: "sixes", map: "cp_sunshine", rounds: [{ firstcap: null }] }), null, "not recorded");
});

test("maps' modes and drawn maps", () => {
  assert.deepEqual(["cp_gullywash_f9", "koth_bagel_rc13", "pl_upward"].map(mapMode), ["5cp", "koth", "other"]);
  assert.equal(drawn({ mapsPlayed: 8, mapsWon: 6, mapsLost: 1 }), 1);
});
