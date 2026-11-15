import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { LineupRating, TeamRosterRow } from "../../api/types";
import { rating } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { classLabel } from "../analysis/common";
import { unitLabel } from "../../lib/gameformat";
import { Loading } from "../Spinner";
import { StatTip } from "../StatTip";

/**
 * How strong each side was coming in (Q78, Flashy: "build the opponent
 * strength on match pages"): every player's competitive career rating from
 * the logs.tf summary built into the app, averaged per side over those with
 * 20+ competitive games, side by side. Who was favoured, and whether the
 * weaker lineup won anyway.
 *
 * A player the summary does not know is left out of their side's average,
 * never counted as an average player; the bar says how many it is over.
 */

export type Side = { label: string; tone: "red" | "blu" | "a" | "b"; players: Array<{ accountId: number; class: string | null }> };

/**
 * Which strength to read: Highlander's logs.tf careers, or for 6v6 (audit
 * P1) each player's 6v6 ratings from games before `before` -- a 6v6 lineup
 * judged on how its players play Highlander said nothing about it.
 */
export type StrengthSource = { format: "hl" } | { format: "sixes"; before: number };
const HL: StrengthSource = { format: "hl" };
/** A 6v6 side needs this many known players for its bar. */
const SIXES_MIN_KNOWN = 3;

/** Each player's career, by account. */
export function useLineupRatings(players: Array<{ accountId: number; class: string | null }>, of: StrengthSource = HL) {
  const key = players.map((p) => `${p.accountId}:${p.class ?? ""}`).sort().join(",");
  const q = useQuery({
    queryKey: of.format === "sixes" ? ["sixes_lineup", key, of.before] : ["logstf_lineup", key],
    queryFn: () => (of.format === "sixes" ? api.sixesLineup(players, of.before) : api.logstfLineup(players)),
    enabled: players.length > 0,
    staleTime: of.format === "sixes" ? 5 * 60_000 : Infinity,
  });
  const by = new Map<number, LineupRating>((q.data ?? []).map((r) => [r.accountId, r]));
  return { by, pending: q.isPending && players.length > 0 };
}

/** The average of the players the summary knows, and how many that is. */
function strength(side: Side, by: Map<number, LineupRating>, min = 1) {
  const known = side.players.map((p) => by.get(p.accountId)?.rating ?? null).filter((r): r is number => r !== null);
  return { avg: known.length >= min ? known.reduce((a, b) => a + b, 0) / known.length : null, known: known.length, of: side.players.length };
}

/** Where a rating sits on the bar: 0.85 at the left edge, 1.20 at the right. */
const at = (r: number) => Math.max(2, Math.min(100, ((r - 0.85) / 0.35) * 100));

export function OpponentStrength({ sides, score, of = HL }: { sides: [Side, Side]; score?: [number | null, number | null]; of?: StrengthSource }) {
  const sixes = of.format === "sixes";
  const { by, pending } = useLineupRatings([...sides[0].players, ...sides[1].players], of);
  if (pending) return <Loading>{sixes ? t("Reading the 6v6 ratings…") : t("Reading the logs.tf summary…")}</Loading>;
  const [a, b] = sides.map((s) => strength(s, by, sixes ? SIXES_MIN_KNOWN : 1));
  if (a.avg === null && b.avg === null) return null;
  const gap = a.avg !== null && b.avg !== null ? a.avg - b.avg : null;
  const favoured = gap === null || Math.abs(gap) < 0.02 ? null : gap > 0 ? 0 : 1;
  const [sa, sb] = score ?? [null, null];
  const winner = sa !== null && sb !== null && sa !== sb ? (sa > sb ? 0 : 1) : null;
  const upset = favoured !== null && winner !== null && winner !== favoured && Math.abs(gap ?? 0) >= 0.03;
  const how = sixes
    ? t("Each player's average 6v6 rating from games before this one, your logs and the league sample, on the role they played here (their most played if they have under 5 games in it), averaged per side over the players with 5 or more. A side needs 3 such players to be shown.")
    : t(
        "Each player's competitive career rating on logs.tf (officials and other team games, every Highlander log since 2012) on the class they played here (their most played if they have under 20 games on it), averaged per side over the players with 20 or more games. Players the summary does not know are left out, not counted as average.",
      );
  return (
    <section className="os" aria-description={how}>
      <header className="os-head">
        <h3>
          <StatTip k="strength">{t("Opponent strength")}</StatTip>
        </h3>
        <span className={upset ? "os-verdict upset" : "os-verdict"}>
          {gap === null
            ? tx("Too few known players on {0} to compare", { "0": sides[a.avg === null ? 0 : 1].label })
            : upset
            ? tx("Upset: {0} won as the weaker lineup", { "0": sides[winner!].label })
            : favoured === null
              ? t("Evenly matched coming in")
              : tx("{0} favoured by {1}", { "0": sides[favoured].label, "1": Math.abs(gap!).toFixed(2) })}
        </span>
      </header>
      {sides.map((s, i) => {
        const st = i === 0 ? a : b;
        return (
          <div key={i} className={`os-row os-${s.tone}`}>
            <span className="os-label">{s.label}</span>
            <span className="os-track" aria-hidden>
              <span className="os-mid" style={{ left: `${at(1)}%` }} />
              {st.avg !== null && <span className="os-fill" style={{ width: `${at(st.avg)}%` }} />}
            </span>
            <span className="os-value">{st.avg !== null ? rating(st.avg) : "–"}</span>
            <span className="os-meta muted">{tx("over {0} of {1}", { "0": st.known, "1": st.of })}</span>
          </div>
        );
      })}
    </section>
  );
}

