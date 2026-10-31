import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglTeam } from "../../api/types";
import { t } from "../../lib/i18n";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import "../etf2l/etf2l.css";
import "./rgl.css";

/** What the full tab will add (PLAN §30), shown until it is built. */
const coming = (): [string, string][] => [
  [t("Matches"), t("Results and the schedule by week and division, with each match's logs.")],
  [t("Players"), t("RGL careers season by season, bans and probation, and the current team.")],
  [t("Teams"), t("Roster history, results, win rate per map and the team across seasons.")],
  [t("Standings"), t("Division tables worked out from the results, and the playoffs.")],
  [t("Bans"), t("The league's list.")],
  [t("Through the app"), t("RGL on profiles and the scoreboard, and RGL officials counted as officials.")],
];

/**
 * The RGL tab, a first and rudimentary page (Flashy: "make a very
 * rudimentary rgl page"): NA Highlander's seasons, a season's map pool and
 * its divisions' teams, read live from RGL's API. The rest is listed as
 * coming soon.
 */
export function RglPage() {
  const seasons = useQuery({ queryKey: ["rgl_seasons"], queryFn: api.getRglSeasons, staleTime: 30 * 60_000 });
  const [picked, setPicked] = useState<number | null>(null);
  useEffect(() => {
    if (picked === null && seasons.data?.length) setPicked(seasons.data[0].id);
  }, [picked, seasons.data]);
  const season = useQuery({
    queryKey: ["rgl_season", picked],
    queryFn: () => api.getRglSeason(picked!),
    enabled: picked !== null,
    staleTime: 30 * 60_000,
  });

  return (
    <div className="content e2-page">
      <header className="e2-head">
        <div className="e2-brand">
          <span className="e2-eyebrow">{t("RGL Highlander")}</span>
          <span className="muted">{t("North America")}</span>
        </div>
        {seasons.data && (
          <label className="rgl-pick">
            <span className="muted">{t("Season")}</span>
            <select value={picked ?? ""} onChange={(e) => setPicked(Number(e.target.value))}>
              {seasons.data.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </label>
        )}
      </header>

      <div className="e2-home">
        <section className="panel e2-panel">
          {seasons.error ? (
            <ApiError error={seasons.error} />
          ) : season.error ? (
            <ApiError error={season.error} />
          ) : !season.data ? (
            <Loading>{t("Loading the season…")}</Loading>
          ) : (
            <>
              <div className="e2-panel-head">
                <h2>{season.data.name}</h2>
                <span className="muted">{t("{0} teams", { "0": season.data.divisions.reduce((n, d) => n + d.teams.length, 0) })}</span>
              </div>
              {season.data.maps.length > 0 && (
                <div className="rgl-maps">
                  {season.data.maps.map((m) => (
                    <span key={m} className="rgl-map">
                      {m}
                    </span>
                  ))}
                </div>
              )}
              {season.data.divisions.length === 0 && <p className="hint">{t("No divisions yet.")}</p>}
              <div className="rgl-divs">
                {season.data.divisions.map((d) => (
                  <div key={d.name} className="rgl-div">
                    <div className="rgl-div-head">
                      <strong>{d.name}</strong>
                      {d.tier && <span className="muted" title={t("Its place on the ETF2L ladder, for comparing")}>≈ {t(d.tier)}</span>}
                    </div>
                    {d.teams.length === 0 ? (
                      <p className="hint">{t("No teams.")}</p>
                    ) : (
                      <ol className="rgl-teams">
                        {d.teams.map((tm) => (
                          <TeamRow key={tm.id} team={tm} />
                        ))}
                      </ol>
                    )}
                  </div>
                ))}
              </div>
            </>
          )}
        </section>

        <aside className="e2-rail">
          <section className="panel e2-panel">
            <div className="e2-panel-head">
              <h2>{t("Coming soon")}</h2>
            </div>
            <ul className="rgl-coming">
              {coming().map(([what, text]) => (
                <li key={what}>
                  <strong>{what}</strong>
                  <span className="muted">{text}</span>
                </li>
              ))}
            </ul>
            <p className="hint">{t("From RGL's public API, api.rgl.gg.")}</p>
          </section>
        </aside>
      </div>
    </div>
  );
}

function TeamRow({ team }: { team: RglTeam }) {
  const out = team.finalRank === 99 || team.teamStatus === "Dead";
  const place = team.finalRank !== null && !out ? `#${team.finalRank}` : team.seeding !== null ? t("seed {0}", { "0": team.seeding }) : "";
  return (
    <li className={out ? "rgl-team out" : "rgl-team"}>
      <span className="rgl-rank">{place}</span>
      <span className="rgl-name">{team.name}</span>
      {team.tag && <span className="rgl-tag">{team.tag}</span>}
      {out && <span className="rgl-out">{t("dropped out")}</span>}
    </li>
  );
}
