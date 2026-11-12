import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglMatch, RglRecord, RglTeamHistory, RglTeamPage } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { openOnKey } from "../../lib/rows";
import { standings } from "../../lib/rglstandings";
import { useShowResults } from "../../lib/spoilers";
import { form, medals, trophyCount, winRate } from "../../lib/rglteam";
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
  const [shown] = useShowResults();
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
                    <span className="tm-bar-w" style={{ width: `${shown ? (wr ?? 0) : 0}%` }} />
                  </span>
                  <span className="tm-card-stats">
                    <span>
                      <strong>{shown ? `${s.wins}–${s.losses}` : "–"}</strong>
                      {shown && wr !== null && <em> {wr.toFixed(0)}%</em>}
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
                                <span className={shown && place === 1 && x.won > 0 ? "tm-place first" : "tm-place"}>{place}</span>
                                <Monogram name={x.name} />
                                <span className="tm-row-name">{x.name}</span>
                                <span className="tm-row-wl">{shown ? `${x.won}–${x.lost}` : "–"}</span>
                                <span className={shown && diff > 0 ? "tm-row-maps up" : shown && diff < 0 ? "tm-row-maps down" : "tm-row-maps"}>{shown ? `${x.for}–${x.against}` : "–"}</span>
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
  const [shown] = useShowResults();
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
  if (rows.length === 0 || !shown) return null;
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

/**
 * A team across its seasons (hl_ingest::rgl::team_history), for the page's
 * cards, trophies, opponents and map pool. It asks RGL for each linked
 * season's team and matches, newest 12 at most: about 13 requests a season
 * the first time (4 a second, so a long-lived team takes a minute or two,
 * and the page's own parts show meanwhile), and none the second time for a
 * finished season, which is kept on disk. Held for half an hour, shared by
 * every view of the same team, the Compare page's included.
 */
export const useTeamHistory = (id: number | null, all = false) =>
  useQuery({ queryKey: ["rgl_team_history", id, all], queryFn: () => api.getRglTeamHistory(id!, all), enabled: id !== null, staleTime: KEEP });

/**
 * A team page reads its newest three seasons on its own; the rest, up to
 * twelve, only when asked here: each is a dozen requests to RGL the first
 * time (Flashy: "make sure to properly use the api to not overburden them").
 */
function ReadOlder({ d, onRead }: { d: RglTeamHistory; onRead: () => void }) {
  const left = Math.min(d.seasonsLinked, 12) - d.seasons.length;
  if (!d.capped || left <= 0) return null;
  return (
    <div className="rgl-older">
      <span className="muted">
        {tx("Read {0} of its {1} seasons.", { "0": d.seasons.length, "1": d.seasonsLinked })}
      </span>
      <button onClick={onRead} title={t("About a dozen requests to RGL for each season not read before; kept after that")}>
        {left === 1 ? t("Read 1 older season") : tx("Read {0} older seasons", { "0": left })}
      </button>
    </div>
  );
}

type Tab = "overview" | "matches" | "roster" | "history";

/** A big figure with what it is and a line under it; `…` while it is being read. */
export function StatCard({ label, value, sub, title, wait }: { label: string; value: ReactNode; sub?: ReactNode; title?: string; wait?: boolean }) {
  return (
    <div className="panel rgl-card" title={title}>
      <span className="e2-label">{label}</span>
      <strong className={wait ? "rgl-card-v wait" : "rgl-card-v"}>{wait ? "…" : value}</strong>
      <span className="muted rgl-card-sub">{wait ? "" : sub}</span>
    </div>
  );
}

/** "1 forfeit won" / "2 forfeits lost", for the line under a record. */
export function forfeitNote(r: RglRecord): string {
  const parts = [r.forfeitsWon > 0 && (r.forfeitsWon === 1 ? t("1 forfeit won") : tx("{0} forfeits won", { "0": r.forfeitsWon })), r.forfeitsLost > 0 && (r.forfeitsLost === 1 ? t("1 forfeit lost") : tx("{0} forfeits lost", { "0": r.forfeitsLost }))];
  return parts.filter(Boolean).join(" · ");
}

