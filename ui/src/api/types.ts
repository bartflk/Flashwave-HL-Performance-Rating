// Mirrors the serde shapes in `src-tauri` and `hl-core`.
//
// Hand-written for now. Once the type surface grows past a handful of structs
// (M1, when match data lands) generate these from Rust with ts-rs or specta
// instead of maintaining two copies.

/** SteamID64, as a string: it exceeds JavaScript's safe integer range. */
export type SteamId64 = string;

export interface AppConfig {
  steamid: SteamId64 | null;
  tfPath: string | null;
}

export interface TfPathInfo {
  path: string;
  valid: boolean;
  demosDir: string | null;
  cfgDir: string | null;
  demoCount: number;
  notes: string[];
}

export interface AppStatus {
  version: string;
  dbPath: string;
  ready: boolean;
  config: AppConfig;
}

/** What every failed command rejects with. */
export interface CmdError {
  kind: string;
  message: string;
}

export function isCmdError(e: unknown): e is CmdError {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}

export function errorMessage(e: unknown): string {
  if (isCmdError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}
