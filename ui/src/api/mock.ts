// Browser-mode stand-in for the Rust backend.
//
// Active only when the page is open outside Tauri (`npm run ui` in a plain
// browser), so UI work does not require a Rust rebuild. Inside the app window
// this file is never used.

import type { Api, StvHandlers, SyncHandlers } from "./client";
import match4109131 from "./fixtures/match_4109131.json";
import match4114301 from "./fixtures/match_4114301.json";
import match4111116 from "./fixtures/match_4111116.json";
// `hl guide --json`: the live models, as the built app serves them.
import ratingGuide from "./fixtures/rating_guide.json";
import seasonsOverview from "./fixtures/seasons_overview.json";
import seasonPodiums33 from "./fixtures/season_podiums_33.json";
import teamHonours35600 from "./fixtures/team_honours_35600.json";
import teamTransfers35600 from "./fixtures/team_transfers_35600.json";
import teamInfo35600 from "./fixtures/team_info_35600.json";
import upcomingFixtures from "./fixtures/fixtures.json";
import newsPosts from "./fixtures/news.json";
import newsHtml from "./fixtures/news_html.json";
import playerTeams139131191 from "./fixtures/player_teams_139131191.json";
import cart4109131 from "./fixtures/cart_4109131.json";
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
import leagues from "./fixtures/leagues.json";
import team37805 from "./fixtures/team_37805.json";
import type {
  Bookmark,
  BuildingRow,
  CmdError,
  MassBatch,
  MassCandidate,
  MassRunStep,
  MassSearchStep,
  MassSummary,
  LifeOnMap,
  LivesOnMap as LivesOnMapView,
  MapWithLives,
  DemoIndexSummary,
  ReRead,
  Fixture,
  NewsItem,
  NewsPost,
  ResultRow,
  TeamHit,
  LeagueMatch,
  NewLogs,
  MatchSides,
  Stay,
  TeamInfo,
  TeamTransfers,
  SpyReport,
  MatchDemoStats,
  DemoLinked,
  CalloutFile,
  PositionsView,
  MapTendencies,
  LeagueView,
  TeamView,
  DemoImported,
  CartView,
  Analysis,
  AppConfig,
  AppStatus,
  ContextKind,
  IndexStats,
  MatchContext,
  MapView,
  MatchDetail,
  Overview,
  OverviewImage,
  Placement,
  MapsOverview,
  PresetCheck,
  Profile,
  MatchPage,
  MatchQuery,
  MatchSummary,
  ProfileResponse,
  Teammates,
  TfPathInfo,
  FightsCard,
  SeasonsView, StvStage, RatingGuide, LeagueSample, LeagueActivity, CatalogueHit, PlayerProfile, PlayerStats, Rankings, MatchDivisions, CareerView, SeasonTile, Podium, TeamHonours, TeamEtf2l } from "./types";

// Starts configured, since setup is not what you are usually iterating on.
// Append `?setup` to the URL to start from the first-run screen instead.
const startInSetup = typeof location !== "undefined" && location.search.includes("setup");
const state: AppConfig = startInSetup
  ? { steamid: null, tfPath: null }
  : {
      steamid: "76561198099396919",
      tfPath: "D:\\SteamLibrary\\steamapps\\common\\Team Fortress 2\\tf",
    };

// URLs only: Vite serves the files, nothing is read into the bundle.
const OVERVIEW_URLS = import.meta.glob("../../../overviews/*.png", { query: "?url", import: "default", eager: true }) as Record<string, string>;

// more.tf's placement for the maps the browser preview has fixtures on.
const MOCK_PLACEMENTS: Record<string, [number, number, number]> = {
  upward: [5.5, -4956, 2216],
  ashville: [8, -7322, 4101],
  vigil: [7.5, -5802, 4940],
  proot: [7.75, -7054, 3968],
};

function mockPlacement(base: string): Placement | null {
  const p = MOCK_PLACEMENTS[base];
  if (!p) return null;
  const [s, x, y] = p;
  const size = 1024 * s;
  return { minX: x + 910 * s - size / 2, maxY: y - 512 * s + size / 2, size };
}

const delay = <T,>(value: T, ms = 120): Promise<T> =>
  new Promise((resolve) => setTimeout(() => resolve(value), ms));

// The repo's callout seeds, as the app ships them; a saved copy lives in
// localStorage so the editor can be worked on in a browser.
const CALLOUT_SEEDS = import.meta.glob("../../../callouts/*.json", { eager: true, import: "default" }) as Record<string, CalloutFile>;

function mapBase(map: string): string {
  const m = map.toLowerCase().replace(/^(koth|pl|cp|ctf|pass)_/, "");
  return m.split("_")[0];
}

function mockCallouts(map: string, saved?: CalloutFile): CalloutFile {
  const base = mapBase(map);
  let mine = saved ?? null;
  if (!mine) {
    try {
      const raw = localStorage.getItem(`hl.mock.callouts.${base}`);
      mine = raw ? (JSON.parse(raw) as CalloutFile) : null;
    } catch {
      mine = null;
    }
  }
  if (mine) return { ...mine, map: base, origin: "yours" };
  const seed = Object.entries(CALLOUT_SEEDS).find(([path]) => path.endsWith(`/${base}.json`))?.[1];
  if (seed) return { ...seed, origin: "built in" };
  return { map: base, draft: false, source: "", zones: [], names: [], origin: "none" };
}

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

/** 4109131 is a real combined log: these are the parts it was built from. */
const FIXTURE_PARTS: Record<number, MatchDetail["parts"]> = {
  4109131: [
    { logId: 4109086, title: "serveme.tf #1563599 RED vs BLU", map: "koth_product_final", playedAt: 1757619000, durationS: 778, playerCount: 18 },
    { logId: 4109098, title: "serveme.tf #1563599 RED vs BLU", map: "pl_vigil_rc10", playedAt: 1757620200, durationS: 916, playerCount: 18 },
    { logId: 4109125, title: "serveme.tf #1563599 RED vs BLU", map: "pl_vigil_rc10", playedAt: 1757621600, durationS: 1122, playerCount: 18 },
  ],
};

