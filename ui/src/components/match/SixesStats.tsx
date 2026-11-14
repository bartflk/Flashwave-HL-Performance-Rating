import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { MatchDetail, SixesMedicStats, SixesPlayerStats, SixesRatingRow } from "../../api/types";
import { t } from "../../lib/i18n";
import { teamLabel } from "../../lib/format";
import { roleLabel } from "../../lib/gameformat";
import { ClassIcon } from "../ClassIcon";

/**
 * A 6v6 match's own numbers (the 6v6 plan, P4): what boSe. asked each role
 * to be judged on that the log records -- damage dealt and taken and their
 * ratio, airshots, deaths beside the Medic's, sticky, reflect and sentry
 * kills -- and the two Medics side by side.
 */
export function SixesStats({ d }: { d: MatchDetail }) {
  const q = useQuery({ queryKey: ["sixes_match_stats", d.logId], queryFn: () => api.getSixesMatchStats(d.logId), staleTime: 5 * 60_000 });
  const roles = useQuery({ queryKey: ["match_roles", d.logId], queryFn: () => api.getMatchRoles(d.logId), staleTime: 5 * 60_000 }).data ?? [];
  // The preview rating of each player's main role (the 6v6 plan, P6).
  const ratings = useQuery({ queryKey: ["sixes_ratings", d.logId], queryFn: () => api.getSixesRatings({ logId: d.logId }), staleTime: 5 * 60_000 }).data ?? [];
  const ratingOf = (id: number) => ratings.find((r) => r.accountId === id && !r.role.startsWith("off:")) ?? null;
  const names = useMemo(() => new Map(d.players.map((p) => [p.accountId, p.name])), [d.players]);
  const roleOf = (id: number) => roles.map((r) => (r.accountId === id ? roleLabel(r.role) : null)).find(Boolean) ?? null;
  const s = q.data;
  if (!s || s.players.length === 0) return null;
  const order = [d.leftTeam, d.leftTeam === "Red" ? "Blue" : "Red"];
  const players = [...s.players].sort((a, b) => order.indexOf(a.team as never) - order.indexOf(b.team as never));
  const anySticky = s.players.some((p) => p.stickyKills > 0);
  const anyReflect = s.players.some((p) => p.reflectKills > 0);
  const anySentry = s.players.some((p) => p.sentryKills > 0);
  const anySaps = s.players.some((p) => (p.saps ?? 0) > 0);
  const anyDenies = s.players.some((p) => (p.denies ?? 0) > 0);
  const hasPicks = s.players.some((p) => p.picks !== null);
  // Proposed definitions for the reviewers, in no rating (the 6v6 plan, P4).
  const hasProposed = s.players.some((p) => p.forcesProposed != null);
  const anySniper = s.players.some((p) => (p.sniperComboDamage ?? 0) > 0);
  const f = (v: number | null, digits = 0) => (v === null ? "–" : v.toFixed(digits));
  return (
    <section className="panel sx6-stats">
      <h2>{t("6v6 numbers")}</h2>
      <p className="hint">{t("What each 6v6 role is judged on that the log records. The rating is a preview: weighed by role against the other players in your stored 6v6 logs, without round impact yet.")}</p>
      <div className="table-wrap">
        <table className="match-table">
          <thead>
            <tr>
              <th>{t("Team")}</th>
              <th>{t("Player")}</th>
              {ratings.length > 0 && <th className="num" title={t("The 6v6 rating's preview: 1.00 is an average game in the role")}>{t("Rating")}</th>}
              <th className="num" title={t("Damage per minute")}>{t("DPM")}</th>
              <th className="num" title={t("Damage taken per minute")}>{t("DTM")}</th>
              <th className="num" title={t("Damage dealt over damage taken")}>{t("Ratio")}</th>
              <th className="num" title={t("Airshots")}>{t("AS")}</th>
              <th className="num" title={t("Deaths, and their own Medic's deaths")}>{t("Deaths · Medic's")}</th>
              <th className="num" title={t("Enemy Medics killed")}>{t("Med picks")}</th>
              {hasPicks && <th className="num" title={t("First kills of a fight")}>{t("Picks")}</th>}
              {anySticky && <th className="num" title={t("Kills with stickies")}>{t("Stickies")}</th>}
              {anyReflect && <th className="num" title={t("Kills with reflected projectiles")}>{t("Reflects")}</th>}
              {anySentry && <th className="num" title={t("Kills by the sentry")}>{t("Sentry")}</th>}
              {anySaps && <th className="num" title={t("Buildings destroyed as Spy")}>{t("Saps")}</th>}
              {anyDenies && <th className="num" title={t("Soldiers and Demomen killed as Heavy")}>{t("Denies")}</th>}
              {hasProposed && (
                <th className="num sx6-proposed" title={t("Proposed for the reviewers, in no rating: enemy übers used after this player dealt 50 damage or more to that Medic's team in the 4 seconds before")}>
                  {t("Forces*")}
                </th>
              )}
              {hasProposed && (
                <th className="num sx6-proposed" title={t("Proposed for the reviewers, in no rating: captures made without their own Medic on the point")}>
                  {t("Backcaps*")}
                </th>
              )}
              {anySniper && (
                <th className="num sx6-proposed" title={t("Proposed for the reviewers, in no rating: damage as Sniper on the enemy Medic, Demoman and pocket")}>
                  {t("Combo dmg*")}
                </th>
              )}
            </tr>
          </thead>
          <tbody>
            {players.map((p) => (
              <Row
                key={p.accountId}
                p={p}
                name={names.get(p.accountId) ?? String(p.accountId)}
                role={roleOf(p.accountId)}
                rating={ratings.length > 0 ? ratingOf(p.accountId) : undefined}
                cols={[anySticky, anyReflect, anySentry, anySaps, anyDenies, hasPicks, hasProposed, anySniper]}
                f={f}
              />
            ))}
          </tbody>
        </table>
      </div>
      {hasProposed && <p className="hint">{t("* Proposed definitions for the reviewers to argue: shown here, counted in no rating until agreed.")}</p>}
      {s.medics.length === 2 && <MedicDuel a={s.medics.find((m) => m.team === order[0]) ?? s.medics[0]} b={s.medics.find((m) => m.team === order[1]) ?? s.medics[1]} names={names} />}
    </section>
  );
}

