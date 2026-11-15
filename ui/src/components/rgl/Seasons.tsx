import { divisionRows } from "../../lib/rglseeds";
import { useMemo, useState } from "react";
import { useQuery, useQueries } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglMatch, RglSeasonCard, RglSeasonItem } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { decided, weekOf, standings } from "../../lib/rglstandings";
import { useShowResults } from "../../lib/spoilers";
import { champions, mapStats, orderSeasons, playedCount, seasonStatus, spanLabel, type MapRow } from "../../lib/rglseasons";
import { useSort, SortTh } from "../../lib/sort";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { MedalGlyph } from "../players/MedalGlyph";
import { BookmarkButton } from "../Bookmarks";
import { KEEP, useSeasons, useSeason, placing, day } from "./data";
import { Reading, WeekLine } from "./bits";
import { StatCard } from "./Team";

/** A season's card, read once and kept for half an hour (and on disk for good once the season is over). */
/** `full`: the season's own page, which reads its dates and every division
 *  (and keeps them); the list asks without, so a cold list costs no more than
 *  it did (hl_ingest::rgl::season_card). */
const cardQuery = (id: number, full = false) => ({ queryKey: ["rgl_season_card", id, full], queryFn: () => api.getRglSeasonCard(id, full), staleTime: KEEP, retry: false });

/** A date from unix seconds, as the tab shows dates. */
const dayOf = (secs: number) => day(new Date(secs * 1000).toISOString());

/** How a season's dates read: its first and last match, when it starts, or the week it is in. */
function spanText(c: RglSeasonCard, now = Date.now() / 1000): string {
  const span = spanLabel(c, now);
  switch (span.kind) {
    case "range":
      return `${dayOf(span.from)} – ${dayOf(span.to)}`;
    case "starts":
      return t("starts {0}", { "0": dayOf(span.at) });
    case "ongoing":
      return span.week !== null ? t("ongoing, week {0}", { "0": span.week }) : t("ongoing");
    default:
      return t("not scheduled yet");
  }
}

const statusWord = (c: RglSeasonCard) => ({ ongoing: t("Ongoing"), upcoming: t("Upcoming"), completed: t("Completed") })[seasonStatus(c, Date.now() / 1000)];

export function SeasonList({ onSeason }: { onSeason: (id: number) => void }) {
  const q = useSeasons();
  // The cards are read one season at a time as they are anyway (a finished
  // season's from disk); the list asks for them under the same keys so it can
  // put the seasons being played first, as tf2esports' Events page does.
  const cards = useQueries({ queries: (q.data ?? []).map((s) => cardQuery(s.id)) });
  const byId = new Map((q.data ?? []).map((s, i) => [s.id, cards[i]?.data] as const));
  const now = Date.now() / 1000;
  const seasons = orderSeasons(q.data ?? [], (s) => {
    const c = byId.get(s.id);
    return c ? seasonStatus(c, now) : undefined;
  });
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Seasons")}</h2>
        <span className="muted">{t("NA Highlander")}</span>
      </div>
      <p className="hint">{t("Seasons being played come first, then the finished ones. A season's dates are its first and last match, read from RGL once for each season.")}</p>
      {q.error ? (
        <ApiError error={q.error} />
      ) : !q.data ? (
        <Loading>{t("Loading the seasons…")}</Loading>
      ) : q.data.length === 0 ? (
        <p className="hint">{t("RGL lists no seasons.")}</p>
      ) : (
        <div className="ts-grid rgl-season-grid">
          {seasons.map((s) => (
            <SeasonCard key={s.id} s={s} onOpen={() => onSeason(s.id)} />
          ))}
        </div>
      )}
    </section>
  );
}

