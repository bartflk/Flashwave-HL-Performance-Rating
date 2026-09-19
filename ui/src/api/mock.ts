// Browser-mode stand-in for the Rust backend.
//
// Active only when the page is open outside Tauri (`npm run ui` in a plain
// browser), so UI work does not require a Rust rebuild. Inside the app window
// this file is never used.

import type { Api, StvHandlers, SyncHandlers } from "./client";
import match4109131 from "./fixtures/match_4109131.json";
import match4114301 from "./fixtures/match_4114301.json";
import match4111116 from "./fixtures/match_4111116.json";
import match3863290 from "./fixtures/match_3863290.json";
import analysis3863290 from "./fixtures/analysis_3863290.json";
import mapviewAshville from "./fixtures/mapview_ashville.json";
import profileSniper from "./fixtures/profile_sniper.json";
import profileEngineer from "./fixtures/profile_engineer.json";
import teammatesTeam from "./fixtures/teammates_team.json";
import analysis4109131 from "./fixtures/analysis_4109131.json";
import mapviewUpward from "./fixtures/mapview_upward.json";
import teammatesAll from "./fixtures/teammates_all.json";
import seasonsSniper from "./fixtures/seasons_sniper.json";
import fightsSniper from "./fixtures/fights_sniper.json";
import type {
  Analysis,
  AppConfig,
  AppStatus,
  ContextKind,
  IndexStats,
  MatchContext,
  MapView,
  MatchDetail,
  Overview,
  MatchPage,
  MatchQuery,
  MatchSummary,
  ProfileResponse,
  Teammates,
  TfPathInfo,
  FightsCard,
  SeasonsView,
} from "./types";

// Starts configured, since setup is not what you are usually iterating on.
// Append `?setup` to the URL to start from the first-run screen instead.
const startInSetup = typeof location !== "undefined" && location.search.includes("setup");
const state: AppConfig = startInSetup
  ? { steamid: null, tfPath: null }
  : {
      steamid: "76561198099396919",
      tfPath: "D:\\SteamLibrary\\steamapps\\common\\Team Fortress 2\\tf",
    };

const delay = <T,>(value: T, ms = 120): Promise<T> =>
  new Promise((resolve) => setTimeout(() => resolve(value), ms));

// Numbers taken from a real install, so browser-mode layout matches reality.
function fakeTfPath(path: string, valid: boolean): TfPathInfo {
  if (!valid) {
    return {
      path,
      valid,
      demoDirs: [],
      cfgDir: null,
      demoCount: 0,
      notes: [
        "No TF2 marker files found here (expected gameinfo.txt, tf2_misc_dir.vpk or cfg/).",
      ],
    };
  }
  const demoDirs = [
    { path, demoCount: 6 },
    { path: `${path}\\demos`, demoCount: 95 },
  ];
  return {
    path,
    valid,
    demoDirs,
    cfgDir: `${path}\\cfg`,
    demoCount: demoDirs.reduce((n, d) => n + d.demoCount, 0),
    notes: demoDirs.map((d) => `${d.demoCount} demo file(s) in \`${d.path}\`.`),
  };
}

// ---- real match fixtures -----------------------------------------------------

// Exported from the real database with `hl match <id> --json`, so browser mode
// renders genuine matches rather than invented ones.
// JSON imports widen tuples to arrays, so the cast goes through `unknown`. Safe
// here: these files are the Rust serializer's own output.
// Contexts as the real context pass classified these three.
const FIXTURE_CONTEXT: Record<number, MatchContext> = {
  4109131: {
    kind: "official",
    etf2lMatchId: 92883,
    linkMethod: "trends",
    teamName: "DD14",
    oppName: "ЭТО МОЁ БОЛОТО",
    regulars: 3,
    official: {
      competition: "Highlander Season 36 (Autumn 2026): High",
      category: "Highlander Season",
      division: "High",
      tier: 1,
      week: 1,
      round: "Round 1",
      score: [4, 2],
      defaultWin: false,
    },
  },
  4111116: { kind: "pug", etf2lMatchId: null, linkMethod: null, teamName: null, oppName: null, regulars: 2, official: null },
  4114301: { kind: "pug", etf2lMatchId: null, linkMethod: null, teamName: null, oppName: null, regulars: 0, official: null },
  3863290: {
    kind: "official",
    etf2lMatchId: 90482,
    linkMethod: "trends",
    teamName: "SBQRRA",
    oppName: "Champions of Light",
    regulars: 8,
    official: {
      competition: "Highlander Season 33 (Spring 2025): Low Playoffs",
      category: "Highlander Season",
      division: "Low",
      tier: 3,
      week: null,
      round: "Grand Final",
      score: [6, 3],
      defaultWin: false,
    },
  },
};

