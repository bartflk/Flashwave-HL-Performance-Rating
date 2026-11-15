import type { MatchDetail, SixesRatingRow, StoredRole, Team } from "../api/types";

/** The six roles, as a team sheet lists them. */
const ROLES = ["pocket", "roamer", "flank_scout", "pocket_scout", "demoman", "medic"] as const;
/** A gap this small is even. */
const EVEN = 0.03;

export interface Pick {
  accountId: number;
  name: string;
  rating: SixesRatingRow | null;
}
export interface RoleRow {
  role: string;
  left: Pick | null;
  right: Pick | null;
  diff: number | null;
  winner: "left" | "right" | "even" | null;
  decisive: boolean;
  involvesMe: boolean;
}

/** `[component, percentile, share, raw?]`, as the 6v6 rating stores them. */
export function partsOf(r: SixesRatingRow | null): Array<[string, number, number]> {
  if (!r) return [];
  try {
    return (JSON.parse(r.parts) as Array<[string, number, number]>).map(([c, p, s]) => [c, p, s]);
  } catch {
    return [];
  }
}

/** Each role's two players, their 6v6 ratings and who won the role. */
export function roleRows(d: MatchDetail, roles: StoredRole[], ratings: SixesRatingRow[]): RoleRow[] {
  const right: Team = d.leftTeam === "Red" ? "Blue" : "Red";
  const pick = (team: Team, role: string): Pick | null => {
    const ps = d.players.filter((p) => p.team === team && roles.some((r) => r.accountId === p.accountId && r.role === role));
    // Two on one role (a swap): whoever played longer.
    const p = [...ps].sort((a, b) => b.timeS - a.timeS)[0];
    if (!p) return null;
    return { accountId: p.accountId, name: p.name, rating: ratings.find((r) => r.accountId === p.accountId && r.role === role) ?? null };
  };
  const rows: RoleRow[] = ROLES.map((role) => {
    const l = pick(d.leftTeam, role);
    const r = pick(right, role);
    const diff = l?.rating && r?.rating ? l.rating.rating - r.rating.rating : null;
    const winner = diff === null ? null : Math.abs(diff) < EVEN ? "even" : diff > 0 ? "left" : "right";
    const me = d.players.find((p) => p.isMe)?.accountId;
    return { role, left: l, right: r, diff, winner, decisive: false, involvesMe: me !== undefined && (l?.accountId === me || r?.accountId === me) };
  });
  // The two widest gaps of the match, when they are real ones.
  const gaps = rows.filter((x) => x.diff !== null && Math.abs(x.diff) >= 0.15).sort((a, b) => Math.abs(b.diff!) - Math.abs(a.diff!));
  for (const g of gaps.slice(0, 2)) g.decisive = true;
  return rows.filter((x) => x.left || x.right);
}