/** A season's card: its best division's top three, or that it is played. */
export function SeasonCard({ s, onOpen }: { s: RglSeasonItem; onOpen: () => void }) {
  const q = useQuery(cardQuery(s.id));
  const c = q.data;
  const [shown] = useShowResults();
  // Whether a match has been played, from the season's matches if Home or
  // Matches has read them already; never a read of its own for the card.
  const read = useQuery({ queryKey: ["rgl_season_matches", s.id], queryFn: () => api.getRglSeasonMatches(s.id), enabled: false, staleTime: KEEP }).data;
  const status = c ? seasonStatus(c, Date.now() / 1000) : null;
  const notStarted = status === "upcoming" || (!!read && !read.some(decided));
  const number = s.name.match(/\d+/)?.[0];
  const place = ["", t("1st"), t("2nd"), t("3rd")];
  return (
    <button className="ts-card tn-card rgl-sc" onClick={onOpen} aria-label={t("Open {0}", { "0": s.name })}>
      <span className="ts-art">
        <span className="ts-art-img" aria-hidden />
        <span className="ts-art-label">{number ? `S${number}` : "RGL"}</span>
        {c && status !== "completed" && (
          <span className="ts-chips">
            <span className="ts-chip">{statusWord(c)}</span>
          </span>
        )}
      </span>
      <span className="ts-body">
        <span className="ts-title">{s.name}</span>
        <span className="ts-stats">
          {c && c.totalTeams !== null && c.totalTeams > 0 ? (
            <span className="ts-stat" title={t("Teams across every division")}>
              {c.totalTeams === 1 ? t("1 team") : tx("{0} teams", { "0": c.totalTeams })}
              {/* t, not tx: tx gives an element, and in a string it read "[object Object]" (0.14.1 audit). */}
              {c.divisions > 1 && ` · ${t("{0} divisions", { "0": c.divisions })}`}
            </span>
          ) : (
            c && c.teams > 0 && (
              <span className="ts-stat">
                {c.division
                  ? c.teams === 1 ? tx("1 team in {0}", { "0": c.division }) : tx("{0} teams in {1}", { "0": c.teams, "1": c.division })
                  : c.teams === 1 ? t("1 team") : tx("{0} teams", { "0": c.teams })}
              </span>
            )
          )}
          {c && c.maps > 0 && <span className="ts-stat">{tx("{0} maps", { "0": c.maps })}</span>}
          {c && (
            <span className="ts-stat" title={t("Highlander is nine players a side, one on each class")}>
              {t("9v9 Highlander")}
              {c.mapsPerMatch ? ` · ${t("{0} maps a match", { "0": c.mapsPerMatch })}` : ""}
            </span>
          )}
        </span>
        <span className="ts-rows">
          {c && (
            <span className="ts-row">
              <span className="ts-row-label">{t("Dates")}</span>
              <span className="ts-row-value">{spanText(c)}</span>
            </span>
          )}
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
          ) : !shown ? (
            <span className="ts-row">
              <span className="ts-row-label">{t("Podium")}</span>
              <span className="ts-row-value muted">{t("hidden: results are off")}</span>
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
  const [shown] = useShowResults();
  const card = useQuery(cardQuery(id, true)).data;
  // The matches only if the season's page below has read them: this header
  // never asks for them (Home shows it without them).
  const read = useQuery({ queryKey: ["rgl_season_matches", id], queryFn: () => api.getRglSeasonMatches(id), enabled: false, staleTime: KEEP }).data;
  if (q.error || !s)
    return <section className="panel e2-panel">{q.error ? <ApiError error={q.error} /> : <Loading>{t("Loading the season…")}</Loading>}</section>;
  const teams = s.divisions.reduce((sum, d) => sum + d.teams.length, 0);
  const champs = champions(s.divisions);
  return (
    <>
    <section className="panel e2-panel">
      <>
          <div className="e2-panel-head">
            <div className="e2-titles">
              <h2>
                {s.name} <BookmarkButton b={{ kind: "rgl_season", id: s.id, label: s.name, sub: t("RGL Highlander") }} />
              </h2>
              <span className="muted">{teams === 1 ? t("1 team") : t("{0} teams", { "0": teams })}</span>
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
          {champs.length > 0 &&
            (shown ? (
              <ul className="rgl-champs" aria-label={t("Champions")}>
                {champs.map(({ division, team }) => (
                  <li key={division.name}>
                    <span className="muted">{division.name}</span>
                    <MedalGlyph size={16} place={1} />
                    <button className="linkish" onClick={() => onTeam(team.id)}>
                      {team.name}
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="hint">{t("The champions are results, so they are hidden. Turn on Show results to see them.")}</p>
            ))}
      </>
    </section>
    <div className="rgl-cards rgl-cards-season">
      <StatCard
        label={t("Season")}
        wait={!card}
        value={card ? statusWord(card) : ""}
        sub={card ? spanText(card) : ""}
        title={t("Its first and last match, or when it starts")}
      />
      <StatCard
        label={t("Teams")}
        value={card?.totalTeams ?? teams}
        sub={s.divisions.length === 1 ? t("in 1 division") : t("in {0} divisions", { "0": s.divisions.length })}
        title={t("Teams across every division, byes left out")}
      />
      <StatCard
        label={t("Matches")}
        wait={!card}
        value={card ? (read ? `${playedCount(read)} / ${card.matches}` : card.matches) : ""}
        sub={card ? (read ? t("played so far, of those scheduled") : t("scheduled so far")) : ""}
        title={t("Matches played so far and in all; the played count shows once the results below are read")}
      />
      <StatCard
        label={t("Map pool")}
        value={s.maps.length}
        sub={card?.mapsPerMatch ? `9v9 · ${t("{0} maps a match", { "0": card.mapsPerMatch })}` : t("9v9 Highlander")}
        title={t("Maps this season is played on")}
      />
    </div>
    <section className="panel e2-panel">
      <>
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
                    {/* Seeds over the teams still playing; dropped teams and the free-agent slot after them (0.14.1). */}
                    {divisionRows(d.teams).map(({ team: tm, seed, out }) => {
                      return (
                        <li key={tm.id}>
                          <button className={out ? "rgl-team out" : "rgl-team"} onClick={() => onTeam(tm.id)}>
                            <span className="rgl-rank">
                              {out ? "" : tm.finalRank !== null && tm.finalRank <= 3 ? <MedalGlyph size={16} place={tm.finalRank} /> : placing(tm.finalRank, seed)}
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
    </section>
    </>
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
      <MapPool matches={matches} />
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

/**
 * How often each map of the season has been played so far, and how its maps
 * split between the home and the away side (tf2esports' maps stats). It is
 * counted from the matches the page above already read, so it asks for
 * nothing. RGL's API does not say who picked a map, so the split is by the
 * side that was at home. How often a map was played is no result; who won
 * it is, and hides with the rest.
 */
function MapPool({ matches }: { matches: RglMatch[] }) {
  const [shown] = useShowResults();
  const rows = useMemo(() => mapStats(matches), [matches]);
  const sort = useSort<MapRow>(
    "rgl-season-maps",
    rows,
    {
      map: (r) => r.map,
      played: (r) => r.played,
      share: (r) => r.share,
      home: (r) => (shown ? r.homeWins : null),
      away: (r) => (shown ? r.awayWins : null),
      rate: (r) => (shown ? r.homeRate : null),
    },
    { key: "played", dir: "desc" },
  );
  if (rows.length === 0) return null;
  return (
    <section className="panel e2-panel rgl-mappool">
      <div className="e2-panel-head">
        <h2>{t("Map pool")}</h2>
        <span className="muted">{t("played so far")}</span>
      </div>
      <table className="match-table rgl-table">
        <thead>
          <tr>
            <SortTh k="map" s={sort}>
              {t("Map")}
            </SortTh>
            <SortTh k="played" s={sort} className="num" title={t("Maps played so far, with a score in")}>
              {t("Played")}
            </SortTh>
            <SortTh k="share" s={sort} className="num" title={t("Its share of all the maps played")}>
              {t("Share")}
            </SortTh>
            <SortTh k="home" s={sort} className="num" title={t("Maps won by the team at home")}>
              {t("Home wins")}
            </SortTh>
            <SortTh k="away" s={sort} className="num" title={t("Maps won by the team away")}>
              {t("Away wins")}
            </SortTh>
            <SortTh k="rate" s={sort} className="num" title={t("The home side's share of the maps won, draws left out")}>
              {t("Home win %")}
            </SortTh>
          </tr>
        </thead>
        <tbody>
          {sort.sorted.map((r) => (
            <tr key={r.map}>
              <td>{r.map}</td>
              <td className="num">{r.played}</td>
              <td className="num muted">{r.share.toFixed(0)}%</td>
              <td className="num">{shown ? r.homeWins : "–"}</td>
              <td className="num">{shown ? r.awayWins : "–"}</td>
              <td className="num muted">{shown && r.homeRate !== null ? `${r.homeRate.toFixed(0)}%` : "–"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="hint">{t("RGL does not say who picked a map, so the split is by the side that was at home. The wins hide with the results.")}</p>
    </section>
  );
}
