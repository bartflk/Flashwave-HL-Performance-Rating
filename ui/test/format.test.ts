import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { applyUserFiles, setLanguage } from "../src/lib/i18n.ts";
import { RATING_BAND, capitalize, clock, formatDate, formatMonth, formatStay, minutes, rating, ratingPercent, signed, splitMap, teamLabel } from "../src/lib/format.ts";
import { DEMOSTF_OLDEST, beforeDemosTf } from "../src/lib/stv.ts";
import { country } from "../src/lib/countries.ts";
import { friendlyError, friendlyText } from "../src/lib/apierror.ts";

beforeEach(() => {
  localStorage.clear();
  applyUserFiles([]);
  setLanguage("en");
});

test("splitMap: mode and name, with the version suffix dropped", () => {
  assert.deepEqual(splitMap("pl_swiftwater_final1"), { mode: "pl", name: "swiftwater" });
  assert.deepEqual(splitMap("koth_product_final"), { mode: "koth", name: "product" });
  assert.deepEqual(splitMap("cp_process_f12"), { mode: "cp", name: "process" });
  assert.deepEqual(splitMap("cp_gullywash_f9"), { mode: "cp", name: "gullywash" });
  assert.deepEqual(splitMap("koth_bagel_rc7a"), { mode: "koth", name: "bagel" });
  assert.deepEqual(splitMap("KOTH_Viaduct_Pro4"), { mode: "koth", name: "Viaduct" });
});

test("splitMap: no map, an unknown mode, or a bare mode", () => {
  assert.deepEqual(splitMap(null), { mode: null, name: null });
  assert.deepEqual(splitMap(""), { mode: null, name: null });
  assert.deepEqual(splitMap("ultiduo_baloo"), { mode: null, name: "ultiduo_baloo" });
  assert.deepEqual(splitMap("koth_"), { mode: null, name: "koth_" });
  assert.deepEqual(splitMap("pl_"), { mode: null, name: "pl_" });
});

test("splitMap: only a trailing version tag goes, not part of the name", () => {
  assert.deepEqual(splitMap("cp_final_destination"), { mode: "cp", name: "final_destination" });
  assert.deepEqual(splitMap("cp_a_b1"), { mode: "cp", name: "a" });
});

test("clock: minutes and zero-padded seconds, null and negative as a dash", () => {
  assert.equal(clock(288), "4:48");
  assert.equal(clock(0), "0:00");
  assert.equal(clock(59.9), "0:59");
  assert.equal(clock(3600), "60:00");
  assert.equal(clock(null), "—");
  assert.equal(clock(-1), "—");
});

test("minutes: rounds to whole minutes", () => {
  assert.equal(minutes(2816), "47 min");
  assert.equal(minutes(0), "0 min");
  assert.equal(minutes(29), "0 min");
  assert.equal(minutes(30), "1 min");
});

test("formatStay: days, months, years", () => {
  assert.equal(formatStay(0), "0 days");
  assert.equal(formatStay(-100), "0 days");
  assert.equal(formatStay(12 * 86400), "12 days");
  assert.equal(formatStay(30 * 86400), "30 days");
  assert.equal(formatStay(5 * 30.44 * 86400), "5 mo");
  assert.equal(formatStay(365 * 86400), "12 mo".replace("12 mo", "1 yr"));
  assert.equal(formatStay(2 * 365.25 * 86400 + 3 * 30.44 * 86400), "2 yr 3 mo");
});

test("formatStay: 11.6 months does not read as '12 mo'", () => {
  // 353 days is 11.6 months: rounds to 12 and must roll over to a year.
  assert.equal(formatStay(353 * 86400), "1 yr");
});

test("formatDate / formatMonth: null is a dash, a date is a string in the chosen locale", () => {
  assert.equal(formatDate(null), "—");
  assert.equal(formatMonth(null), "—");
  assert.match(formatDate(1_700_000_000, true), /2023/);
  assert.match(formatMonth(1_700_000_000), /2023/);
});

test("capitalize and teamLabel", () => {
  assert.equal(capitalize(""), "");
  assert.equal(capitalize("scout"), "Scout");
  assert.equal(capitalize("éclair"), "Éclair");
  assert.equal(teamLabel("Red"), "RED");
  assert.equal(teamLabel("Blue"), "BLU");
});

