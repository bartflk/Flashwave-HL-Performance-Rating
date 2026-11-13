import type { RglMatch, RglRecord, RglTeamHistory } from "../api/types";

/**
 * The pure parts of the RGL team page and its Compare view: form, the
 * trophy cabinet, win rates and the two teams set against each other. Kept
 * apart from the components so they can be tested without React.
 */

/** Wins as a share of the games played (forfeits are not games); `null` before any. */
export function winRate(r: Pick<RglRecord, "won" | "lost">): number | null {
  const n = r.won + r.lost;
  return n > 0 ? (r.won / n) * 100 : null;
}

/**
 * A team's last results, newest first: the matches with a winner, a forfeit
 * included (it is a result in the table), a match still to be played not.
 * `matches` are in any order.
 */
export function form(matches: RglMatch[], us: number | number[], n = 10): Array<{ match: RglMatch; won: boolean }> {
  // A team has a new id each season: `us` may be all of them.
  const mine = new Set(Array.isArray(us) ? us : [us]);
  return matches
    .filter((m) => m.winner !== null)
    .sort((a, b) => (b.matchDate ?? "").localeCompare(a.matchDate ?? "") || b.matchId - a.matchId)
    .slice(0, n)
    .map((match) => ({ match, won: mine.has(match.winner as number) }));
}

/** Every podium place (1 to 3), newest season first, as the history lists its seasons. */
export function medals(seasons: RglTeamHistory["seasons"]): Array<RglTeamHistory["seasons"][number] & { place: 1 | 2 | 3 }> {
  return seasons.flatMap((s) => (s.finalRank === 1 || s.finalRank === 2 || s.finalRank === 3 ? [{ ...s, place: s.finalRank }] : []));
}

/** Titles and podiums (a title is a podium too), as the headline card counts them. */
export function trophyCount(seasons: RglTeamHistory["seasons"]): { titles: number; podiums: number } {
  const m = medals(seasons);
  return { titles: m.filter((x) => x.place === 1).length, podiums: m.length };
}

/**
 * The matches two teams played each other, from `a`'s side, newest first.
 * RGL gives a team a new id each season, so the opponent is any of `b`'s
 * ids across the seasons read. Forfeits are counted apart.
 */
export function headToHead(a: RglTeamHistory, b: RglTeamHistory): { matches: RglMatch[]; won: number; lost: number; forfeits: number } {
  const mine = new Set(a.teamIds);
  const theirs = new Set(b.teamIds);
  const matches = a.matches.filter((m) => m.teams.some((x) => mine.has(x.teamId)) && m.teams.some((x) => theirs.has(x.teamId)));
  let won = 0;
  let lost = 0;
  let forfeits = 0;
  for (const m of matches) {
    if (m.winner === null) continue;
    if (m.isForfeit) forfeits += 1;
    else if (mine.has(m.winner)) won += 1;
    else lost += 1;
  }
  return { matches, won, lost, forfeits };
}

/** The maps both teams played, most played (by both) first, each side's record on it. */
export function sharedMaps(a: RglTeamHistory, b: RglTeamHistory): Array<{ map: string; a: RglTeamHistory["maps"][number]; b: RglTeamHistory["maps"][number] }> {
  const other = new Map(b.maps.map((m) => [m.map, m]));
  return a.maps
    .flatMap((m) => {
      const o = other.get(m.map);
      return o ? [{ map: m.map, a: m, b: o }] : [];
    })
    .sort((x, y) => y.a.won + y.a.lost + y.b.won + y.b.lost - (x.a.won + x.a.lost + x.b.won + x.b.lost) || x.map.localeCompare(y.map));
}
