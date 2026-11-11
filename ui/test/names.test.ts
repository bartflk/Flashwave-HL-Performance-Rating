import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { onNamesChanged, setEtf2lNames, setNameSource, withChosenNames } from "../src/lib/names.ts";

beforeEach(() => {
  setNameSource("log");
  setEtf2lNames({});
});

test("log names: the response comes back untouched, even with ETF2L names known", () => {
  setEtf2lNames({ "100": "Etf Name" });
  const r = { accountId: 100, name: "log name" };
  assert.equal(withChosenNames(r), r);
});

test("etf2l names: swapped by account id, in a copy", () => {
  setEtf2lNames({ "100": "Etf Name" });
  setNameSource("etf2l");
  const r = { rows: [{ accountId: 100, name: "log name", kills: 3 }, { accountId: 101, name: "unknown" }] };
  const copy = structuredClone(r);
  const out = withChosenNames(r);
  assert.deepEqual(out, { rows: [{ accountId: 100, name: "Etf Name", kills: 3 }, { accountId: 101, name: "unknown" }] });
  assert.deepEqual(r, copy);
});

test("etf2l names: a steamid64 is turned into an account id", () => {
  // 76561197960265728 + 100
  setEtf2lNames({ "100": "By Steam64" });
  setNameSource("etf2l");
  assert.equal(withChosenNames({ steamid64: "76561197960265828", name: "x" }).name, "By Steam64");
  // Not 17 digits, or not digits: left alone.
  assert.equal(withChosenNames({ steamid64: "7656119796026582", name: "x" }).name, "x");
  assert.equal(withChosenNames({ steamid64: "7656119796026582a", name: "x" }).name, "x");
});

test("etf2l names: a team (no account id) keeps its name, and so does a null or numeric name", () => {
  setEtf2lNames({ "0": "Zero" });
  setNameSource("etf2l");
  assert.deepEqual(withChosenNames({ name: "Team" }), { name: "Team" });
  assert.deepEqual(withChosenNames({ accountId: 0, name: null }), { accountId: 0, name: null });
  // Account id 0 is a real key.
  assert.deepEqual(withChosenNames({ accountId: 0, name: "log" }), { accountId: 0, name: "Zero" });
});

test("etf2l names: primitives, null and nested arrays pass through", () => {
  setEtf2lNames({ "1": "One" });
  setNameSource("etf2l");
  assert.equal(withChosenNames(null), null);
  assert.equal(withChosenNames(5), 5);
  assert.equal(withChosenNames("s"), "s");
  assert.deepEqual(withChosenNames([[{ accountId: 1, name: "a" }]]), [[{ accountId: 1, name: "One" }]]);
});

test("etf2l names: an empty name table is a no-op that returns the same object", () => {
  setNameSource("etf2l");
  const r = { accountId: 1, name: "a" };
  assert.equal(withChosenNames(r), r);
});

test("refetch listeners hear a change of source, and of names only while ETF2L is chosen", () => {
  let n = 0;
  const off = onNamesChanged(() => n++);
  setNameSource("log"); // no change
  assert.equal(n, 0);
  setEtf2lNames({ "1": "A" }); // not shown yet
  assert.equal(n, 0);
  setNameSource("etf2l");
  assert.equal(n, 1);
  setEtf2lNames({ "1": "A" }); // the same names again
  assert.equal(n, 1);
  setEtf2lNames({ "1": "B" });
  assert.equal(n, 2);
  off();
  setEtf2lNames({ "1": "C" });
  assert.equal(n, 2);
});

test("the choice is remembered", () => {
  setNameSource("etf2l");
  assert.equal(localStorage.getItem("hl.names"), "etf2l");
  setNameSource("log");
  assert.equal(localStorage.getItem("hl.names"), "log");
});
