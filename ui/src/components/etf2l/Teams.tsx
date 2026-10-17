import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { TeamEra } from "../../api/types";
import { rating } from "../../lib/format";
import { locale, t, tx } from "../../lib/i18n";
import { DivisionBadge } from "../players/PlayerProfile";
import { seasonTitle, TeamAvatar } from "../teams/TeamsPage";
import { useYourTeams } from "../teams/YourTeams";
import { tierOf } from "./Brackets";

/**
 * The ETF2L tab's Teams (Flashy: "looks very discombobulated… come up with a
 * nice design"): a search beside the title, your teams as cards with their
 * record drawn as a bar, then every team of the current season by division,
 * the way ETF2L lists a season's teams.
 */

/** "Jul 2023 – Mar 2026", or one month. */
function span(e: TeamEra): string {
  const month = (unix: number) => new Date(unix * 1000).toLocaleDateString(locale(), { month: "short", year: "numeric" });
  const a = month(e.firstPlayed);
  const b = month(e.lastPlayed);
  return a === b ? b : `${a} – ${b}`;
}

/** A team with no avatar: its initials on a tile. */
function Monogram({ name }: { name: string }) {
  const letters = name
    .replace(/[^\p{L}\p{N} ]/gu, "")
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0])
    .join("")
    .toUpperCase();
  return <span className="tm-mono">{letters || "?"}</span>;
}

export function TeamsLanding({ onTeam }: { onTeam: (id: number) => void }) {
  const [q, setQ] = useState("");
  const term = q.trim();
  const hits = useQuery({ queryKey: ["search_teams", term], queryFn: () => api.searchTeams(term), enabled: term.length >= 2, placeholderData: keepPreviousData });
  const tiles = useQuery({ queryKey: ["seasons_overview"], queryFn: api.getSeasonsOverview, staleTime: 5 * 60_000 }).data;
  const latest = tiles?.[0] ?? null;
  const tables = useQuery({ queryKey: ["leagues", latest?.season ?? null], queryFn: () => api.getLeagues(latest!.season), enabled: latest !== null, staleTime: 5 * 60_000 }).data;
  // Avatars for your teams, where the season's tables have them.
  const avatars = new Map((tables?.divisions ?? []).flatMap((d) => d.teams.map((x) => [x.teamId, x.avatar] as const)));
  const yours = useYourTeams().filter((e) => e.games >= 3);
  const divisions = (tables?.divisions ?? []).filter((d) => d.teams.length > 0).sort((a, b) => tierOf(a.division) - tierOf(b.division));

  return (
    <>
      <header className="tm-head">
        <div className="ts-pagehead">
          <span className="ts-eyebrow">{t("ETF2L Highlander")}</span>
          <h1>{t("Teams")}</h1>
        </div>
        <div className="e2-search tm-search">
          <input type="search" value={q} placeholder={t("Search any team by name")} onChange={(e) => setQ(e.target.value)} aria-label={t("Search teams")} autoFocus />
        </div>
      </header>

      {term.length >= 2 && (
        <section className="tm-section">
          <h2 className="ts-section">{tx("Teams matching “{0}”", { "0": term })}</h2>
          {(hits.data ?? []).length === 0 && !hits.isFetching ? (
            <p className="hint">{t("No team by that name.")}</p>
          ) : (
            <div className="tm-grid">
              {(hits.data ?? []).map((x) => (
                <button key={x.teamId} className="tm-tile" onClick={() => onTeam(x.teamId)}>
                  {x.avatar ? <TeamAvatar src={x.avatar} /> : <Monogram name={x.name} />}
                  <span className="tm-tile-text">
                    <strong>{x.name}</strong>
                    <span className="muted">
                      {x.lastSeason !== null ? t("last in Season {0}", { "0": x.lastSeason }) : null}
                      {x.lastSeason !== null && " · "}
                      {tx("{0} officials", { "0": x.officials })}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          )}
        </section>
      )}

      {yours.length > 0 && (
        <section className="tm-section">
          <h2 className="ts-section">{t("Your teams")}</h2>
          <div className="tm-yours">
            {yours.map((e) => {
              const total = e.wins + e.losses;
              const wr = total > 0 ? (e.wins / total) * 100 : null;
              const avatar = avatars.get(e.teamId) ?? null;
              return (
                <button key={e.teamId} className="tm-card" onClick={() => onTeam(e.teamId)}>
                  <span className="tm-card-top">
                    {avatar ? <TeamAvatar src={avatar} /> : <Monogram name={e.name} />}
                    <span className="tm-card-name">
                      <strong>{e.name || t("Unnamed team")}</strong>
                      <span className="muted">{span(e)}</span>
                    </span>
                    {e.myAvg !== null && (
                      <span className="tm-rating" title={t("Your average rating on this team")}>
                        {rating(e.myAvg)}
                      </span>
                    )}
                  </span>
                  <span className="tm-bar" aria-hidden>
                    <span className="tm-bar-w" style={{ width: `${wr ?? 0}%` }} />
                  </span>
                  <span className="tm-card-stats">
                    <span>
                      <strong>
                        {e.wins}–{e.losses}
                      </strong>
                      {wr !== null && <em> {wr.toFixed(0)}%</em>}
                    </span>
                    <span className="muted">{tx("{0} games", { "0": e.games })}</span>
                    {e.officials > 0 && <span className="muted">{tx("{0} officials", { "0": e.officials })}</span>}
                  </span>
                </button>
              );
            })}
          </div>
        </section>
      )}

      {latest && divisions.length > 0 && (
        <section className="tm-section">
          <h2 className="ts-section">{tx("Teams in {0}", { "0": seasonTitle(latest) })}</h2>
          <div className="tm-divs">
            {divisions.map((d) => (
              <section key={d.division} className="panel tm-div">
                <div className="tm-div-head">
                  <DivisionBadge d={{ name: d.division, tier: Math.floor(tierOf(d.division)) }} />
                  <span className="muted">{tx("{0} teams", { "0": d.teams.length })}</span>
                </div>
                <ol className="tm-div-list">
                  {d.teams.map((x, i) => (
                    <li key={x.teamId}>
                      <button className="tm-row" onClick={() => onTeam(x.teamId)}>
                        <span className="tm-place">{i + 1}</span>
                        {x.avatar && !x.avatar.endsWith("/") ? <TeamAvatar src={x.avatar} /> : <Monogram name={x.name} />}
                        <span className="tm-row-name">{x.name}</span>
                        <span className="tm-row-wl">
                          {x.record.won}–{x.record.lost}
                        </span>
                      </button>
                    </li>
                  ))}
                </ol>
              </section>
            ))}
          </div>
        </section>
      )}
    </>
  );
}
