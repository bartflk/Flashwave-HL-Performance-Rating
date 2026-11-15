import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open, save } from "@tauri-apps/plugin-dialog";
import { t } from "../lib/i18n";
import type {
  ActivityView,
  EtfBan,
  RglLink,
  Diagnostics,
  ModelShift,
  StvStage,
  LanguageFiles,
  SavedLanguageFile,
  Analysis,
  AppConfig,
  AppStatus,
  Cleaned,
  CmdError,
  ContextCounts,
  Tournament,
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
  RglBan,
  RglMatch,
  RglMatchMore,
  SixesSeason,
  FormatsPlayed,
  SixesMatches,
  StoredRole,
  Stint,
  SixesMatchStats,
  SixesRatingRow,
  SixesProfile,
  SixesSampleStatus,
  SixesBoardRow,
  SixesHonours,
  StoredFormat,
  RglMatchStats,
  RglPlayer,
  RglPlayerMore,
  RglPlayerHit,
  RglRanking,
  RglSeasonCard,
  RglTeamHit,
  RglSeason,
  RglSeasonItem,
  RglTeamPage,
  RglTeamSeason,
  RglTeamHistory,
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
  StorageOverview,
  OldSeasonsStatus,
  OldSeasonAdded,
  StoragePart,
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
  GappyMatch,
  LifeOnMap,
  LivesOnMap,
  StvCoverage,
  StvPart,
  MapPlayer,
  StvQueued,
  FailedLog,
  Imported,
  PlayerHit,
  PlayerResponse,
  NewDemo,
  NewestLog,
  RatingGuide,
  SixesGuide,
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
  DeepProgress,
  DataReport,
  HeadToHead,
  SeasonBreakdown,
  StatScales,
  LeagueCup,
  LeagueCupPage,
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
  getSixesRatingGuide: () => invoke<SixesGuide>("get_sixes_rating_guide"),
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
  getPlayerDeep: (accountId: number, officialsOnly = true, refresh = false) =>
    invoke<PlayerDeep>("get_player_deep", { accountId, officialsOnly, refresh }),
  onDeepProgress: (cb: (p: DeepProgress) => void): Promise<UnlistenFn> => listen<DeepProgress>("deep://progress", (e) => cb(e.payload)),
  getMatchImpact: (logId: number) => invoke<Record<string, number>>("get_match_impact", { logId }),
  getDataAudit: (season: number, tier: number | null) => invoke<DataReport>("get_data_audit", { season, tier }),
  getHeadToHead: (logId: number, a: number, b: number) => invoke<HeadToHead>("get_head_to_head", { logId, a, b }),
  getStatScales: () => invoke<StatScales>("get_stat_scales"),
  getSeasonBreakdown: (season: number, tier: number, cls: string, accountId: number) =>
    invoke<SeasonBreakdown>("get_season_breakdown", { season, tier, class: cls, accountId }),
  syncStart: (full: boolean) => invoke<void>("sync_start", { full }),
  reprocessStart: () => invoke<void>("reprocess_start"),
  /** The rating pass alone. */
  rerateStart: () => invoke<void>("rerate_start"),
  /** Every demo on this machine read again. */
  rereadDemosStart: () => invoke<void>("reread_demos_start"),
  getDiagnostics: () => invoke<Diagnostics>("get_diagnostics"),
  /** The last lines of this run's log file. */
  recentLog: (lines?: number) => invoke<string>("recent_log", { lines: lines ?? null }),
  /** The activity feed after line `since`, and every site's requests this run. */
  getActivity: (since: number, limit?: number) => invoke<ActivityView>("get_activity", { since, limit: limit ?? null }),
  setActivityVerbose: (on: boolean) => invoke<boolean>("set_activity_verbose", { on }),
  /** Every active ETF2L ban, read from ETF2L once a day. */
  getEtf2lBans: () => invoke<EtfBan[]>("get_etf2l_bans"),
  /** Your matches that were RGL matches, worked out from RGL once a day. */
  getRglOwnLinks: () => invoke<RglLink[]>("get_rgl_own_links"),
  getModelShift: () => invoke<ModelShift | null>("get_model_shift"),
  /** The rating the app runs at startup for a new model: its progress, and its end. */
  onStartupRating: async (h: { onProgress: (p: Progress) => void; onDone: () => void }): Promise<UnlistenFn> => {
    const offs = await Promise.all([
      listen<Progress>("startup://progress", (e) => h.onProgress(e.payload)),
      listen<null>("startup://rated", () => h.onDone()),
    ]);
    return () => offs.forEach((off) => off());
  },
  getMatch: (logId: number) => invoke<MatchDetail | null>("get_match", { logId }),
  getProfile: (cls: string | null, kind: ContextKind | null = null, from: number | null = null, to: number | null = null) =>
    invoke<ProfileResponse>("get_profile", { class: cls, kind, from, to }),
  /** The same profile for 6v6, by role: `classes` holds the roles played (the 6v6 plan, P7). */
  getProfileSixes: (role: string | null, kind: ContextKind | null = null, from: number | null = null, to: number | null = null, account: number | null = null) =>
    invoke<ProfileResponse>("get_profile_sixes", { role, kind, from, to, account }),
  /** Your name and picture; null before a SteamID is set. */
  getOwner: () => invoke<Owner | null>("get_owner"),
  /** Seasons from your officials, newest first. */
  listSeasons: () => invoke<Season[]>("list_seasons"),
  getSeasons: (cls: string) => invoke<SeasonsView>("get_seasons", { class: cls }),
  /** `all` includes pugs; otherwise officials and scrims only. */
  getTeammates: (all: boolean, sixes = false) => invoke<Teammates>("get_teammates", { all, sixes }),
  contextCounts: () => invoke<ContextCounts>("context_counts"),
  myTournaments: () => invoke<Tournament[]>("my_tournaments"),
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
  getStorageOverview: () => invoke<StorageOverview>("get_storage_overview"),
  getStorageBreakdown: () => invoke<StoragePart[]>("get_storage_breakdown"),
  deleteBackup: (path: string) => invoke<number>("delete_backup", { path }),
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
      title: t("Save a copy of the database"),
      defaultPath: suggested,
      filters: [{ name: t("SQLite database"), extensions: ["sqlite3"] }],
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
  getLeagues: (season?: number, format?: StoredFormat) => invoke<LeagueView>("get_leagues", { season: season ?? null, format: format ?? null }),
  getTeam: (teamId: number) => invoke<TeamView | null>("get_team", { teamId }),
  getSeasonsOverview: () => invoke<SeasonTile[]>("get_seasons_overview"),
  getSeasonBanner: (season: number, seasonName: string, large = false, format: StoredFormat = "highlander") =>
    invoke<string | null>("get_season_banner", { season, seasonName, large, format }),
  getSeasonPodiums: (season: number) => invoke<Podium[]>("get_season_podiums", { season }),
  getTeamHonours: (teamId: number) => invoke<TeamHonours>("get_team_honours", { teamId }),
  getMatchSides: (logId: number) => invoke<MatchSides | null>("get_match_sides", { logId }),
  getTeamEtf2l: (teamId: number) => invoke<TeamEtf2l>("get_team_etf2l", { teamId }),
  getTeamTransfers: (teamId: number) => invoke<TeamTransfers>("get_team_transfers", { teamId }),
  getTeamInfo: (teamId: number) => invoke<TeamInfo>("get_team_info", { teamId }),
  getFixtures: () => invoke<Fixture[]>("get_fixtures"),
  getNews: (format?: StoredFormat) => invoke<NewsItem[]>("get_news", { format: format ?? null }),
  getRglSeasons: () => invoke<RglSeasonItem[]>("get_rgl_seasons"),
  /** The power ranking from the matches already held; `read` reads one older season first (asked for by a button). */
  getRglRanking: (read: number | null = null) => invoke<RglRanking>("get_rgl_ranking", { read }),
  getRglSeason: (id: number) => invoke<RglSeason>("get_rgl_season", { id }),
  getRglSeasonMatches: (id: number) => invoke<RglMatch[]>("get_rgl_season_matches", { id }),
  getRglBans: (skip: number) => invoke<RglBan[]>("get_rgl_bans", { skip }),
  /** Matches as a calendar file: asks where to save it; the path, or null if cancelled. */
  saveCalendar: async (text: string, suggested: string): Promise<string | null> => {
    const path = await save({
      title: t("Save the calendar"),
      defaultPath: suggested,
      filters: [{ name: t("Calendar"), extensions: ["ics"] }],
    });
    if (!path) return null;
    await invoke<void>("export_calendar", { path, text });
    return path;
  },
  getRglMatch: (id: number) => invoke<RglMatch | null>("get_rgl_match", { id }),
  getRglMatchMore: (id: number) => invoke<RglMatchMore | null>("get_rgl_match_more", { id }),
  /** ETF2L 6v6's current season (prototype). */
  getSixesSeason: () => invoke<SixesSeason | null>("get_sixes_season"),
  getSixesMatches: () => invoke<SixesMatches | null>("get_sixes_matches"),
  getMatchRoles: (logId: number) => invoke<StoredRole[]>("get_match_roles", { logId }),
  getMatchStints: (logId: number) => invoke<Stint[]>("get_match_stints", { logId }),
  getSixesMatchStats: (logId: number) => invoke<SixesMatchStats | null>("get_sixes_match_stats", { logId }),
  getSixesProfile: () => invoke<SixesProfile>("get_sixes_profile"),
  getSixesSample: () => invoke<SixesSampleStatus>("get_sixes_sample"),
  getSixesBoard: (role: string) => invoke<SixesBoardRow[]>("get_sixes_board", { role }),
  setOwnRole: (logId: number, role: string | null) => invoke<StoredRole[]>("set_own_role", { logId, role }),
  getSixesHonours: (season: number) => invoke<SixesHonours[]>("get_sixes_honours", { season }),
  setSixesSample: (on: boolean) => invoke<SixesSampleStatus>("set_sixes_sample", { on }),
  getSixesRatings: (q: { logId?: number; accountId?: number }) =>
    invoke<SixesRatingRow[]>("get_sixes_ratings", { logId: q.logId ?? null, accountId: q.accountId ?? null }),
  formatsPlayed: () => invoke<FormatsPlayed>("formats_played"),
  setFormatsPlayed: (formats: StoredFormat[]) => invoke<FormatsPlayed>("set_formats_played", { formats }),
  getRglMatchStats: (id: number) => invoke<RglMatchStats | null>("get_rgl_match_stats", { id }),
  getRglTeam: (id: number) => invoke<RglTeamPage | null>("get_rgl_team", { id }),
  getRglTeamSeasons: (ids: number[]) => invoke<RglTeamSeason[]>("get_rgl_team_seasons", { ids }),
  /** `all`: every linked season (up to 12), not only the newest three. */
  getRglTeamHistory: (id: number, all = false) => invoke<RglTeamHistory | null>("get_rgl_team_history", { id, all }),
  getRglPlayer: (input: string) => invoke<RglPlayer | null>("get_rgl_player", { input }),
  getRglPlayerMore: (input: string) => invoke<RglPlayerMore>("get_rgl_player_more", { input }),
  getRglSeasonCard: (id: number, full = false) => invoke<RglSeasonCard>("get_rgl_season_card", { id, full }),
  searchRglPlayers: (q: string) => invoke<RglPlayerHit[]>("search_rgl_players", { q }),
  searchRglTeams: (q: string) => invoke<RglTeamHit[]>("search_rgl_teams", { q }),
  getNewsPost: (id: number) => invoke<NewsPost | null>("get_news_post", { id }),
  getSeasonNews: (season: number, seasonName: string) => invoke<NewsItem[]>("get_season_news", { season, seasonName }),
  getRecentResults: (limit?: number) => invoke<ResultRow[]>("get_recent_results", { limit }),
  getTournaments: () => invoke<LeagueCup[]>("get_tournaments"),
  getTournament: (name: string) => invoke<LeagueCupPage | null>("get_tournament", { name }),
  searchTeams: (query: string) => invoke<TeamHit[]>("search_teams", { query }),
  getLeagueMatch: (matchId: number) => invoke<LeagueMatch | null>("get_league_match", { matchId }),
  getMatchPage: (matchId: number) => invoke<Etf2lMatchPage | null>("get_match_page", { matchId }),
  getRecruitment: (format?: StoredFormat) => invoke<Recruitment>("get_recruitment", { format: format ?? null }),
  getRecruitPost: (id: number) => invoke<RecruitDetail | null>("get_recruit_post", { id }),
  getSeasonResults: (season: number, format?: StoredFormat) => invoke<ResultRow[]>("get_season_results", { season, format: format ?? null }),
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
  getTendencies: (cls: string, sixes = false) => invoke<MapTendencies[]>("get_tendencies", { class: cls, sixes }),
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
      title: t("Top-down image of the map"),
      multiple: false,
      directory: false,
      filters: [{ name: t("Images"), extensions: ["png", "jpg", "jpeg", "webp"] }],
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
      title: t("Save the callouts"),
      defaultPath: `${map}.callouts.json`,
      filters: [{ name: t("Callouts"), extensions: ["json"] }],
    });
    if (!path) return null;
    await invoke<void>("export_callouts", { map, path });
    return path;
  },
  pickCalloutFile: async (): Promise<string | null> => {
    const path = await open({ title: t("Callouts to import"), multiple: false, directory: false, filters: [{ name: t("Callouts"), extensions: ["json"] }] });
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
  oldSeasonsStatus: () => invoke<OldSeasonsStatus>("old_seasons_status"),
  setLogsDumpIndex: (path: string) => invoke<OldSeasonsStatus>("set_logs_dump_index", { path }),
  importOldSeason: (season: number) => invoke<OldSeasonAdded>("import_old_season", { season }),
  onOldSeasonProgress: (cb: (p: { what: string; logs?: number }) => void): Promise<UnlistenFn> =>
    listen<{ what: string; logs?: number }>("oldseason://progress", (e) => cb(e.payload)),
  onLeagueProgress: (cb: (p: { what: string; logs: number; raw: number; rosters: number }) => void): Promise<UnlistenFn> =>
    listen<{ what: string; logs: number; raw: number; rosters: number }>("league://progress", (e) => cb(e.payload)),
  refreshEtf2l: () => invoke<Etf2lRefreshed>("refresh_etf2l"),
  findParts: (logId: number) => invoke<FoundParts>("find_parts", { logId }),
  refreshDemostf: (logId: number) => invoke<DemosTfFound>("refresh_demostf", { logId }),
  setMatchKind: (logId: number, kind: ContextKind | null) => invoke<boolean>("set_match_kind", { logId, kind }),
  livesMaps: (accountId: number) => invoke<MapWithLives[]>("lives_maps", { accountId }),
  livesOnMap: (accountId: number, map: string) => invoke<LivesOnMap>("lives_on_map", { accountId, map }),
  livesPlayers: (accountId: number, map: string) => invoke<MapPlayer[]>("lives_players", { accountId, map }),
  livesOpposite: (accountId: number, map: string, cls: string | null) => invoke<LivesOnMap>("lives_opposite", { accountId, map, class: cls }),
  matchOpenings: (logId: number) => invoke<LifeOnMap[]>("match_openings", { logId }),
  stvCoverage: (logId: number) => invoke<StvCoverage | null>("stv_coverage", { logId }),
  findStvParts: (logId: number) => invoke<StvPart[]>("find_stv_parts", { logId }),
  missingStvParts: () => invoke<GappyMatch[]>("missing_stv_parts"),
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

  /**
   * The startup index of the demos folder finished (it runs after the window
   * is up), so the demo links on screen may be out of date.
   */
  onDemosIndexed: async (h: () => void): Promise<UnlistenFn> => listen("demos://indexed", () => h()),

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