/** One player's career number, small, for a lineup row. */
export function CareerChip({ r, sixes = false }: { r: LineupRating | undefined; sixes?: boolean }) {
  if (!r || r.rating === null) return null;
  const tip = sixes
    ? r.onClass
      ? t("6v6 as {0}: {1} over {2} games", { "0": unitLabel(r.class), "1": rating(r.rating), "2": r.games })
      : t("6v6 as {0}, their usual role (under 5 games in this one): {1} over {2} games", { "0": unitLabel(r.class), "1": rating(r.rating), "2": r.games })
    : r.onClass
    ? t("logs.tf career on {0}: {1} over {2} competitive games", { "0": classLabel(r.class ?? ""), "1": rating(r.rating), "2": r.games })
    : t("logs.tf career on {0}, their usual class (under 20 games on this one): {1} over {2} competitive games", { "0": classLabel(r.class ?? ""), "1": rating(r.rating), "2": r.games });
  return (
    <span className={r.onClass ? "os-chip" : "os-chip off"} title={tip}>
      {rating(r.rating)}
    </span>
  );
}

/** A team's latest lineup from its roster: everyone in its last official
 *  when that was six or more players, else the nine seen most recently. */
export function latestLineup(roster: TeamRosterRow[]): Array<{ accountId: number; class: string | null }> {
  const rows = [...roster].sort((x, y) => y.lastPlayed - x.lastPlayed);
  const last = rows[0]?.lastPlayed ?? 0;
  const inLast = rows.filter((r) => r.lastPlayed === last);
  return (inLast.length >= 6 ? inLast : rows.slice(0, 9)).map((r) => ({ accountId: r.accountId, class: r.class }));
}

/**
 * A team's strength from its latest lineup (Q78, Flashy: "do the team page
 * strength"): the average logs.tf career of its players, and each of them.
 */
export function RosterStrength({ roster, onPlayer, sixes = false }: { roster: TeamRosterRow[]; onPlayer: (accountId: number) => void; sixes?: boolean }) {
  // 6v6 (audit P4): their 6v6 ratings to date, each on their most played
  // role -- a roster's Highlander class says nothing about it.
  const lineup = sixes ? latestLineup(roster).map((p) => ({ ...p, class: null })) : latestLineup(roster);
  // Taken once, to the minute, so the query key holds still.
  const [now] = useState(() => Math.floor(Date.now() / 60_000) * 60);
  const { by, pending } = useLineupRatings(lineup, sixes ? { format: "sixes", before: now } : HL);
  if (lineup.length === 0) return null;
  if (pending) return <Loading>{sixes ? t("Reading the 6v6 ratings…") : t("Reading the logs.tf summary…")}</Loading>;
  const named = new Map(roster.map((r) => [r.accountId, r.name]));
  const rows = lineup.map((p) => ({ ...p, name: named.get(p.accountId) ?? String(p.accountId), r: by.get(p.accountId) }));
  const known = rows.map((x) => x.r?.rating ?? null).filter((r): r is number => r !== null);
  const avg = known.length > 0 ? known.reduce((a, b) => a + b, 0) / known.length : null;
  rows.sort((a, b) => (b.r?.rating ?? -1) - (a.r?.rating ?? -1));
  return (
    <section className="os os-roster">
      <header className="os-head">
        <h3>
          <StatTip k="strength">{t("Roster strength")}</StatTip>
        </h3>
        <span className="os-verdict">
          {avg !== null ? rating(avg) : "–"} <span className="muted os-meta">{tx("over {0} of {1}", { "0": known.length, "1": rows.length })}</span>
        </span>
      </header>
      <div className={`os-row os-a`}>
        <span className="os-label">{t("Latest lineup")}</span>
        <span className="os-track" aria-hidden>
          <span className="os-mid" style={{ left: `${at(1)}%` }} />
          {avg !== null && <span className="os-fill" style={{ width: `${at(avg)}%` }} />}
        </span>
        <span className="os-value">{avg !== null ? rating(avg) : "–"}</span>
        <span />
      </div>
      <ul className="os-players">
        {rows.map((x) => (
          <li key={x.accountId}>
            <button className="linkish" onClick={() => onPlayer(x.accountId)}>
              {x.name}
            </button>
            <span className="muted os-class">{sixes ? unitLabel(x.r?.class) : x.r?.class ? classLabel(x.r.class) : x.class ? classLabel(x.class) : ""}</span>
            {x.r?.rating !== null && x.r?.rating !== undefined ? <CareerChip r={x.r} sixes={sixes} /> : <span className="muted os-unknown">{t("unknown")}</span>}
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Two teams' latest lineups side by side, before they meet. */
export function FixtureStrength({ us, them, sixes = false }: { us: { name: string; roster: TeamRosterRow[] }; them: { id: number; name: string }; sixes?: boolean }) {
  const other = useQuery({ queryKey: ["team", them.id], queryFn: () => api.getTeam(them.id), staleTime: 5 * 60_000 });
  const [now] = useState(() => Math.floor(Date.now() / 60_000) * 60);
  if (other.isPending) return <Loading>{t("Reading the other team's roster…")}</Loading>;
  if (!other.data) return null;
  return (
    <OpponentStrength
      sides={[
        { label: us.name, tone: "a", players: latestLineup(us.roster).map((p) => (sixes ? { ...p, class: null } : p)) },
        { label: them.name, tone: "b", players: latestLineup(other.data.roster).map((p) => (sixes ? { ...p, class: null } : p)) },
      ]}
      of={sixes ? { format: "sixes", before: now } : HL}
    />
  );
}
