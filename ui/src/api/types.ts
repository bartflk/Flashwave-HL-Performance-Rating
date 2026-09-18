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
  /** A demo on this machine is linked to the match. */
  hasDemo: boolean;
  /** Null when the owner does not appear in the log. */
  me: MyLine | null;
  /** Official, scrim or pug. Null outside Highlander, or when you did not play. */
  context: MatchContext | null;
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
  | { kind: "reprocessing"; done: number; total: number }
  | { kind: "rating"; done: number; total: number }
  | { kind: "etf2l"; done: number; total: number }
  | { kind: "etf2lFailed"; error: string };

/** Sent once on `sync://done`. */
export interface SyncDone {
  kind: "sync" | "reprocess";
  fetched: number;
  failed: number;
  stats: IndexStats;
}

export interface MatchQuery {
  format: string | null;
  kind: ContextKind | null;
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

/** One component of a rating. `percentile` is already flipped for
 *  lower-is-better components, so higher is always better. */
export interface Part {
  component: string;
  label: string;
  unit: string;
  raw: number;
  percentile: number;
  /** Share of the rating, 0-1. */
  weight: number;
}

/** 0-100: the weighted average of the component percentiles, measured against
 *  every other player's performances on the class in your stored matches. */
export interface Rating {
  class: string;
  score: number;
  minutes: number;
  parts: Part[];
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
  rating: Rating | null;
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
  rating: Rating | null;
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
  /** A killstreak's length, for `killstreak` events. */
  value: string | null;
  /** This moment in a linked demo, 5 s early to show the lead-up. */
  jump: Jump | null;
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
  /** The round's start in a linked demo. */
  jump: Jump | null;
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
  /** False until the first rating pass has built the baselines. */
  rated: boolean;
  format: string | null;
  league: string | null;
  etf2lMatchId: number | null;
  demosTfId: number | null;
  weightsWarning: string | null;
  demos: DemoView[];
  context: MatchContext | null;
}

// ---- M4: demos --------------------------------------------------------------------

/** Where to jump: open the demo with `playdemo`, then `demo_gototick`. */
export interface Jump {
  demoId: number;
  tick: number;
}

export interface DemoView {
  demoId: number;
  fileName: string;
  /** The argument to `playdemo`, relative to tf. */
  playdemoArg: string;
  kind: "pov" | "stv";
  recorder: string | null;
  durationS: number;
  recordedAt: number | null;
  sizeBytes: number;
  method: string;
  /** Share of this match's rounds inside the demo, 0-1. */
  logShare: number;
  markers: number;
  /** Ticks are estimated (STV), not derived from exact file times. */
  approximate: boolean;
}

export interface DemoStats {
  demos: number;
  linked: number;
  stv: number;
  markers: number;
  matchesWithDemo: number;
}

export interface DemoIndexSummary {
  scanned: number;
  unreadable: number;
  removed: number;
  logsPlaced: number;
  links: number;
  demosLinked: number;
  matchesWithDemo: number;
  markers: number;
}

export interface StvProgress {
  logId: number;
  bytes: number;
  total: number | null;
}

export interface StvFetched {
  demoId: number;
  fileName: string;
  bytes: number;
  logShare: number;
}

// ---- M3: profile ------------------------------------------------------------------

export interface TrendPoint {
  logId: number;
  playedAt: number | null;
  map: string | null;
  score: number;
  /** Rolling average ending at this game; null until the window fills. */
  rolling: number | null;
  result: "W" | "L" | "T" | null;
  kind: ContextKind | null;
}

export interface ComponentSummary {
  component: string;
  label: string;
  unit: string;
  weight: number;
  formPct: number;
  careerPct: number;
  formRaw: number;
}

export interface GameRef {
  logId: number;
  playedAt: number | null;
  map: string | null;
  title: string | null;
  league: string | null;
  kind: ContextKind | null;
  result: "W" | "L" | "T" | null;
  score: number;
}

export interface Extra {
  label: string;
  value: string;
  detail: string | null;
  hint: string | null;
}

export interface Profile {
  class: string;
  games: number;
  careerAvg: number;
  formAvg: number;
  prevFormAvg: number | null;
  winRate: number | null;
  /** Oldest first. */
  trend: TrendPoint[];
  components: ComponentSummary[];
  best: GameRef[];
  worst: GameRef[];
  recent: GameRef[];
  formWindow: number;
  rollingWindow: number;
  extras: Extra[];
  /** Every game, split by kind, whatever the profile is filtered to. */
  contexts: ContextSplit[];
  /** The kind the rest of the profile is filtered to. */
  filter: ContextKind | null;
}

export interface ContextSplit {
  kind: ContextKind;
  games: number;
  avg: number;
  winRate: number | null;
}

export interface ProfileResponse {
  /** [class, rated games], most played first. */
  classes: Array<[string, number]>;
  profile: Profile | null;
}

// ---- M5: context and teammates ----------------------------------------------------

export type ContextKind = "official" | "scrim" | "pug";

export interface OfficialInfo {
  competition: string | null;
  category: string | null;
  division: string | null;
  /** 1 is the top tier. */
  tier: number | null;
  week: number | null;
  round: string | null;
  /** ETF2L's score from your side, when your side is known. */
  score: [number, number] | null;
  defaultWin: boolean;
}

export interface MatchContext {
  kind: ContextKind;
  etf2lMatchId: number | null;
  /** How an official was recognised: tagged by trends.tf, or found by roster. */
  linkMethod: "trends" | "roster" | null;
  teamName: string | null;
  oppName: string | null;
  /** Teammates who played with you regularly around then. */
  regulars: number;
  official: OfficialInfo | null;
}

export interface ContextCounts {
  officials: number;
  scrims: number;
  pugs: number;
  rosterOfficials: number;
  etf2lMatches: number;
  etf2lPlayer: number | null;
  lastFetch: number | null;
}

export interface Teammate {
  accountId: number;
  steamid64: string;
  name: string;
  games: number;
  officials: number;
  wins: number;
  losses: number;
  firstPlayed: number;
  lastPlayed: number;
  mainClass: string | null;
  current: boolean;
  teams: string[];
  /** Your average rating in games with them; null with too few rated games. */
  myAvgWith: number | null;
  /** That, minus your average in the other games. */
  myAvgDelta: number | null;
}

export interface CoreMate {
  accountId: number;
  name: string;
  mainClass: string | null;
  games: number;
}

export interface TeamEra {
  teamId: number;
  name: string;
  firstPlayed: number;
  lastPlayed: number;
  games: number;
  officials: number;
  wins: number;
  losses: number;
  myAvg: number | null;
  core: CoreMate[];
}

export interface Teammates {
  games: number;
  teams: TeamEra[];
  teammates: Teammate[];
  minGames: number;
}
