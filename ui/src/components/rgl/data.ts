import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglMatch } from "../../api/types";
import { locale, t } from "../../lib/i18n";

export type Section = "home" | "matches" | "seasons" | "teams" | "rankings" | "players" | "bans";

export type Loc =
  | { at: "home" }
  | { at: "matches" }
  | { at: "match"; id: number; from: Loc }
  | { at: "seasons" }
  | { at: "season"; id: number }
  | { at: "team"; id: number; from: Loc }
  | { at: "compare"; a: number; b: number | null; from: Loc }
  | { at: "teams" }
  | { at: "rankings" }
  | { at: "players" }
  | { at: "bans" }
  | { at: "player"; steamId: string; from: Loc };

export const KEEP = 30 * 60_000;

export const useSeasons = () => useQuery({ queryKey: ["rgl_seasons"], queryFn: api.getRglSeasons, staleTime: KEEP });
export const useSeason = (id: number | null) =>
  useQuery({ queryKey: ["rgl_season", id], queryFn: () => api.getRglSeason(id!), enabled: id !== null, staleTime: KEEP });
/**
 * The power ranking, held for half an hour. It is worked out from matches
 * the app already has (hl_ingest::rgl_rank), so reading it asks RGL for
 * nothing beyond what Home has read; it is left alone while results are hidden.
 */
export const useRanking = (on = true) => useQuery({ queryKey: ["rgl_ranking"], queryFn: () => api.getRglRanking(), staleTime: KEEP, retry: false, enabled: on });
export const useTeam = (id: number | null) => useQuery({ queryKey: ["rgl_team", id], queryFn: () => api.getRglTeam(id!), enabled: id !== null, staleTime: KEEP });
export const useMatch = (id: number | null) => useQuery({ queryKey: ["rgl_match", id], queryFn: () => api.getRglMatch(id!), enabled: id !== null, staleTime: KEEP });
/**
 * The live season and its matches: RGL's newest, unless that has no match
 * yet (between seasons its empty successor is already listed), then the one
 * before it. The second read happens only in that case.
 */
export function useLive() {
  const seasons = useSeasons();
  const list = seasons.data;
  const newest = list?.[0] ?? null;
  const first = useQuery({
    queryKey: ["rgl_season_matches", newest?.id ?? null],
    queryFn: () => api.getRglSeasonMatches(newest!.id),
    enabled: newest !== null,
    staleTime: KEEP,
    retry: false,
  });
  const before = list?.[1] ?? null;
  const fallback = before !== null && first.data !== undefined && first.data.length === 0;
  const second = useQuery({
    queryKey: ["rgl_season_matches", before?.id ?? null],
    queryFn: () => api.getRglSeasonMatches(before!.id),
    enabled: fallback,
    staleTime: KEEP,
    retry: false,
  });
  const matches = fallback ? second : first;
  return {
    season: fallback ? before : newest,
    error: seasons.error ?? matches.error,
    matches: matches.data ?? null,
    /** RGL listed no season at all. */
    none: list !== undefined && list.length === 0,
    /** Asks again, whichever of the reads failed. */
    retry: () => void (seasons.error ? seasons.refetch() : matches.refetch()),
  };
}
export const secs = (iso: string | null) => (iso ? Date.parse(iso) / 1000 : 0);
/** RGL's division on the ETF2L ladder (PLAN §30), as the backend's tier_of. */
export const LADDER: Record<string, string> = { Invite: "Premiership", Advanced: "High", Main: "Mid", Intermediate: "Low", Amateur: "Low", Newcomer: "Open" };
/** "Today", "Tomorrow" or the date, for a day's heading. */
export function dayLabel(time: number): string {
  const d = new Date(time * 1000);
  const today = new Date();
  const off = Math.round((new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() - new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime()) / 86_400_000);
  if (off === 0) return t("Today");
  if (off === 1) return t("Tomorrow");
  if (off === -1) return t("Yesterday");
  return d.toLocaleDateString(locale(), { weekday: "long", day: "numeric", month: "long" });
}
export function byDay(rows: RglMatch[]): Array<[string, RglMatch[]]> {
  const out: Array<[string, RglMatch[]]> = [];
  for (const r of rows) {
    const label = dayLabel(secs(r.matchDate));
    const last = out[out.length - 1];
    if (last && last[0] === label) last[1].push(r);
    else out.push([label, [r]]);
  }
  return out;
}
/** What was typed, once typing stops: RGL is asked per word, not per key. */
export function useSettled(value: string, ms = 450): string {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const id = window.setTimeout(() => setSettled(value), ms);
    return () => window.clearTimeout(id);
  }, [value, ms]);
  return settled;
}
/** RGL's name search: three letters at least, each term asked once. */
export const SEARCH_FROM = 3;
export function useNameSearch(term: string) {
  const q = useSettled(term.trim());
  const on = q.length >= SEARCH_FROM && !looksLikeSteamId(q);
  const players = useQuery({ queryKey: ["rgl_search_players", q.toLowerCase()], queryFn: () => api.searchRglPlayers(q), enabled: on, staleTime: KEEP, retry: false });
  const teams = useQuery({ queryKey: ["rgl_search_teams", q.toLowerCase()], queryFn: () => api.searchRglTeams(q), enabled: on, staleTime: KEEP, retry: false });
  return { on, waiting: term.trim() !== q || (on && (players.isFetching || teams.isFetching)), players: players.data ?? [], teams: teams.data ?? [], playersError: players.error, teamsError: teams.error };
}
export const usePlayer = (steamId: string | null) =>
  useQuery({ queryKey: ["rgl_player", steamId], queryFn: () => api.getRglPlayer(steamId!), enabled: steamId !== null, staleTime: KEEP, retry: false });

/** An ISO time as a short date. */
export function day(iso: string | null): string {
  if (!iso) return "–";
  const d = new Date(iso);
  return d.toLocaleDateString(locale(), { day: "numeric", month: "short", year: d.getFullYear() === new Date().getFullYear() ? undefined : "numeric" });
}

/** RGL's final placing as the list shows it; 99 is a team that dropped out. */
export function placing(rank: number | null, seeding?: number | null): string {
  if (rank === 99) return t("dropped out");
  if (rank !== null) return `#${rank}`;
  return seeding != null ? t("seed {0}", { "0": seeding }) : "";
}

/** A SteamID in any form a person pastes, or an rgl.gg / Steam profile link. */
export const looksLikeSteamId = (q: string) => /^(\d{17}|\[?U:1:\d+\]?|STEAM_\d:\d:\d+)$/i.test(q) || /profiles\/\d{17}|[?&]p=\d{17}/.test(q);

/** A ban that ends after this year is for good. */
export const forGood = (iso: string | null) => !!iso && new Date(iso).getFullYear() > 2100;
