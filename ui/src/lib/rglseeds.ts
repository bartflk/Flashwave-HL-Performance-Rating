import type { RglTeam } from "../api/types";

/** One row of a division's list: the team, the seed it shows, and whether it is out. */
export interface DivisionRow {
  team: RglTeam;
  /** 1, 2, 3 over the teams still playing; `null` for a team out, a placeholder, or no seeds. */
  seed: number | null;
  out: boolean;
}

/** RGL's "Free Agent - Invite" and the like: a slot, not a team. */
export function isPlaceholder(name: string): boolean {
  return /^free agents?\b/i.test(name.trim());
}

export function isOut(t: RglTeam): boolean {
  return t.finalRank === 99 || t.teamStatus === "Dead";
}

/**
 * A division's teams in the order the list shows them (Flashy, 16 Nov 2026:
 * "dead teams take up a seed slot and throw the order out of whack"): the
 * teams still playing first, by RGL's seed or final place, their seeds
 * numbered again 1, 2, 3 over themselves; then the teams that dropped out,
 * unseeded; then the free-agent slot. RGL's own seeds count the dropped
 * teams, so its list read "seed 2, seed 4, seed 5".
 */
export function divisionRows(teams: RglTeam[]): DivisionRow[] {
  const key = (t: RglTeam) => t.finalRank ?? t.seeding ?? Number.MAX_SAFE_INTEGER;
  const playing = teams.filter((t) => !isOut(t) && !isPlaceholder(t.name));
  const sorted = playing.map((t, i) => ({ t, i })).sort((a, b) => key(a.t) - key(b.t) || a.i - b.i).map((x) => x.t);
  const seeded = sorted.some((t) => t.seeding != null);
  const rows: DivisionRow[] = sorted.map((team, i) => ({ team, seed: seeded && team.seeding != null ? i + 1 : null, out: false }));
  for (const team of teams.filter((t) => isOut(t) && !isPlaceholder(t.name))) rows.push({ team, seed: null, out: true });
  for (const team of teams.filter((t) => isPlaceholder(t.name))) rows.push({ team, seed: null, out: isOut(team) });
  return rows;
}
