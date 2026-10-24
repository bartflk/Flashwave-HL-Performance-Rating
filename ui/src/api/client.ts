import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  StvStage,
  LanguageFiles,
  SavedLanguageFile,
  Analysis,
  AppConfig,
  AppStatus,
  Cleaned,
  CmdError,
  ContextCounts,
  ContextKind,
  DownloadedDemo,
  DemoIndexSummary,
  DemoStats,
  IndexStats,
  MapView,
  MapsOverview,
  OverviewImage,
  Placement,
  PresetCheck,
  MatchDetail,
  Overview,
  MatchPage,
  MatchQuery,
  ProfileResponse,
  Progress,
  RawlogStats,
  StvFetched,
  StvProgress,
  SyncDone,
  Teammates,
  ReRead,
  MatchSides,
  TfPathInfo,
  Owner,
  Season,
  SeasonsView,
  AimResponse,
  SpyReport,
  MatchDemoStats,
  CalloutFile,
  PositionsView,
  MapTendencies,
  LeagueView,
  TeamView,
  SeasonTile,
  Podium,
  TeamHonours,
  TeamEtf2l,
  TeamTransfers,
  TeamInfo,
  Fixture,
  NewsItem,
  NewsPost,
  ResultRow,
  TeamHit,
  LeagueMatch,
  Etf2lMatchPage,
  Recruitment,
  RecruitDetail,
  NewLogs,
  Stay,
  DemoImported,
  DemoLinked,
  DemosTfFound,
  FoundParts,
  Etf2lRefreshed,
  LeagueSides,
  DailyCheck,
  LeagueFreshness,
  LeagueCatchUp,
  Ledger,
  CartView,
  Backup,
  Backups,
  PathRow,
  BuildingRow,
  PartScore,
  PlayedFilters,
  PlayedFilterQuery,
  Bookmark,
  MassBatch,
  MassCandidate,
  MassRunStep,
  MassSearchStep,
  MassSummary,
  MapWithLives,
  LivesOnMap,
  StvQueued,
  FailedLog,
  Imported,
  PlayerHit,
  PlayerResponse,
  NewDemo,
  NewestLog,
  RatingGuide,
  LeagueSample,
  LeagueActivity,
  CatalogueHit,
  LineupRating,
  LogsTfCareer,
  LogsTfPlayer,
  PlayerProfile,
  PlayerStats,
  CareerView,
  Rankings,
  MatchDivisions,
  SeasonTable,
  PlayerDeep,
  DataReport,
} from "./types";
import { withChosenNames } from "../lib/names";