function Row({
  p,
  name,
  role,
  rating,
  cols,
  f,
}: {
  p: SixesPlayerStats;
  name: string;
  role: { short: string; long: string } | null;
  /** Undefined when no rating is shown at all; null for a player not rated. */
  rating: SixesRatingRow | null | undefined;
  cols: [boolean, boolean, boolean, boolean, boolean, boolean, boolean, boolean];
  f: (v: number | null, digits?: number) => string;
}) {
  return (
    <tr className={`row-${p.team.toLowerCase()}`}>
      <td className={`sb-team sb-team-${p.team.toLowerCase()}`}>
        <span className={`mx-colour mx-colour-${p.team.toLowerCase()}`}>{teamLabel(p.team as never)}</span>
      </td>
      <td className="nowrap">
        {p.mainClass && <ClassIcon cls={p.mainClass} size={16} />} {name}
        {role && (
          <span className="role-tag" title={role.long}>
            {role.short}
          </span>
        )}
      </td>
      {rating !== undefined && (
        <td className="num" title={rating ? partsText(rating.parts) : undefined}>
          {rating ? <strong>{rating.rating.toFixed(2)}</strong> : "–"}
        </td>
      )}
      <td className="num">{f(p.dpm)}</td>
      <td className="num muted">{f(p.dtm)}</td>
      <td className="num">{f(p.damageRatio, 2)}</td>
      <td className="num">{p.airshots ?? "–"}</td>
      <td className="num">
        {p.deaths}
        <span className="muted">{p.medicDeaths !== null ? ` · ${p.medicDeaths}` : ""}</span>
      </td>
      <td className="num">{p.medicPicks ?? "–"}</td>
      {cols[5] && <td className="num">{p.picks ?? "–"}</td>}
      {cols[0] && <td className="num">{p.stickyKills || ""}</td>}
      {cols[1] && <td className="num">{p.reflectKills || ""}</td>}
      {cols[2] && <td className="num">{p.sentryKills || ""}</td>}
      {cols[3] && <td className="num">{p.saps || ""}</td>}
      {cols[4] && <td className="num">{p.denies || ""}</td>}
      {cols[6] && <td className="num sx6-proposed">{p.forcesProposed || ""}</td>}
      {cols[6] && <td className="num sx6-proposed">{p.backcapsProposed || ""}</td>}
      {cols[7] && <td className="num sx6-proposed">{p.sniperComboDamage ?? ""}</td>}
    </tr>
  );
}

/** A rating's breakdown for its tooltip: each component's percentile and share. */
function partsText(parts: string): string {
  try {
    const rows = JSON.parse(parts) as Array<[string, number, number]>;
    return rows.map(([c, pct, share]) => `${c.replace(/_/g, " ")}: ${Math.round(pct)} (${Math.round(share * 100)}%)`).join("\n");
  } catch {
    return "";
  }
}

/** The two Medics' numbers side by side, the better of each marked. */
function MedicDuel({ a, b, names }: { a: SixesMedicStats; b: SixesMedicStats; names: Map<number, string> }) {
  const rows: Array<[string, number | null, number | null, boolean]> = [
    [t("Übers"), a.ubers, b.ubers, true],
    [t("Drops"), a.drops, b.drops, false],
    [t("Deaths"), a.deaths, b.deaths, false],
    [t("Healing per minute"), a.healPerMin, b.healPerMin, true],
    [t("Advantages lost"), a.advantagesLost, b.advantagesLost, false],
    [t("Deaths within 20 s of an über"), a.deathsAfterUber, b.deathsAfterUber, false],
    [t("Seconds to build an über"), a.avgBuildS, b.avgBuildS, false],
  ];
  const better = (x: number | null, y: number | null, high: boolean) => x !== null && y !== null && x !== y && (high ? x > y : x < y);
  return (
    <div className="sx6-duel">
      <h3>{t("Medic against Medic")}</h3>
      <div className="sx6-duel-grid">
        <span className={`sx6-duel-name team-${a.team.toLowerCase()}`}>{names.get(a.accountId) ?? a.accountId}</span>
        <span />
        <span className={`sx6-duel-name team-${b.team.toLowerCase()}`}>{names.get(b.accountId) ?? b.accountId}</span>
        {rows.map(([label, x, y, high]) => (
          <MedicLine key={label} label={label} x={x} y={y} bx={better(x, y, high)} by={better(y, x, high)} />
        ))}
      </div>
      <p className="hint">{t("The better of each in bold. From logs.tf's Medic stats.")}</p>
    </div>
  );
}

function MedicLine({ label, x, y, bx, by }: { label: string; x: number | null; y: number | null; bx: boolean; by: boolean }) {
  const v = (n: number | null) => (n === null ? "–" : Number.isInteger(n) ? String(n) : n.toFixed(1));
  return (
    <>
      <span className={bx ? "num sx6-duel-v on" : "num sx6-duel-v"}>{v(x)}</span>
      <span className="sx6-duel-label muted">{label}</span>
      <span className={by ? "num sx6-duel-v on" : "num sx6-duel-v"}>{v(y)}</span>
    </>
  );
}
