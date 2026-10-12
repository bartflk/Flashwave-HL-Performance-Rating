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
  /** Set only when this database is empty and a backup beside it is not. */
  restore: RestoreOffer | null;
}

/** Why the app is asking before it does anything else, and what it can offer. */
export interface RestoreOffer {
  /** `empty` — the database opened and holds nothing.
   *  `unreadable` — it would not open at all and was moved aside. */
  reason: "empty" | "unreadable";
  /** Where the unreadable file went: `<path>|<the error>`. */
  setAside: string | null;
  /** The backup on offer; null when there is nothing to go back to. */
  backup: BackupOffer | null;
}

export interface BackupOffer {
  path: string;
  bytes: number;
  /** Unix seconds. */
  madeAt: number;
  /** Matches inside it. */
  matches: number;
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
  /** Under a `killed:`/`diedto:` sort (Q61): your kills on that class, or
   *  deaths to it. Null otherwise, or with no server log. */
  classCount: number | null;
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
  /** Old logs the two-year window is holding back; 0 on full history. */
  outsideWindow: number;
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
  /** A short stage with no count: named, with an indeterminate bar. */
  | { kind: "stage"; what: string }
  /** Fights: who was in each one, and what each kill's state was. */
  | { kind: "fights"; done: number; total: number }
  /** Demos being read for aim, routes and deaths. The slowest phase there is. */
  | {
      kind: "readingDemos";
      done: number;
      total: number;
      logId: number | null;
      /** The match being read, to name it on the card. */
      what?: { map: string | null; playedAt: number | null; kind: string | null; opponent: string | null } | null;
    }
  /** The per-map logs a combined log was built from. */
  | { kind: "parts"; done: number; total: number }
  /** A source could not be reached; the sync carries on without it. */
  | { kind: "sourceFailed"; source: string; error: string }
  /** Downloading stopped early because the server stopped answering. */
  | { kind: "gaveUp"; source: string; done: number; total: number }
  /** logs.tf refused us: new logs come from more.tf's copy instead. */
  | { kind: "standIns"; done: number; total: number };

/** One component of a class's model, for "How ratings work". */
export interface ComponentGuide {
  key: string;
  label: string;
  unit: string;
  higherIsBetter: boolean;
  /** Its share of the class's rating, 0 to 1. */
  share: number;
  group: "fragging" | "survival" | "teamplay" | "objective" | "medic" | "speciality";
  /** logs.tf's summary, or the raw server log. */
  source: "log" | "serverLog";
  description: string;
}

export interface ClassGuide {
  class: string;
  ownModel: boolean;
  /** Biggest share first. */
  components: ComponentGuide[];
}

/** Every class's model as the rating uses it, from the live weights. */
export interface RatingGuide {
  modelVersion: string;
  ratingSpread: number;
  minMinutes: number;
  classes: ClassGuide[];
  /** What killing each class is worth, before map and side. */
  victimValues: [string, number][];
  glossary: ComponentGuide[];
}

/** One ETF2L division's part of the league sample. */
export interface LeagueTier {
  tier: number;
  division: string;
  matches: number;
  logs: number;
  jsonLogstf: number;
  /** From drops.tf (by Icewind): logs.tf's own JSON. */
  jsonDropstf: number;
  jsonMoretf: number;
  raw: number;
  rawMissing: number;
  maps: number;
  rosters: number;
  oldest: number | null;
  newest: number | null;
}

/** An ETF2L team, as a profile shows it. */
export interface CatTeam {
  id: number;
  name: string;
  avatar: string | null;
}

/** A division, and its tier: 0 Premiership, 1 High, 2 Mid, 3 Low, 4 Open. */
export interface CatDivision {
  name: string;
  tier: number;
}

/** One row of a player search across the catalogue. */
export interface CatalogueHit {
  accountId: number;
  name: string;
  highest: CatDivision | null;
  mainClass: string | null;
  /** Gold, silver, bronze. */
  medals: [number, number, number];
  officials: number;
  lastSeen: number | null;
}

export interface PlayerSeason {
  season: number;
  seasonName: string;
  division: string;
  tier: number | null;
  team: CatTeam;
  played: number;
  won: number;
  lost: number;
  place: number | null;
  /** Only ever a merc for this team that season. */
  merc?: boolean;
  /** On the roster, but left before the team's last match of the season (Q48). */
  leftEarly?: boolean;
}

/** One stay on a team, from ETF2L's transfers (Q48). `to` null: still on it. */
export interface Stay {
  teamId: number;
  teamName: string;
  teamType: string | null;
  accountId: number | null;
  name: string;
  /** Null when ETF2L has no join: a founder, or older than its records. */
  from: number | null;
  to: number | null;
}

/** One join or leave on a team's roster. */
export interface TransferRow {
  time: number;
  joined: boolean;
  name: string;
  accountId: number | null;
  /** The leader who made the change, when not the player themselves. */
  by: string | null;
}

/** An upcoming official (Q48), with the pairing's record from clan1's side. */
export interface Fixture {
  matchId: number;
  time: number;
  competition: string;
  season: number | null;
  division: string | null;
  tier: number | null;
  round: string | null;
  week: number | null;
  clan1: { id: number; name: string; avatar: string | null };
  clan2: { id: number; name: string; avatar: string | null };
  /** Empty while the maps are still to be picked. */
  maps: string[];
  h2h: LeagueRecord;
}

/** What the startup look found, before any sync. */
export interface NewLogs {
  count: number;
  source: string;
  /** When the app was last opened before now. */
  since: number | null;
}

/** A team as ETF2L's API has it (Q48). */
export interface TeamInfo {
  tag: string | null;
  homepage: string | null;
  steamGroup: string | null;
  /** Oldest first. */
  formerNames: Array<{ from: string; to: string; time: number }>;
  /** Who is on the team now; role as ETF2L has it: Leader, Deputy, Member, Buddy, Inactive. */
  members: Array<{ accountId: number | null; name: string; role: string }>;
  /** Cups entered, newest first; place from ETF2L's awards. */
  cups: Array<{ name: string; division: string | null; tier: number | null; place: string | null; competitionId: number }>;
  fetchedAt: number | null;
}

export interface TeamTransfers {
  /** Newest first. */
  rows: TransferRow[];
  /** The current roster first, longest on the team first; then who left. */
  stays: Stay[];
  fetchedAt: number | null;
}

export interface Medal {
  season: number;
  seasonName: string;
  division: string;
  tier: number | null;
  /** 1 gold, 2 silver, 3 bronze. */
  place: number;
  team: CatTeam;
  how: string;
}

/** An event's MVP: per class from the Grand Final, or the event's own. */
export interface Mvp {
  season: number;
  seasonName: string;
  division: string;
  tier: number | null;
  class: string;
  accountId: number;
  team: CatTeam;
  won: boolean;
  event: boolean;
  score: number;
  finalRating: number;
  /** Logs of the final: one per KOTH round, one per stopwatch half. */
  finalMaps: number;
  playoffsRating: number | null;
  playoffsMaps: number;
}

export interface Official {
  matchId: number;
  time: number | null;
  season: number;
  competition: string;
  division: string;
  tier: number | null;
  stage: string;
  round: string | null;
  team: CatTeam;
  opponent: CatTeam;
  scoreFor: number | null;
  scoreAgainst: number | null;
  won: boolean | null;
}

/** A player's profile (Q35): who they are, their teams, divisions and medals. */
export interface PlayerProfile {
  accountId: number;
  steamid64: string;
  name: string;
  aliases: string[];
  country: string | null;
  avatar: string | null;
  declaredClasses: string[];
  playedClasses: [string, number][];
  mainClass: string | null;
  current: PlayerSeason | null;
  highest: CatDivision | null;
  medals: Medal[];
  /** The events they were MVP of. */
  mvps: Mvp[];
  seasons: PlayerSeason[];
  officials: Official[];
  etf2lId: number | null;
  /** Signed up on ETF2L, unix seconds. */
  registered: number | null;
  /** ETF2L's title for them: "Player", "Admin", ... */
  etf2lTitle: string | null;
  /** The ETF2L teams they are on now, every format. */
  etf2lTeams: Etf2lRoster[];
  bans: Etf2lBan[];
}

export interface Etf2lRoster {
  id: number;
  name: string;
  tag: string | null;
  /** "Highlander", "6v6", "Highlander Fun Team", ... */
  kind: string | null;
  country: string | null;
  avatar: string | null;
}

export interface Etf2lBan {
  start: number | null;
  end: number | null;
  reason: string | null;
}

/** A player's career on trends.tf (Q37): Highlander only. */
export interface Career {
  wins: number;
  losses: number;
  ties: number;
  winrate: number | null;
  timeS: number;
  classes: { class: string; wins: number; losses: number; ties: number; winrate: number | null; dpm: number | null; accuracy: number | null; timeS: number }[];
  aliases: [string, number][];
  teams: { league: string; team: string; competitions: string; division: string }[];
}

export interface CareerView {
  career: Career | null;
  fetchedAt: number | null;
  error: string | null;
  url: string;
}

/** One class of a player's ratings (Q36). */
export interface ClassStats {
  class: string;
  games: number;
  career: number;
  recent: number | null;
  recentGames: number;
  best: number;
  /** Component group -> 0-100: the stat bars. */
  groups: [string, number][];
  groupsRecent: boolean;
}

export interface Rank {
  season: number;
  seasonName: string;
  division: string;
  tier: number;
  class: string;
  rank: number;
  of: number;
  avg: number;
  games: number;
}

export interface PlayerStats {
  classes: ClassStats[];
  ranks: Rank[];
}

export interface Rankings {
  season: number;
  seasonName: string;
  division: string;
  tier: number;
  class: string;
  rows: { rank: number; accountId: number; name: string; team: CatTeam | null; games: number; avg: number }[];
  seasons: [number, string][];
  divisions: [number, string][];
}

/** Every player's ETF2L division at the time of a match (Q38). */
export interface MatchDivisions {
  players: Record<number, { tier: number; division: string; season: number; seasonName?: string; exact: boolean }>;
  tierNames: Record<number, string>;
}

/** What the league sample's job is doing right now. */
export interface LeagueActivity {
  state: "starting" | "working" | "waiting" | "resting" | "sync" | "paused" | "done";
  doing: string | null;
  nextAt: number | null;
  lastHour: number;
  logstfRestLeft: number | null;
  /** Newest first. */
  recent: { at: number; text: string; ok: boolean }[];
}

/** Where the league sample stands. */
export interface LeagueSample {
  enabled: boolean;
  /** The player catalogue: officials in the window, rosters read, players named. */
  officials: number;
  rostersRead: number;
  players: number;
  discoveredAt: number | null;
  logsListed: number;
  tiers: LeagueTier[];
  bytes: number;
  logstfResting: boolean;
  targetPerTier: number;
}

/** The newest log the owner is in, and whether a sync has seen it. */
export interface NewestLog {
  logId: number;
  source: string;
  known: boolean;
}

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
  /** Your main class in the match, e.g. `sniper`. */
  class: string | null;
  /** A map without its version: `upward` matches `pl_upward_f12`. */
  map: string | null;
  /** Only matches with a SourceTV demo on this machine (Q56). */
  stvOnly?: boolean;
}

