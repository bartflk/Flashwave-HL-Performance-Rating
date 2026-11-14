import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { t, tx } from "../../lib/i18n";
import { roleLabel } from "../../lib/gameformat";
import { classLabel } from "../analysis/common";
import { formatDate } from "../../lib/format";
import { DivisionBadge } from "../players/PlayerProfile";
import { tierOf } from "../etf2l/Brackets";
import "./sixes.css";

const STEAM_BASE = 76561197960265728n;

/**
 * The Profile tab in 6v6 (the 6v6 plan; the full profile is P7). For now:
 * your preview ratings by role and off-class, never averaged with your
 * Highlander ones (question 9), and how much of your 6v6 history is in.
 */
export function SixesSoon({ steamid, onOpenMatch }: { steamid: string | null; onOpenMatch: (logId: number) => void }) {
  const profile = useQuery({ queryKey: ["sixes_profile"], queryFn: api.getSixesProfile }).data;
  const stats = useQuery({ queryKey: ["index_stats"], queryFn: api.indexStats });
  const accountId = steamid ? Number(BigInt(steamid) - STEAM_BASE) : null;
  const ratings = useQuery({
    queryKey: ["sixes_ratings_of", accountId],
    queryFn: () => api.getSixesRatings({ accountId: accountId! }),
    enabled: accountId !== null,
  }).data ?? [];
  const byRole = useMemo(() => {
    const m = new Map<string, number[]>();
    for (const r of ratings) m.set(r.role, [...(m.get(r.role) ?? []), r.rating]);
    return [...m.entries()].map(([role, xs]) => ({ role, games: xs.length, avg: xs.reduce((a, b) => a + b, 0) / xs.length })).sort((a, b) => b.games - a.games);
  }, [ratings]);
  const s = stats.data;
  const label = (role: string) => roleLabel(role)?.long ?? (role.startsWith("off:") ? tx("{0}, off-class", { "0": classLabel(role.slice(4)) }) : classLabel(role));
  return (
    <div className="content sx-soon">
      <section className="panel sx-soon-panel">
        <span className="ts-eyebrow">{t("6v6")}</span>
        <h1 className="sx-title">{t("Your 6v6 profile")}</h1>
        {byRole.length > 0 ? (
          <>
            <p className="hint">{t("Preview ratings: each role weighed against the other players in your stored 6v6 logs, without round impact yet. The 6v6 rating replaces them once it is fitted on the league.")}</p>
            <table className="match-table sx-roles">
              <thead>
                <tr>
                  <th>{t("Role")}</th>
                  <th className="num">{t("Games")}</th>
                  <th className="num">{t("Average")}</th>
                </tr>
              </thead>
              <tbody>
                {byRole.map((r) => (
                  <tr key={r.role}>
                    <td>{label(r.role)}</td>
                    <td className="num muted">{r.games}</td>
                    <td className="num">
                      <strong>{r.avg.toFixed(2)}</strong>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        ) : (
          <p>{t("Your 6v6 career will be here: ratings by role and off-class, seasons, divisions and best games, next to your Highlander career and never averaged with it.")}</p>
        )}
        {profile && profile.best.length > 0 && (
          <>
            <h2 className="ts-section">{t("Best games")}</h2>
            <ul className="sx-best">
              {profile.best.map((g) => (
                <li key={g.logId}>
                  <button className="linkish sx-best-go" onClick={() => onOpenMatch(g.logId)}>
                    <strong className="sx-best-rating">{g.rating.toFixed(2)}</strong>
                    <span>{g.map ?? t("Unknown map")}</span>
                    <span className="muted">{roleLabel(g.role)?.short ?? label(g.role)}</span>
                    <span className="muted">{g.playedAt ? formatDate(g.playedAt) : ""}</span>
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}
        {profile && profile.seasons.length > 0 && (
          <>
            <h2 className="ts-section">{t("ETF2L 6v6 seasons")}</h2>
            <table className="match-table sx-roles">
              <thead>
                <tr>
                  <th>{t("Season")}</th>
                  <th>{t("Division")}</th>
                  <th>{t("Team")}</th>
                  <th className="num" title={t("Matches won, drawn and lost")}>{t("W–D–L")}</th>
                </tr>
              </thead>
              <tbody>
                {profile.seasons.map((l) => (
                  <tr key={`${l.season}-${l.division}-${l.team}`}>
                    <td>{tx("Season {0}", { "0": l.season })}</td>
                    <td>
                      <DivisionBadge d={{ name: l.division, tier: Math.floor(tierOf(l.division)) }} />
                    </td>
                    <td>{l.team}</td>
                    <td className="num">
                      <span className="team-won">{l.won}</span>–<span className="muted">{l.drawn}</span>–<span className="team-lost">{l.lost}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        )}
        {s && (
          <p className="muted">
            {tx("{0} of your {1} 6v6 logs are downloaded. Matches › 6v6 lists them.", { "0": s.sixesFetched.toLocaleString(), "1": s.sixes.toLocaleString() })}
          </p>
        )}
        <p className="hint">{t("Switch back to Highlander from the menu on this tab.")}</p>
      </section>
    </div>
  );
}