const FIXTURES: MatchDetail[] = [match4109131, match4114301, match4111116, match3863290].map((f) => {
  const d = f as unknown as MatchDetail;
  const segments = FIXTURE_SEGMENTS[d.logId] ?? [
    { map: d.map, firstRound: 1, lastRound: d.rounds.length, rounds: d.rounds.length, redWins: 0, blueWins: 0 },
  ];
  return { ...d, context: FIXTURE_CONTEXT[d.logId] ?? null, segments, parts: FIXTURE_PARTS[d.logId] ?? [] };
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
    parts: d.parts?.length ?? 0,
    rating: me?.rating?.score ?? null,
    classCount: null,
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

let newestLooks = 0;

let rereadListeners: Array<(s: { logId: number; step: string }) => void> = [];

const MOCK_HITS: CatalogueHit[] = [
  { accountId: 139131191, name: "Flashy", highest: { name: "Mid", tier: 2 }, mainClass: "sniper", medals: [1, 0, 0], officials: 25, lastSeen: 1_790_532_000 },
  { accountId: 227974365, name: "rzeke", highest: { name: "Premiership", tier: 0 }, mainClass: "spy", medals: [2, 1, 1], officials: 88, lastSeen: 1_790_532_000 },
  { accountId: 164932866, name: "Mifune", highest: { name: "High", tier: 1 }, mainClass: "engineer", medals: [0, 1, 0], officials: 41, lastSeen: 1_789_000_000 },
  // Only in your pugs, never in an official: greyed in the lists (Q74).
  { accountId: 412000001, name: "Flamingo", highest: null, mainClass: "scout", medals: [0, 0, 0], officials: 0, lastSeen: 1_789_500_000 },
];

const mockTeam = (id: number, name: string) => ({ id, name, avatar: null });
function mockProfile(accountId: number): PlayerProfile {
  const hit = MOCK_HITS.find((h) => h.accountId === accountId) ?? MOCK_HITS[0];
  const dd = mockTeam(37805, "DD14"), sbq = mockTeam(35600, "SBQRRA");
  return {
    accountId,
    steamid64: "76561198099396919",
    name: hit.name,
    aliases: ["flashy", "SBQR flashy", "FlashBangy"],
    country: "Netherlands",
    avatar: null,
    declaredClasses: ["Sniper"],
    playedClasses: [["sniper", 690], ["engineer", 30], ["scout", 16]],
    mainClass: hit.mainClass,
    current: { season: 36, seasonName: "Autumn 2026", division: "High", tier: 1, team: dd, played: 5, won: 4, lost: 1, place: null },
    highest: hit.highest,
    medals: [{ season: 33, seasonName: "Spring 2025", division: "Low", tier: 3, place: 1, team: sbq, how: "Grand Final 6-3" }],
    // From `hl mvp 30` on a copy, 1 October 2026.
    mvps: [
      { season: 30, seasonName: "Autumn 2023", division: "Open", tier: 4, class: "sniper", accountId, team: sbq, won: true, event: false, score: 1.13, finalRating: 1.21, finalMaps: 2, playoffsRating: 0.93, playoffsMaps: 2 },
    ],
    seasons: [
      { season: 36, seasonName: "Autumn 2026", division: "High", tier: 1, team: dd, played: 5, won: 4, lost: 1, place: null },
      { season: 34, seasonName: "Summer 2025", division: "Mid", tier: 2, team: sbq, played: 6, won: 4, lost: 2, place: null },
      { season: 33, seasonName: "Spring 2025", division: "Low", tier: 3, team: sbq, played: 9, won: 7, lost: 2, place: 1 },
    ],
    officials: [
      { matchId: 93055, competition: "Highlander Season 36", tier: 2, time: 1_790_532_000, season: 36, division: "High", stage: "regular", round: "Week 5", team: dd, opponent: mockTeam(1, "TWS"), scoreFor: 4, scoreAgainst: 2, won: true },
      { matchId: 92011, competition: "Highlander Season 36", tier: 2, time: 1_789_900_000, season: 36, division: "High", stage: "regular", round: "Week 4", team: dd, opponent: mockTeam(2, "ЭТО МОЁ БОЛОТО"), scoreFor: 1, scoreAgainst: 5, won: false },
      { matchId: 85001, competition: "Highlander Season 33 Playoffs", tier: 4, time: 1_757_000_000, season: 33, division: "Low", stage: "Playoffs", round: "Grand Final", team: sbq, opponent: mockTeam(3, "Gibus and The Gang"), scoreFor: 6, scoreAgainst: 3, won: true },
    ],
    etf2lId: 97913,
    // Flashy's api-v2 /player answer, 3 October 2026.
    registered: 1_402_247_571,
    etf2lTitle: "Player",
    etf2lTeams: [
      { id: 35849, name: "The 9 Stooges", tag: "9S", kind: "Highlander Fun Team", country: "Croatia", avatar: "https://etf2l.org/wp-content/uploads/avatars/657792a02e49d.png" },
      { id: 37805, name: "DD14", tag: "DD14", kind: "Highlander", country: "Netherlands", avatar: null },
    ],
    bans: [],
  };
}

const tier = (tier: number, division: string, matches: number, logs: number, json: number, maps: number) => ({
  tier, division, matches, logs, jsonLogstf: Math.round(json / 4), jsonDropstf: json - Math.round(json / 4), jsonMoretf: Math.round(json / 10), raw: Math.round(json / 3), rawMissing: 2, maps, rosters: Math.round(matches / 2), oldest: 1_724_000_000, newest: 1_790_538_131,
});
let leagueSample: LeagueSample = {
  enabled: true,
  officials: 2950,
  rostersRead: 1210,
  players: 3480,
  discoveredAt: 1_790_700_000,
  logsListed: 5695,
  tiers: [tier(0, "Premiership", 78, 177, 140, 8), tier(1, "High", 83, 186, 90, 9), tier(2, "Mid", 124, 277, 60, 12), tier(3, "Low", 145, 322, 20, 9), tier(4, "Open", 154, 327, 0, 11)],
  bytes: 21_000_000,
  logstfResting: false,
  targetPerTier: 300,
};

/** Q69: the browser build's mass imports, for the page's life. */
type MassHandlers = {
  onSearch: (s: MassSearchStep) => void;
  onProgress: (s: MassRunStep) => void;
  onDone: (s: MassSummary) => void;
  onError: (e: { batchId: number; error: CmdError }) => void;
};
let massListeners: MassHandlers[] = [];
let mockBatches: MassBatch[] = [];
let mockMassCancel = false;

/** Q60: the browser build's bookmarks, for the page's life. */
let mockBookmarks: Bookmark[] = [{ kind: "season", id: 46, label: "Season 46", added: 0 }];

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
      parts: 0,
      // A rating, not a percentile: a spread around 1.00 like the real one.
      rating: Math.round((0.6 + r() * 0.9) * 100) / 100,
      classCount: null,
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
  outsideWindow: allHistory ? 0 : 282,
});

// The full-history setting, for the Settings panel's dialog.
let allHistory = false;
let autoDelete = false;

let handlers: SyncHandlers | null = null;
let stvHandlers: StvHandlers | null = null;
const stvQueue: number[] = [];

function announceStv() {
  stvQueue.forEach((logId, position) => stvHandlers?.onQueued({ logId, position }));
}

/**
 * What the backend does once the file is down, at a watchable pace: link it,
 * read your recording and the server's (each then kept as a timeline), save.
 */
function afterDownload(logId: number, then: () => void) {
  const steps: Array<Partial<StvStage> & { step: StvStage["step"] }> = [{ step: "linking" }];
  for (const [demo, kind] of [[1, "pov"], [2, "stv"]] as const) {
    for (let pct = 0; pct <= 95; pct += 5) steps.push({ step: "reading", demo, of: 2, kind, pct });
    steps.push({ step: "keeping", demo, of: 2 });
  }
  steps.push({ step: "saving" });
  let i = 0;
  const next = () => {
    const s = steps[i++];
    if (!s) return then();
    stvHandlers?.onStage({ logId, demo: null, of: null, kind: null, pct: null, ...s });
    setTimeout(next, s.step === "reading" ? 90 : 500);
  };
  next();
}

/** Download whatever is at the head of the queue, then move on to the next. */
function runStv() {
  const logId = stvQueue[0];
  if (logId === undefined) return;
  const total = 48_000_000;
  let bytes = 0;
  const step = () => {
    bytes = Math.min(total, bytes + 6_000_000);
    stvHandlers?.onProgress({ logId, bytes, total });
    if (bytes < total) {
      setTimeout(step, 150);
      return;
    }
    afterDownload(logId, () => {
      stvHandlers?.onDone({ logId, demoId: 999, fileName: "match-20260823-1956-pl_upward_f12.dem", bytes, logShare: 0.37 });
      stvQueue.shift();
      announceStv();
      runStv();
    });
  };
  setTimeout(step, 150);
}
let busy = false;
/** Logs still waiting to be fetched; a completed sync clears it. */
let pending = 24;

/** Walks through every progress stage the real sync emits, quickly. */
let syncTimers: number[] = [];
let demoFolders: string[] = ["E:/TF2 archive/demoreviews"];

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
    // In the real order (sync_start): ETF2L before anything is fetched.
    for (let done = 0; done <= 3; done++) {
      steps.push(() => handlers?.onProgress({ kind: "etf2l", done, total: 3 }));
    }
    for (let done = 0; done <= 24; done += 2) {
      steps.push(() =>
        handlers?.onProgress({ kind: "fetching", done, total: 24, logId: 4_122_234 - done }),
      );
    }
    for (let done = 0; done <= 2; done++) {
      steps.push(() => handlers?.onProgress({ kind: "parts", done, total: 2 }));
    }
    for (let done = 0; done <= 24; done += 6) {
      steps.push(() => handlers?.onProgress({ kind: "rawLogs", done, total: 24 }));
    }
    for (const what of ["Refreshing your profile", "Matching demos.tf", "Reading ETF2L seasons", "Scanning your demos folder", "Resolving each round's map"]) {
      steps.push(() => handlers?.onProgress({ kind: "stage", what }));
      steps.push(() => handlers?.onProgress({ kind: "stage", what }));
    }
    for (let done = 0; done <= 24; done += 8) {
      steps.push(() => handlers?.onProgress({ kind: "fights", done, total: 24 }));
    }
    for (let done = 0; done <= 2; done++) {
      steps.push(() =>
        handlers?.onProgress({
          kind: "readingDemos",
          done,
          total: 2,
          logId: 4_122_234,
          what: { map: "pl_upward", playedAt: 1789401600, kind: "official", opponent: "Kebab" },
        }),
      );
    }
    for (let done = 0; done <= 666; done += 111) {
      steps.push(() => handlers?.onProgress({ kind: "rating", done, total: 666 }));
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
  // Slow enough to read each step on the card.
  syncTimers = steps.map((step, i) => window.setTimeout(step, 450 * (i + 1)));
}