/** Q57: a map a player has routes on. */
export interface MapWithLives {
  /** Without its version: `product`. */
  map: string;
  /** The newest version played, for the picture. */
  name: string;
  matches: number;
  lives: number;
}

/** Q57: one life on the map. */
export interface LifeOnMap {
  logId: number;
  playedAt: number | null;
  demoId: number;
  stv: boolean;
  class: string | null;
  team: "Red" | "Blue" | null;
  roundNum: number | null;
  died: boolean;
  seconds: number;
  /** `[x, y]` in map units, about four a second. */
  points: Array<[number, number]>;
}

export interface LivesOnMap {
  map: string;
  matches: number;
  stvMatches: number;
  lives: LifeOnMap[];
}

/** Q60: a page kept to come back to. */
export type BookmarkKind = "match" | "player" | "team" | "season";
export interface Bookmark {
  kind: BookmarkKind;
  /** The log, account, team or season number. */
  id: number;
  /** What the page was called when it was kept. */
  label: string;
  sub?: string | null;
  /** Unix seconds. */
  added?: number;
}

/** The match list's filters, for counting what each class and map holds. */
export interface PlayedFilterQuery {
  stvOnly?: boolean;
  format?: string | null;
  kind?: string | null;
  from?: number | null;
  to?: number | null;
  class?: string | null;
  map?: string | null;
}

