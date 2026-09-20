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

/** The owner's name and picture, from ETF2L or Steam's public profile. */
export interface Owner {
  steamid64: string;
  name: string | null;
  /** A data: URL. */
  avatar: string | null;
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
  /** The maps played, in order; more than one for a combined log. */
  maps: string[];
  /** How many per-round logs this one was combined from. */
  parts: number;
  /** Your rating in this match on your main class, where it has one. */
  rating: number | null;
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
  | { kind: "rawLogs"; done: number; total: number }
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
  /** Played between these, unix seconds, inclusive. */
  from: number | null;
  to: number | null;
  limit: number;
  offset: number;
  /** date | kills | deaths | assists | dmg | dpm | kd | rating */
  sort: string | null;
  ascending: boolean;
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
  /** Health packs picked up. */
  medkits: number;
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
  /** The maps played, in order, with rounds won on each (stable teams). */
  segments: Segment[];
  /** The per-round logs this one was combined from; empty for a normal log. */
  parts: PartView[];
}

/** One of the logs a combined log was built from. */
export interface PartView {
  logId: number;
  title: string | null;
  map: string | null;
  playedAt: number | null;
  durationS: number | null;
  playerCount: number | null;
}

export interface Segment {
  map: string | null;
  firstRound: number;
  lastRound: number;
  rounds: number;
  redWins: number;
  blueWins: number;
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
  /** Kills in context against the players you face, under the same filters. */
  fights: FightsCard | null;
  /** What your demos say, under the same filters and over everything read. */
  aim: AimTotals | null;
  life: LifeTotals | null;
  aimAll: AimTotals | null;
  lifeAll: LifeTotals | null;
}

// ---- Seasons and fights -------------------------------------------------------

/** A season, from your officials in it. */
export interface Season {
  key: string;
  name: string;
  /** Unix seconds, inclusive: six days before the first official to the day after the last. */
  from: number;
  to: number;
  officials: number;
  divisions: string[];
  /** Still being played: `to` is now. */
  ongoing: boolean;
}

export interface PeriodStats {
  games: number;
  officials: number;
  scrims: number;
  pugs: number;
  wins: number;
  losses: number;
  ties: number;
  rating: number | null;
  minutes: number;
  dpm: number | null;
  kd: number | null;
  killsPer10: number | null;
  deathsPer10: number | null;
  /** 0-1 */
  openingWon: number | null;
  /** 0-1 */
  traded: number | null;
  /** 0-1 */
  fightKast: number | null;
}

export interface SeasonsView {
  class: string;
  seasons: Array<{ season: Season; stats: PeriodStats }>;
  allTime: PeriodStats;
}

export interface FightLine {
  label: string;
  unit: string;
  you: number | null;
  pool: number | null;
  /** 1 higher is better, -1 lower is better, 0 neither. */
  better: number;
  hint: string;
}

