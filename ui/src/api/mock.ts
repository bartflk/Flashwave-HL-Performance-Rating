// Browser-mode stand-in for the Rust backend.
//
// Active only when the page is open outside Tauri (`npm run dev` in a plain
// browser), so UI work does not require a Rust rebuild. Inside the app window
// this file is never used.

import type { AppConfig, AppStatus, TfPathInfo } from "./types";

const state: AppConfig = { steamid: null, tfPath: null };

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
    // The real backend canonicalises to SteamID64; approximate that by keeping
    // a SteamID64 as typed and standing in for any other format.
    const trimmed = input.trim();
    state.steamid = /^\d{17}$/.test(trimmed) ? trimmed : "76561198099396919";
    return delay({ ...state });
  },

  detectTfPath: (): Promise<TfPathInfo | null> =>
    delay(fakeTfPath("D:\\SteamLibrary\\steamapps\\common\\Team Fortress 2\\tf", true)),

  inspectTfPath: (path: string): Promise<TfPathInfo> =>
    delay(fakeTfPath(path, path.toLowerCase().includes("tf"))),

  setTfPath: (path: string): Promise<TfPathInfo> => {
    const info = fakeTfPath(path, true);
    state.tfPath = info.path;
    return delay(info);
  },
};