export const mockApi: Api = {
  updateKind: () => delay("self" as const),
  appStatus: (): Promise<AppStatus> =>
    delay({
      version: "0.1.0-mock",
      dbPath: "C:\\Users\\you\\AppData\\Roaming\\gg.highlander.rating\\hl.sqlite3",
      ready: state.steamid !== null && state.tfPath !== null,
      config: { ...state },
      // The browser mock is never a wiped install; the banner has its own
      // story in the preview when this is filled in by hand.
      restore: null,
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
        (q.to === null || (m.playedAt ?? 0) <= q.to) &&
        (!q.stvOnly || m.hasDemo),
    );
    // A class sort (Q61): a made-up count per match, highest first.
    const rows = /^(killed|diedto):/.test(q.sort ?? "")
      ? filtered
          .map((m) => ({ ...m, classCount: m.logId % 7 === 0 ? null : (m.logId * 31 + (q.sort ?? "").length) % 9 }))
          .sort((a, b) => (a.classCount === null ? 1 : b.classCount === null ? -1 : q.ascending ? a.classCount - b.classCount : b.classCount - a.classCount))
      : filtered;
    return delay({ total: rows.length, items: rows.slice(q.offset, q.offset + q.limit) });
  },

  // Generated rows have no detail behind them; they open a real fixture,
  // relabelled, so every row in browser mode leads somewhere.
  getMatch: (logId: number) => {
    const exact = FIXTURES.find((f) => f.logId === logId);
    // The list's second match was stored from more.tf, so its banner can be seen.
    return delay(exact ?? { ...FIXTURES[0], logId, standIn: logId === FAKE_MATCHES[1]?.logId ? "more.tf" : null });
  },

  // Real profiles exported with `hl profile <class> --json`. Classes without
  // a fixture come back empty, like a class with no rated games.
  // A filtered profile keeps the fixture's numbers but narrows its game lists,
  // which is enough to exercise the layout.
  // Fights from `hl fights sniper --json`; the period narrows nothing here.
  getProfile: (cls: string | null, kind: ContextKind | null = null) => {
    const fights = (cls ?? "sniper") === "sniper" ? (fightsSniper as unknown as FightsCard) : null;
    // Aim only exists for matches with a demo, so the mock carries the real
    // shape of it: a smaller sample than the rating's.
    const aim = { kills: 312, errorDeg: 5.8, beforeDeg: 15.2, flickDeg: 10.4, rangeUnits: 1147, heldShare: 0.24, biasX: 0.6, biasY: 2.4 };
    const aimAll = { kills: 496, errorDeg: 6.3, beforeDeg: 16.5, flickDeg: 11.1, rangeUnits: 1096, heldShare: 0.22, biasX: 0.8, biasY: 2.9 };
    const life = { scopedShare: 0.23, minutes: 412, deaths: 218, nearestMate: 502, aloneShare: 0.08, scopedShareDeaths: 0.51, behindShare: 0.28 };
    const lifeAll = { scopedShare: 0.21, minutes: 640, deaths: 354, nearestMate: 449, aloneShare: 0.1, scopedShareDeaths: 0.49, behindShare: 0.31 };
    const byClass: Record<string, ProfileResponse> = {
      sniper: { ...(profileSniper as unknown as ProfileResponse), fights, aim, life, aimAll, lifeAll },
      engineer: { ...(profileEngineer as unknown as ProfileResponse), fights: null, aim: null, life: null, aimAll: null, lifeAll: null },
    };
    const hit = byClass[cls ?? "sniper"];
    if (!hit?.profile || kind === null)
      return delay(hit ?? { classes: byClass.sniper.classes, profile: null, fights: null, aim: null, life: null, aimAll: null, lifeAll: null });
    const p = hit.profile;
    const trend = p.trend.filter((t) => t.kind === kind);
    const only = <T extends { kind: ContextKind | null }>(xs: T[]) => xs.filter((x) => x.kind === kind);
    return delay({
      ...hit,
      profile: trend.length === 0 ? null : { ...p, filter: kind, games: trend.length, trend, best: only(p.best), worst: only(p.worst) },
    });
  },

  getOwner: () => {
    // Whoever the SteamID was last set to; only the fixtures' owner has a name.
    const id = state.steamid ?? "76561198099396919";
    return delay({ steamid64: id, name: id === "76561198099396919" ? "Flashy" : null, avatar: null });
  },

  // From `hl seasons sniper --json`.
  listSeasons: () => delay((seasonsSniper as unknown as SeasonsView).seasons.map((r) => r.season)),
  getSeasons: (cls: string) => delay({ ...(seasonsSniper as unknown as SeasonsView), class: cls }),

  // One real analysis (the TWS official on Upward); every match opens it.
  getAim: (logId: number, player?: number) => {
    // Plausible readings so the tab can be worked on in a browser: mostly
    // held angles, a few flicks, Sniper ranges.
    const r = rng(logId);
    const kills = Array.from({ length: 18 }, (_, i) => {
      const flick = r() < 0.25 ? 20 + r() * 60 : r() * 6;
      const errorDeg = 0.4 + r() * 2.5;
      const beforeDeg = flick > 10 ? 15 + r() * 40 : r() * 6;
      // A miss has a direction: a slight high-right habit, as a real one is.
      const split = (mag: number, biasX: number, biasY: number) => {
        const a = r() * Math.PI * 2;
        return [mag * Math.cos(a) + biasX, mag * Math.sin(a) + biasY] as const;
      };
      const [dxDeg, dyDeg] = split(errorDeg * 0.7, 0.3, 0.9);
      const [beforeDxDeg, beforeDyDeg] = split(beforeDeg * 0.7, 0.4, 1.1);
      // A path that closes on the head, with a little overshoot on flicks.
      const path: Array<[number, number]> = Array.from({ length: 10 }, (_, j) => {
        const t = j / 9;
        const ease = 1 - Math.pow(1 - t, 2.5);
        const over = flick > 10 && t > 0.6 && t < 0.9 ? -0.25 : 0;
        return [
          beforeDxDeg * (1 - ease + over) + dxDeg * ease,
          beforeDyDeg * (1 - ease + over) + dyDeg * ease,
        ] as [number, number];
      });
      return {
        demoId: 1,
        shooter: player ?? 139131191,
        path,
        tick: 5_000 + i * 1_800,
        atRaw: null,
        roundNum: 1 + (i % 5),
        victim: null,
        errorDeg,
        beforeDeg,
        dxDeg,
        dyDeg,
        beforeDxDeg,
        beforeDyDeg,
        flickDeg: flick,
        rangeUnits: 300 + r() * 1_900,
        height: Math.round((r() - 0.5) * 600),
        victimSeen: r() > 0.1,
        shooterSeen: true,
        headshot: r() > 0.45,
      };
    });
    const seen = kills.filter((k) => k.victimSeen);
    const mean = (f: (k: (typeof kills)[number]) => number) => seen.reduce((n, k) => n + f(k), 0) / seen.length;
    const totals = {
      kills: seen.length,
      errorDeg: mean((k) => k.errorDeg),
      beforeDeg: mean((k) => k.beforeDeg),
      flickDeg: mean((k) => k.flickDeg),
      rangeUnits: mean((k) => k.rangeUnits),
      heldShare: seen.filter((k) => k.beforeDeg <= 3).length / seen.length,
      biasX: mean((k) => k.dxDeg),
      biasY: mean((k) => k.dyDeg),
    };
    const deaths = Array.from({ length: 11 }, (_, i) => ({
      demoId: 1,
      who: player ?? 139131191,
      tick: 6_000 + i * 2_600,
      atRaw: null,
      roundNum: 1 + (i % 5),
      killer: null,
      killerRange: r() < 0.3 ? null : 200 + r() * 1_600,
      // Most deaths come from ahead, a third from beside or behind.
      killerDxDeg: (r() < 0.33 ? 90 + r() * 90 : r() * 60) * (r() < 0.5 ? 1 : -1),
      killerDyDeg: (r() - 0.5) * 30,
      nearestMate: 100 + r() * 1_400,
      matesNear: r() < 0.15 ? 0 : 1 + Math.floor(r() * 3),
      scoped: r() < 0.5,
      seen: true,
    }));
    const life = {
      scopedShare: 0.24,
      minutes: 31,
      deaths: deaths.length,
      nearestMate: deaths.reduce((n, d) => n + (d.nearestMate ?? 0), 0) / deaths.length,
      aloneShare: deaths.filter((d) => d.matesNear === 0).length / deaths.length,
      scopedShareDeaths: deaths.filter((d) => d.scoped).length / deaths.length,
      behindShare: deaths.filter((d) => Math.abs(d.killerDxDeg) > 90).length / deaths.length,
    };
    return delay(
      {
        player: player ?? 139131191,
        stv: true,
        kills,
        deaths,
        totals,
        life,
        career: { ...totals, errorDeg: 2.1, beforeDeg: 16.5, flickDeg: 11.1, rangeUnits: 1096, heldShare: 0.22, biasX: 0.8, biasY: 2.9 },
        careerLife: { ...life, scopedShare: 0.21, nearestMate: 449, aloneShare: 0.1, scopedShareDeaths: 0.49, behindShare: 0.31 },
      },
      200,
    );
  },

  playedFilters: (q?: { kind?: string | null; class?: string | null; map?: string | null }) => {
    // Narrower filters, smaller counts: enough to see them follow.
    const share = (q?.kind ? 0.3 : 1) * (q?.map ? 0.2 : 1);
    const scale = (rows: Array<[string, number]>, by: number) => rows.map(([n, c]) => [n, Math.max(1, Math.round(c * by))] as [string, number]);
    return delay({
      classes: scale([
        ["sniper", 646],
        ["engineer", 58],
        ["scout", 21],
        ["spy", 14],
        ["medic", 9],
      ], share),
      maps: scale([
        ["product", 143],
        ["upward", 119],
        ["vigil", 98],
        ["swiftwater", 74],
        ["steel", 41],
        ["proot", 22],
      ], (q?.kind ? 0.3 : 1) * (q?.class ? 0.5 : 1)),
    });
  },

  getParts: (logId: number) => {
    // The combined fixture's three logs, two of them already "fetched": the
    // third exercises the fetch-on-demand path.
    const d = FIXTURES.find((f) => f.logId === logId);
    const parts = d?.parts ?? [];
    return delay(
      parts.map((p, i) => ({
        logId: p.logId,
        title: p.title,
        map: p.map,
        playedAt: p.playedAt,
        durationS: p.durationS,
        // A part is one map, so it has no segments -- the mock used to
        // inherit the combined log's three, which is why the browser never
        // reproduced the crash the built app had.
        detail: i < 2 && d ? ({ ...d, logId: p.logId, map: p.map, parts: [], segments: [] } as MatchDetail) : null,
        // Two rounds per part, as a real combined log splits them.
        parentRounds: [i * 2 + 1, i * 2 + 2],
      })),
      150,
    );
  },
  fetchPart: (partId: number) => {
    const d = FIXTURES[0];
    return delay({ ...d, logId: partId, parts: [], segments: [] } as MatchDetail, 600);
  },

  // Stateful, so the checkbox behaves in a browser the way it behaves in
  // the app. A mock that always answers "off" makes a working toggle look
  // broken, which is exactly the trap the parts fixture set with segments.
  autoDeleteDemos: () => delay(autoDelete),
  setAutoDeleteDemos: (on: boolean) => {
    autoDelete = on;
    return delay(on);
  },

  // Four downloaded STVs, one of them not finished with, so the panel's
  // "left alone" path is reachable in a browser.
  downloadedDemos: () =>
    delay([
      { demoId: 11, fileName: "stv_upward_2026-09-17.dem", sizeBytes: 82_400_000, map: "pl_upward_f12", startUtc: 1758100000, logs: 1, read: true },
      { demoId: 12, fileName: "stv_product_2026-09-14.dem", sizeBytes: 74_100_000, map: "koth_product_final", startUtc: 1757800000, logs: 2, read: true },
      { demoId: 13, fileName: "stv_vigil_2026-09-10.dem", sizeBytes: 69_800_000, map: "pl_vigil_rc9", startUtc: 1757400000, logs: 1, read: true },
      { demoId: 14, fileName: "stv_steel_2026-09-09.dem", sizeBytes: 91_200_000, map: "cp_steel_f12", startUtc: 1757300000, logs: 1, read: false },
    ]),

  deleteDownloadedDemos: (only: number | null = null, force = false) =>
    delay(
      only === null
        ? { deleted: force ? 4 : 3, bytes: force ? 317_500_000 : 226_300_000, skipped: force ? 0 : 1 }
        : { deleted: 1, bytes: 82_400_000, skipped: 0 },
      400,
    ),

  // The buildings beside the first life's teleport, as the routes are laid out.
  getBuildings: async (logId: number) => {
    const paths = await mockApi.getPaths(logId);
    const life = paths[0];
    const [, ax, ay] = life.points[19];
    const [, bx, by] = life.points[20];
    const b = (kind: BuildingRow["kind"], end: BuildingRow["end"], x: number, y: number, team: number): BuildingRow => ({
      demoId: 1, kind, end, team, builder: 1, builderName: "engie", x, y, z: 0, fromTick: 4_000, toTick: 20_000, level: 3,
    });
    return [
      b("teleporter", null, ax + 40, ay - 30, 3),
      b("teleporter", null, bx - 40, by + 30, 3),
      b("sentry", null, bx + 400, by + 250, 3),
      b("dispenser", null, bx + 470, by + 180, 3),
      b("sentry", null, ax - 900, ay - 300, 2),
    ];
  },
  // Q69: a pretend demos.tf, and a pretend run that takes a moment per demo.
  massSearch: async (_accountId: number, map: string, cls: string | null, want: number) => {
    for (let page = 1; page <= 3; page++) {
      massListeners.forEach((h) => h.onSearch({ step: "listing", page, found: page * 9 }));
      await delay(null, 150);
    }
    const out: MassCandidate[] = Array.from({ length: Math.min(want, 40) }, (_, i) => ({
      demosTfId: 1_515_729 - i * 37,
      time: 1_791_313_449 - i * 86_400 * 3,
      durationS: 1500,
      map: `koth_${map}_final`,
      name: `match-${i}.dem`,
      class: cls,
      team: i % 2 === 0 ? "Blue" : "Red",
      here: i === 3,
      localLog: i % 3 === 0 ? 4_115_749 - i : null,
    }));
    return out;
  },
  massStart: async (accountId: number, map: string, cls: string | null, picks: Array<{ demosTfId: number }>) => {
    const batchId = mockBatches.length + 1;
    const batch: MassBatch = {
      batchId,
      accountId,
      map,
      class: cls,
      createdAt: Math.floor(Date.now() / 1000),
      finishedAt: null,
      kept: null,
      demos: picks.map((p) => ({ demosTfId: p.demosTfId, demoId: null, logId: null, status: "queued", error: null, sizeBytes: null })),
    };
    mockBatches.unshift(batch);
    void (async () => {
      for (const [i, d] of batch.demos.entries()) {
        if (mockMassCancel) {
          d.status = "skipped";
          continue;
        }
        for (const f of [0.3, 0.7, 1]) {
          massListeners.forEach((h) => h.onProgress({ batchId, done: i, total: batch.demos.length, demosTfId: d.demosTfId, stage: "download", bytes: f * 60e6, ofBytes: 60e6 }));
          await delay(null, 120);
        }
        const failed = i === 2;
        Object.assign(d, failed ? { status: "failed", error: "has this match's players, but none of its kills line up" } : { status: "done", demoId: 100 + i, logId: 4_100_000 + i, sizeBytes: 60e6 });
      }
      batch.finishedAt = Math.floor(Date.now() / 1000);
      const sum: MassSummary = {
        batchId,
        done: batch.demos.filter((d) => d.status === "done").length,
        failed: batch.demos.filter((d) => d.status === "failed").length,
        skipped: batch.demos.filter((d) => d.status === "skipped").length,
        bytesOnDisk: batch.demos.reduce((n, d) => n + (d.sizeBytes ?? 0), 0),
      };
      mockMassCancel = false;
      massListeners.forEach((h) => h.onDone(sum));
    })();
    return batchId;
  },
  massCancel: async () => {
    mockMassCancel = true;
  },
  massBatches: () => delay(mockBatches.map((b) => ({ ...b, demos: b.demos.map((d) => ({ ...d })) }))),
  massKeep: async (batchId: number, keep: boolean) => {
    const b = mockBatches.find((x) => x.batchId === batchId);
    if (!b) return 0;
    b.kept = keep;
    let freed = 0;
    if (!keep)
      for (const d of b.demos) {
        freed += d.sizeBytes ?? 0;
        d.sizeBytes = null;
      }
    return freed;
  },
  massForget: async (batchId: number) => {
    mockBatches = mockBatches.filter((b) => b.batchId !== batchId);
  },
  onMass: async (h: MassHandlers) => {
    massListeners.push(h);
    return () => {
      massListeners = massListeners.filter((x) => x !== h);
    };
  },
  setMatchKind: (logId: number, kind: ContextKind | null) => {
    const m = FAKE_MATCHES.find((x) => x.logId === logId);
    if (!m?.context) return delay(false);
    if (kind === null) m.context = { ...m.context, linkMethod: null };
    else m.context = { ...m.context, kind, linkMethod: "manual" };
    return delay(true, 100);
  },
  livesMaps: (_accountId: number) =>
    delay<MapWithLives[]>([
      { map: "product", name: "koth_product_final", matches: 10, lives: 143 },
      { map: "upward", name: "pl_upward_f12", matches: 4, lives: 57 },
    ]),
  livesOnMap: (_accountId: number, map: string) => {
    // Wandering routes from each side's half, so the view has a pattern.
    const r = rng(map.length * 7);
    const classes = ["sniper", "sniper", "engineer", "scout", "spy"];
    // Product at Emiel's scale (Q64: about 3,000 lives), Upward small.
    const lives: LifeOnMap[] = Array.from({ length: map === "product" ? 3000 : 60 }, (_, i) => {
      const team = i % 2 === 0 ? "Red" : "Blue";
      let [x, y] = [team === "Red" ? -2_200 + r() * 600 : 1_600 + r() * 600, -800 + r() * 1_600];
      const points: Array<[number, number]> = [];
      for (let j = 0; j < 30 + Math.floor(r() * 60); j++) {
        x += (team === "Red" ? 1 : -1) * r() * 90;
        y += (r() - 0.5) * 120;
        points.push([Math.round(x), Math.round(y)]);
      }
      return { logId: 4_100_000 + (i % 10), playedAt: 1_790_000_000 - (i % 10) * 86_400 * 9, demoId: i % 10, stv: i % 10 === 0, class: classes[i % classes.length], team, roundNum: 1 + (i % 5), died: r() < 0.7, seconds: points.length / 4, points, capped: i % 4 === 0, afterEnemyCap: i % 6 === 1, midFight: i % 2 === 0 };
    });
    return delay<LivesOnMapView>({ map, matches: 10, stvMatches: 1, lives }, 300);
  },
  getPaths: (logId: number) => {
    // A lap of a small loop, so the layer has something to draw in a browser.
    const r = rng(logId);
    const lives = Array.from({ length: 6 }, (_, i) => {
      const cx = -1_000 + r() * 2_000;
      const cy = -1_000 + r() * 2_000;
      const points = Array.from({ length: 40 }, (_, j) => {
        const t = (j / 39) * Math.PI * 2;
        // The first life takes a teleporter half way: a jump across the map.
        const tele = i === 0 && j >= 20 ? 2_600 : 0;
        return [5_000 + i * 900 + j * 16, Math.round(cx + Math.cos(t) * 700) + tele, Math.round(cy + Math.sin(t) * 500), 0] as [
          number,
          number,
          number,
          number,
        ];
      });
      return {
        demoId: 1,
        seq: i,
        accountId: i % 3 === 0 ? 139131191 : 1000 + i,
        fromTick: points[0][0],
        toTick: points[39][0],
        roundNum: 1 + (i % 5),
        died: r() < 0.6,
        points,
        // A long life takes a point or three; a short one takes none.
        caps: (i % 2 === 0
          ? ([
              [43, 1],
              [95, 2],
              [229, 3],
            ] as Array<[number, number]>)
          : []
        ).slice(0, 1 + (i % 3)),
      };
    });
    return delay(lives, 150);
  },

  allHistory: () => delay(allHistory),
  setAllHistory: (on: boolean) => {
    allHistory = on;
    return delay(on);
  },
  listBackups: () =>
    delay({
      dir: "C:\Users\you\AppData\Roaming\gg.highlander.rating\backups",
      items: [
        { path: "…\hl-20260920-120340.sqlite3", bytes: 183_730_176, madeAt: Math.floor(Date.now() / 1000) - 3_600 },
        { path: "…\hl-20260919-201112.sqlite3", bytes: 182_100_000, madeAt: Math.floor(Date.now() / 1000) - 90_000 },
      ],
    }),
  // Files to try the Reload path with, from localStorage "hl.mock.langFiles"
  // as [{ id, text }]; none by default.
  languageFiles: () => {
    let files: Array<{ id: string; text: string }> = [];
    try {
      files = JSON.parse(localStorage.getItem("hl.mock.langFiles") ?? "[]");
    } catch {
      // Not valid JSON: try it with none.
    }
    return delay({ dir: "C:\\Users\\you\\AppData\\Roaming\\gg.highlander.rating\\lang", files });
  },
  // A few "ETF2L names" for the browser preview: the fixture teammates'
  // names in capitals, so the swap is easy to see.
  etf2lNames: () =>
    delay(
      Object.fromEntries(
        (teammatesAll as unknown as Teammates).teammates.slice(0, 12).map((m) => [String(m.accountId), m.name.toUpperCase()]),
      ),
    ),
  saveLanguageFile: (id: string, text: string) => {
    console.info("would save", `${id}.lang`, `${text.length} characters`);
    return delay({ path: `C:\\Users\\you\\AppData\\Roaming\\gg.highlander.rating\\lang\\${id}.lang`, created: true });
  },
  revealPath: (path: string) => {
    console.info("would reveal", path);
    return delay(undefined as void);
  },
  restoreBackup: (path: string) => {
    console.info("would restore", path);
    return delay(undefined as void);
  },
  declineRestore: () => delay(undefined as void),
  backupNow: () =>
    delay({ path: "…\hl-now.sqlite3", bytes: 183_900_000, madeAt: Math.floor(Date.now() / 1000) }),
  saveBackupAs: async (suggested: string) =>
    delay({ path: "D:/backups/" + suggested, bytes: 183_900_000, madeAt: Math.floor(Date.now() / 1000) }),

  // Spies found now and then, by whoever is not a Spy on the other team.
  getDemoStats: (logId: number) => {
    const m = FIXTURES.find((f) => f.logId === logId) ?? FIXTURES[0];
    const ps = m.players as Array<{ accountId: number; name: string; team: string; mainClass: string | null }>;
    const pyro = ps.find((p) => p.mainClass === "pyro") ?? ps[0];
    const enemy = ps.find((p) => p.team !== pyro.team && p.mainClass === "soldier") ?? ps.find((p) => p.team !== pyro.team) ?? ps[1];
    const line = (i: number) => {
      const base = 15 + ((i * 37) % 70);
      const points: Array<[number, number]> = Array.from({ length: 40 }, (_, k) => [k * 45, base + ((k * 7 + i) % 9) - 4 + (i === 3 && k > 20 && k < 24 ? 55 : 0)]);
      return { accountId: ps[i].accountId, name: ps[i].name, avg: base, median: base, min: base - 4, max: i === 3 ? base + 59 : base + 4, spikes: i === 3 ? ([[900, 1080, base + 59]] as Array<[number, number, number]>) : [], points };
    };
    const reflects = [
      { demoId: 1, atS: 312, jumpTick: 20400, by: pyro.accountId, what: "rocket", outcome: "hit" as const, victims: [enemy.accountId], damage: 84, killed: false, threat: true },
      { demoId: 1, atS: 655, jumpTick: 43100, by: pyro.accountId, what: "pipe", outcome: "miss" as const, victims: [], damage: 0, killed: false, threat: false },
      { demoId: 1, atS: 902, jumpTick: 59400, by: pyro.accountId, what: "rocket", outcome: "hit" as const, victims: [enemy.accountId], damage: 120, killed: true, threat: true },
      { demoId: 1, atS: 1210, jumpTick: 79700, by: pyro.accountId, what: "sticky", outcome: "unknown" as const, victims: [], damage: 0, killed: false, threat: null },
    ];
    return delay<MatchDemoStats>({
      demos: 1,
      stv: true,
      tooOld: 0,
      pings: ps.map((_, i) => line(i)).sort((a, b) => b.avg - a.avg),
      pyros: [{ accountId: pyro.accountId, name: pyro.name, reflects: 4, hits: 2, misses: 1, sentBack: 0, unknown: 1, kills: 1, damage: 204, threats: 2, judged: 3 }],
      reflects,
    });
  },
  getSpychecks: (logId: number) => {
    const m = FIXTURES.find((f) => f.logId === logId) ?? FIXTURES[0];
    const players = m.players as Array<{ accountId: number; name: string; team: string; mainClass: string | null }>;
    const spies = players.filter((p) => p.mainClass === "spy");
    if (spies.length === 0) return delay<SpyReport | null>(null);
    const r = rng(logId + 27);
    const checks: SpyReport["checks"] = [];
    for (let i = 0; i < 24; i++) {
      const spy = spies[Math.floor(r() * spies.length)];
      const foes = players.filter((p) => p.team !== spy.team && p.mainClass !== "spy");
      const by = foes[Math.floor(Math.pow(r(), 1.8) * foes.length)];
      const atS = 60 + i * 70 + r() * 50;
      checks.push({ demoId: 1, atS, jumpTick: Math.round((atS - 5) * 66.67), attacker: by.accountId, spy: spy.accountId, damage: Math.round(10 + r() * 80), killed: r() < 0.2 });
    }
    const tally = new Map<number, { accountId: number; name: string; checks: number; found: number }>();
    for (const c of checks) {
      for (const [id, made] of [[c.attacker, true], [c.spy, false]] as const) {
        const p = players.find((x) => x.accountId === id)!;
        const row = tally.get(id) ?? { accountId: id, name: p.name, checks: 0, found: 0 };
        if (made) row.checks++;
        else row.found++;
        tally.set(id, row);
      }
    }
    const rows = [...tally.values()].sort((a, b) => b.checks - a.checks || b.found - a.found);
    return delay<SpyReport | null>({ demos: 1, players: rows, checks, fading: 31, blinking: 22, marked: 6, cooldown: 14 });
  },

  // From `hl momentum 4109131 --json` on the pl_upward STV: the real view.
  getCart: (logId: number) => {
    const m = FIXTURES.find((f) => f.logId === logId) ?? FIXTURES[0];
    if (!/^pl_/.test(String(m.map ?? ""))) return delay<CartView | null>(null);
    return delay<CartView | null>(cart4109131 as CartView);
  },

  importDemo: (path: string) =>
    delay<DemoImported>(
      {
        logId: -781931655,
        partOf: null,
        title: `${path.split(/[\\/]/).pop()?.replace(/\.dem$/, "")} (from the demo)`,
        map: "koth_proot_b5b",
        playedAt: 1790375521,
        players: 18,
        yours: true,
        rounds: 3,
        kills: 262,
        path,
      },
      900,
    ),

  // From `hl leagues --json` and `hl team 37805 --json` on a real fetch.
  getLeagues: (_season?: number) => delay(leagues as unknown as LeagueView),
  // From a copy of the database, 3 October 2026: the tiles, S33's podiums
  // and SBQRRA's honours; the banners ETF2L's news had.
  getSeasonsOverview: () => delay(seasonsOverview as unknown as SeasonTile[]),
  getSeasonBanner: (season: number) =>
    delay<string | null>(
      ({
        36: "https://etf2l.org/wp-content/uploads/2026/07/ETF2L_HL_autumn_2026-1024x576.jpg",
        35: "https://etf2l.org/wp-content/uploads/2026/03/ETF2L_HL_SPRING_2026-1024x576.jpg",
        34: "https://etf2l.org/wp-content/uploads/2025/07/ETF2L_HL_SUMMER_2025-1024x576.png",
        33: "https://etf2l.org/wp-content/uploads/2025/03/etf2l_HL_SPRING_25-1024x576.png",
        32: "https://etf2l.org/wp-content/uploads/2024/06/hls32soda-1024x576.png",
        29: "https://etf2l.org/wp-content/uploads/2023/04/etf2l_hl_spring_23-1024x576.png",
        28: "https://etf2l.org/wp-content/uploads/2023/03/ETF2L_HL_winter23-banner.png",
      } as Record<number, string>)[season] ?? null,
    ),
  getSeasonPodiums: () => delay(seasonPodiums33 as unknown as Podium[]),
  getTeamHonours: () => delay(teamHonours35600 as unknown as TeamHonours),
  // The TWS official's sides, as `get_match_sides` gave them on a backup
  // (3 October 2026); every other match is a scrim of DD14's.
  getMatchSides: (logId: number) =>
    delay<MatchSides | null>(
      logId === 4109131
        ? {
            team: { id: 37805, name: "DD14", country: "France", avatar: "https://etf2l.org/wp-content/uploads/avatars/6a18b9b147176.png" },
            opp: { id: 37921, name: "ЭТО МОЁ БОЛОТО", country: "Russia", avatar: "https://etf2l.org/wp-content/uploads/avatars/6aa4884eab429.jpg" },
            season: 36,
            seasonName: "Autumn 2026",
            scheduled: 1_787_512_500,
          }
        : { team: { id: 37805, name: "DD14", country: "France", avatar: "https://etf2l.org/wp-content/uploads/avatars/6a18b9b147176.png" }, opp: null, season: null, seasonName: null, scheduled: null },
    ),
  // SBQRRA's transfers and Flashy's teams from ETF2L, 3 October 2026.
  getTeamTransfers: () => delay(teamTransfers35600 as unknown as TeamTransfers),
  getTeamInfo: () => delay(teamInfo35600 as unknown as TeamInfo),
  // ETF2L's scheduled matches, 3 October 2026; DD14 given the first one so
  // a team page shows its next match.
  getFixtures: () =>
    delay(
      (upcomingFixtures as unknown as Fixture[]).map((f, i) => {
        // Moved to the days ahead of now, so the countdowns have something to count.
        const time = Math.floor(Date.now() / 1000) + [-1800, 3 * 3600, 26 * 3600, 50 * 3600, 4 * 86400][i % 5] + Math.floor(i / 5) * 86400;
        return i === 0 ? { ...f, time, clan1: { id: 37805, name: "DD14", avatar: null } } : { ...f, time };
      }),
    ),
  // ETF2L's Highlander posts, 16 October 2026.
  getNews: () => delay(newsPosts as unknown as NewsItem[]),
  getNewsPost: (id: number) => {
    const item = (newsPosts as unknown as NewsItem[]).find((n) => n.id === id);
    const html = (newsHtml as Record<string, string>)[String(id)];
    return delay<NewsPost | null>(item && html ? { ...item, html } : null);
  },
  // DD14's season so far, read as everyone's results.
  getRecentResults: (limit = 10) =>
    delay<ResultRow[]>(
      (team37805 as unknown as TeamView).results.slice(0, limit).map((r) => ({
        matchId: r.matchId,
        time: r.time ?? 0,
        season: r.season,
        division: r.division,
        tier: 2,
        round: r.round,
        week: null,
        clan1: { id: 37805, name: "DD14", avatar: "https://etf2l.org/wp-content/uploads/avatars/6a18b9b147176.png" },
        clan2: { id: r.opponentId, name: r.opponent, avatar: null },
        r1: r.scoreFor,
        r2: r.scoreAgainst,
        defaultWin: r.defaultWin,
      })),
    ),
  // One of DD14's officials, with DD14's roster as its lineup; a fixture's
  // id is not in the database, so the page reads it from the fixtures.
  getLeagueMatch: (matchId: number) => {
    const base = team37805 as unknown as TeamView;
    const r = base.results.find((x) => x.matchId === matchId);
    if (!r) return delay<LeagueMatch | null>(null);
    const half = (n: number | null) => (n === null ? 0 : Math.round(n / 2));
    return delay<LeagueMatch | null>({
      matchId: r.matchId,
      time: r.time ?? 0,
      season: r.season,
      division: r.division,
      tier: 1,
      round: r.round,
      week: null,
      clan1: { id: 37805, name: "DD14", avatar: base.avatar },
      clan2: { id: r.opponentId, name: r.opponent, avatar: null },
      r1: r.scoreFor,
      r2: r.scoreAgainst,
      defaultWin: r.defaultWin,
      maps: r.defaultWin ? [] : r.maps.map((map) => ({ map, clan1: half(r.scoreFor), clan2: half(r.scoreAgainst), goldenCap: false })),
      players: base.roster.map((p) => ({ accountId: p.accountId, name: p.name, teamId: 37805, class: p.class, rating: p.rating })),
      meetings: [{ matchId: 1, time: 1_780_000_000, season: 35, division: "Mid", round: "Week 4", scoreA: 4, scoreB: 2, defaultWin: false }],
      logs: [
        { logId: 4109131, map: r.maps[0] ?? null, stored: true },
        { logId: 4109132, map: r.maps[1] ?? null, stored: false },
      ],
      url: `https://etf2l.org/matches/${r.matchId}/`,
    });
  },
  searchTeams: (query: string) => {
    const q = query.trim().toLowerCase();
    const all = (leagues as unknown as LeagueView).divisions.flatMap((d) => d.teams);
    return delay<TeamHit[]>(
      q.length < 2
        ? []
        : all
            .filter((x) => x.name.toLowerCase().includes(q))
            .map((x) => ({ teamId: x.teamId, name: x.name, country: null, avatar: x.avatar, officials: x.record.played, lastSeason: 36, lastPlayed: null })),
    );
  },
  checkNewLogs: () => delay<NewLogs | null>({ count: 2, source: "logs.tf", since: Math.floor(Date.now() / 1000) - 2 * 86400 }),
  getPlayerTeams: () => delay(playerTeams139131191 as unknown as Stay[]),
  // SBQRRA's ETF2L page, 3 October 2026.
  getTeamEtf2l: () =>
    delay<TeamEtf2l>({
      description: "\u201cSe ni\u2019 mondo esistesse un po\u2019 di bene\ne ognun si honsiderasse suo fratello\nci sarebbe meno pensieri e meno pene\ne il mondo ne sarebbe assai pi\u00f9 bello\u201d\nP.P.\n\n\u2013 1st Place S30: tiad \u2013 bad \u2013 mob \u2013 steko \u2013 bull \u2013 tonno \u2013 zero \u2013 flashy \u2013 mata\n\u2013 3rd Place S34: Kosta \u2013 scrly \u2013 mob \u2013 bad \u2013 Mathis \u2013 tonno \u2013 eron \u2013 flashy \u2013 belfast\n\nSBQRRA Invicta!",
      awards: [
        { place: "1st", competition: "Highlander Autumn 2023 (Open B)" },
        { place: "1st", competition: "Highlander Winter 2024: Low (Low)" },
        { place: "1st", competition: "Highlander Winter 2024 Preseason Cup (Low A)" },
        { place: "1st", competition: "Highlander Season 33 (Spring 2025): Low Playoffs" },
        { place: "3rd", competition: "Highlander Season 34 (Summer 2025) (Mid)" },
      ],
      url: "https://etf2l.org/teams/35600/",
      fetchedAt: Math.floor(Date.now() / 1000),
    }),
  getTeam: (teamId: number) => {
    const row = (leagues as unknown as LeagueView).divisions.flatMap((d) => d.teams).find((x) => x.teamId === teamId);
    const base = team37805 as unknown as TeamView;
    return delay<TeamView | null>(teamId === base.teamId ? base : row ? { ...base, teamId, name: row.name, avatar: row.avatar, record: row.record } : null);
  },

  getCallouts: (map: string) => delay(mockCallouts(map)),
  saveCallouts: (map: string, file: CalloutFile) => {
    try {
      localStorage.setItem(`hl.mock.callouts.${mapBase(map)}`, JSON.stringify(file));
    } catch {
      // Storage can be blocked; the save only lasts the page then.
    }
    return delay(mockCallouts(map, file));
  },
  knownMaps: () => delay(["koth_ashville_final1", "koth_product_final", "koth_proot_b5b", "cp_steel_f12", "pl_swiftwater_final1", "pl_upward_f12", "pl_vigil_rc10"]),
  setRoundMap: (logId: number, rounds: number[], map: string | null) => {
    console.info("would set", logId, rounds, map);
    return delay(undefined);
  },
  resetCallouts: (map: string) => {
    try {
      localStorage.removeItem(`hl.mock.callouts.${mapBase(map)}`);
    } catch {
      // As above.
    }
    return delay(mockCallouts(map));
  },
  // The owner's Sniper on a copy of the database, 1 October 2026.
  getTendencies: () =>
    delay([
      {
        map: "product", draft: true, matches: 133, unzoned: 900, stvs: 1, aliveS: 900,
        fights: [
          { zone: "Own Left", kills: 734, deaths: 406 }, { zone: "Own China", kills: 375, deaths: 232 }, { zone: "Own Rock", kills: 394, deaths: 204 },
          { zone: "Own Hill", kills: 310, deaths: 111 }, { zone: "Own Valley", kills: 209, deaths: 205 }, { zone: "Own Grass", kills: 153, deaths: 77 },
        ],
        time: [{ zone: "Own Left", share: 0.15 }, { zone: "Point", share: 0.1 }, { zone: "Own Valley", share: 0.09 }, { zone: "Own Concrete", share: 0.09 }, { zone: "Own Rock", share: 0.06 }],
        paths: [{ from: "Own Valley", to: "Own Left", times: 11 }, { from: "Own Valley", to: "Point", times: 7 }, { from: "Own Rock", to: "Own Valley", times: 7 }],
      },
      {
        map: "ashville", draft: true, matches: 70, unzoned: 300, stvs: 0, aliveS: 0,
        fights: [{ zone: "Mid", kills: 268, deaths: 214 }, { zone: "Own Toxic", kills: 203, deaths: 127 }, { zone: "Own Battlements", kills: 163, deaths: 111 }],
        time: [], paths: [],
      },
    ] as MapTendencies[]),
  getPositions: (logId: number, map: string) => {
    const f = mockCallouts(map);
    if (f.zones.length === 0) return delay<PositionsView | null>(null);
    const m = FIXTURES.find((x) => x.logId === logId) ?? FIXTURES[0];
    const r = rng(logId + 28);
    const classes: Record<string, number> = { scout: 1, sniper: 2, soldier: 3, demoman: 4, medic: 5, heavyweapons: 6, pyro: 7, spy: 8, engineer: 9 };
    const players = (m.players as Array<{ accountId: number; name: string; team: string; mainClass: string | null }>).map((p) => {
      const aliveS = 900 + Math.round(r() * 300);
      const picks = [...f.zones].sort(() => r() - 0.5).slice(0, 5);
      let left = aliveS * 0.7;
      const zones = picks.map((z) => {
        const s = Math.round(left * (0.25 + r() * 0.35));
        left -= s;
        return { zone: z.name, seconds: s };
      });
      return { accountId: p.accountId, name: p.name, class: classes[p.mainClass ?? ""] ?? 0, team: p.team === "Red" ? 2 : 3, aliveS, zones };
    });
    return delay<PositionsView | null>({ map: f.map, zones: f.zones.length, draft: f.draft, players });
  },

  rereadMatch: async () => {
    for (const step of ["Reading the demo", "Reading the demo", "Reading aim from the demo", "Reading the server log"]) {
      rereadListeners.forEach((h) => h({ logId: 0, step }));
      await delay(null, 700);
    }
    return { demos: 2, missing: 0, aim: true, fights: true } satisfies ReRead;
  },
  onRereadStep: async (h: (s: { logId: number; step: string }) => void) => {
    rereadListeners.push(h);
    return () => {
      rereadListeners = rereadListeners.filter((x) => x !== h);
    };
  },
  linkDemo: (_logId: number, path: string) =>
    delay<DemoLinked>(
      { demoId: 99, fileName: path.split(/[\\/]/).pop() ?? path, stv: true, killsMatched: 241, logKills: 262, playersShared: 18, path },
      1200,
    ),
  getBookmarks: () => delay([...mockBookmarks], 50),
  setBookmark: (b: Bookmark, on: boolean) => {
    mockBookmarks = mockBookmarks.filter((x) => !(x.kind === b.kind && x.id === b.id));
    if (on) mockBookmarks.unshift({ ...b, added: Math.floor(Date.now() / 1000) });
    return delay([...mockBookmarks], 50);
  },
  linkDemostf: (_logId: number, link: string) =>
    delay<DemoLinked>(
      { demoId: 98, fileName: `match-from-${link.split("/").pop()}.dem`, stv: true, killsMatched: 88, logKills: 262, playersShared: 18, path: link },
      1500,
    ),

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

  // The map images the app ships (overviews/ in the repo, more.tf's renders).
  getMapOverview: async (map: string): Promise<Overview | null> => {
    const base = Object.keys(MOCK_PLACEMENTS).find((b) => map.includes(b));
    if (!base) return null;
    const url = OVERVIEW_URLS[`../../../overviews/${base}.png`];
    if (!url) return null;
    return { mapBase: base, ...mockPlacement(base)!, aspect: 1, image: url };
  },

  mapsOverview: () =>
    delay<MapsOverview>({
      unknownMatches: 3,
      maps: [
        { base: "product", name: "koth_product_final", matches: 182, image: "built in", placement: "built in", callouts: "built in", zones: 27, unplaced: 0, draft: true, calloutsUndo: false, calloutsAuthor: null },
        { base: "upward", name: "pl_upward_f12", matches: 120, image: "built in", placement: "built in", callouts: "built in", zones: 0, unplaced: 14, draft: true, calloutsUndo: false, calloutsAuthor: null },
        { base: "vigil", name: "pl_vigil_rc10", matches: 96, image: "yours", placement: "yours", callouts: "yours", zones: 12, unplaced: 0, draft: false, calloutsUndo: true, calloutsAuthor: "Flashy" },
        { base: "ashville", name: "koth_ashville_final1", matches: 61, image: "built in", placement: "built in", callouts: "none", zones: 0, unplaced: 0, draft: false, calloutsUndo: false, calloutsAuthor: null },
        { base: "lakeside", name: "koth_lakeside_final", matches: 9, image: "none", placement: "none", callouts: "none", zones: 0, unplaced: 0, draft: false, calloutsUndo: false, calloutsAuthor: null },
      ],
    }),
  overviewImage: async (map: string): Promise<OverviewImage | null> => {
    const base = Object.keys(MOCK_PLACEMENTS).find((b) => map.includes(b));
    const url = base && OVERVIEW_URLS[`../../../overviews/${base}.png`];
    return url ? { image: url, aspect: 1, placement: mockPlacement(base) } : null;
  },
  importOverview: async (map: string) => {
    console.info("would pick an image for", map);
    return delay(true);
  },
  saveOverviewPlacement: (map: string, placement: Placement) => {
    console.info("would save placement", map, placement);
    return delay(undefined);
  },
  exportCallouts: async (map: string) => {
    console.info("would export the callouts of", map);
    return delay(`C:\\Users\\you\\Downloads\\${map}.callouts.json`);
  },
  pickCalloutFile: async () => delay("C:\\Users\\you\\Downloads\\product.callouts.json"),
  inspectCallouts: (map: string | null, path: string) =>
    delay<PresetCheck>({
      map: "product",
      target: map ?? "product",
      zones: 27,
      names: 2,
      draft: false,
      author: "boSe",
      source: path,
      currentOrigin: "built in",
      currentZones: 27,
    }),
  importCallouts: (map: string, path: string, anyMap: boolean) => {
    console.info("would import", path, "onto", map, anyMap ? "(another map's file)" : "");
    return delay(mockCallouts(map));
  },
  undoCallouts: (map: string) => delay(mockCallouts(map)),
  removeOverview: (map: string) => {
    console.info("would remove the image of", map);
    return delay(undefined);
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
  demoStats: () =>
    delay({ demos: 101, linked: 25, stv: 0, markers: 883, matchesWithDemo: 23, timelines: 24, timelineBytes: 71_400_000, timelinesFileGone: 3 }),

  // Simulates a download, one at a time, so the queue UI can be exercised
  // in a browser: ask for three and the second and third wait their turn.
  fetchStv: async (logId: number) => {
    if (stvQueue.includes(logId)) return;
    stvQueue.push(logId);
    announceStv();
    if (stvQueue.length === 1) runStv();
  },

  cancelStv: async (logId: number) => {
    if (stvQueue[0] === logId) return false;
    const at = stvQueue.indexOf(logId);
    if (at === -1) return false;
    stvQueue.splice(at, 1);
    announceStv();
    return true;
  },

  // The browser build: `mockNewDemo()` in the console plays the part of TF2.
  onNewDemo: async (h) => {
    (window as unknown as { mockNewDemo: () => void }).mockNewDemo = () => h({ fileName: "flashwav2026-09-28_22-18-30.dem", bytes: 48_000_000 });
    return () => {};
  },

  onStv: async (h) => {
    stvHandlers = h;
    return () => {
      if (stvHandlers === h) stvHandlers = null;
    };
  },

  searchPlayers: async (query: string) => {
    const all = [
      { accountId: 279623496, name: "boSe.", games: 158, lastSeen: 1757621600, topClass: "scout" },
      { accountId: 137154322, name: "W.", games: 24, lastSeen: 1748370090, topClass: "heavy" },
      { accountId: 111222333, name: "Taiga", games: 61, lastSeen: 1756000000, topClass: "medic" },
    ];
    const q = query.toLowerCase();
    return delay(all.filter((p) => p.name.toLowerCase().includes(q) || String(p.accountId) === query));
  },

  getPlayer: async (accountId: number, className: string | null) => {
    const p = (profileSniper as unknown as { profile: Profile }).profile;
    return delay({
      summary: {
        accountId,
        steamid64: "76561198239889224",
        name: "boSe.",
        alsoKnownAs: ["boSe", "Stephane Merveille", "boSih"],
        games: 158,
        firstSeen: 1690000000,
        lastSeen: 1757621600,
        withYou: 151,
        againstYou: 7,
        youBeatThem: 3,
        theyBeatYou: 4,
        classes: [
          { class: "scout", games: 88, avg: 1.14 },
          { class: "heavy", games: 57, avg: 1.22 },
          { class: "engineer", games: 8, avg: 0.93 },
        ],
      },
      profile: { ...p, games: 88, careerAvg: 1.14 },
      class: className ?? "scout",
    });
  },

  failedLogs: () =>
    delay([
      {
        logId: 3998211,
        attempts: 3,
        lastAttemptAt: "2026-09-25 22:14:03",
        error: "logs.tf answered 404",
        title: "Highlander: bandwagon vs FUNCTION",
        map: "pl_vigil_rc10",
        playedAt: 1758300000,
      },
    ]),
  retryFailed: async () => 1,
  importLog: async (text: string) => {
    // The same rule as the backend's parse_log_id: the last path segment,
    // up to its first non-digit, so a logs.tf link with a #player anchor works.
    const tail = text.trim().replace(/\/+$/, "").split("/").pop() ?? "";
    const logId = Number(/^\d+/.exec(tail)?.[0] ?? 0);
    if (!logId) throw new Error("That is not a log id or a logs.tf link.");
    // 4132222 is the part of a combined log a tester could not find (Q71).
    const partOf = logId === 4132222 ? 4132265 : null;
    return { logId, title: "Highlander", map: "koth_product_final", playedAt: 1758300000, players: 18, yours: true, partOf };
  },

  indexStats: () => delay(fakeStats(pending)),
  syncBusy: () => delay(busy),
  getRatingGuide: () => delay(ratingGuide as unknown as RatingGuide),
  // The browser build: the numbers from the first discovery run.
  getLeagueSample: () => delay(leagueSample),
  // The browser build: a job partway through, alternating working and waiting.
  getLeagueActivity: () => {
    const t = Math.floor(Date.now() / 1000);
    const working = t % 6 < 2;
    return delay({
      state: working ? "working" : "waiting",
      doing: working ? `Downloading log ${4127300 - (t % 97)} from logs.tf` : null,
      nextAt: working ? null : t + (6 - (t % 6)),
      lastHour: 583,
      logstfRestLeft: null,
      recent: [
        { at: t - 5, text: "Log 4127318 from logs.tf", ok: true },
        { at: t - 11, text: "Who played ETF2L match 93005", ok: true },
        { at: t - 17, text: "Server log of 4127319", ok: true },
        { at: t - 60, text: "logs.tf: HTTP 403 Forbidden", ok: false },
      ],
    } as LeagueActivity);
  },
  // The browser build: three players, the owner's real profile shape.
  searchCatalogue: (query: string) =>
    delay(MOCK_HITS.filter((h) => h.name.toLowerCase().includes(query.trim().toLowerCase()))),
  getPlayerProfile: (accountId: number) => delay(mockProfile(accountId)),
  // The numbers the owner's real profile gave on a copy of the database.
  // The browser build: every player of the match gets a division by the
  // account number, a few from a nearby season.
  getMatchDivisions: async (logId: number) => {
    const d = (await mockApi.getMatch(logId)) as MatchDetail | null;
    const names = ["Premiership", "High", "Mid", "Low", "Open"];
    const players: MatchDivisions["players"] = {};
    for (const p of d?.players ?? []) {
      const tier = (p.accountId % 7) % 5;
      players[p.accountId] = { tier, division: names[tier], season: 36, exact: p.accountId % 3 !== 0 };
    }
    return { players, tierNames: { 0: "Premiership", 1: "High", 2: "Mid", 3: "Low", 4: "Open" } };
  },
  // The owner's real trends.tf page, ETF2L officials only, 1 October 2026.
  getTrendsCareer: () =>
    delay({
      career: {
        wins: 68, losses: 40, ties: 2, winrate: 62.73, timeS: 104303,
        classes: [{ class: "sniper", wins: 68, losses: 40, ties: 2, winrate: 62.73, dpm: 342, accuracy: 37, timeS: 104239 }],
        aliases: [["flashy", 1045], ["Flashy", 207], ["SBQR flashy", 16]],
        teams: [{ league: "ETF2L", team: "DD14", competitions: "Highlander Season 36 (Autumn 2026): High", division: "High" }],
      },
      fetchedAt: Math.floor(Date.now() / 1000) - 3600,
      error: null,
      url: "https://trends.tf/player/76561198099396919/?format=highlander&league=etf2l",
    } as CareerView),
  getPlayerStats: () =>
    delay({
      classes: [
        { class: "sniper", games: 664, career: 1.02, recent: 1.0, recentGames: 41, best: 1.65, groups: [["fragging", 46], ["survival", 51], ["teamplay", 59], ["objective", 50]], groupsRecent: true },
        { class: "engineer", games: 29, career: 0.98, recent: 0.94, recentGames: 9, best: 1.39, groups: [["fragging", 67], ["survival", 39], ["teamplay", 45], ["objective", 30]], groupsRecent: true },
        { class: "scout", games: 15, career: 1.05, recent: null, recentGames: 2, best: 1.47, groups: [["fragging", 61], ["survival", 46], ["teamplay", 55], ["objective", 44]], groupsRecent: false },
      ],
      ranks: [
        { season: 34, seasonName: "Summer 2025", division: "Mid", tier: 2, class: "sniper", rank: 6, of: 7, avg: 0.97, games: 66 },
        { season: 33, seasonName: "Spring 2025", division: "Low", tier: 3, class: "sniper", rank: 3, of: 7, avg: 1.1, games: 102 },
      ],
    } as PlayerStats),
  getRankings: (season: number | null, tier: number | null, cls: string) =>
    delay({
      season: season ?? 36,
      seasonName: "Autumn 2026",
      division: ["Premiership", "High", "Mid", "Low", "Open"][tier ?? 0],
      tier: tier ?? 0,
      class: cls,
      rows: MOCK_HITS.map((h, i) => ({ rank: i + 1, accountId: h.accountId, name: h.name, team: { id: i, name: ["DD14", "Froyotech", "SBQRRA"][i], avatar: null }, games: 14 - i * 3, avg: 1.24 - i * 0.11 })),
      seasons: [[36, "Autumn 2026"], [35, "Spring 2026"], [34, "Summer 2025"]],
      divisions: [[0, "Premiership"], [1, "High"], [2, "Mid"], [3, "Low"], [4, "Open"]],
    } as Rankings),
  setLeagueSample: async (on: boolean) => {
    leagueSample = { ...leagueSample, enabled: on };
  },
  // The browser build: a new log turns up on the third look.
  newestLog: () => delay({ logId: 4200000, source: "logs.tf", known: ++newestLooks < 3 }),

  getDemoFolders: () => delay([...demoFolders]),
  setDemoFolders: (folders: string[]) => {
    demoFolders = folders;
    return delay<DemoIndexSummary>({ scanned: 120 + 40 * folders.length, unreadable: 0, removed: 0, logsPlaced: 30, links: 30, demosLinked: 30, matchesWithDemo: 30, markers: 12 });
  },
  syncCancel: () => {
    if (!busy) return delay(false);
    for (const id of syncTimers) window.clearTimeout(id);
    syncTimers = [];
    busy = false;
    window.setTimeout(() => handlers?.onError({ kind: "cancelled", message: "Sync cancelled." }), 100);
    return delay(true);
  },

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
