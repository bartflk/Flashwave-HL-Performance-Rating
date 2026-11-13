import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglSeasonItem } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { decided, weekOf, standings } from "../../lib/rglstandings";
import { useShowResults } from "../../lib/spoilers";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { MedalGlyph } from "../players/MedalGlyph";
import { BookmarkButton } from "../Bookmarks";
import { KEEP, useSeasons, useSeason, placing } from "./data";
import { Reading, WeekLine } from "./bits";

export function SeasonList({ onSeason }: { onSeason: (id: number) => void }) {
  const q = useSeasons();
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Seasons")}</h2>
        <span className="muted">{t("NA Highlander")}</span>
      </div>
      {q.error ? (
        <ApiError error={q.error} />
      ) : !q.data ? (
        <Loading>{t("Loading the seasons…")}</Loading>
      ) : q.data.length === 0 ? (
        <p className="hint">{t("RGL lists no seasons.")}</p>
      ) : (
        <div className="ts-grid rgl-season-grid">
          {q.data.map((s, i) => (
            <SeasonCard key={s.id} s={s} latest={i === 0} onOpen={() => onSeason(s.id)} />
          ))}
        </div>
      )}
    </section>
  );
}

/** A season's card: its best division's top three, or that it is played. */
export function SeasonCard({ s, latest, onOpen }: { s: RglSeasonItem; latest: boolean; onOpen: () => void }) {
  const q = useQuery({ queryKey: ["rgl_season_card", s.id], queryFn: () => api.getRglSeasonCard(s.id), staleTime: KEEP, retry: false });
  const c = q.data;
  // Whether a match has been played, from the season's matches if Home or
  // Matches has read them already; never a read of its own for the card.
  const read = useQuery({ queryKey: ["rgl_season_matches", s.id], queryFn: () => api.getRglSeasonMatches(s.id), enabled: false, staleTime: KEEP }).data;
  const notStarted = !!read && !read.some(decided);
  // A season still being played has no podium; "Latest" belongs on it alone.
  const live = !!c && c.podium.length === 0;
  const number = s.name.match(/\d+/)?.[0];
  const place = ["", t("1st"), t("2nd"), t("3rd")];
  return (
    <button className="ts-card tn-card rgl-sc" onClick={onOpen} aria-label={t("Open {0}", { "0": s.name })}>
      <span className="ts-art">
        <span className="ts-art-img" aria-hidden />
        <span className="ts-art-label">{number ? `S${number}` : "RGL"}</span>
        {latest && live && (
          <span className="ts-chips">
            <span className="ts-chip">{t("Latest")}</span>
          </span>
        )}
      </span>
      <span className="ts-body">
        <span className="ts-title">{s.name}</span>
        <span className="ts-stats">
          {c && c.teams > 0 && <span className="ts-stat">{c.division
                ? c.teams === 1 ? tx("1 team in {0}", { "0": c.division }) : tx("{0} teams in {1}", { "0": c.teams, "1": c.division })
                : c.teams === 1 ? t("1 team") : tx("{0} teams", { "0": c.teams })}</span>}
          {c && c.maps > 0 && <span className="ts-stat">{tx("{0} maps", { "0": c.maps })}</span>}
        </span>
        <span className="ts-rows">
          {q.error ? (
            <span className="ts-row">
              <span className="ts-row-value muted">{t("RGL isn't answering right now.")}</span>
            </span>
          ) : !c ? (
            <span className="ts-row">
              <span className="ts-row-value muted">{t("Loading…")}</span>
            </span>
          ) : c.podium.length === 0 ? (
            <span className="ts-row">
              <span className="ts-row-label">{t("Season")}</span>
              <span className="ts-row-value muted">{notStarted ? t("not started") : t("being played")}</span>
            </span>
          ) : (
            c.podium.map((tm) => (
              <span key={tm.id} className="ts-row ts-row-champ">
                <span className="ts-row-label">{place[tm.finalRank ?? 0]}</span>
                <span className="ts-row-value">
                  <MedalGlyph size={16} place={tm.finalRank ?? 1} />
                  <strong title={tm.name}>{tm.name}</strong>
                </span>
              </span>
            ))
          )}
        </span>
        <span className="ts-open">
          {t("Open season")}
          <span className="ts-open-arrow" aria-hidden>
            ›
          </span>
        </span>
      </span>
    </button>
  );
}