// Maps as the round-map pass resolved them; the grand final spans three.
const FIXTURE_SEGMENTS: Record<number, MatchDetail["segments"]> = {
  3863290: [
    { map: "koth_ashville_final1", firstRound: 1, lastRound: 6, rounds: 6, redWins: 2, blueWins: 4 },
    { map: "pl_vigil_rc10", firstRound: 7, lastRound: 10, rounds: 4, redWins: 3, blueWins: 1 },
    { map: "koth_proot_b5b", firstRound: 11, lastRound: 17, rounds: 7, redWins: 3, blueWins: 4 },
  ],
};

const FIXTURES: MatchDetail[] = [match4109131, match4114301, match4111116, match3863290].map((f) => {
  const d = f as unknown as MatchDetail;
  const segments = FIXTURE_SEGMENTS[d.logId] ?? [
    { map: d.map, firstRound: 1, lastRound: d.rounds.length, rounds: d.rounds.length, redWins: 0, blueWins: 0 },
  ];
  return { ...d, context: FIXTURE_CONTEXT[d.logId] ?? null, segments };
});

// ---- fake match history -----------------------------------------------------

/** Deterministic PRNG so the fake list is stable across reloads. */
function rng(seed: number) {
  return () => {
    seed = (seed * 1_103_515_245 + 12_345) & 0x7fffffff;
    return seed / 0x7fffffff;
  };
}

// Weighted towards the real map pool and the real class split (mostly Sniper).
const MAPS = ["koth_product_final", "pl_vigil_rc10", "pl_upward_f12", "koth_proot_b5b",
  "koth_ashville_final1", "cp_steel_f12", "pl_swiftwater_final1", null];
const CLASSES = ["sniper", "sniper", "sniper", "sniper", "sniper", "engineer", "spy", "medic"];

/** The two real fixtures, as list rows, ahead of the generated ones. */
const FIXTURE_ROWS: MatchSummary[] = FIXTURES.map((d) => {
  const me = d.players.find((p) => p.isMe) ?? null;
  return {
    logId: d.logId,
    playedAt: d.playedAt,
    map: d.map,
    title: d.title,
    durationS: d.durationS,
    format: d.format,
    league: d.league,
    etf2lMatchId: d.etf2lMatchId,
    demosTfId: d.demosTfId,
    redScore: d.redScore,
    blueScore: d.blueScore,
    hasDemo: d.demos.length > 0,
    context: d.context,
    maps: d.segments.map((s) => s.map).filter((m): m is string => m !== null),
    me: me && d.result
      ? {
          team: me.team,
          mainClass: me.mainClass,
          kills: me.kills,
          deaths: me.deaths,
          assists: me.assists,
          dmg: me.dmg,
          timeS: me.timeS,
          result: d.result,
        }
      : null,
  };
});