test("rating: two decimals, null and undefined as a dash", () => {
  assert.equal(rating(1), "1.00");
  assert.equal(rating(0), "0.00");
  assert.equal(rating(1.235), "1.24".replace("1.24", (1.235).toFixed(2)));
  assert.equal(rating(null), "—");
  assert.equal(rating(undefined), "—");
});

test("ratingPercent: 1.00 is the middle, ends clamp", () => {
  assert.ok(Math.abs(ratingPercent(1.0) - 50) < 1e-9);
  assert.equal(ratingPercent(RATING_BAND.min), 0);
  assert.equal(ratingPercent(RATING_BAND.max), 100);
  assert.equal(ratingPercent(-5), 0);
  assert.equal(ratingPercent(99), 100);
});

test("signed: plus sign only above zero", () => {
  assert.equal(signed(2.5), "+2.5");
  assert.equal(signed(0), "0.0");
  assert.equal(signed(-2.5), "-2.5");
  assert.equal(signed(1.234, 2), "+1.23");
  // A value that rounds to zero must not read "+0.0" or "-0.0" as a change.
  assert.equal(signed(0.04), "0.0");
  assert.equal(signed(-0.04), "0.0");
});

test("beforeDemosTf matches the backend's demos.tf cut-off", () => {
  const rust = readFileSync(join(import.meta.dirname, "..", "..", "crates", "hl-ingest", "src", "demostf.rs"), "utf8");
  const oldest = Number(/OLDEST_AVAILABLE: i64 = ([\d_]+);/.exec(rust)![1].replaceAll("_", ""));
  const late = Number(/LATE_S: i64 = 3 \* 3600;/.test(rust) ? 3 * 3600 : NaN);
  assert.equal(DEMOSTF_OLDEST, oldest - late);
  assert.equal(beforeDemosTf(null), false);
  assert.equal(beforeDemosTf(DEMOSTF_OLDEST - 1), true);
  assert.equal(beforeDemosTf(DEMOSTF_OLDEST), false);
});

test("country: ETF2L's run-together names get a flag and a readable name", () => {
  assert.deepEqual(country("UnitedKingdom"), { code: "gb", name: "United Kingdom" });
  assert.deepEqual(country("TheIsleOfMan"), { code: "im", name: "Isle of Man" });
  assert.deepEqual(country("European"), { code: "eu", name: "Europe" });
  assert.deepEqual(country("united kingdom"), { code: "gb", name: "United Kingdom" });
  assert.equal(country("Côte d'Ivoire (Ivory Coast)").code, "ci");
});

test("country: unknown names keep their own, tidied", () => {
  assert.deepEqual(country("International"), { code: null, name: "International" });
  assert.deepEqual(country("International (Locked)"), { code: null, name: "International" });
  assert.deepEqual(country("SomeCountry"), { code: null, name: "Some Country" });
  assert.deepEqual(country(""), { code: null, name: "" });
});

test("friendlyError: a site that is down says so without the URL", () => {
  const r = friendlyError("error sending request for url (https://logs.tf/api/v1/log?player=1): tcp connect error");
  assert.equal(r.text, "logs.tf isn't answering right now; try again later.");
  assert.match(r.detail, /https:\/\/logs\.tf/);
});

test("friendlyError: 429 and 5xx are told apart", () => {
  assert.match(friendlyText("RGL returned status 429 Too Many Requests"), /busy right now/);
  assert.match(friendlyText("ETF2L returned 503 service unavailable"), /having trouble/);
  assert.match(friendlyText("https://example.com said no"), /^The site could not be read/);
});

test("friendlyError: a local error is left as it was", () => {
  assert.equal(friendlyText("The system cannot find the file specified. (os error 2)"), "The system cannot find the file specified. (os error 2)");
  assert.equal(friendlyText(new Error("database is locked")), "database is locked");
  assert.equal(friendlyText({ kind: "db", message: "no such table" }), "no such table");
});

test("friendlyError: a port number or byte count is not a status code", () => {
  // "5000" has no word boundary at the 5xx inside it; a plain number is not an outage.
  assert.doesNotMatch(friendlyText("ETF2L sent 5000 rows"), /having trouble/);
});
