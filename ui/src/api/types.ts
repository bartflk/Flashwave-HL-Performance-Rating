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

// ---- M2: match page -----------------------------------------------------------

export type Team = "Red" | "Blue";

export interface LogFlags {
  realDamage: boolean;
  accuracy: boolean;
  hs: boolean;
  hsHit: boolean;
  bs: boolean;
  cp: boolean;
  dt: boolean;
  airshots: boolean;
  hr: boolean;
}

/** A class value with its working shown. Every term is per 10 minutes. */
export interface Value {
  score: number;
  minutes: number;
  impactKills: number;
  impactAssists: number;
  deathCost: number;
  medicTerm: number;
  /** Kills could not be split by victim class, so they were weighted as average. */
  approximate: boolean;
}

export interface Side {
  accountId: number;
  name: string;
  subs: string[];
  timeS: number;
  kills: number;
  deaths: number;
  assists: number;
  dmg: number;
  value: Value;
}

export interface Matchup {
  class: string;
  left: Side | null;
  right: Side | null;
  /** [left's kills on right, right's kills on left], when attributable. */
  headToHead: [number, number] | null;
  diff: number | null;
  winner: "left" | "right" | "even" | null;
  decisive: boolean;
  involvesMe: boolean;
}

export interface PlayerRow {
  accountId: number;
  steamid64: string;
  name: string;
  team: Team;
  mainClass: string | null;
  classes: Array<[string, number]>;
  timeS: number;
  kills: number;
  deaths: number;
  assists: number;
  dmg: number;
  dpm: number;
  dt: number;
  hr: number;
  heal: number;
  ubers: number;
  drops: number;
  headshots: number;
  headshotsHit: number;
  backstabs: number;
  airshots: number;
  cpc: number;
  value: Value | null;
  isMe: boolean;
}

export interface EventRow {
  atS: number;
  kind: "pointcap" | "charge" | "drop" | "medic_death" | "round_win" | string;
  team: Team | null;
  player: string | null;
  killer: string | null;
  killerIsMe: boolean;
  medigun: string | null;
  point: number | null;
}

export interface RoundRow {
  roundNum: number;
  startOffsetS: number | null;
  lengthS: number | null;
  winner: Team | null;
  firstcap: Team | null;
  redKills: number | null;
  blueKills: number | null;
  redDmg: number | null;
  blueDmg: number | null;
  redUbers: number | null;
  blueUbers: number | null;
  events: EventRow[];
  /** A stopwatch half: each team wore the other's colour. Teams in this row
   *  are still the stable teams; this only says which colour they wore. */
  coloursSwapped: boolean;
}

export interface MatchDetail {
  logId: number;
  title: string | null;
  map: string | null;
  playedAt: number | null;
  durationS: number;
  redScore: number;
  blueScore: number;
  flags: LogFlags;
  myTeam: Team | null;
  result: "W" | "L" | "T" | null;
  leftTeam: Team;
  matchups: Matchup[];
  players: PlayerRow[];
  rounds: RoundRow[];
  modelVersion: string;
  format: string | null;
  league: string | null;
  etf2lMatchId: number | null;
  demosTfId: number | null;
  weightsWarning: string | null;
}