const FAKE_MATCHES: MatchSummary[] = (() => {
  const r = rng(42);
  const pick = <T,>(xs: T[]) => xs[Math.floor(r() * xs.length)];
  const out: MatchSummary[] = [];
  let t = 1_789_675_330;
  for (let i = 0; i < 180; i++) {
    t -= Math.floor(3_600 * (4 + r() * 60));
    const official = r() < 0.07;
    const kind: ContextKind = official ? "official" : r() < 0.75 ? "scrim" : "pug";
    const cls = pick(CLASSES);
    const opp = pick(["Valhalla", "Olutlaatikko", "The Openhatters", "BLEU", null]);
    const dur = Math.floor(1_200 + r() * 1_800);
    const [red, blue] = [Math.floor(r() * 4), Math.floor(r() * 4)];
    const team: "Red" | "Blue" = r() < 0.5 ? "Red" : "Blue";
    const [mine, theirs] = team === "Red" ? [red, blue] : [blue, red];
    const kills = Math.floor(4 + r() * 32);
    out.push({
      logId: 4_122_234 - i * 17,
      playedAt: t,
      map: pick(MAPS),
      title: official
        ? `ETF2L HL S36 High - DD14 vs ${pick(["TWS", "GOYDA", "9S", "Kebab"])}`
        : pick(["serveme.tf #1563599 RED vs BLU", "TF2Center Lobby #1330112", "pro vs noob scrim"]),
      durationS: dur,
      format: "highlander",
      league: official ? "etf2l" : null,
      etf2lMatchId: official ? 92_883 - i : null,
      demosTfId: r() < 0.82 ? 1_507_898 - i : null,
      redScore: red,
      blueScore: blue,
      hasDemo: r() < 0.15,
      maps: [],
      context: {
        kind,
        etf2lMatchId: official ? 92_883 - i : null,
        linkMethod: official ? "trends" : null,
        teamName: kind === "pug" ? null : "SBQRRA",
        oppName: kind === "pug" ? null : opp,
        regulars: kind === "pug" ? Math.floor(r() * 3) : 5 + Math.floor(r() * 4),
        official: official
          ? {
              competition: "Highlander Season 35 (Spring 2026): Division 2",
              category: "Highlander Season",
              division: "Division 2",
              tier: 2,
              week: 1 + (i % 7),
              round: `Week ${1 + (i % 7)}`,
              score: mine > theirs ? [2, 0] : [0, 2],
              defaultWin: false,
            }
          : null,
      },
      me: {
        team,
        mainClass: cls,
        kills,
        deaths: Math.floor(3 + r() * 25),
        assists: Math.floor(r() * 12),
        dmg: Math.floor(kills * (180 + r() * 260)),
        timeS: dur,
        result: mine > theirs ? "W" : mine < theirs ? "L" : "T",
      },
    });
  }
  return [...FIXTURE_ROWS, ...out].sort((a, b) => (b.playedAt ?? 0) - (a.playedAt ?? 0));
})();

const fakeStats = (pending: number): IndexStats => ({
  indexed: 1494,
  superseded: 480,
  highlander: 666,
  sixes: 117,
  other: 17,
  unclassified: 214,
  officials: 58,
  fetched: 759 - pending,
  normalized: 759 - pending,
  pending,
  failed: 0,
});

let handlers: SyncHandlers | null = null;
let stvHandlers: StvHandlers | null = null;
let busy = false;
/** Logs still waiting to be fetched; a completed sync clears it. */
let pending = 24;

/** Walks through every progress stage the real sync emits, quickly. */
function simulateSync(kind: "sync" | "reprocess") {
  busy = true;
  const steps: Array<() => void> = [];
  if (kind === "sync") {
    for (const rows of [100, 400, 800, 1200, 1280]) {
      steps.push(() => handlers?.onProgress({ kind: "indexing", source: "trends.tf", rows }));
    }
    steps.push(() => handlers?.onProgress({ kind: "indexing", source: "logs.tf", rows: 0 }));
    steps.push(() =>
      handlers?.onProgress({ kind: "indexed", trendsRows: 1280, logstfRows: 1492, superseded: 480 }),
    );
    for (let done = 0; done <= 24; done += 2) {
      steps.push(() =>
        handlers?.onProgress({ kind: "fetching", done, total: 24, logId: 4_122_234 - done }),
      );
    }
    for (let done = 0; done <= 24; done += 6) {
      steps.push(() => handlers?.onProgress({ kind: "rawLogs", done, total: 24 }));
    }
    for (let done = 0; done <= 3; done++) {
      steps.push(() => handlers?.onProgress({ kind: "etf2l", done, total: 3 }));
    }
  } else {
    for (let done = 0; done <= 759; done += 69) {
      steps.push(() => handlers?.onProgress({ kind: "reprocessing", done, total: 759 }));
    }
  }
  steps.push(() => {
    busy = false;
    const fetched = kind === "sync" ? pending : 0;
    if (kind === "sync") pending = 0;
    handlers?.onDone({ kind, fetched, failed: 0, stats: fakeStats(pending) });
  });
  steps.forEach((step, i) => setTimeout(step, 180 * (i + 1)));
}

