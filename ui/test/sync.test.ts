import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { applyUserFiles, setLanguage } from "../src/lib/i18n.ts";
import { fractionOf, labelOf, phaseOf } from "../src/lib/sync.ts";
import { diagnosticsText, jobName, sourceName, sqliteTime } from "../src/lib/diagnostics.ts";
import type { Diagnostics } from "../src/api/types.ts";

beforeEach(() => {
  localStorage.clear();
  applyUserFiles([]);
  setLanguage("en");
});

test("phaseOf: a sync's steps, and none for the other jobs", () => {
  const fights = { kind: "fights" as const, done: 3, total: 9 };
  assert.equal(phaseOf(fights)?.step, 7, "a sync reading fights is on step 7");
  // A rebuild sends the same fights and rating progress; it used to read
  // "Syncing, step 7 of 9" there (audit, 12 Nov 2026).
  for (const job of ["reprocess", "rerate", "demos", "startup"] as const) {
    assert.equal(phaseOf(fights, job), null, job);
    assert.equal(phaseOf({ kind: "rating", done: 1, total: 1 }, job), null, job);
  }
});

test("a rebuild's player numbers have a label and a fraction", () => {
  const p = { kind: "playerNumbers" as const, done: 50, total: 200 };
  assert.equal(fractionOf(p), 0.25);
  assert.equal(labelOf(p), "Player numbers 50 of 200 logs");
  assert.equal(fractionOf({ kind: "playerNumbers", done: 0, total: 0 }), 1, "nothing to do is done");
});

test("job and source names", () => {
  assert.equal(jobName("reprocess"), "Rebuild");
  assert.equal(jobName("rerate"), "Re-rate");
  assert.equal(jobName("sync"), "Sync");
  assert.equal(sourceName("trends"), "trends.tf");
  assert.equal(sourceName("logstf_search"), "logs.tf, whole history");
  assert.equal(sourceName("etf2l"), "etf2l", "an unknown source keeps its key");
});

test("sqliteTime: SQLite's UTC text, or a dash", () => {
  assert.equal(sqliteTime(null), "—");
  assert.equal(sqliteTime("not a time"), "not a time", "unreadable text is shown as it is");
  assert.equal(sqliteTime("2026-11-12 08:31:40"), new Date(Date.UTC(2026, 10, 12, 8, 31, 40)).toLocaleString());
});

test("diagnosticsText: everything a tester sends, failures included", () => {
  const d: Diagnostics = {
    version: "0.14.0 beta",
    model: "v14",
    dataDir: "C:/data",
    logFile: "C:/data/logs/flashwave.log",
    logDir: "C:/data/logs",
    stats: { indexed: 10, superseded: 0, highlander: 9, sixes: 0, other: 1, unclassified: 0, officials: 2, fetched: 9, normalized: 9, sixesFetched: 0, pending: 1, failed: 0, outsideWindow: 0 },
    rated: [["v13", 8], ["v14", 9]],
    sources: [{ source: "trends", cursor: "1", lastRunAt: null, lastError: "timed out" }],
    runs: [{ kind: "reprocess", at: 1_789_000_000, seconds: 30, fetched: 0, failed: 0, error: "database is locked", problems: ["raw logs: 503"] }],
  };
  const text = diagnosticsText(d);
  assert.match(text, /Flashwave\.tf 0\.14\.0 beta · model v14/);
  assert.match(text, /9 stored, 1 waiting, 0 failed, 0 given up/, "a missing given-up count reads 0");
  assert.match(text, /v13 8, v14 9/);
  assert.match(text, /trends\.tf: — — timed out/);
  assert.match(text, /Rebuild 30s — FAILED: database is locked/);
  assert.match(text, /\n {4}raw logs: 503/);
});
