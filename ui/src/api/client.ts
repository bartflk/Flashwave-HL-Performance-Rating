import { invoke } from "@tauri-apps/api/core";
import { mockApi } from "./mock";
import type { AppConfig, AppStatus, TfPathInfo } from "./types";

/** True inside the Tauri window, false in a plain browser tab. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const realApi = {
  appStatus: () => invoke<AppStatus>("app_status"),
  getConfig: () => invoke<AppConfig>("get_config"),
  setSteamId: (input: string) => invoke<AppConfig>("set_steamid", { input }),
  detectTfPath: () => invoke<TfPathInfo | null>("detect_tf_path"),
  inspectTfPath: (path: string) => invoke<TfPathInfo>("inspect_tf_path", { path }),
  setTfPath: (path: string) => invoke<TfPathInfo>("set_tf_path", { path }),
};

export const api = inTauri ? realApi : mockApi;