export const mockApi: Api = {
  appStatus: (): Promise<AppStatus> =>
    delay({
      version: "0.1.0-mock",
      dbPath: "C:\\Users\\you\\AppData\\Roaming\\gg.highlander.rating\\hl.sqlite3",
      ready: state.steamid !== null && state.tfPath !== null,
      config: { ...state },
    }),

  getConfig: () => delay({ ...state }),

  setSteamId: (input: string) => {
    if (!/^\d{17}$|^\[?U:1:\d+\]?$|^STEAM_[0-5]:[01]:\d+$|profiles\/\d+/i.test(input.trim())) {
      return Promise.reject({ kind: "invalid_steamid", message: `unrecognised format \`${input}\`` });
    }
    // The real backend canonicalises to SteamID64; approximate that by keeping
    // a SteamID64 as typed and standing in for any other format.
    const trimmed = input.trim();
    state.steamid = /^\d{17}$/.test(trimmed) ? trimmed : "76561198099396919";
    return delay({ ...state });
  },

  detectTfPath: () =>
    delay(fakeTfPath("D:\\SteamLibrary\\steamapps\\common\\Team Fortress 2\\tf", true)),

  inspectTfPath: (path: string) => delay(fakeTfPath(path, path.toLowerCase().includes("tf"))),

  setTfPath: (path: string) => {
    const info = fakeTfPath(path, true);
    state.tfPath = info.path;
    return delay(info);
  },

  listMatches: (q: MatchQuery): Promise<MatchPage> => {
    const filtered = FAKE_MATCHES.filter(
      (m) =>
        (q.format === null || m.format === q.format) &&
        (q.kind === null || m.context?.kind === q.kind) &&
        (q.from === null || (m.playedAt ?? 0) >= q.from) &&
        (q.to === null || (m.playedAt ?? 0) <= q.to),
    );
    return delay({ total: filtered.length, items: filtered.slice(q.offset, q.offset + q.limit) });
  },

  // Generated rows have no detail behind them; they open a real fixture,
  // relabelled, so every row in browser mode leads somewhere.
  getMatch: (logId: number) => {
    const exact = FIXTURES.find((f) => f.logId === logId);
    return delay(exact ?? { ...FIXTURES[0], logId });
  },

  // Real profiles exported with `hl profile <class> --json`. Classes without
  // a fixture come back empty, like a class with no rated games.
  // A filtered profile keeps the fixture's numbers but narrows its game lists,
  // which is enough to exercise the layout.
  // Fights from `hl fights sniper --json`; the period narrows nothing here.
  getProfile: (cls: string | null, kind: ContextKind | null = null) => {
    const fights = (cls ?? "sniper") === "sniper" ? (fightsSniper as unknown as FightsCard) : null;
    const byClass: Record<string, ProfileResponse> = {
      sniper: { ...(profileSniper as unknown as ProfileResponse), fights },
      engineer: { ...(profileEngineer as unknown as ProfileResponse), fights: null },
    };
    const hit = byClass[cls ?? "sniper"];
    if (!hit?.profile || kind === null) return delay(hit ?? { classes: byClass.sniper.classes, profile: null });
    const p = hit.profile;
    const trend = p.trend.filter((t) => t.kind === kind);
    const only = <T extends { kind: ContextKind | null }>(xs: T[]) => xs.filter((x) => x.kind === kind);
    return delay({
      ...hit,
      profile: trend.length === 0 ? null : { ...p, filter: kind, games: trend.length, trend, best: only(p.best), worst: only(p.worst) },
    });
  },

  // From `hl seasons sniper --json`.
  listSeasons: () => delay((seasonsSniper as unknown as SeasonsView).seasons.map((r) => r.season)),
  getSeasons: (cls: string) => delay({ ...(seasonsSniper as unknown as SeasonsView), class: cls }),

  // One real analysis (the TWS official on Upward); every match opens it.
  getMatchAnalysis: (logId: number) =>
    delay(
      logId === 3863290
        ? (analysis3863290 as unknown as Analysis)
        : { ...(analysis4109131 as unknown as Analysis), logId },
      200,
    ),
  getMapView: (map: string) =>
    delay(
      map.includes("upward")
        ? (mapviewUpward as unknown as MapView)
        : map.includes("ashville")
          ? (mapviewAshville as unknown as MapView)
          : null,
      150,
    ),

  // Map images are third-party files kept out of the repo. For local UI work,
  // put them in ui/public/overviews-local/ (git-ignored) and they are used.
  getMapOverview: async (map: string): Promise<Overview | null> => {
    const placements: Record<string, [number, number, number]> = {
      upward: [5.5, -4956, 2216],
      ashville: [8, -7322, 4101],
      vigil: [7.5, -5802, 4940],
      proot: [7.75, -7054, 3968],
    };
    const base = Object.keys(placements).find((b) => map.includes(b));
    if (!base) return null;
    const url = `/overviews-local/${base}.png`;
    const ok = await fetch(url, { method: "HEAD" }).then((r) => r.ok && (r.headers.get("content-type") ?? "").startsWith("image"), () => false);
    if (!ok) return null;
    const [s, x, y] = placements[base];
    const size = 1024 * s;
    return { mapBase: base, minX: x + 910 * s - size / 2, maxY: y - 512 * s + size / 2, size, image: url };
  },

  rawlogStats: () => delay({ stored: 740, pending: 16, missing: 2, bytes: 79_900_000, kills: 226_784 }),

  getTeammates: (all: boolean) => delay((all ? teammatesAll : teammatesTeam) as unknown as Teammates),

  contextCounts: () =>
    delay({ officials: 58, scrims: 544, pugs: 156, rosterOfficials: 15, etf2lMatches: 46, etf2lPlayer: 97913, lastFetch: 1_789_700_000 }),

  openExternal: async (url: string) => {
    window.open(url, "_blank", "noopener");
  },

  copyText: async (text: string) => {
    await navigator.clipboard?.writeText(text).catch(() => undefined);
    (window as unknown as { __lastCopied?: string }).__lastCopied = text;
  },

  scanDemos: () =>
    delay({ scanned: 101, unreadable: 0, removed: 0, logsPlaced: 759, links: 26, demosLinked: 25, matchesWithDemo: 23, markers: 883 }),
  demoStats: () => delay({ demos: 101, linked: 25, stv: 0, markers: 883, matchesWithDemo: 23 }),

  // Simulates a download so the progress UI can be exercised in a browser.
  fetchStv: async (logId: number) => {
    const total = 48_000_000;
    let bytes = 0;
    const step = () => {
      bytes = Math.min(total, bytes + 6_000_000);
      stvHandlers?.onProgress({ logId, bytes, total });
      if (bytes < total) setTimeout(step, 150);
      else stvHandlers?.onDone({ demoId: 999, fileName: "match-20260823-1956-pl_upward_f12.dem", bytes, logShare: 0.37 });
    };
    setTimeout(step, 150);
  },

  onStv: async (h) => {
    stvHandlers = h;
    return () => {
      if (stvHandlers === h) stvHandlers = null;
    };
  },

  indexStats: () => delay(fakeStats(pending)),
  syncBusy: () => delay(busy),

  syncStart: () => {
    if (busy) return Promise.reject({ kind: "busy", message: "A sync is already running." });
    simulateSync("sync");
    return delay(undefined);
  },

  reprocessStart: () => {
    if (busy) return Promise.reject({ kind: "busy", message: "A sync is already running." });
    simulateSync("reprocess");
    return delay(undefined);
  },

  onSync: async (h: SyncHandlers) => {
    handlers = h;
    return () => {
      if (handlers === h) handlers = null;
    };
  },
};