/** What the team's page leads with: this season, its record, its trophies, how long it has been around. */
function Cards({ v, h }: { v: RglTeamPage; h: ReturnType<typeof useTeamHistory> }) {
  const d = h.data;
  const wait = h.isPending;
  const trophies = d ? trophyCount(d.seasons) : { titles: 0, podiums: 0 };
  const rate = d ? winRate(d.total) : null;
  const note = d ? forfeitNote(d.total) : "";
  return (
    <div className="rgl-cards">
      <StatCard
        label={t("This season")}
        value={placing(v.finalRank) || t("In progress")}
        sub={[v.seasonName, v.division && (v.tier ? `${v.division} ≈ ${t(v.tier)}` : v.division)].filter(Boolean).join(" · ")}
        title={t("Its place in the division this season, as RGL ranks it")}
      />
      <StatCard
        label={t("Record")}
        wait={wait}
        value={
          d && (
            <>
              <span className="team-won">{d.total.won}</span>–<span className="team-lost">{d.total.lost}</span>
              {rate !== null && <em className="rgl-card-rate"> {rate.toFixed(0)}%</em>}
            </>
          )
        }
        sub={[d && (d.seasons.length === 1 ? t("1 season") : tx("{0} seasons", { "0": d.seasons.length })), note].filter(Boolean).join(" · ")}
        title={t("Matches won and lost across the seasons read, forfeits apart")}
      />
      <StatCard
        label={t("Titles")}
        wait={wait}
        value={trophies.titles}
        sub={trophies.podiums === 1 ? t("1 podium") : tx("{0} podiums", { "0": trophies.podiums })}
        title={t("Seasons finished first, and in the top three")}
      />
      <StatCard
        label={t("Active since")}
        wait={wait}
        value={d?.firstSeason ?? "–"}
        sub={d?.capped ? t("or earlier: only its newest seasons are read") : t("Its first NA Highlander season")}
        title={t("The oldest season read for this team")}
      />
    </div>
  );
}