/** What the match list's filters can offer, most played first. */
export interface PlayedFilters {
  classes: Array<[string, number]>;
  maps: Array<[string, number]>;
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

/** The weighted average of the component percentiles, measured against every
 *  other player's performances on the class in your stored matches, and put on
 *  the scale HLTV uses: 1.00 is an average game, one standard deviation is
 *  worth 0.25. The `parts` stay percentiles — they are the working. */
export interface Rating {
  class: string;
  /** Around 1.00. */
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
  /** Seconds of respawn this player's caps cost their own dead (v11: shown, not rated). */
  spawnDelayS?: number | null;
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
  /** What one percentile point of a component is worth as a rating, so a
   *  breakdown's swings add up to the gap between two ratings. */
  ratingPerPercentile: number;
  /** False until the first rating pass has built the baselines. */
  rated: boolean;
  format: string | null;
  league: string | null;
  etf2lMatchId: number | null;
  demosTfId: number | null;
  /** SourceTV demos on ETF2L's match page, for a match demos.tf has none of (Q48). */
  etf2lDemos?: number;
  weightsWarning: string | null;
  demos: DemoView[];
  context: MatchContext | null;
  /** The maps played, in order, with rounds won on each (stable teams). */
  segments: Segment[];
  /** The per-round logs this one was combined from; empty for a normal log. */
  parts: PartView[];
  /** Where the log came from while logs.tf refused us ("more.tf"), until the
   *  real one replaces it. Absent from older builds' fixtures. */
  standIn?: string | null;
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
  /** The file was deleted to save space (Q23). Everything read from it is
   *  still on the page; watching it needs it downloaded again. */
  deleted: boolean;
}

export interface DemoStats {
  demos: number;
  linked: number;
  stv: number;
  markers: number;
  matchesWithDemo: number;
  /** Demos kept as timelines (Q3), their size, and how many lost the file. */
  timelines: number;
  timelineBytes: number;
  timelinesFileGone: number;
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

/** A demo TF2 finished writing while the app was open. */
export interface NewDemo {
  fileName: string;
  bytes: number;
}

/** A player found by search — someone who played in one of your matches. */
export interface PlayerHit {
  accountId: number;
  name: string;
  games: number;
  lastSeen: number | null;
  topClass: string | null;
}

export interface PlayerClassRow {
  class: string;
  games: number;
  avg: number;
}

/** The header of another player's page. Everything is "in your matches". */
export interface PlayerSummary {
  accountId: number;
  steamid64: string;
  name: string;
  alsoKnownAs: string[];
  games: number;
  firstSeen: number | null;
  lastSeen: number | null;
  withYou: number;
  againstYou: number;
  youBeatThem: number;
  theyBeatYou: number;
  classes: PlayerClassRow[];
}

export interface PlayerResponse {
  summary: PlayerSummary;
  profile: Profile | null;
  class: string | null;
}

/** A log the sync could not download, and why. */
export interface FailedLog {
  logId: number;
  attempts: number;
  lastAttemptAt: string;
  error: string;
  title: string | null;
  map: string | null;
  playedAt: number | null;
}

/** What came back from importing a log by hand. */
export interface Imported {
  logId: number;
  title: string | null;
  map: string | null;
  playedAt: number | null;
  players: number;
  /** False when the owner is not in it: stored, but not in their matches. */
  yours: boolean;
  /** Q71: the combined log this one is a part of; the list shows that one. */
  partOf: number | null;
}

/** Where a queued download sits. 0 means it is the one running. */
export interface StvQueued {
  logId: number;
  position: number;
}

/**
 * A step after a demo's last byte (stv://stage): linking it to its match,
 * reading it (with how far), keeping it as a timeline, saving what it held.
 */
export interface StvStage {
  logId: number;
  step: "linking" | "reading" | "keeping" | "saving";
  /** Which of the match's demos, and of how many: 1 of 2 with a POV and an STV. */
  demo: number | null;
  of: number | null;
  /** "pov" or "stv". */
  kind: string | null;
  pct: number | null;
}

export interface StvProgress {
  logId: number;
  bytes: number;
  total: number | null;
}

export interface StvFetched {
  /** The match it was fetched for. */
  logId: number;
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
  /** How the player does against weaker, even and stronger opposition. */
  opposition: OppositionBand[];
  /** ...and against opposite numbers of each ETF2L division, top first. */
  byDivision: DivisionBand[];
}

export interface DivisionBand {
  tier: number;
  division: string;
  games: number;
  avg: number;
  winRate: number | null;
}

export interface OppositionBand {
  band: "weaker" | "even" | "stronger";
  games: number;
  /** The player's average rating in those games. */
  avg: number;
  /** What those opponents average over their other games. */
  opponentAvg: number;
  winRate: number | null;
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

/** Both teams of a classified match, for the match header. */
/** What reading a match again did. */
export interface ReRead {
  /** Demos read again from their files. */
  demos: number;
  /** Linked demos whose file is gone; what was read from them is kept. */
  missing: number;
  fights: boolean;
  aim: boolean;
}

export interface MatchSides {
  /** Your side. */
  team: SideTeam | null;
  opp: SideTeam | null;
  season: number | null;
  seasonName: string | null;
  /** When ETF2L had the official scheduled. */
  scheduled: number | null;
}

export interface SideTeam {
  /** ETF2L's team id; null for a scrim side known only by name. */
  id: number | null;
  name: string;
  country: string | null;
  avatar: string | null;
}

export interface MatchContext {
  kind: ContextKind;
  etf2lMatchId: number | null;
  /** How an official was recognised: tagged by trends.tf, or found by
   *  roster; `manual` when the kind was set by hand (Q63). */
  linkMethod: "trends" | "roster" | "manual" | null;
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

/** One teamfight, in game time (Q7b). */
export interface TeamfightView {
  /** Game seconds at the first kill. */
  t: number;
  roundNum: number;
  arrivals: ArrivalView[];
}

export interface ArrivalView {
  accountId: number;
  /** Seconds after the first kill that they joined; negative if before. */
  joinedS: number;
  diedS: number | null;
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
  /** Who turned up to each fight, and when (Q7b). Fights with a side of
   *  three or more only. Optional so an older payload still renders. */
  teamfights?: TeamfightView[];
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
  /** Game units the image spans across. */
  size: number;
  /** Height over width: 1 for the built-in renders. */
  aspect: number;
  /** A data URL (or, in the browser mock, a plain URL). */
  image: string;
}

/** Where an overview image sits, in game units (Q31). */
export interface Placement {
  minX: number;
  maxY: number;
  size: number;
}

/** A map's image for lining up, placed or not (Q31). */
export interface OverviewImage {
  image: string;
  aspect: number;
  placement: Placement | null;
}

/** "yours", "built in" or "none". */
export type Origin = "yours" | "built in" | "none";

/** One map in the Maps section of Settings (Q31). */
export interface MapRow {
  base: string;
  name: string;
  matches: number;
  image: Origin;
  placement: Origin;
  callouts: Origin;
  zones: number;
  unplaced: number;
  draft: boolean;
  /** Who drew the callouts, where the file says. */
  calloutsAuthor: string | null;
  /** The last callout import can still be taken back. */
  calloutsUndo: boolean;
}

export interface MapsOverview {
  maps: MapRow[];
  /** Matches with a round on no known map. */
  unknownMatches: number;
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
  /** Whose kill it was. */
  shooter: number;
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
  /** Where the crosshair sat over the second before, oldest first and ending
   *  at the shot: pairs of sideways and vertical degrees from the head. */
  path: Array<[number, number]>;
  /** The same miss split in two: positive is right of the head, and above it. */
  dxDeg: number;
  dyDeg: number;
  beforeDxDeg: number;
  beforeDyDeg: number;
  /** How far the view turned in the half second before the shot. */
  flickDeg: number;
  rangeUnits: number;
  height: number;
  /** The demo carried the victim throughout; if not, the numbers are stale. */
  victimSeen: boolean;
  /** And the shooter, whose angles these are. */
  shooterSeen: boolean;
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
  /** Who died. */
  who: number;
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
  /** The demo carried them across the whole window. */
  seen: boolean;
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
  /** Whose aim this is: the player asked for, or the owner. */
  player: number;
  /** The match has an STV demo, which is the only kind that carries all
   *  eighteen players' angles. Without one there is nothing for anyone but
   *  the owner, and the tab has to say so rather than look empty. */
  stv: boolean;
  kills: AimRow[];
  deaths: DeathRow[];
  totals: AimTotals | null;
  life: LifeTotals | null;
  /** The same averages over every match with a demo. */
  career: AimTotals | null;
  careerLife: LifeTotals | null;
}

/** One STV demo the app downloaded and still holds on disk (Q23). */
export interface DownloadedDemo {
  demoId: number;
  fileName: string;
  sizeBytes: number;
  map: string | null;
  /** Recording start, unix seconds. */
  startUtc: number | null;
  /** Matches this demo is linked to. */
  logs: number;
  /** Every one of those matches has been read at the current pass version,
   *  so the app is finished with the file. */
  read: boolean;
}

/** What one round of demo cleanup did. */
export interface Cleaned {
  deleted: number;
  bytes: number;
  /** Left alone because the app has not finished reading them. */
  skipped: number;
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

/** One life as a route across the map, from your own demo (PLAN §14). */
export interface PathRow {
  demoId: number;
  seq: number;
  /** Whose route it is. */
  accountId: number;
  fromTick: number;
  toTick: number;
  /** The round the life started in. */
  roundNum: number | null;
  /** It ended in a death rather than a round end. */
  died: boolean;
  /** `[tick, x, y, z]` in map units, about four a second. */
  points: Array<[number, number, number, number]>;
  /** Points this player's team captured during the life:
   *  `[seconds into the life, the point's number]`. */
  caps: Array<[number, number]>;
}

/** An engineer building a match's demos saw (Emiel's ticket). */
export interface BuildingRow {
  demoId: number;
  kind: "sentry" | "dispenser" | "teleporter";
  /** A teleporter's end, when the demo was read with it recorded. */
  end: "entrance" | "exit" | null;
  /** 2 RED, 3 BLU. */
  team: number | null;
  builder: number | null;
  builderName: string | null;
  x: number;
  y: number;
  z: number;
  /** Demo ticks, the routes' frame. */
  fromTick: number;
  toTick: number;
  level: number;
}

/** One log a combined log was built from, with its own scoreboard. */
export interface PartScore {
  logId: number;
  title: string | null;
  map: string | null;
  playedAt: number | null;
  durationS: number | null;
  /** Null until the part's log has been fetched from logs.tf. */
  detail: MatchDetail | null;
  /** The combined log's rounds this part covers, matched by start time. */
  parentRounds: number[];
}

/** The user's lang folder and the `.lang` files in it, as text. */
export type LanguageFiles = { dir: string; files: Array<{ id: string; text: string }> };

/** Where a language was saved for editing; `created` is false when a file of that name was already there and was left alone. */
export type SavedLanguageFile = { path: string; created: boolean };

/** Q27: hits on fully cloaked Spies, read off a match's kept STV timelines. */
export interface SpyReport {
  /** STV demos with a timeline: the ones this could be read from. */
  demos: number;
  players: SpyPlayer[];
  /** Oldest first. */
  checks: SpyCheck[];
  /** Hits on a cloaked Spy that did not count, and why. */
  fading: number;
  blinking: number;
  marked: number;
  cooldown: number;
}

export interface SpyPlayer {
  accountId: number;
  /** The name the demo carried, for anyone the log does not. */
  name: string;
  /** Spychecks this player made. */
  checks: number;
  /** Times this player was found while fully cloaked. */
  found: number;
}

/** Q44, Q45: ping and reflects from a match's demo timelines. */
export interface MatchDemoStats {
  demos: number;
  /** Read from STV demos (everyone, every projectile), else POV ones. */
  stv: boolean;
  /** Timelines from before ping and reflects were recorded. */
  tooOld: number;
  pings: PlayerPing[];
  pyros: PyroLine[];
  reflects: ReflectRow[];
}

export interface PlayerPing {
  accountId: number;
  name: string;
  avg: number;
  median: number;
  min: number;
  max: number;
  /** [from s, to s, peak ms] */
  spikes: Array<[number, number, number]>;
  /** [s, ms] */
  points: Array<[number, number]>;
}

export interface PyroLine {
  accountId: number;
  name: string;
  reflects: number;
  hits: number;
  misses: number;
  sentBack: number;
  unknown: number;
  kills: number;
  damage: number;
  threats: number;
  judged: number;
}

export type ReflectOutcome = "hit" | "miss" | "sentBack" | "unknown";

export interface ReflectRow {
  demoId: number;
  atS: number;
  jumpTick: number;
  by: number | null;
  what: string;
  outcome: ReflectOutcome;
  victims: number[];
  damage: number;
  killed: boolean;
  threat: boolean | null;
}

export interface SpyCheck {
  demoId: number;
  /** Seconds into the demo. */
  atS: number;
  /** The demo tick to jump to: a few seconds before the hit. */
  jumpTick: number;
  attacker: number;
  spy: number;
  damage: number;
  killed: boolean;
}

/** Q11: the cart in a numbers advantage, from a match's kept STV timelines. */
export interface CartView {
  /** Up this many players or more counts as up. */
  up: number;
  /** Seconds after a won fight the push is measured over. */
  afterS: number;
  rounds: CartRound[];
  stalls: CartStall[];
  fights: CartFight[];
  /** Q12: every fight of every round, whoever won it. */
  allFights: RoundFight[];
  /** Q12: the cart still ten seconds or more with fights in it. */
  holds: CartHold[];
}

export interface RoundFight {
  round: number;
  fromS: number;
  toS: number;
  lostAttackers: number;
  lostDefenders: number;
}

export interface CartHold {
  demoId: number;
  round: number;
  /** Seconds into the round. */
  fromS: number;
  seconds: number;
  fights: number;
  pushesFailed: number;
  lostAttackers: number;
  lostDefenders: number;
  broke: boolean;
  zone: string | null;
  jumpTick: number;
}

export interface CartRound {
  demoId: number;
  seconds: number;
  movingS: number;
  upS: number;
  upStillS: number;
  /** Of the still seconds, those with no attacker near the cart; the rest
   *  had one there and a defender blocking. */
  upStillEmptyS: number;
  /** Q12: units the cart had come, every two seconds from setup's end. */
  progress: number[];
}

export interface CartStall {
  demoId: number;
  round: number;
  atS: number;
  seconds: number;
  mostUp: number;
  emptyS: number;
  jumpTick: number;
}

export interface CartFight {
  demoId: number;
  round: number;
  nth: number;
  atS: number;
  lostAttackers: number;
  lostDefenders: number;
  windowS: number;
  movingS: number;
  jumpTick: number;
}

/** Q18: a match built from a demo alone. */
export interface DemoImported extends Imported {
  rounds: number;
  kills: number;
  /** Where the demo is kept now (copied into tf/demos when picked from elsewhere). */
  path: string;
}

/** Q29: ETF2L Highlander seasons, every team. */
export interface LeagueSeason {
  season: number;
  name: string;
  divisions: string[];
  pool: string[];
}

export interface LeagueRecord {
  played: number;
  won: number;
  lost: number;
  drawn: number;
}

export interface LeagueTableRow {
  teamId: number;
  name: string;
  avatar: string | null;
  record: LeagueRecord;
  scoreFor: number;
  scoreAgainst: number;
}

export interface LeagueView {
  seasons: LeagueSeason[];
  season: LeagueSeason | null;
  divisions: Array<{ division: string; teams: LeagueTableRow[] }>;
  /** Matches whose own page is still to be read: the per-map numbers are partial until then. */
  pendingDetails: number;
}

export interface TeamMapRecord {
  map: string;
  record: LeagueRecord;
  roundsFor: number;
  roundsAgainst: number;
  inPool: boolean;
}

export interface TeamResult {
  matchId: number;
  season: number;
  division: string;
  stage: string;
  round: string | null;
  time: number | null;
  opponentId: number;
  opponent: string;
  scoreFor: number | null;
  scoreAgainst: number | null;
  defaultWin: boolean;
  maps: string[];
}

/** A league season as a tile on the Teams tab. */
export interface SeasonTile {
  season: number;
  seasonName: string;
  from: number;
  to: number;
  divisions: CatDivision[];
  teams: number;
  matches: number;
  champion: CatTeam | null;
  championDivision: string | null;
  you: PlayerSeason | null;
}

export interface PodiumPlace {
  place: number;
  team: CatTeam;
  how: string;
}

/** One division's podium in a season, and its event MVP. */
export interface Podium {
  division: string;
  tier: number | null;
  medals: PodiumPlace[];
  mvp: Mvp | null;
  mvpName: string | null;
}

export interface TeamSeason {
  season: number;
  seasonName: string;
  division: string;
  tier: number | null;
  played: number;
  won: number;
  lost: number;
  place: number | null;
}

export interface TeamHonours {
  medals: Medal[];
  seasons: TeamSeason[];
}

export interface TeamRosterRow {
  accountId: number;
  name: string;
  matches: number;
  lastPlayed: number;
  /** The class played most for this team in its officials, officials rated on it, and the average there. */
  class: string | null;
  games: number;
  rating: number | null;
  /** The same per season, newest first: the lineup of each season. */
  seasons: { season: number; officials: number; class: string | null; rating: number | null }[];
}

/** A team's own ETF2L page: its description and ETF2L's list of its awards. */
export interface TeamEtf2l {
  description: string | null;
  awards: { place: string; competition: string }[];
  url: string;
  fetchedAt: number;
}

export interface TeamView {
  teamId: number;
  name: string;
  country: string | null;
  avatar: string | null;
  /** [season, division], newest first. */
  seasons: Array<[number, string]>;
  record: LeagueRecord;
  maps: TeamMapRecord[];
  results: TeamResult[];
  roster: TeamRosterRow[];
}

/** Q28: one named zone on a map, in game units. */
export interface CalloutZone {
  name: string;
  points: Array<[number, number]>;
}

export interface CalloutFile {
  map: string;
  /** Not yet checked by someone who plays the map. */
  draft: boolean;
  source: string;
  /** Most specific first: a position counts for the first zone holding it. */
  zones: CalloutZone[];
  /** Callouts known by name and not yet drawn. */
  names: string[];
  /** Who drew them, when a shared preset says (Q32). */
  author?: string;
  /** "yours", "built in" or "none". */
  origin: string;
}

/** What importing a callout preset would do (Q32). */
export interface PresetCheck {
  /** The map the file is for. */
  map: string;
  /** The map it would go to. */
  target: string;
  zones: number;
  names: number;
  draft: boolean;
  author: string | null;
  source: string;
  currentOrigin: Origin;
  currentZones: number;
}

/** A player's habits on one map and class, over every match (Q28). */
export interface MapTendencies {
  map: string;
  draft: boolean;
  matches: number;
  fights: { zone: string; kills: number; deaths: number }[];
  unzoned: number;
  stvs: number;
  aliveS: number;
  time: { zone: string; share: number }[];
  paths: { from: string; to: string; times: number }[];
}

export interface PositionsView {
  map: string;
  zones: number;
  draft: boolean;
  players: Array<{
    accountId: number;
    name: string;
    /** 1 Scout ... 9 Engineer. */
    class: number;
    team: number;
    aliveS: number;
    zones: Array<{ zone: string; seconds: number }>;
  }>;
}

/** A demo dropped on a match page and linked to it. */
export interface DemoLinked {
  demoId: number;
  fileName: string;
  stv: boolean;
  /** The log's kills the demo holds, lined up at one offset. */
  killsMatched: number;
  logKills: number;
  playersShared: number;
  path: string;
}
