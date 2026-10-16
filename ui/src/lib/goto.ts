/**
 * Asking the app to open a page from somewhere deep inside another one --
 * the "How ratings work" link on a match's class matchups -- without passing
 * a navigation callback down through every component in between.
 */
const EVENT = "hl:goto";

export type Destination = "rating" | "etf2l";

const PLAYER_EVENT = "hl:player";
let pendingPlayer: number | null = null;

/** The ETF2L tab's Players, on one player's profile (Q40's "Open profile"). */
export function openPlayer(accountId: number) {
  pendingPlayer = accountId;
  goTo("etf2l");
  window.dispatchEvent(new CustomEvent<number>(PLAYER_EVENT, { detail: accountId }));
}

/** For the Players page: the player asked for before it was first shown. */
export function takePendingPlayer(): number | null {
  const p = pendingPlayer;
  pendingPlayer = null;
  return p;
}

/** For the ETF2L tab: whether a player is waiting to be shown, so it opens
 *  on Players. Leaves the player for the Players page to take. */
export function hasPendingPlayer(): boolean {
  return pendingPlayer !== null;
}

/** For the ETF2L tab: a player was asked for, so show Players. Unlike
 *  `onOpenPlayer` it leaves the player for the Players page to take. */
export function onPlayerAsked(show: () => void): () => void {
  const handler = () => show();
  window.addEventListener(PLAYER_EVENT, handler);
  return () => window.removeEventListener(PLAYER_EVENT, handler);
}

/** For the Players page: a player asked for while it is mounted. */
export function onOpenPlayer(open: (accountId: number) => void): () => void {
  const handler = (e: Event) => {
    pendingPlayer = null;
    open((e as CustomEvent<number>).detail);
  };
  window.addEventListener(PLAYER_EVENT, handler);
  return () => window.removeEventListener(PLAYER_EVENT, handler);
}

const TEAM_EVENT = "hl:team";
let pendingTeam: number | null = null;

/** The ETF2L tab, on one team's page (from a match header). */
export function openTeam(teamId: number) {
  pendingTeam = teamId;
  goTo("etf2l");
  window.dispatchEvent(new CustomEvent<number>(TEAM_EVENT, { detail: teamId }));
}

/** For the ETF2L tab: the team asked for before it was first shown. */
export function takePendingTeam(): number | null {
  const id = pendingTeam;
  pendingTeam = null;
  return id;
}

/** For the ETF2L tab: a team asked for while it is mounted. */
export function onOpenTeam(open: (teamId: number) => void): () => void {
  const handler = (e: Event) => {
    pendingTeam = null;
    open((e as CustomEvent<number>).detail);
  };
  window.addEventListener(TEAM_EVENT, handler);
  return () => window.removeEventListener(TEAM_EVENT, handler);
}

const SEASON_EVENT = "hl:season";
let pendingSeason: number | null = null;

/** The ETF2L tab, on one season's page (Q60: from a bookmark). */
export function openSeason(season: number) {
  pendingSeason = season;
  goTo("etf2l");
  window.dispatchEvent(new CustomEvent<number>(SEASON_EVENT, { detail: season }));
}

/** For the ETF2L tab: the season asked for before it was first shown. */
export function takePendingSeason(): number | null {
  const s = pendingSeason;
  pendingSeason = null;
  return s;
}

/** For the ETF2L tab: a season asked for while it is mounted. */
export function onOpenSeason(open: (season: number) => void): () => void {
  const handler = (e: Event) => {
    pendingSeason = null;
    open((e as CustomEvent<number>).detail);
  };
  window.addEventListener(SEASON_EVENT, handler);
  return () => window.removeEventListener(SEASON_EVENT, handler);
}

export function goTo(page: Destination) {
  window.dispatchEvent(new CustomEvent<Destination>(EVENT, { detail: page }));
}

/** For the app shell: call `open` whenever something asks for a page. */
export function onGoTo(open: (page: Destination) => void): () => void {
  const handler = (e: Event) => open((e as CustomEvent<Destination>).detail);
  window.addEventListener(EVENT, handler);
  return () => window.removeEventListener(EVENT, handler);
}
