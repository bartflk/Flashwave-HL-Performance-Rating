// Browser-mode stand-in for the Rust backend.
//
// Active only when the page is open outside Tauri (`npm run ui` in a plain
// browser), so UI work does not require a Rust rebuild. Inside the app window
// this file is never used.

import type { Api, SyncHandlers } from "./client";
import type {
  AppConfig,
  AppStatus,
  IndexStats,
  MatchPage,
  MatchQuery,
  MatchSummary,
  TfPathInfo,
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

const FAKE_MATCHES: MatchSummary[] = (() => {
  const r = rng(42);
  const pick = <T,>(xs: T[]) => xs[Math.floor(r() * xs.length)];
  const out: MatchSummary[] = [];
  let t = 1_789_675_330;
  for (let i = 0; i < 180; i++) {
    t -= Math.floor(3_600 * (4 + r() * 60));
    const official = r() < 0.07;
    const cls = pick(CLASSES);
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
  return out;
})();

const fakeStats = (pending: number): IndexStats => ({
  indexed: 1494,
  superseded: 480,
  highlander: 666,
  sixes: 117,
  other: 17,
  unclassified: 214,
  officials: 43,
  fetched: 759 - pending,
  normalized: 759 - pending,
  pending,
  failed: 0,
});

let handlers: SyncHandlers | null = null;
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
      (m) => (q.format === null || m.format === q.format) && (!q.officialsOnly || m.league !== null),
    );
    return delay({ total: filtered.length, items: filtered.slice(q.offset, q.offset + q.limit) });
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
