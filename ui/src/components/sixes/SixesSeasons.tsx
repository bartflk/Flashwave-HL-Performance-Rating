import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { ResultRow } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { formatDate } from "../../lib/format";
import { Loading } from "../Spinner";
import { DivisionBadge } from "../players/PlayerProfile";
import { tierOf } from "../etf2l/Brackets";

/**
 * The 6v6 seasons the sync has read (the 6v6 plan, P2): the newest and the
 * two before it, each with its division tables from the stored results,
 * its playoffs and the Nations' Cup. Read by a sync once setup has 6v6
 * ticked; until then it says so.
 */
export function SixesSeasons({ onTeam }: { onTeam: (id: number) => void }) {
  const [season, setSeason] = useState<number | null>(null);
  const view = useQuery({ queryKey: ["leagues", "sixes", season], queryFn: () => api.getLeagues(season ?? undefined, "sixes"), staleTime: 5 * 60_000 });
  const v = view.data;
  const chosen = v?.season?.season ?? null;
  const results = useQuery({
    queryKey: ["season_results", "sixes", chosen],
    queryFn: () => api.getSeasonResults(chosen!, "sixes"),
    enabled: chosen !== null,
    staleTime: 5 * 60_000,
  }).data;
  // Playoffs and third-place matches by division, and the Nations' Cup.
  const later = useMemo(() => {
    const by = new Map<string, ResultRow[]>();
    for (const r of results ?? []) {
      if (r.stage === "regular") continue;
      const k = r.stage === "Cup" ? r.competition : tx("{0}: {1}", { "0": r.division ?? t("Playoffs"), "1": r.stage }) as string;
      by.set(k, [...(by.get(k) ?? []), r]);
    }
    return [...by.entries()];
  }, [results]);

  if (view.isPending) return <Loading>{t("Loading seasons…")}</Loading>;
  if (!v || v.seasons.length === 0)
    return <p className="hint">{t("No 6v6 seasons read yet. A sync reads the last year of ETF2L 6v6 once setup has 6v6 ticked: every division, team and result.")}</p>;
  return (
    <div className="sx-matches">
      <div className="sx-week-bar">
        <div className="segmented" role="tablist" aria-label={t("Season")}>
          {v.seasons.map((s) => (
            <button key={s.season} role="tab" aria-selected={s.season === chosen} className={s.season === chosen ? "seg active" : "seg"} onClick={() => setSeason(s.season)}>
              {tx("Season {0}", { "0": s.season })}
            </button>
          ))}
        </div>
        {v.season && <span className="muted sx-week-count">{v.season.name}</span>}
      </div>
      {v.pendingDetails > 0 && <p className="hint sx-explain">{tx("{0} matches' pages are still to be read; their maps fill in over the next syncs.", { "0": v.pendingDetails })}</p>}
      <div className="sx-week-groups">
        {[...v.divisions]
          .sort((a, b) => tierOf(a.division) - tierOf(b.division) || a.division.localeCompare(b.division))
          .map((d) => (
            <section key={d.division} className="panel e2-panel">
              <div className="e2-panel-head">
                <h2 className="sx-div-name">
                  <DivisionBadge d={{ name: d.division, tier: Math.floor(tierOf(d.division)) }} />
                </h2>
                <span className="muted">{d.teams.length === 1 ? t("1 team") : tx("{0} teams", { "0": d.teams.length })}</span>
              </div>
              <div className="table-wrap">
                <table className="match-table sx-table">
                  <thead>
                    <tr>
                      <th className="num">#</th>
                      <th>{t("Team")}</th>
                      <th className="num" title={t("Matches won")}>{t("W")}</th>
                      <th className="num" title={t("Matches drawn")}>{t("D")}</th>
                      <th className="num" title={t("Matches lost")}>{t("L")}</th>
                      <th className="num" title={t("League points for and against, as ETF2L gives them")}>{t("Pts")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {d.teams.map((r, i) => (
                      <tr key={r.teamId}>
                        <td className="num muted">{i + 1}</td>
                        <td className="sx-team">
                          <button className="linkish sx-team-link" onClick={() => onTeam(r.teamId)}>
                            {r.name}
                          </button>
                        </td>
                        <td className="num team-won">{r.record.won}</td>
                        <td className="num muted">{r.record.drawn}</td>
                        <td className="num team-lost">{r.record.lost}</td>
                        <td className="num sx-pts">
                          {r.scoreFor}
                          <span className="muted">–{r.scoreAgainst}</span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>
          ))}
      </div>
      {later.length > 0 && (
        <>
          <h2 className="ts-section">{t("Playoffs and cups")}</h2>
          <div className="sx-week-groups">
            {later.map(([name, rows]) => (
              <section key={name} className="panel e2-panel">
                <div className="e2-panel-head">
                  <h2 className="sx-div-name">{name}</h2>
                  <span className="muted">{rows.length === 1 ? t("1 match") : tx("{0} matches", { "0": rows.length })}</span>
                </div>
                <ul className="sx-fixtures">
                  {rows.map((m) => {
                    const won = m.r1 !== null && m.r2 !== null && m.r1 !== m.r2 ? (m.r1 > m.r2 ? 1 : 2) : 0;
                    const cls = (side: 1 | 2) => (won === 0 ? "sx-fx-team" : won === side ? "sx-fx-team sx-fx-won" : "sx-fx-team sx-fx-lost");
                    return (
                      <li key={m.matchId} className="sx-fx">
                        <span className="sx-fx-when muted">{m.round ?? (m.time ? formatDate(m.time) : "")}</span>
                        <button className={cls(1)} onClick={() => onTeam(m.clan1.id)}>
                          <span className="sx-fx-name">{m.clan1.name}</span>
                        </button>
                        <span className="sx-fx-score">{m.r1 !== null && m.r2 !== null ? `${m.r1}–${m.r2}` : t("vs")}</span>
                        <button className={cls(2)} onClick={() => onTeam(m.clan2.id)}>
                          <span className="sx-fx-name">{m.clan2.name}</span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </section>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
