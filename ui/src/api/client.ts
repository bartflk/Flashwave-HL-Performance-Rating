import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type {
  Analysis,
  AppConfig,
  AppStatus,
  CmdError,
  ContextCounts,
  ContextKind,
  DemoIndexSummary,
  DemoStats,
  IndexStats,
  MapView,
  MatchDetail,
  Overview,
  MatchPage,
  MatchQuery,
  ProfileResponse,
  Progress,
  RawlogStats,
  StvFetched,
  StvProgress,
  SyncDone,
  Teammates,
  TfPathInfo,
  Owner,
  Season,
  SeasonsView,
  AimResponse,
} from "./types";

/** True inside the Tauri window, false in a plain browser tab. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface StvHandlers {
  onProgress: (p: StvProgress) => void;
  onDone: (d: StvFetched) => void;
  onError: (e: CmdError & { logId: number }) => void;
}

export interface SyncHandlers {
  onProgress: (p: Progress) => void;
  onDone: (d: SyncDone) => void;
  onError: (e: CmdError) => void;
}

const realApi = {
  appStatus: () => invoke<AppStatus>("app_status"),
  getConfig: () => invoke<AppConfig>("get_config"),
  setSteamId: (input: string) => invoke<AppConfig>("set_steamid", { input }),
  detectTfPath: () => invoke<TfPathInfo | null>("detect_tf_path"),
  inspectTfPath: (path: string) => invoke<TfPathInfo>("inspect_tf_path", { path }),
  setTfPath: (path: string) => invoke<TfPathInfo>("set_tf_path", { path }),

  listMatches: (q: MatchQuery) => invoke<MatchPage>("list_matches", { ...q }),
  indexStats: () => invoke<IndexStats>("index_stats"),
  syncBusy: () => invoke<boolean>("sync_busy"),
  syncStart: (full: boolean) => invoke<void>("sync_start", { full }),
  reprocessStart: () => invoke<void>("reprocess_start"),
  getMatch: (logId: number) => invoke<MatchDetail | null>("get_match", { logId }),
  getProfile: (cls: string | null, kind: ContextKind | null = null, from: number | null = null, to: number | null = null) =>
    invoke<ProfileResponse>("get_profile", { class: cls, kind, from, to }),
  /** Your name and picture; null before a SteamID is set. */
  getOwner: () => invoke<Owner | null>("get_owner"),
  /** Seasons from your officials, newest first. */
  listSeasons: () => invoke<Season[]>("list_seasons"),
  getSeasons: (cls: string) => invoke<SeasonsView>("get_seasons", { class: cls }),
  /** `all` includes pugs; otherwise officials and scrims only. */
  getTeammates: (all: boolean) => invoke<Teammates>("get_teammates", { all }),
  contextCounts: () => invoke<ContextCounts>("context_counts"),
  rawlogStats: () => invoke<RawlogStats>("rawlog_stats"),
  /** Null when the match's raw log is not stored. */
  getMatchAnalysis: (logId: number) => invoke<Analysis | null>("get_match_analysis", { logId }),
  /** What the demo says about your aim in one match (PLAN §14). */
  getAim: (logId: number) => invoke<AimResponse>("get_aim", { logId }),
  /** Null when too few kills are stored on the map to draw it. */
  getMapView: (map: string) => invoke<MapView | null>("get_map_view", { map }),
  /** Null when no image for the map is saved in the app's overviews folder. */
  getMapOverview: (map: string) => invoke<Overview | null>("get_map_overview", { map }),
  /** Opens in the system browser, never inside the app window. */
  openExternal: (url: string) => openUrl(url),
  copyText: (text: string) => writeText(text),

  scanDemos: () => invoke<DemoIndexSummary>("scan_demos"),
  demoStats: () => invoke<DemoStats>("demo_stats"),
  fetchStv: (logId: number) => invoke<void>("fetch_stv", { logId }),

  /** STV download events. Returns a function that unsubscribes all three. */
  onStv: async (h: StvHandlers): Promise<UnlistenFn> => {
    const offs = await Promise.all([
      listen<StvProgress>("stv://progress", (e) => h.onProgress(e.payload)),
      listen<StvFetched>("stv://done", (e) => h.onDone(e.payload)),
      listen<CmdError & { logId: number }>("stv://error", (e) => h.onError(e.payload)),
    ]);
    return () => offs.forEach((off) => off());
  },

  /** Subscribe to sync events. Returns a function that unsubscribes all three. */
  onSync: async (h: SyncHandlers): Promise<UnlistenFn> => {
    const offs = await Promise.all([
      listen<Progress>("sync://progress", (e) => h.onProgress(e.payload)),
      listen<SyncDone>("sync://done", (e) => h.onDone(e.payload)),
      listen<CmdError>("sync://error", (e) => h.onError(e.payload)),
    ]);
    return () => offs.forEach((off) => off());
  },
};

export type Api = typeof realApi;

// The mock, and the real-match fixtures it carries, load only in a plain
// browser during development. `import.meta.env.DEV` is false in a release
// build, so the branch and the fixtures are dropped from the bundle.
export const api: Api = inTauri || !import.meta.env.DEV ? realApi : (await import("./mock")).mockApi;
