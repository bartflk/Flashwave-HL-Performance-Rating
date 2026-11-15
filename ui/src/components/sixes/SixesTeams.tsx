import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { SixesSeason } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { Loading } from "../Spinner";
import { DivisionBadge } from "../players/PlayerProfile";
import { tierOf } from "../etf2l/Brackets";
import { TeamScreen } from "../teams/TeamsPage";

/**
 * Every team of the current 6v6 season, by division (the 6v6 plan, P2),
 * each opening its team page.
 */
export function SixesTeamsList({ s, onTeam }: { s: SixesSeason; onTeam: (id: number) => void }) {
  const divisions = [...s.divisions].sort((a, b) => tierOf(a.name) - tierOf(b.name));
  return (
    <div className="sx-teams">
      {divisions.map((d) => (
        <section key={d.name} className="panel e2-panel">
          <div className="e2-panel-head">
            <h2 className="sx-div-name">
              <DivisionBadge d={{ name: d.name, tier: Math.floor(tierOf(d.name)) }} />
            </h2>
            <span className="muted">{d.teams.length === 1 ? t("1 team") : tx("{0} teams", { "0": d.teams.length })}</span>
          </div>
          <ul className="sx-team-list">
            {[...d.teams]
              .sort((a, b) => a.name.localeCompare(b.name))
              .map((r) => (
                <li key={r.id}>
                  <button className="linkish sx-team-link" onClick={() => onTeam(r.id)}>
                    {r.name}
                  </button>
                  {r.dropped && <span className="muted"> {t("dropped")}</span>}
                </li>
              ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

/**
 * One 6v6 team's page: the same page a Highlander team has, from the 6v6
 * seasons the sync reads for someone who plays 6v6. Until then, a link to
 * the team on ETF2L.
 */
export function SixesTeam({ teamId, onTeam, onBack }: { teamId: number; onTeam: (id: number) => void; onBack: () => void }) {
  const q = useQuery({ queryKey: ["team", teamId], queryFn: () => api.getTeam(teamId) });
  return (
    <div className="sx-team-page">
      <button className="linkish sx-back" onClick={onBack}>
        {t("‹ Back to the season")}
      </button>
      {q.isPending ? (
        <Loading>{t("Loading team…")}</Loading>
      ) : q.data ? (
        <TeamScreen teamId={teamId} onTeam={onTeam} sixes />
      ) : (
        <section className="panel sx-soon-panel">
          <p>{t("This team's 6v6 seasons are not read yet. They are read by a sync once setup has 6v6 ticked.")}</p>
          <a className="sx-ext" href={`https://etf2l.org/teams/${teamId}/`} target="_blank" rel="noreferrer">
            {t("Open on ETF2L")}
          </a>
        </section>
      )}
    </div>
  );
}
