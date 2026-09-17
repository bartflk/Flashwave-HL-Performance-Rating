// Browser-mode stand-in for the Rust backend.
//
// Active only when the page is open outside Tauri (`npm run dev` in a plain
// browser), so UI work does not require a Rust rebuild. Inside the app window
// this file is never used.

import type { AppConfig, AppStatus, TfPathInfo } from "./types";

const state: AppConfig = { steamid: null, tfPath: null };

const delay = <T,>(value: T, ms = 120): Promise<T> =>
  new Promise((resolve) => setTimeout(() => resolve(value), ms));

function fakeTfPath(path: string, valid: boolean): TfPathInfo {
  return {
    path,
    valid,
    demosDir: valid ? `${path}\\demos` : null,
    cfgDir: valid ? `${path}\\cfg` : null,
    demoCount: valid ? 14 : 0,
    notes: valid
      ? [`Found 14 demo file(s) in \`${path}\\demos\`.`]
      : ["No TF2 marker files found here (expected gameinfo.txt, tf2_misc_dir.vpk or cfg/)."],
  };
}

export const mockApi = {
  appStatus: (): Promise<AppStatus> =>
    delay({
      version: "0.1.0-mock",
      dbPath: "C:\\Users\\you\\AppData\\Roaming\\gg.highlander.rating\\hl.sqlite3",
      ready: state.steamid !== null && state.tfPath !== null,
      config: { ...state },
    }),

  getConfig: (): Promise<AppConfig> => delay({ ...state }),

  setSteamId: (input: string): Promise<AppConfig> => {
    if (!/^\d{17}$|^\[?U:1:\d+\]?$|^STEAM_[0-5]:[01]:\d+$|profiles\/\d+/i.test(input.trim())) {
      return Promise.reject({ kind: "invalid_steamid", message: `unrecognised format \`${input}\`` });
    }
    state.steamid = "76561198000000000";
    return delay({ ...state });
  },

  detectTfPath: (): Promise<TfPathInfo | null> =>
    delay(fakeTfPath("C:\\Program Files (x86)\\Steam\\steamapps\\common\\Team Fortress 2\\tf", true)),

  inspectTfPath: (path: string): Promise<TfPathInfo> =>
    delay(fakeTfPath(path, path.toLowerCase().includes("tf"))),

  setTfPath: (path: string): Promise<TfPathInfo> => {
    const info = fakeTfPath(path, true);
    state.tfPath = info.path;
    return delay(info);
  },
};