/** True inside the Tauri window, false in a plain browser tab. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface StvHandlers {
  onQueued: (q: StvQueued) => void;
  onProgress: (p: StvProgress) => void;
  onDone: (d: StvFetched) => void;
  onError: (e: CmdError & { logId: number }) => void;
  onStage: (s: StvStage) => void;
}

export interface SyncHandlers {
  onProgress: (p: Progress) => void;
  onDone: (d: SyncDone) => void;
  onError: (e: CmdError) => void;
}

const realApi = {
  appStatus: () => invoke<AppStatus>("app_status"),
  getConfig: () => invoke<AppConfig>("get_config"),
  /** "self": the updater installs it. "manual": a Linux package, updated from the download page. */
  updateKind: () => invoke<"self" | "manual">("update_kind"),
  setSteamId: (input: string) => invoke<AppConfig>("set_steamid", { input }),
  detectTfPath: () => invoke<TfPathInfo | null>("detect_tf_path"),
  inspectTfPath: (path: string) => invoke<TfPathInfo>("inspect_tf_path", { path }),
  setTfPath: (path: string) => invoke<TfPathInfo>("set_tf_path", { path }),

  listMatches: (q: MatchQuery) => invoke<MatchPage>("list_matches", { ...q }),
  indexStats: () => invoke<IndexStats>("index_stats"),
  syncBusy: () => invoke<boolean>("sync_busy"),
  newestLog: () => invoke<NewestLog | null>("newest_log"),
  getRatingGuide: () => invoke<RatingGuide>("get_rating_guide"),
  getLeagueSample: () => invoke<LeagueSample>("get_league_sample"),
  setLeagueSample: (on: boolean) => invoke<void>("set_league_sample", { on }),
  getLeagueActivity: () => invoke<LeagueActivity>("get_league_activity"),
  searchCatalogue: (query: string) => invoke<CatalogueHit[]>("search_catalogue", { query }),
  logstfSearch: (query: string) => invoke<LogsTfPlayer[]>("logstf_search", { query }),
  logstfCareer: (accountId: number) => invoke<LogsTfCareer | null>("logstf_career", { accountId }),
  logstfLineup: (players: Array<{ accountId: number; class: string | null }>) => invoke<LineupRating[]>("logstf_lineup", { players }),
  getPlayerProfile: (accountId: number) => invoke<PlayerProfile>("get_player_profile", { accountId }),
  getPlayerStats: (accountId: number) => invoke<PlayerStats>("get_player_stats", { accountId }),
  getTrendsCareer: (accountId: number) => invoke<CareerView>("get_trends_career", { accountId }),
  getMatchDivisions: (logId: number) => invoke<MatchDivisions>("get_match_divisions", { logId }),
  getRankings: (season: number | null, tier: number | null, cls: string) => invoke<Rankings>("get_rankings", { season, tier, class: cls }),
  getSeasonTable: (season: number | null, tier: number | null, cls: string, stage: string) =>
    invoke<SeasonTable>("get_season_table", { season, tier, class: cls, stage }),
  getPlayerDeep: (accountId: number) => invoke<PlayerDeep>("get_player_deep", { accountId }),
  getMatchImpact: (logId: number) => invoke<Record<string, number>>("get_match_impact", { logId }),
  getDataAudit: (season: number, tier: number | null) => invoke<DataReport>("get_data_audit", { season, tier }),
  syncStart: (full: boolean) => invoke<void>("sync_start", { full }),
  reprocessStart: () => invoke<void>("reprocess_start"),
  getMatch: (logId: number) => invoke<MatchDetail | null>("get_match", { logId }),
  getProfile: (cls: string | null, kind: ContextKind | null = null, from: number | null = null, to: number | null = null) =>
    invoke<ProfileResponse>("get_profile", { class: cls, kind, from, to }),
  /** Your name and picture; null before a SteamID is set. */
  getOwner: () => invoke<Owner | null>("get_owner"),
  /** Seasons from your officials, newest first. */
  listSeasons: () => invoke<Season[]>("list_seasons"),
  getSeasons: (cls: string) => invoke<SeasonsView>("get_seasons", { class: cls }),
  /** `all` includes pugs; otherwise officials and scrims only. */
  getTeammates: (all: boolean) => invoke<Teammates>("get_teammates", { all }),
  contextCounts: () => invoke<ContextCounts>("context_counts"),
  rawlogStats: () => invoke<RawlogStats>("rawlog_stats"),
  /** Null when the match's raw log is not stored. */
  getMatchAnalysis: (logId: number) => invoke<Analysis | null>("get_match_analysis", { logId }),
  /** The classes and maps you have played, for the match list's filters. */
  /** Counted under the list's other filters (Q59); none gives every match. */
  playedFilters: (q?: PlayedFilterQuery) =>
    invoke<PlayedFilters>("played_filters", { format: null, kind: null, from: null, to: null, class: null, map: null, stvOnly: false, ...q }),
  /** The scoreboards of the logs a combined log was built from. */
  getParts: (logId: number) => invoke<PartScore[]>("get_parts", { logId }),
  /** Fetch one part's log from logs.tf and score it. */
  fetchPart: (partId: number) => invoke<MatchDetail | null>("fetch_part", { partId }),
  /** Where you walked in one match, one route per life (PLAN §14). */
  getPaths: (logId: number) => invoke<PathRow[]>("get_paths", { logId }),
  getBuildings: (logId: number) => invoke<BuildingRow[]>("get_buildings", { logId }),
  /** Whether a downloaded demo is deleted once it has been read (Q23). */
  autoDeleteDemos: () => invoke<boolean>("auto_delete_demos"),
  setAutoDeleteDemos: (on: boolean) => invoke<boolean>("set_auto_delete_demos", { on }),
  /** STV demos the app downloaded and still holds (Q23). */
  downloadedDemos: () => invoke<DownloadedDemo[]>("downloaded_demos"),
  /** Delete downloaded demos to reclaim space. One, or all of them. */
  deleteDownloadedDemos: (only: number | null = null, force = false) =>
    invoke<Cleaned>("delete_downloaded_demos", { only, force }),
  /** Copies of the database, newest first. */
  allHistory: () => invoke<boolean>("all_history"),
  setAllHistory: (on: boolean) => invoke<boolean>("set_all_history", { on }),
  listBackups: () => invoke<Backups>("list_backups"),
  /** Show a file or folder in Explorer. */
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  languageFiles: () => invoke<LanguageFiles>("language_files"),
  saveLanguageFile: (id: string, text: string) => invoke<SavedLanguageFile>("save_language_file", { id, text }),
  /** Players' ETF2L names by account id, from rosters already stored. */
  etf2lNames: () => invoke<Record<string, string>>("etf2l_names"),
  /** Put a backup back and restart. Refused unless this database is empty. */
  restoreBackup: (path: string) => invoke<void>("restore_backup", { path }),
  /** Start fresh on purpose: stop offering the backup. */
  declineRestore: () => invoke<void>("decline_restore"),
  /** Copy the database now, whatever the last copy's age. */
  backupNow: () => invoke<Backup | null>("backup_now"),

  /**
   * Ask where to put a copy, then write it there. `null` if the dialog was
   * dismissed — the automatic copies sit beside the database and go with it
   * if the app is uninstalled with "delete application data" ticked, so this
   * is the one that survives that.
   */
  saveBackupAs: async (suggested: string): Promise<Backup | null> => {
    const path = await save({
      title: "Save a copy of the database",
      defaultPath: suggested,
      filters: [{ name: "SQLite database", extensions: ["sqlite3"] }],
    });
    if (!path) return null;
    return invoke<Backup>("save_backup_as", { path });
  },
  /** What the demo says about one player's aim in one match (PLAN §14).
   *  Without a player it answers for the owner, as it always did. */
  getAim: (logId: number, player?: number) => invoke<AimResponse>("get_aim", { logId, player }),
  /** Null when the match has no STV timeline to read (Q27). */
  getSpychecks: (logId: number) => invoke<SpyReport | null>("get_spychecks", { logId }),
  /** Q44, Q45: ping and reflects; null when no demo of the match has a timeline. */
  getDemoStats: (logId: number) => invoke<MatchDemoStats | null>("get_demo_stats", { logId }),
  /** Null for a match with no cart to read (Q11). */
  getCart: (logId: number, map?: string) => invoke<CartView | null>("get_cart", { logId, map: map ?? null }),
  /** Q29: one season's division tables; the newest when no season is given. */
  getLeagues: (season?: number) => invoke<LeagueView>("get_leagues", { season: season ?? null }),
  getTeam: (teamId: number) => invoke<TeamView | null>("get_team", { teamId }),
  getSeasonsOverview: () => invoke<SeasonTile[]>("get_seasons_overview"),
  getSeasonBanner: (season: number, seasonName: string, large = false) => invoke<string | null>("get_season_banner", { season, seasonName, large }),
  getSeasonPodiums: (season: number) => invoke<Podium[]>("get_season_podiums", { season }),
  getTeamHonours: (teamId: number) => invoke<TeamHonours>("get_team_honours", { teamId }),
  getMatchSides: (logId: number) => invoke<MatchSides | null>("get_match_sides", { logId }),
  getTeamEtf2l: (teamId: number) => invoke<TeamEtf2l>("get_team_etf2l", { teamId }),
  getTeamTransfers: (teamId: number) => invoke<TeamTransfers>("get_team_transfers", { teamId }),
  getTeamInfo: (teamId: number) => invoke<TeamInfo>("get_team_info", { teamId }),
  getFixtures: () => invoke<Fixture[]>("get_fixtures"),
  getNews: () => invoke<NewsItem[]>("get_news"),
  getNewsPost: (id: number) => invoke<NewsPost | null>("get_news_post", { id }),
  getRecentResults: (limit?: number) => invoke<ResultRow[]>("get_recent_results", { limit }),
  searchTeams: (query: string) => invoke<TeamHit[]>("search_teams", { query }),
  getLeagueMatch: (matchId: number) => invoke<LeagueMatch | null>("get_league_match", { matchId }),
  getMatchPage: (matchId: number) => invoke<Etf2lMatchPage | null>("get_match_page", { matchId }),
  getRecruitment: () => invoke<Recruitment>("get_recruitment"),
  getRecruitPost: (id: number) => invoke<RecruitDetail | null>("get_recruit_post", { id }),
  getSeasonResults: (season: number) => invoke<ResultRow[]>("get_season_results", { season }),
  checkNewLogs: () => invoke<NewLogs | null>("check_new_logs"),
  getPlayerTeams: (accountId: number) => invoke<Stay[]>("get_player_teams", { accountId }),
  /** Q28: a map's callouts; the owner's copy wins over the built-in one. */
  getCallouts: (map: string) => invoke<CalloutFile>("get_callouts", { map }),
  saveCallouts: (map: string, file: CalloutFile) => invoke<CalloutFile>("save_callouts", { map, file }),
  resetCallouts: (map: string) => invoke<CalloutFile>("reset_callouts", { map }),
  /** Q30: maps to pick from, and "these rounds were on ___" (null takes it back). */
  knownMaps: () => invoke<string[]>("known_maps"),
  setRoundMap: (logId: number, rounds: number[], map: string | null) => invoke<void>("set_round_map", { logId, rounds, map }),
  getPositions: (logId: number, map: string) => invoke<PositionsView | null>("get_positions", { logId, map }),
  getTendencies: (cls: string) => invoke<MapTendencies[]>("get_tendencies", { class: cls }),
  /** Null when too few kills are stored on the map to draw it. */
  getMapView: (map: string) => invoke<MapView | null>("get_map_view", { map }),
  /** Null when no image for the map is saved in the app's overviews folder. */
  getMapOverview: (map: string) => invoke<Overview | null>("get_map_overview", { map }),
  /** Q31: the Maps section of Settings. */
  mapsOverview: () => invoke<MapsOverview>("maps_overview"),
  overviewImage: (map: string) => invoke<OverviewImage | null>("overview_image", { map }),
  /** Pick an image and put it in for this map; false when the dialog was closed. */
  importOverview: async (map: string): Promise<boolean> => {
    const path = await open({
      title: "Top-down image of the map",
      multiple: false,
      directory: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
    });
    if (typeof path !== "string") return false;
    await invoke<void>("import_overview", { map, path });
    return true;
  },
  saveOverviewPlacement: (map: string, placement: Placement) => invoke<void>("save_overview_placement", { map, placement }),
  removeOverview: (map: string) => invoke<void>("remove_overview", { map }),
  /** Q32: a map's callouts to a preset file; the path, or null if cancelled. */
  exportCallouts: async (map: string): Promise<string | null> => {
    const path = await save({
      title: "Save the callouts",
      defaultPath: `${map}.callouts.json`,
      filters: [{ name: "Callouts", extensions: ["json"] }],
    });
    if (!path) return null;
    await invoke<void>("export_callouts", { map, path });
    return path;
  },
  pickCalloutFile: async (): Promise<string | null> => {
    const path = await open({ title: "Callouts to import", multiple: false, directory: false, filters: [{ name: "Callouts", extensions: ["json"] }] });
    return typeof path === "string" ? path : null;
  },
  /** What importing would do; with no map, onto the map the file names. */
  inspectCallouts: (map: string | null, path: string) => invoke<PresetCheck>("inspect_callouts", { map, path }),
  importCallouts: (map: string, path: string, anyMap: boolean) => invoke<CalloutFile>("import_callouts", { map, path, anyMap }),
  undoCallouts: (map: string) => invoke<CalloutFile>("undo_callouts", { map }),
  /** Opens in the system browser, never inside the app window. */
  openExternal: (url: string) => openUrl(url),
  copyText: (text: string) => writeText(text),

  scanDemos: () => invoke<DemoIndexSummary>("scan_demos"),
  demoStats: () => invoke<DemoStats>("demo_stats"),
  fetchStv: (logId: number) => invoke<void>("fetch_stv", { logId }),
  /** Drop a download that has not started. False if it is already running. */
  cancelStv: (logId: number) => invoke<boolean>("cancel_stv", { logId }),

  /** Find someone who played in one of your matches (Q14). */
  searchPlayers: (query: string) => invoke<PlayerHit[]>("search_players", { query }),
  /** One player's page: their summary, and their profile on one class. */
  getPlayer: (accountId: number, className: string | null) =>
    invoke<PlayerResponse>("get_player", { accountId, class: className }),

  /** Logs the sync gave up on, newest first. */
  failedLogs: () => invoke<FailedLog[]>("failed_logs"),
  /** Forget a log's failures, or every log's, so the next sync retries. */
  retryFailed: (logId?: number) => invoke<number>("retry_failed", { logId: logId ?? null }),
  /** Fetch one log now, by id or logs.tf link. */
  importLog: (text: string) => invoke<Imported>("import_log", { text }),
  /** Q18: a match from a demo alone, for a server that wrote no log. */
  importDemo: (path: string) => invoke<DemoImported>("import_demo", { path }),
  /** A demo dropped on a match page, linked to that match once it checks out. */
  linkDemo: (logId: number, path: string) => invoke<DemoLinked>("link_demo", { logId, path }),
  linkDemostf: (logId: number, link: string) => invoke<DemoLinked>("link_demostf", { logId, link }),
  getCombinedLedger: (logId: number) => invoke<Ledger | null>("get_combined_ledger", { logId }),
  getLeagueSides: (logId: number) => invoke<LeagueSides | null>("get_league_sides", { logId }),
  etf2lDailyCheck: (force = false) => invoke<DailyCheck>("etf2l_daily_check", { force }),
  leagueFreshness: () => invoke<LeagueFreshness>("league_freshness"),
  updateLeague: () => invoke<LeagueCatchUp>("update_league"),
  onLeagueProgress: (cb: (p: { what: string; logs: number; raw: number; rosters: number }) => void): Promise<UnlistenFn> =>
    listen<{ what: string; logs: number; raw: number; rosters: number }>("league://progress", (e) => cb(e.payload)),
  refreshEtf2l: () => invoke<Etf2lRefreshed>("refresh_etf2l"),
  findParts: (logId: number) => invoke<FoundParts>("find_parts", { logId }),
  refreshDemostf: (logId: number) => invoke<DemosTfFound>("refresh_demostf", { logId }),
  setMatchKind: (logId: number, kind: ContextKind | null) => invoke<boolean>("set_match_kind", { logId, kind }),
  livesMaps: (accountId: number) => invoke<MapWithLives[]>("lives_maps", { accountId }),
  livesOnMap: (accountId: number, map: string) => invoke<LivesOnMap>("lives_on_map", { accountId, map }),
  /** Q69: mass import of a player's demos on one map. */
  massSearch: (accountId: number, map: string, cls: string | null, want: number) =>
    invoke<MassCandidate[]>("mass_search", { accountId, map, class: cls, want }),
  massStart: (accountId: number, map: string, cls: string | null, picks: Array<{ demosTfId: number; time: number; map: string }>) =>
    invoke<number>("mass_start", { accountId, map, class: cls, picks }),
  massCancel: () => invoke<void>("mass_cancel"),
  massBatches: () => invoke<MassBatch[]>("mass_batches"),
  massKeep: (batchId: number, keep: boolean) => invoke<number>("mass_keep", { batchId, keep }),
  massForget: (batchId: number) => invoke<void>("mass_forget", { batchId }),
  onMass: async (h: {
    onSearch: (s: MassSearchStep) => void;
    onProgress: (s: MassRunStep) => void;
    onDone: (s: MassSummary) => void;
    onError: (e: { batchId: number; error: CmdError }) => void;
  }): Promise<UnlistenFn> => {
    const offs = await Promise.all([
      listen<MassSearchStep>("mass://search", (e) => h.onSearch(e.payload)),
      listen<MassRunStep>("mass://progress", (e) => h.onProgress(e.payload)),
      listen<MassSummary>("mass://done", (e) => h.onDone(e.payload)),
      listen<{ batchId: number; error: CmdError }>("mass://error", (e) => h.onError(e.payload)),
    ]);
    return () => offs.forEach((off) => off());
  },
  getBookmarks: () => invoke<Bookmark[]>("get_bookmarks"),
  setBookmark: (bookmark: Bookmark, on: boolean) => invoke<Bookmark[]>("set_bookmark", { bookmark, on }),
  rereadMatch: (logId: number) => invoke<ReRead>("reread_match", { logId }),
  /** Each step of a match being read again, as it starts. */
  onRereadStep: async (h: (s: { logId: number; step: string }) => void): Promise<UnlistenFn> =>
    listen<{ logId: number; step: string }>("reread://step", (e) => h(e.payload)),

  /** STV download events. Returns a function that unsubscribes all three. */
  onStv: async (h: StvHandlers): Promise<UnlistenFn> => {
    const offs = await Promise.all([
      listen<StvQueued>("stv://queued", (e) => h.onQueued(e.payload)),
      listen<StvProgress>("stv://progress", (e) => h.onProgress(e.payload)),
      listen<StvStage>("stv://stage", (e) => h.onStage(e.payload)),
      listen<StvFetched>("stv://done", (e) => h.onDone(e.payload)),
      listen<CmdError & { logId: number }>("stv://error", (e) => h.onError(e.payload)),
    ]);
    return () => offs.forEach((off) => off());
  },

  /**
   * A demo TF2 has just finished writing. Fires once per file, and only for
   * demos that appear while the app is running.
   */
  onNewDemo: async (h: (d: NewDemo) => void): Promise<UnlistenFn> =>
    listen<NewDemo>("demos://new", (e) => h(e.payload)),

  /** Subscribe to sync events. Returns a function that unsubscribes all three. */
  syncCancel: () => invoke<boolean>("sync_cancel"),
  getDemoFolders: () => invoke<string[]>("get_demo_folders"),
  setDemoFolders: (folders: string[]) => invoke<DemoIndexSummary>("set_demo_folders", { folders }),
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

// The mock, and the real-match fixtures it carries, load only in a plain
// browser during development. `import.meta.env.DEV` is false in a release
// build, so the branch and the fixtures are dropped from the bundle.
const base: Api = inTauri || !import.meta.env.DEV ? realApi : (await import("./mock")).mockApi;

/**
 * Every call's answer passes through the chosen player names (../lib/names.ts),
 * so no page has to know there is a choice. Results that hold no players,
 * and the event subscriptions, come through unchanged.
 */
export const api: Api = Object.fromEntries(
  Object.entries(base).map(([key, fn]) => [
    key,
    (...args: unknown[]) => {
      const out = (fn as (...a: unknown[]) => unknown)(...args);
      return out instanceof Promise ? out.then(withChosenNames) : out;
    },
  ]),
) as Api;
