import { test } from "node:test";
import assert from "node:assert/strict";
import { form, headToHead, medals, sharedMaps, trophyCount, winRate } from "../src/lib/rglteam.ts";
import type { RglMatch, RglTeamHistory } from "../src/api/types.ts";

let id = 0;
function match(date: string, home: [number, string], away: [number, string], winner: number | null, forfeit = false): RglMatch {
  return {
    matchId: ++id,
    matchDate: date,
    matchName: null,
    isForfeit: forfeit,
    winner,
    teams: [
      { teamId: home[0], teamName: home[1], teamTag: null, isHome: true },
      { teamId: away[0], teamName: away[1], teamTag: null, isHome: false },
    ],
    maps: [],
  };
}

const rec = { won: 0, lost: 0, forfeitsWon: 0, forfeitsLost: 0 };
function history(ids: number[], matches: RglMatch[], maps: RglTeamHistory["maps"] = [], ranks: Array<number | null> = []): RglTeamHistory {
  return {
    teamIds: ids,
    seasons: ranks.map((finalRank, i) => ({ teamId: ids[i] ?? 0, name: "X", tag: null, seasonId: 100 - i, seasonName: `S${100 - i}`, division: "Advanced", tier: "High", finalRank, record: rec })),
    total: rec,
    formerNames: [],
    firstSeason: null,
    capped: false,
    opponents: [],
    maps,
    matches,
  };
}

test("winRate: wins over games played, none before a game", () => {
  assert.equal(winRate({ won: 3, lost: 1 }), 75);
  assert.equal(winRate({ won: 0, lost: 0 }), null);
});

test("form: the newest results first, a match still to be played left out", () => {
  const rows = form(
    [match("2026-01-01", [1, "A"], [2, "B"], 1), match("2026-01-08", [1, "A"], [3, "C"], 3), match("2026-01-15", [1, "A"], [4, "D"], null), match("2026-01-22", [1, "A"], [5, "E"], 1, true)],
    1,
    2,
  );
  assert.deepEqual(rows.map((r) => r.won), [true, false]);
  assert.equal(rows[0].match.teams[1].teamId, 5);
});

test("medals and trophyCount: places one to three, a title is a podium too", () => {
  const h = history([1, 2, 3, 4], [], [], [1, 2, null, 4]);
  assert.deepEqual(medals(h.seasons).map((m) => m.place), [1, 2]);
  assert.deepEqual(trophyCount(h.seasons), { titles: 1, podiums: 2 });
});

test("headToHead: another team's ids across seasons, forfeits apart", () => {
  const a = history(
    [10, 11],
    [
      match("2026-03-01", [10, "A"], [20, "B"], 10),
      match("2026-02-01", [11, "A"], [21, "B"], 21),
      match("2026-01-01", [11, "A"], [21, "B"], 11, true),
      match("2026-01-01", [11, "A"], [99, "Other"], 11),
    ],
  );
  const b = history([20, 21], []);
  const h = headToHead(a, b);
  assert.deepEqual([h.matches.length, h.won, h.lost, h.forfeits], [3, 1, 1, 1]);
});

test("sharedMaps: only maps both played", () => {
  const m = (map: string, won: number, lost: number) => ({ map, won, lost, roundsFor: 0, roundsAgainst: 0 });
  const s = sharedMaps(history([1], [], [m("pl_vigil", 3, 1), m("cp_gully", 1, 1)]), history([2], [], [m("pl_vigil", 0, 2), m("koth_x", 5, 0)]));
  assert.deepEqual(s.map((x) => x.map), ["pl_vigil"]);
  assert.equal(s[0].b.lost, 2);
});
