import { test } from "node:test";
import assert from "node:assert/strict";
import { filterMatches, formIndex, icsEscape, icsOf, liveStrip, NO_FILTER, parseFilter } from "../src/lib/rglmatches.ts";
import type { RglMatch } from "../src/api/types.ts";

let n = 1;
function match(a: number, b: number, at: string | null, winner: number | null, over: Partial<RglMatch> = {}): RglMatch {
  return {
    matchId: n++,
    matchDate: at,
    matchName: "Week 1 - Vigil",
    divisionName: "Advanced",
    isForfeit: false,
    winner,
    teams: [
      { teamId: a, teamName: `Team ${a}`, teamTag: `t${a}`, isHome: true },
      { teamId: b, teamName: `Team ${b}`, teamTag: null, isHome: false },
    ],
    maps: [{ mapName: "pl_vigil", homeScore: null, awayScore: null }],
    ...over,
  };
}
// Noon UTC; "today" in any zone still runs to at least a few hours after it.
const NOW = Date.UTC(2026, 10, 12, 12, 0) / 1000;
const iso = (h: number) => new Date((NOW + h * 3600) * 1000).toISOString();

test("liveStrip: started within 2.5 h and undecided is live, older or decided is not", () => {
  const ms = [match(1, 2, iso(-1), null), match(3, 4, iso(-3), null), match(5, 6, iso(-1), 5), match(9, 10, null, null)];
  const { live } = liveStrip(ms, NOW);
  assert.deepEqual(live.map((m) => m.teams[0].teamId), [1]);
});

test("liveStrip: a match later today is tonight, one tomorrow is not", () => {
  const ms = [match(1, 2, iso(0.01), null), match(3, 4, iso(48), null)];
  const { tonight } = liveStrip(ms, NOW);
  assert.ok(tonight.length <= 1);
  assert.ok(tonight.every((m) => m.teams[0].teamId === 1));
});

test("liveStrip: a bye week is not a match", () => {
  const bye = match(1, 2, iso(-1), null);
  bye.teams[1].teamName = "Bye Week";
  assert.equal(liveStrip([bye], NOW).live.length, 0);
});

test("formIndex: last five, newest on the right, only before the given time", () => {
  // Games 1..7, team 1 wins the even ones.
  const ms = [1, 2, 3, 4, 5, 6, 7].map((i) => match(1, 2, iso(-100 + i), i % 2 === 0 ? 1 : 2));
  const form = formIndex(ms);
  assert.deepEqual(form(1), ["L", "W", "L", "W", "L"]);
  assert.deepEqual(form(2), ["W", "L", "W", "L", "W"]);
  assert.deepEqual(form(1, Date.parse(iso(-100 + 3)) / 1000), ["L", "W"]);
  assert.deepEqual(form(1, null, 2), ["W", "L"]);
  assert.deepEqual(form(99), []);
});

test("formIndex: a forfeit with no winner is a loss for both; undecided counts for nothing", () => {
  const f = match(1, 2, iso(-5), null, { isForfeit: true });
  const open = match(1, 2, iso(-4), null);
  const form = formIndex([f, open]);
  assert.deepEqual(form(1), ["L"]);
  assert.deepEqual(form(2), ["L"]);
});

test("filterMatches: division, week, team by name or tag, and status", () => {
  const a = match(1, 2, iso(1), null);
  const b = match(3, 4, iso(-9), 3, { divisionName: "Main", matchName: "Week 2 - Product" });
  const c = match(5, 6, iso(-8), null, { isForfeit: true });
  const all = [a, b, c];
  assert.equal(filterMatches(all, NO_FILTER).length, 3);
  assert.deepEqual(filterMatches(all, { ...NO_FILTER, division: "Main" }), [b]);
  assert.deepEqual(filterMatches(all, { ...NO_FILTER, week: "Week 2" }), [b]);
  assert.deepEqual(filterMatches(all, { ...NO_FILTER, team: "T1" }), [a]);
  assert.deepEqual(filterMatches(all, { ...NO_FILTER, team: "team 6" }), [c]);
  assert.deepEqual(filterMatches(all, { ...NO_FILTER, status: "toPlay" }), [a]);
  assert.deepEqual(filterMatches(all, { ...NO_FILTER, status: "played" }), [b]);
  assert.deepEqual(filterMatches(all, { ...NO_FILTER, status: "forfeit" }), [c]);
});

test("parseFilter: garbage becomes no filter", () => {
  assert.deepEqual(parseFilter(null), NO_FILTER);
  assert.deepEqual(parseFilter("{nope"), NO_FILTER);
  assert.deepEqual(parseFilter('{"division":"Main","status":"x","team":3}'), { ...NO_FILTER, division: "Main" });
});

test("icsEscape: backslash, semicolon, comma and newline", () => {
  assert.equal(icsEscape("a,b;c\\d\ne"), "a\\,b\\;c\\\\d\\ne");
});

test("icsOf: UTC times, one event per match with a date, stable UIDs, CRLF", () => {
  const m = match(1, 2, "2026-11-12T19:30:00.000Z", null, { matchId: 77 });
  m.teams[0].teamName = "Duds, The";
  const undated = match(3, 4, null, null);
  const text = icsOf([m, undated], { name: "RGL; Advanced", stamp: NOW });
  assert.equal(text.match(/BEGIN:VEVENT/g)?.length, 1);
  assert.ok(text.includes("UID:rgl-match-77@flashwave.tf\r\n"));
  assert.ok(text.includes("DTSTART:20261112T193000Z\r\n"));
  assert.ok(text.includes("DTEND:20261112T213000Z\r\n"));
  assert.ok(text.includes("SUMMARY:Duds\\, The vs Team 2\r\n"));
  assert.ok(text.includes("X-WR-CALNAME:RGL\\; Advanced\r\n"));
  assert.ok(text.endsWith("END:VCALENDAR\r\n"));
  assert.equal(text, icsOf([m, undated], { name: "RGL; Advanced", stamp: NOW }));
});

test("icsOf: long lines fold at 75 octets", () => {
  const m = match(1, 2, iso(1), null);
  m.teams[0].teamName = "x".repeat(200);
  const text = icsOf([m], { name: "n", stamp: NOW });
  for (const line of text.split("\r\n")) assert.ok(new TextEncoder().encode(line).length <= 75);
});
