import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglMatch, RglTeamPage } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { openOnKey } from "../../lib/rows";
import { standings } from "../../lib/rglstandings";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { Monogram } from "../etf2l/Teams";
import { tierOf } from "../etf2l/Brackets";
import { DivisionBadge } from "../players/PlayerProfile";
import { BookmarkButton } from "../Bookmarks";
import { KEEP, useSeason, useTeam, useLive, LADDER, useSettled, SEARCH_FROM, usePlayer, day, placing } from "./data";
import { Reading, OnRgl } from "./bits";

/**
 * Teams, as the ETF2L tab's (Flashy: "up to parity with etf2l"): a name
 * search, your teams from your RGL career, and the live season's teams by
 * division with their record so far. The search asks RGL once a word, three
 * letters at least, as the header's does; the rest is read already for Home.
 */
export function TeamsLanding({ steamid, onTeam }: { steamid: string | null; onTeam: (id: number) => void }) {
  const [q, setQ] = useState("");
  const term = useSettled(q.trim());
  const on = term.length >= SEARCH_FROM;
  const hits = useQuery({ queryKey: ["rgl_search_teams", term.toLowerCase()], queryFn: () => api.searchRglTeams(term), enabled: on, staleTime: KEEP, retry: false });
  const me = usePlayer(steamid);
  const live = useLive();
  const season = useSeason(live.season?.id ?? null).data;
  const tables = useMemo(() => standings(live.matches ?? []), [live.matches]);
  // Your seasons, one card a team, newest first.
  const yours = me.data?.seasons ?? [];
  const mine = new Set(yours.map((s) => s.teamId));
  const divisions = (season?.divisions ?? []).filter((d) => d.teams.length > 0);

  return (
    <>
      <header className="tm-head">
        <div className="ts-pagehead">
          <span className="ts-eyebrow">{t("RGL Highlander")}</span>
          <h1>{t("Teams")}</h1>
        </div>
        <div className="e2-search tm-search">
          <input type="search" value={q} placeholder={t("Search any team by name")} onChange={(e) => setQ(e.target.value)} aria-label={t("Search teams")} autoFocus />
        </div>
      </header>

      {q.trim().length > 0 && q.trim().length < SEARCH_FROM && <p className="hint">{t("Three letters at least.")}</p>}
      {on && (
        <section className="tm-section">
          <h2 className="ts-section">{tx("Teams matching “{0}”", { "0": term })}</h2>
          {hits.error ? (
            <ApiError error={hits.error} />
          ) : hits.isPending ? (
            <Loading>{t("Asking RGL…")}</Loading>
          ) : (hits.data ?? []).length === 0 ? (
            <p className="hint">{t("No team by that name.")}</p>
          ) : (
            <div className="tm-grid">
              {(hits.data ?? []).map((x) => (
                <button key={x.id} className="tm-tile" onClick={() => onTeam(x.id)}>
                  <Monogram name={x.name} />
                  <span className="tm-tile-text">
                    <strong>{x.name}</strong>
                    <span className="muted">{[x.seasonName, x.division].filter(Boolean).join(" · ")}</span>
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
            {yours.map((s) => {
              const total = s.wins + s.losses;
              const wr = total > 0 ? (s.wins / total) * 100 : null;
              return (
                <button key={`${s.teamId}-${s.seasonId}`} className="tm-card" onClick={() => onTeam(s.teamId)}>
                  <span className="tm-card-top">
                    <Monogram name={s.teamName} />
                    <span className="tm-card-name">
                      <strong>{s.teamName}</strong>
                      <span className="muted">{[s.seasonName, s.division].filter(Boolean).join(" · ")}</span>
                    </span>
                  </span>
                  <span className="tm-bar" aria-hidden>
                    <span className="tm-bar-w" style={{ width: `${wr ?? 0}%` }} />
                  </span>
                  <span className="tm-card-stats">
                    <span>
                      <strong>
                        {s.wins}–{s.losses}
                      </strong>
                      {wr !== null && <em> {wr.toFixed(0)}%</em>}
                    </span>
                    {s.tier && <span className="muted">≈ {t(s.tier)}</span>}
                  </span>
                </button>
              );
            })}
          </div>
        </section>
      )}

      {live.error ? (
        <ApiError error={live.error} />
      ) : !live.none && (!season || live.matches === null) ? (
        <Reading />
      ) : season && divisions.length > 0 ? (
        <section className="tm-section">
          <h2 className="ts-section">{tx("Teams in {0}", { "0": season.name })}</h2>
          <div className="tm-divs">
            {divisions.map((d, i) => {
              const odd = divisions.length % 2 === 1 && i === divisions.length - 1;
              // The table so far where the season has results, else RGL's seeding.
              const table = tables.get(d.name) ?? [];
              const rows = d.teams
                .map((x) => {
                  const r = table.find((s) => s.id === x.id);
                  return { id: x.id, name: x.name, won: r?.won ?? 0, lost: r?.lost ?? 0, for: r?.for ?? 0, against: r?.against ?? 0, at: r ? table.indexOf(r) : table.length + (x.seeding ?? 99) };
                })
                .sort((a, b) => a.at - b.at || a.name.localeCompare(b.name));
              // One left over takes the row, its list split in two -- unless it
              // is too short to split.
              const wide = odd && rows.length > 6;
              const half = Math.ceil(rows.length / 2);
              const ladder = LADDER[d.name];
              return (
                <section key={d.name} className={odd ? "panel tm-div wide" : "panel tm-div"}>
                  <div className="tm-div-head">
                    <DivisionBadge d={{ name: d.name, tier: Math.floor(tierOf(ladder ?? "Open")) }} />
                    <span className="muted">
                      {rows.length === 1 ? t("1 team") : tx("{0} teams", { "0": rows.length })}
                      {ladder && ` · ≈ ${t(ladder)}`}
                    </span>
                  </div>
                  <div className="tm-div-cols">
                    {(wide ? [rows.slice(0, half), rows.slice(half)] : [rows]).map((col, c) => (
                      <ol key={c} className="tm-div-list" start={c * half + 1}>
                        <li className="tm-row tm-row-head" aria-hidden>
                          <span>#</span>
                          <span />
                          <span>{t("Team")}</span>
                          <span title={t("Matches won and lost")}>{t("W–L")}</span>
                          <span title={t("Rounds won and lost")}>{t("Rounds")}</span>
                        </li>
                        {col.map((x, j) => {
                          const place = (wide ? c * half : 0) + j + 1;
                          const diff = x.for - x.against;
                          return (
                            <li key={x.id}>
                              <button className={mine.has(x.id) ? "tm-row mine" : "tm-row"} onClick={() => onTeam(x.id)}>
                                <span className={place === 1 && x.won > 0 ? "tm-place first" : "tm-place"}>{place}</span>
                                <Monogram name={x.name} />
                                <span className="tm-row-name">{x.name}</span>
                                <span className="tm-row-wl">
                                  {x.won}–{x.lost}
                                </span>
                                <span className={diff > 0 ? "tm-row-maps up" : diff < 0 ? "tm-row-maps down" : "tm-row-maps"}>
                                  {x.for}–{x.against}
                                </span>
                              </button>
                            </li>
                          );
                        })}
                      </ol>
                    ))}
                  </div>
                </section>
              );
            })}
          </div>
        </section>
      ) : null}
    </>
  );
}

// ---- Seasons ---------------------------------------------------------------

/** A team's record on each map it played, by the map's own score. */
export function MapRecord({ team }: { team: RglTeamPage }) {
  const rows = useMemo(() => {
    const by = new Map<string, { won: number; lost: number }>();
    for (const m of team.matches) {
      if (m.isForfeit) continue;
      const home = m.teams.find((x) => x.teamId === team.id)?.isHome ?? true;
      for (const x of m.maps) {
        if (x.homeScore === null || x.awayScore === null || x.homeScore === x.awayScore) continue;
        const r = by.get(x.mapName) ?? { won: 0, lost: 0 };
        if ((x.homeScore > x.awayScore) === home) r.won += 1;
        else r.lost += 1;
        by.set(x.mapName, r);
      }
    }
    return [...by.entries()].sort((a, b) => b[1].won + b[1].lost - (a[1].won + a[1].lost) || a[0].localeCompare(b[0]));
  }, [team]);
  if (rows.length === 0) return null;
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Maps")}</h2>
        <span className="muted">{t("won – lost")}</span>
      </div>
      <ul className="rgl-maprec">
        {rows.map(([map, r]) => (
          <li key={map}>
            <span>{map}</span>
            <span className="rgl-maprec-wl">
              <span className="team-won">{r.won}</span>–<span className="team-lost">{r.lost}</span>
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

// ---- The league's bans ---------------------------------------------------------

export function TeamView({ id, onTeam, onPlayer, onMatch }: { id: number; onTeam: (id: number) => void; onPlayer: (s: string) => void; onMatch: (id: number) => void }) {
  const q = useTeam(id);
  const v = q.data;
  if (q.error) return <ApiError error={q.error} />;
  if (q.isPending) return <Loading>{t("Loading the team and its matches…")}</Loading>;
  if (!v) return <p className="hint">{t("RGL has no such team.")}</p>;
  const won = v.matches.filter((m) => m.winner === v.id).length;
  const played = v.matches.filter((m) => m.winner !== null).length;
  return (
    <>
      <div className="panel pp-head">
        <div className="pp-top">
          <div className="pp-avatar ts-teamavatar">
            <span>{(v.tag ?? v.name).slice(0, 1).toUpperCase()}</span>
          </div>
          <div className="pp-who">
            <div className="pp-name-row">
              <h2 className="pp-name">{v.name}</h2>
              {v.tag && <span className="ts-tag">{v.tag}</span>}
              <BookmarkButton b={{ kind: "rgl_team", id: v.id, label: v.name, sub: [t("RGL"), v.seasonName, v.division].filter(Boolean).join(" · ") }} />
            </div>
            <span className="muted">
              {[v.seasonName, v.division && (v.tier ? `${v.division} ≈ ${t(v.tier)}` : v.division), placing(v.finalRank), played > 0 && `${won}-${played - won}`].filter(Boolean).join(" · ")}
            </span>
          </div>
          <OnRgl href={`https://rgl.gg/Public/Team?t=${v.id}`} />
        </div>
      </div>
      <div className="rgl-cols">
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <h2>{t("Results")}</h2>
          </div>
          {v.matches.length === 0 ? <p className="hint">{t("No matches yet.")}</p> : <Results team={v} onTeam={onTeam} onMatch={onMatch} />}
        </section>
        <div className="e2-rail">
          <Roster team={v} onPlayer={onPlayer} />
          <MapRecord team={v} />
          {v.linked.length > 0 && <OtherSeasons ids={v.linked} onTeam={onTeam} />}
        </div>
      </div>
    </>
  );
}

export function Results({ team, onTeam, onMatch }: { team: RglTeamPage; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  return (
    <div className="table-wrap">
      <table className="match-table">
        <thead>
          <tr>
            <th>{t("Date")}</th>
            <th>{t("Round")}</th>
            <th>{t("Against")}</th>
            <th>{t("Map")}</th>
            <th className="num">{t("Score")}</th>
          </tr>
        </thead>
        <tbody>
          {team.matches.map((m) => (
            <ResultRow key={m.matchId} m={m} us={team.id} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function ResultRow({ m, us, onTeam, onOpen }: { m: RglMatch; us: number; onTeam: (id: number) => void; onOpen: () => void }) {
  const them = m.teams.find((x) => x.teamId !== us);
  const home = m.teams.find((x) => x.teamId === us)?.isHome ?? true;
  const ours = m.maps.reduce((n, x) => n + ((home ? x.homeScore : x.awayScore) ?? 0), 0);
  const theirs = m.maps.reduce((n, x) => n + ((home ? x.awayScore : x.homeScore) ?? 0), 0);
  const cls = m.winner === null ? "num" : m.winner === us ? "num team-won" : "num team-lost";
  return (
    <tr className="rgl-open" tabIndex={0} onClick={onOpen} onKeyDown={openOnKey(onOpen)} title={t("Open the match")}>
      <td className="nowrap muted">{day(m.matchDate)}</td>
      <td className="nowrap muted">{m.matchName ?? "–"}</td>
      <td>
        {them ? (
          <button
            className="linkish"
            onClick={(e) => {
              e.stopPropagation();
              onTeam(them.teamId);
            }}
          >
            {them.teamName}
          </button>
        ) : (
          "–"
        )}
      </td>
      <td className="nowrap">{m.maps.map((x) => x.mapName).join(", ") || "–"}</td>
      <td className={cls}>{m.isForfeit ? t("forfeit") : m.winner === null ? "–" : `${ours}:${theirs}`}</td>
    </tr>
  );
}

export function Roster({ team, onPlayer }: { team: RglTeamPage; onPlayer: (s: string) => void }) {
  // A closed team's last members all left together when it closed: then
  // the latest leaving date is the roster's end, not a departure.
  const closed = team.players.every((p) => p.leftAt);
  const end = closed ? team.players.reduce<string | null>((x, p) => (p.leftAt && (!x || p.leftAt > x) ? p.leftAt : x), null) : null;
  const stayed = team.players.filter((p) => !p.leftAt || p.leftAt === end);
  const left = team.players.filter((p) => p.leftAt && p.leftAt !== end);
  const row = (p: RglTeamPage["players"][number]) => (
    <li key={p.steamId}>
      <button className="linkish" onClick={() => onPlayer(p.steamId)}>
        {p.name}
      </button>
      {p.isLeader && <span className="badge">{t("Leader")}</span>}
      <span className="muted rgl-when">{p.leftAt && p.leftAt !== end ? `${day(p.joinedAt)} – ${day(p.leftAt)}` : t("from {0}", { "0": day(p.joinedAt) })}</span>
    </li>
  );
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Roster")}</h2>
        <span className="muted">{stayed.length}</span>
      </div>
      <ul className="rgl-roster">{stayed.map(row)}</ul>
      {left.length > 0 && (
        <>
          <h4 className="e2-label">{t("Left")}</h4>
          <ul className="rgl-roster rgl-roster-left">{left.map(row)}</ul>
        </>
      )}
    </section>
  );
}

export function OtherSeasons({ ids, onTeam }: { ids: number[]; onTeam: (id: number) => void }) {
  const q = useQuery({ queryKey: ["rgl_team_seasons", ids], queryFn: () => api.getRglTeamSeasons(ids), staleTime: KEEP });
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Other seasons")}</h2>
      </div>
      {q.error ? (
        <ApiError error={q.error} />
      ) : !q.data ? (
        <Loading>{t("Loading its other seasons…")}</Loading>
      ) : q.data.length === 0 ? (
        <p className="hint">{t("No other NA Highlander seasons.")}</p>
      ) : (
        <ul className="rgl-roster">
          {q.data.map((s) => (
            <li key={s.id}>
              <button className="linkish" onClick={() => onTeam(s.id)}>
                {s.seasonName}
              </button>
              <span className="muted rgl-when">{[s.division, placing(s.finalRank), s.name].filter(Boolean).join(" · ")}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

// ---- Players ---------------------------------------------------------------
