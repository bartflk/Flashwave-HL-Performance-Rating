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

export interface DemoDir {
  path: string;
  demoCount: number;
}

export interface TfPathInfo {
  path: string;
  valid: boolean;
  /** Both `tf` and `tf/demos` — real installs accumulate demos in each. */
  demoDirs: DemoDir[];
  cfgDir: string | null;
  /** Total across every entry in `demoDirs`. */
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

// ---- M1: matches and sync ---------------------------------------------------

export interface MyLine {
  team: "Red" | "Blue";
  mainClass: string | null;
  kills: number;
  deaths: number;
  assists: number;
  dmg: number;
  timeS: number;
  result: "W" | "L" | "T";
}

export interface MatchSummary {
  logId: number;
  playedAt: number | null;
  map: string | null;
  title: string | null;
  durationS: number | null;
  format: string | null;
  league: string | null;
  etf2lMatchId: number | null;
  demosTfId: number | null;
  redScore: number | null;
  blueScore: number | null;
  /** Null when the owner does not appear in the log. */
  me: MyLine | null;
}

export interface MatchPage {
  total: number;
  items: MatchSummary[];
}

export interface IndexStats {
  indexed: number;
  superseded: number;
  highlander: number;
  sixes: number;
  other: number;
  unclassified: number;
  officials: number;
  fetched: number;
  normalized: number;
  pending: number;
  failed: number;
}

/** Streamed on `sync://progress`. Mirrors `hl_ingest::Progress`. */
export type Progress =
  | { kind: "indexing"; source: string; rows: number }
  | { kind: "indexed"; trendsRows: number; logstfRows: number; superseded: number }
  | { kind: "fetching"; done: number; total: number; logId: number }
  | { kind: "fetchFailed"; logId: number; error: string }
  | { kind: "reprocessing"; done: number; total: number };

/** Sent once on `sync://done`. */
export interface SyncDone {
  kind: "sync" | "reprocess";
  fetched: number;
  failed: number;
  stats: IndexStats;
}

export interface MatchQuery {
  format: string | null;
  officialsOnly: boolean;
  limit: number;
  offset: number;
}