/** The last ten results as squares, newest first, and the results under them. */
function Form({ matches, us, onMatch }: { matches: RglMatch[]; us: number[]; onMatch: (id: number) => void }) {
  const rows = useMemo(() => form(matches, us), [matches, us]);
  const mine = new Set(us);
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Recent form")}</h2>
        <span className="muted">{t("newest first")}</span>
      </div>
      {rows.length === 0 ? (
        <p className="hint">{t("No results yet.")}</p>
      ) : (
        <>
          <div className="rgl-form">
            {rows.map(({ match: m, won }) => {
              const them = m.teams.find((x) => !mine.has(x.teamId));
              return (
                <button key={m.matchId} className={won ? "rgl-sq won" : "rgl-sq lost"} title={`${won ? t("Won") : t("Lost")} · ${them?.teamName ?? "–"} · ${day(m.matchDate)}`} onClick={() => onMatch(m.matchId)}>
                  {won ? t("W") : t("L")}
                </button>
              );
            })}
          </div>
          <ul className="rgl-formlist">
            {rows.map(({ match: m, won }) => {
              const them = m.teams.find((x) => !mine.has(x.teamId));
              const home = m.teams.find((x) => mine.has(x.teamId))?.isHome ?? true;
              const ours = m.maps.reduce((n, x) => n + ((home ? x.homeScore : x.awayScore) ?? 0), 0);
              const theirs = m.maps.reduce((n, x) => n + ((home ? x.awayScore : x.homeScore) ?? 0), 0);
              const open = () => onMatch(m.matchId);
              return (
                <li key={m.matchId} className="rgl-open" tabIndex={0} onClick={open} onKeyDown={openOnKey(open)} title={t("Open the match")}>
                  <span className="muted rgl-wl-day">{day(m.matchDate)}</span>
                  <span className="rgl-formlist-vs">{them?.teamName ?? "–"}</span>
                  <span className={won ? "rgl-formlist-s team-won" : "rgl-formlist-s team-lost"}>{m.isForfeit ? t("forfeit") : `${ours}:${theirs}`}</span>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </section>
  );
}

/** Every podium place of the team's seasons, newest first. */
function Trophies({ d }: { d: RglTeamHistory }) {
  const rows = medals(d.seasons);
  const tone = { 1: "pp-gold", 2: "pp-silver", 3: "pp-bronze" } as const;
  const word = { 1: t("Champions"), 2: t("Runners-up"), 3: t("Third place") } as const;
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Trophy cabinet")}</h2>
        <span className="muted">{rows.length}</span>
      </div>
      {rows.length === 0 ? (
        <p className="hint">{t("No podium finish in the seasons read.")}</p>
      ) : (
        <ul className="rgl-trophies">
          {rows.map((s) => (
            <li key={s.teamId}>
              <span className={`pp-medal-tile rgl-medal ${tone[s.place]}`} title={word[s.place]}>
                {s.place}
              </span>
              <span className="rgl-trophy-text">
                <strong>{s.seasonName}</strong>
                <span className="muted">{[word[s.place], s.division].filter(Boolean).join(" · ")}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** The opponents met most, with the record against each: forfeits are not meetings. */
function Opponents({ d, onTeam }: { d: RglTeamHistory; onTeam: (id: number) => void }) {
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Most played opponents")}</h2>
        <span className="muted">{t("won – lost")}</span>
      </div>
      {d.opponents.length === 0 ? (
        <p className="hint">{t("No matches played yet.")}</p>
      ) : (
        <ul className="rgl-maprec">
          {d.opponents.map((o) => (
            <li key={o.name.toLowerCase()}>
              <button className="linkish" onClick={() => onTeam(o.teamId)}>
                {o.name}
              </button>
              <span className="rgl-maprec-wl">
                <span className="team-won">{o.won}</span>–<span className="team-lost">{o.lost}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
      <p className="hint">{t("Teams are told apart by name across seasons, as RGL gives each season's team a new id.")}</p>
    </section>
  );
}

/** Record and rounds on each map across the seasons. */
export function MapPool({ d }: { d: RglTeamHistory }) {
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Map pool")}</h2>
        <span className="muted">{t("across its seasons")}</span>
      </div>
      {d.maps.length === 0 ? (
        <p className="hint">{t("No maps played yet.")}</p>
      ) : (
        <ul className="rgl-pool">
          <li className="rgl-pool-head" aria-hidden>
            <span>{t("Map")}</span>
            <span title={t("Maps won and lost")}>{t("W–L")}</span>
            <span title={t("Rounds won and lost")}>{t("Rounds")}</span>
            <span title={t("Maps won, of those decided")}>{t("Win %")}</span>
          </li>
          {d.maps.map((m) => {
            const rate = winRate(m);
            return (
              <li key={m.map}>
                <span className="rgl-pool-map">{m.map}</span>
                <span className="rgl-maprec-wl">
                  <span className="team-won">{m.won}</span>–<span className="team-lost">{m.lost}</span>
                </span>
                <span className="muted rgl-pool-n">
                  {m.roundsFor}–{m.roundsAgainst}
                </span>
                <span className="rgl-pool-n">{rate === null ? "–" : `${rate.toFixed(0)}%`}</span>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

/** Each season the team played: its name then, division, place and record. */
function SeasonsTable({ d, now, onTeam }: { d: RglTeamHistory; now: number; onTeam: (id: number) => void }) {
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Seasons")}</h2>
        {d.capped && <span className="muted">{tx("the newest {0}", { "0": d.seasons.length })}</span>}
      </div>
      <div className="table-wrap">
        <table className="match-table">
          <thead>
            <tr>
              <th>{t("Season")}</th>
              <th>{t("Team")}</th>
              <th>{t("Division")}</th>
              <th className="num">{t("Place")}</th>
              <th className="num">{t("W–L")}</th>
              <th className="num">{t("Win %")}</th>
            </tr>
          </thead>
          <tbody>
            {d.seasons.map((s) => {
              const rate = winRate(s.record);
              const open = () => onTeam(s.teamId);
              return (
                <tr key={s.teamId} className={s.teamId === now ? undefined : "rgl-open"} tabIndex={s.teamId === now ? undefined : 0} onClick={s.teamId === now ? undefined : open} onKeyDown={s.teamId === now ? undefined : openOnKey(open)}>
                  <td className="nowrap">{s.seasonName}</td>
                  <td>{s.name}</td>
                  <td className="nowrap muted">{s.division ? (s.tier ? `${s.division} ≈ ${t(s.tier)}` : s.division) : "–"}</td>
                  <td className="num">{placing(s.finalRank) || "–"}</td>
                  <td className="num">
                    {s.record.won}–{s.record.lost}
                  </td>
                  <td className="num">{rate === null ? "–" : `${rate.toFixed(0)}%`}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="hint">{t("A forfeit is left out of the win rate.")}</p>
    </section>
  );
}

/** The wait for a team's other seasons, said as long as it is. */
function HistoryWait() {
  return (
    <section className="panel e2-panel">
      <Loading>{t("Reading the team's other seasons from RGL… a minute or two the first time for a team with many seasons. You can look at another page meanwhile: the read carries on and is kept.")}</Loading>
    </section>
  );
}

export function TeamView({ id, onTeam, onPlayer, onMatch, onCompare }: { id: number; onTeam: (id: number) => void; onPlayer: (s: string) => void; onMatch: (id: number) => void; onCompare: (a: number) => void }) {
  const q = useTeam(id);
  const [all, setAll] = useState(false);
  const h = useTeamHistory(id, all);
  const [tab, setTab] = useState<Tab>("overview");
  const v = q.data;
  const [shown] = useShowResults();
  if (q.error) return <ApiError error={q.error} />;
  if (q.isPending) return <Loading>{t("Loading the team and its matches…")}</Loading>;
  if (!v) return <p className="hint">{t("RGL has no such team.")}</p>;
  const won = v.matches.filter((m) => m.winner === v.id).length;
  const played = v.matches.filter((m) => m.winner !== null).length;
  const d = h.data;
  // Form from every season read once they are in, this season's till then.
  const formOf = d ? { matches: d.matches, us: d.teamIds } : { matches: v.matches, us: [v.id] };
  const tabs: [Tab, string][] = [
    ["overview", t("Overview")],
    ["matches", t("Matches")],
    ["roster", t("Players")],
    ["history", t("History")],
  ];
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
            {d && d.formerNames.length > 0 && (
              <span className="muted rgl-aka">
                {t("Also known as")} {d.formerNames.join(", ")}
              </span>
            )}
            <span className="muted">
              {[v.seasonName, v.division && (v.tier ? `${v.division} ≈ ${t(v.tier)}` : v.division), placing(v.finalRank), shown && played > 0 && `${won}-${played - won}`].filter(Boolean).join(" · ")}
            </span>
          </div>
          <div className="rgl-head-links">
            <button onClick={() => onCompare(v.id)} title={t("Set this team against another one")}>
              {t("Compare")}
            </button>
            <OnRgl href={`https://rgl.gg/Public/Team?t=${v.id}`} />
          </div>
        </div>
      </div>
      {h.error ? <ApiError error={h.error} /> : <Cards v={v} h={h} />}
      {d && !all && <ReadOlder d={d} onRead={() => setAll(true)} />}
      <div className="rgl-tabbed">
        <nav className="pp-tabs" role="tablist">
          {tabs.map(([k, label]) => (
            <button key={k} role="tab" aria-selected={tab === k} className={tab === k ? "pp-tab on" : "pp-tab"} onClick={() => setTab(k)}>
              {label}
            </button>
          ))}
        </nav>
        {tab === "overview" && (
          <div className="rgl-cols">
            <div className="e2-rail">
              {h.error ? null : d ? (
                <>
                  <Trophies d={d} />
                  <Opponents d={d} onTeam={onTeam} />
                  <MapPool d={d} />
                </>
              ) : (
                <HistoryWait />
              )}
            </div>
            <div className="e2-rail">
              <Form matches={formOf.matches} us={formOf.us} onMatch={onMatch} />
            </div>
          </div>
        )}
        {tab === "matches" && (
          <div className="rgl-cols">
            <section className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{t("Results")}</h2>
              </div>
              {v.matches.length === 0 ? <p className="hint">{t("No matches yet.")}</p> : <Results team={v} onTeam={onTeam} onMatch={onMatch} />}
            </section>
            <div className="e2-rail">
              <MapRecord team={v} />
            </div>
          </div>
        )}
        {tab === "roster" && (
          <div className="rgl-narrow">
            <Roster team={v} onPlayer={onPlayer} />
          </div>
        )}
        {tab === "history" && (
          <div className="rgl-cols">
            {h.error ? null : d ? <SeasonsTable d={d} now={v.id} onTeam={onTeam} /> : <HistoryWait />}
            <div className="e2-rail">{v.linked.length > 0 && <OtherSeasons ids={v.linked} onTeam={onTeam} />}</div>
          </div>
        )}
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
  const [shown] = useShowResults();
  const them = m.teams.find((x) => x.teamId !== us);
  const home = m.teams.find((x) => x.teamId === us)?.isHome ?? true;
  const ours = m.maps.reduce((n, x) => n + ((home ? x.homeScore : x.awayScore) ?? 0), 0);
  const theirs = m.maps.reduce((n, x) => n + ((home ? x.awayScore : x.homeScore) ?? 0), 0);
  const cls = !shown || m.winner === null ? "num" : m.winner === us ? "num team-won" : "num team-lost";
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
      <td className={cls}>{!shown ? "–" : m.isForfeit ? t("forfeit") : m.winner === null ? "–" : `${ours}:${theirs}`}</td>
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
