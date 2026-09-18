import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { mockApi } from "./mock";
import type {
  AppConfig,
  AppStatus,
  CmdError,
  IndexStats,
  MatchDetail,
  MatchPage,
  MatchQuery,
  Progress,
  SyncDone,
  TfPathInfo,
} from "./types";

/** True inside the Tauri window, false in a plain browser tab. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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
  /** Opens in the system browser, never inside the app window. */
  openExternal: (url: string) => openUrl(url),

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

export const api: Api = inTauri ? realApi : mockApi;