export function SeasonView({ id, onTeam, more }: { id: number; onTeam: (id: number) => void; more?: () => void }) {
  const q = useSeason(id);
  const s = q.data;
  return (
    <section className="panel e2-panel">
      {q.error ? (
        <ApiError error={q.error} />
      ) : !s ? (
        <Loading>{t("Loading the season…")}</Loading>
      ) : (
        <>
          <div className="e2-panel-head">
            <div className="e2-titles">
              <h2>
                {s.name} <BookmarkButton b={{ kind: "rgl_season", id: s.id, label: s.name, sub: t("RGL Highlander") }} />
              </h2>
              <span className="muted">{(() => {
                const n = s.divisions.reduce((sum, d) => sum + d.teams.length, 0);
                return n === 1 ? t("1 team") : t("{0} teams", { "0": n });
              })()}</span>
            </div>
            {more && (
              <button className="e2-more" onClick={more}>
                {t("All seasons")} ›
              </button>
            )}
          </div>
          {s.maps.length > 0 && (
            <div className="rgl-maps">
              {s.maps.map((m) => (
                <span key={m} className="rgl-map">
                  {m}
                </span>
              ))}
            </div>
          )}
          {s.divisions.length === 0 && <p className="hint">{t("No divisions yet.")}</p>}
          <div className="rgl-divs">
            {s.divisions.map((d) => (
              <div key={d.name} className="rgl-div">
                <div className="rgl-div-head">
                  <strong>{d.name}</strong>
                  {d.tier && <span className="muted" title={t("Its place on the ETF2L ladder, for comparing")}>≈ {t(d.tier)}</span>}
                </div>
                {d.teams.length === 0 ? (
                  <p className="hint">{t("No teams.")}</p>
                ) : (
                  <ol className="rgl-teams">
                    {d.teams.map((tm) => {
                      const out = tm.finalRank === 99 || tm.teamStatus === "Dead";
                      return (
                        <li key={tm.id}>
                          <button className={out ? "rgl-team out" : "rgl-team"} onClick={() => onTeam(tm.id)}>
                            <span className="rgl-rank">
                              {out ? "" : tm.finalRank !== null && tm.finalRank <= 3 ? <MedalGlyph size={16} place={tm.finalRank} /> : placing(tm.finalRank, tm.seeding)}
                            </span>
                            <span className="rgl-name" title={tm.name}>{tm.name}</span>
                            {tm.tag && <span className="rgl-tag">{tm.tag}</span>}
                            {out && <span className="rgl-out">{t("dropped out")}</span>}
                          </button>
                        </li>
                      );
                    })}
                  </ol>
                )}
              </div>
            ))}
          </div>
        </>
      )}
    </section>
  );
}

// ---- A season's standings and results ----------------------------------------

/**
 * A season's standings and results (Flashy: "continue work on the RGL
 * page"). RGL gives only a final rank, so the tables are worked out from
 * the matches, each read on its own at RGL's pace: about 40 s the first
 * time, then kept for half an hour.
 */
export function SeasonPlay({ id, onTeam, onMatch }: { id: number; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const season = useSeason(id).data;
  const [shown] = useShowResults();
  const q = useQuery({ queryKey: ["rgl_season_matches", id], queryFn: () => api.getRglSeasonMatches(id), staleTime: KEEP, retry: false });
  const matches = useMemo(() => q.data ?? [], [q.data]);
  const tables = useMemo(() => standings(matches), [matches]);
  const weeks = useMemo(() => [...new Set(matches.map(weekOf))], [matches]);
  // Open on the week being played: the first with a match not yet decided.
  const live = weeks.find((w) => matches.some((m) => weekOf(m) === w && !decided(m)));
  const current = live ?? weeks[weeks.length - 1];
  const [picked, setPicked] = useState<string | null>(null);
  const week = picked && weeks.includes(picked) ? picked : current;
  const order = (season?.divisions ?? []).map((d) => d.name);
  const divs = [...new Set([...order, ...matches.map((m) => m.divisionName ?? "")])].filter((d) => d && (tables.has(d) || matches.some((m) => m.divisionName === d)));
  if (q.error) return <ApiError error={q.error} />;
  if (q.isPending) return <Reading />;
  if (matches.length === 0) return null;
  const inWeek = matches.filter((m) => weekOf(m) === week);
  return (
    <>
      <h2 className="ts-section">{t("Standings")}</h2>
      {!shown && <p className="hint">{t("The standings are results, so they are hidden. Turn on Show results to see them.")}</p>}
      <div className="rgl-standings">
        {divs
          .filter((d) => shown && tables.has(d))
          .map((d) => (
            <section key={d} className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{d}</h2>
                <span className="muted" title={t("RGL ranks by points, which its API does not give: this table is by wins and losses, from the regular season's results.")}>{t("by W-L")}</span>
              </div>
              <table className="match-table rgl-table">
                <thead>
                  <tr>
                    <th className="num">#</th>
                    <th>{t("Team")}</th>
                    <th className="num" title={t("Played")}>
                      {t("P")}
                    </th>
                    <th className="num">{t("W")}</th>
                    <th className="num">{t("L")}</th>
                    <th className="num" title={t("Rounds won and lost")}>
                      {t("Rounds")}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {tables.get(d)!.map((r, i) => (
                    <tr key={r.id}>
                      <td className="num muted">{i + 1}</td>
                      <td>
                        <button className="linkish" onClick={() => onTeam(r.id)}>
                          {r.name}
                        </button>
                      </td>
                      <td className="num">{r.played}</td>
                      <td className="num team-won">{r.won}</td>
                      <td className="num team-lost">{r.lost}</td>
                      <td className="num muted">
                        {r.for}:{r.against}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          ))}
      </div>
      <h2 className="ts-section">{t("Results")}</h2>
      <div className="e2-rounds" role="tablist" aria-label={t("Week")}>
        {weeks.map((w) => (
          <button key={w} role="tab" aria-selected={w === week} className={w === week ? "on" : undefined} onClick={() => setPicked(w)}>
            {w}
            {w === live && <span className="e2-round-now" aria-hidden />}
          </button>
        ))}
      </div>
      <div className="e2-round-grid">
        {divs
          .filter((d) => inWeek.some((m) => m.divisionName === d))
          .map((d) => {
            const list = inWeek.filter((m) => m.divisionName === d);
            return (
              <section key={d} className="panel e2-panel">
                <div className="e2-panel-head">
                  <h2>{d}</h2>
                  <span className="muted">{list.length === 1 ? t("1 match") : tx("{0} matches", { "0": list.length })}</span>
                </div>
                <ol className="rgl-results">
                  {list.map((m) => (
                    <WeekLine key={m.matchId} m={m} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} />
                  ))}
                </ol>
              </section>
            );
          })}
      </div>
    </>
  );
}