export interface FightsCard {
  games: number;
  poolGames: number;
  lines: FightLine[];
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

// ---- M6: raw logs ------------------------------------------------------------------

export interface RawlogStats {
  /** Kept Highlander logs with their raw server log stored. */
  stored: number;
  pending: number;
  /** logs.tf has no raw file for these. */
  missing: number;
  bytes: number;
  kills: number;
}

// ---- M7: match analysis ------------------------------------------------------------

export interface RoundSpan {
  roundNum: number;
  /** Game seconds: rounds laid end to end, gaps removed. */
  startS: number;
  endS: number;
}

export interface AnalysisPlayer {
  accountId: number;
  name: string;
  /** Stable team, whatever colour a stopwatch half wore. */
  team: Team;
  mainClass: string | null;
  isMe: boolean;
}

export type Vec3 = [number, number, number];

export interface KillView {
  t: number;
  roundNum: number;
  killer: number;
  victim: number;
  assister: number | null;
  killerClass: string | null;
  victimClass: string | null;
  weapon: string;
  custom: string | null;
  killerPos: Vec3 | null;
  victimPos: Vec3 | null;
  distance: number | null;
  /** The map of the kill's round. */
  map: string | null;
  jump: Jump | null;
  /** What the kill meant; null for kills the fights pass leaves out. */
  tags: KillTags | null;
}

/** What one kill meant (PLAN §11 B). Not exclusive. */
export interface KillTags {
  /** First kill of a fight: more than 10 s after the previous kill. */
  opening: boolean;
  firstOfRound: boolean;
  /** The killer's team lost someone within 3 s. */
  traded: boolean;
  /** The killer died within 3 s. */
  diedAfter: boolean;
  /** Avenged a teammate killed within 3 s before. */
  trade: boolean;
  /** The killer's team already had more players alive. */
  cleanup: boolean;
  /** A combo player killed while their team held a ready charge. */
  intoCharge: boolean;
  /** A Medic killed holding a ready charge. */
  drop: boolean;
  /** The victim's team killed back within 3 s. */
  deathTraded: boolean;
  /** The victim died near a spot they had killed from twice this life. */
  stationary: boolean;
}

/** One player's kills in context for one match. */
export interface FightStats {
  accountId: number;
  rounds: number;
  kills: number;
  deaths: number;
  openingKills: number;
  openingDeaths: number;
  firstPicks: number;
  firstDeaths: number;
  tradedKills: number;
  diedAfterKill: number;
  tradeKills: number;
  cleanupKills: number;
  chargedPicks: number;
  drops: number;
  forces: number;
  deathsBeforeUber: number;
  deathsDuringUber: number;
  deathsAfterUber: number;
  /** Deaths the team traded within 3 s. */
  tradedDeaths: number;
  deathsToSniper: number;
  /** Scout, Spy, Soldier. */
  deathsToFlank: number;
  /** Medic, Demoman, Heavy, Pyro. */
  deathsToCombo: number;
  stationaryDeaths: number;
  /** Fights alive for; those with a kill/assist, survival or traded death;
   *  and the same with survival counted only after a shot. */
  fightsPresent: number;
  fightsKast: number;
  fightsKastEngaged: number;
}

export interface FirstPickView {
  t: number;
  roundNum: number;
  /** Seconds after the round went live (the end of setup in stopwatch). */
  afterS: number;
  killer: number;
  victim: number;
}

export interface ClassDamage {
  accountId: number;
  otherClass: string;
  /** The round it happened in; 0 outside every round. */
  roundNum: number;
  dealt: number;
  taken: number;
}

export interface PlayEvent {
  t: number;
  roundNum: number;
  kind: "charge" | "drop" | "pointcap" | "chat" | "streak" | string;
  team: Team | null;
  player: number | null;
  text: string | null;
  victims: number[];
  teamChat: boolean;
  jump: Jump | null;
}

export interface Analysis {
  logId: number;
  map: string | null;
  durationS: number;
  rounds: RoundSpan[];
  players: AnalysisPlayer[];
  kills: KillView[];
  damage: ClassDamage[];
  damageSeries: Array<{ accountId: number; buckets: number[] }>;
  bucketS: number;
  events: PlayEvent[];
  hasPositions: boolean;
  damageCapped: boolean;
  /** The maps played, in order: one for most logs, two or three when combined. */
  segments: MapSegment[];
  /** Players alive and uber charge per game second, in stable teams. */
  state: StateSeries;
  fights: FightStats[];
  firstPicks: FirstPickView[];
}

/** One value per game second; index i covers [i, i + 1). */
export interface StateSeries {
  redAlive: number[];
  blueAlive: number[];
  /** 0-99 building, 100 ready, 101 in use, -1 no Medic alive. */
  redCharge: number[];
  blueCharge: number[];
  /** Uber advantage: 1 Red, -1 Blue, 0 neither. */
  advantage: number[];
}

export interface MapSegment {
  map: string | null;
  firstRound: number;
  lastRound: number;
  /** The segment's rounds in play order. */
  rounds: number[];
  startS: number;
  endS: number;
  redWins: number;
  blueWins: number;
}

/** A map overview image and where it sits in game units. */
export interface Overview {
  mapBase: string;
  minX: number;
  maxY: number;
  /** Game units the square image spans on each side. */
  size: number;
  /** A data URL (or, in the browser mock, a plain URL). */
  image: string;
}

export interface MapView {
  mapBase: string;
  games: number;
  points: number;
  /** Game units at the grid's left and top edges (game y points up). */
  minX: number;
  maxY: number;
  cell: number;
  width: number;
  height: number;
  /** Row-major, row 0 at the top. */
  occupancy: number[];
  myKills: number[];
  myDeaths: number[];
  myGames: number;
}

// ---- PLAN §14: aim from demos -------------------------------------------------

/** One kill, as the demo saw it. Angles in degrees, distances in map units. */
export interface AimRow {
  demoId: number;
  /** The demo's own kill tick. */
  tick: number;
  /** The matching kill in the log's clock, where the log had one. */
  atRaw: number | null;
  /** The round it happened in, where the log's rounds cover it. */
  roundNum: number | null;
  victim: number | null;
  /** View to the victim's head when the kill landed, and a second before. */
  errorDeg: number;
  beforeDeg: number;
  /** The same miss split in two: positive is right of the head, and above it. */
  dxDeg: number;
  dyDeg: number;
  beforeDxDeg: number;
  beforeDyDeg: number;
  /** How far the view turned in the half second before the shot. */
  flickDeg: number;
  rangeUnits: number;
  height: number;
  /** The demo carried both players throughout; if not, the numbers are stale. */
  victimSeen: boolean;
  headshot: boolean;
}

export interface AimTotals {
  kills: number;
  errorDeg: number;
  beforeDeg: number;
  flickDeg: number;
  rangeUnits: number;
  /** Share of kills where the crosshair was within 3° a second before. */
  heldShare: number;
  /** Where the crosshair usually sat: right of the head, and above it. */
  biasX: number;
  biasY: number;
}

/** One death, as the demo saw it. */
export interface DeathRow {
  demoId: number;
  tick: number;
  atRaw: number | null;
  /** The round it happened in, where the log's rounds cover it. */
  roundNum: number | null;
  killer: number | null;
  /** How far away the killer was; null when the demo never carried them. */
  killerRange: number | null;
  /** Where they were relative to your view: right, and above. 180 sideways
   *  is directly behind you. Null when the demo never carried them. */
  killerDxDeg: number | null;
  killerDyDeg: number | null;
  /** Distance to the closest living teammate, and how many were within 900. */
  nearestMate: number | null;
  matesNear: number;
  /** Scoped in at some point in the second before dying. */
  scoped: boolean;
}

export interface LifeTotals {
  /** Share of living time spent scoped in. */
  scopedShare: number;
  minutes: number;
  deaths: number;
  nearestMate: number | null;
  /** Share of deaths with nobody within 900 units, and with you scoped. */
  aloneShare: number;
  scopedShareDeaths: number;
  /** Share of deaths where the killer was over 90° from your crosshair. */
  behindShare: number | null;
}

export interface AimResponse {
  kills: AimRow[];
  deaths: DeathRow[];
  totals: AimTotals | null;
  life: LifeTotals | null;
  /** The same averages over every match with a demo. */
  career: AimTotals | null;
  careerLife: LifeTotals | null;
}

/** A copy of the database, kept beside it. */
export interface Backup {
  path: string;
  bytes: number;
  /** Unix seconds. */
  madeAt: number;
}

export interface Backups {
  dir: string;
  items: Backup[];
}
