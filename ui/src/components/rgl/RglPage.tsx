import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglMatch, RglPlayer, RglSeason, RglTeam, RglTeamPage } from "../../api/types";
import { t } from "../../lib/i18n";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import "../teams/teams.css";
import "../etf2l/etf2l.css";
import "./rgl.css";

/**
 * The RGL tab (PLAN §30), in the ETF2L tab's frame (Flashy: "i do want to
 * follow the template of the etf2l page"): a menu, breadcrumbs, a search,
 * and the mouse's back button stepping back within the tab. What RGL's API
 * answers by id is here -- seasons, a team's roster, results and other
 * seasons, a player's career by SteamID, your own RGL card -- read live and
 * kept for half an hour. Its lists (bans, name search, matches) are broken
 * on RGL's side, so standings and the rest wait for phase 1.
 */

type Section = "home" | "seasons" | "players";

type Loc =
  | { at: "home" }
  | { at: "seasons" }
  | { at: "season"; id: number }
  | { at: "team"; id: number; from: Loc }
  | { at: "players" }
  | { at: "player"; steamId: string; from: Loc };

function sectionOf(l: Loc): Section {
  if (l.at === "season") return "seasons";
  if (l.at === "team" || l.at === "player") return sectionOf(l.from);
  return l.at;
}

/** What a new team or player page goes back to: never another of those. */
function base(l: Loc): Loc {
  return l.at === "team" || l.at === "player" ? l.from : l;
}

const same = (a: Loc, b: Loc) => JSON.stringify(a) === JSON.stringify(b);
const KEEP = 30 * 60_000;

/** What the full tab will add, shown until it is built. */
const coming = (): [string, string][] => [
  [t("Matches"), t("Results and the schedule by week and division, with each match's logs.")],
  [t("Standings"), t("Division tables worked out from the results, and the playoffs.")],
  [t("Bans"), t("The league's list.")],
  [t("Search by name"), t("RGL's name search is broken on its side for now: players open by SteamID.")],
  [t("Through the app"), t("RGL on profiles and the scoreboard, and RGL officials counted as officials.")],
];

const useSeasons = () => useQuery({ queryKey: ["rgl_seasons"], queryFn: api.getRglSeasons, staleTime: KEEP });
const useSeason = (id: number | null) =>
  useQuery({ queryKey: ["rgl_season", id], queryFn: () => api.getRglSeason(id!), enabled: id !== null, staleTime: KEEP });
const useTeam = (id: number | null) => useQuery({ queryKey: ["rgl_team", id], queryFn: () => api.getRglTeam(id!), enabled: id !== null, staleTime: KEEP });
const usePlayer = (steamId: string | null) =>
  useQuery({ queryKey: ["rgl_player", steamId], queryFn: () => api.getRglPlayer(steamId!), enabled: steamId !== null, staleTime: KEEP, retry: false });

/** An ISO time as a short date. */
function day(iso: string | null): string {
  if (!iso) return "–";
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: d.getFullYear() === new Date().getFullYear() ? undefined : "numeric" });
}

/** RGL's final placing as the list shows it; 99 is a team that dropped out. */
function placing(rank: number | null, seeding?: number | null): string {
  if (rank === 99) return t("dropped out");
  if (rank !== null) return `#${rank}`;
  return seeding != null ? t("seed {0}", { "0": seeding }) : "";
}

export function RglPage({ active, steamid }: { active: boolean; steamid: string | null }) {
  const [stack, setStack] = useState<Loc[]>([{ at: "home" }]);
  const loc = stack[stack.length - 1];
  const go = (l: Loc) => {
    setStack((s) => (same(s[s.length - 1], l) ? s : [...s, l].slice(-50)));
    window.scrollTo({ top: 0 });
  };
  const openTeam = (id: number) => {
    setStack((s) => [...s, { at: "team", id, from: base(s[s.length - 1]) }]);
    window.scrollTo({ top: 0 });
  };
  const openPlayer = (steamId: string) => {
    setStack((s) => [...s, { at: "player", steamId, from: base(s[s.length - 1]) }]);
    window.scrollTo({ top: 0 });
  };

  // Mouse 4 / Alt+Left step back within the tab first, as in the ETF2L tab.
  const depth = useRef(stack.length);
  const activeRef = useRef(active);
  useEffect(() => {
    depth.current = stack.length;
    activeRef.current = active;
  });
  useEffect(() => {
    const can = () => activeRef.current && depth.current > 1;
    const pop = () => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s));
    const onMouse = (e: MouseEvent) => {
      if (e.button !== 3 || !can()) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.type === "mouseup") pop();
    };
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || e.key !== "ArrowLeft" || !can()) return;
      e.preventDefault();
      e.stopPropagation();
      pop();
    };
    window.addEventListener("mousedown", onMouse, true);
    window.addEventListener("mouseup", onMouse, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("mousedown", onMouse, true);
      window.removeEventListener("mouseup", onMouse, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, []);

  const section = sectionOf(loc);
  const menu: Array<[Section, string]> = [
    ["home", t("Home")],
    ["seasons", t("Seasons")],
    ["players", t("Players")],
  ];

  return (
    <div className="content e2-page">
      <header className="e2-head">
        <div className="e2-brand">
          <span className="e2-eyebrow">{t("RGL Highlander")}</span>
          <nav className="e2-menu" aria-label={t("RGL")}>
            {menu.map(([id, label]) => (
              <button key={id} className={section === id ? "on" : undefined} aria-current={section === id ? "page" : undefined} onClick={() => go({ at: id })}>
                {label}
              </button>
            ))}
          </nav>
        </div>
        <div className="e2-head-tools">
          <RglSearch onTeam={openTeam} onPlayer={openPlayer} />
        </div>
      </header>
      <Crumbs loc={loc} go={go} />

      {loc.at === "home" && <Home steamid={steamid} onTeam={openTeam} onPlayer={openPlayer} go={go} />}
      {loc.at === "seasons" && <SeasonList onSeason={(id) => go({ at: "season", id })} />}
      {loc.at === "season" && <SeasonView id={loc.id} onTeam={openTeam} />}
      {loc.at === "team" && <TeamView key={loc.id} id={loc.id} onTeam={openTeam} onPlayer={openPlayer} />}
      {loc.at === "players" && <PlayersLanding steamid={steamid} onPlayer={openPlayer} />}
      {loc.at === "player" && <PlayerView key={loc.steamId} steamId={loc.steamId} onTeam={openTeam} />}
    </div>
  );
}

// ---- Breadcrumbs ---------------------------------------------------------

function Crumbs({ loc, go }: { loc: Loc; go: (l: Loc) => void }) {
  const seasons = useSeasons().data;
  const teamId = loc.at === "team" ? loc.id : null;
  const team = useTeam(teamId).data;
  const steamId = loc.at === "player" ? loc.steamId : null;
  const player = usePlayer(steamId).data;
  const trail: Array<[string, Loc]> = [];
  const add = (l: Loc) => {
    if (l.at === "team") {
      add(l.from);
      trail.push([team?.name ?? t("Team"), l]);
    } else if (l.at === "player") {
      add(l.from);
      trail.push([player?.name ?? t("Player"), l]);
    } else if (l.at === "season") {
      trail.push([t("Seasons"), { at: "seasons" }]);
      trail.push([seasons?.find((s) => s.id === l.id)?.name ?? t("Season"), l]);
    } else if (l.at !== "home") {
      trail.push([{ seasons: t("Seasons"), players: t("Players") }[l.at], l]);
    }
  };
  add(loc);
  if (trail.length === 0) return null;
  return (
    <nav className="e2-crumbs" aria-label={t("Where you are")}>
      <button onClick={() => go({ at: "home" })}>{t("RGL")}</button>
      {trail.map(([label, l], i) => (
        <span key={i} className="e2-crumb">
          <span className="e2-sep" aria-hidden>
            ›
          </span>
          {i === trail.length - 1 ? <span aria-current="page">{label}</span> : <button onClick={() => go(l)}>{label}</button>}
        </span>
      ))}
    </nav>
  );
}

// ---- Search: a SteamID opens the player, a name finds teams ---------------

/** A SteamID in any form a person pastes, or an rgl.gg / Steam profile link. */
const looksLikeSteamId = (q: string) => /^(\d{17}|\[?U:1:\d+\]?|STEAM_\d:\d:\d+)$/i.test(q) || /profiles\/\d{17}|[?&]p=\d{17}/.test(q);

function RglSearch({ onTeam, onPlayer }: { onTeam: (id: number) => void; onPlayer: (steamId: string) => void }) {
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const qc = useQueryClient();
  const term = q.trim();
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", down);
    return () => document.removeEventListener("mousedown", down);
  }, [open]);
  const done = () => {
    setOpen(false);
    setQ("");
  };
  const id = looksLikeSteamId(term);
  // RGL's own name search is broken, so a name is looked for among the
  // teams of the seasons already read.
  const teams: Array<RglTeam & { season: string }> = [];
  if (!id && term.length >= 2) {
    const needle = term.toLowerCase();
    for (const [, s] of qc.getQueriesData<RglSeason>({ queryKey: ["rgl_season"] })) {
      for (const d of s?.divisions ?? []) {
        for (const tm of d.teams) {
          if (tm.name.toLowerCase().includes(needle) || tm.tag?.toLowerCase().includes(needle)) teams.push({ ...tm, season: s!.name });
        }
      }
    }
  }
  return (
    <div className="e2-search" ref={box}>
      <input
        type="search"
        value={q}
        placeholder={t("SteamID, or a team of a season read")}
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={(e) => {
          if (e.key === "Escape") done();
          if (e.key === "Enter" && id) {
            done();
            onPlayer(term);
          }
        }}
        aria-label={t("Search RGL")}
      />
      {open && term.length >= 2 && (
        <div className="e2-search-panel">
          {id ? (
            <button
              className="e2-hit"
              onClick={() => {
                done();
                onPlayer(term);
              }}
            >
              <span className="e2-hit-name">{t("Open this player's RGL profile")}</span>
              <span className="e2-hit-meta">↵</span>
            </button>
          ) : (
            <>
              {teams.length > 0 && <h4>{t("Teams")}</h4>}
              {teams.slice(0, 8).map((x) => (
                <button
                  key={x.id}
                  className="e2-hit"
                  onClick={() => {
                    done();
                    onTeam(x.id);
                  }}
                >
                  <span className="e2-hit-name">{x.name}</span>
                  <span className="e2-hit-meta">{x.season}</span>
                </button>
              ))}
              {teams.length === 0 && <p className="hint">{t("No team by that name in the seasons read. A player opens by SteamID.")}</p>}
            </>
          )}
        </div>
      )}
    </div>
  );
}

// ---- Home ------------------------------------------------------------------

function Home({ steamid, onTeam, onPlayer, go }: { steamid: string | null; onTeam: (id: number) => void; onPlayer: (s: string) => void; go: (l: Loc) => void }) {
  const seasons = useSeasons();
  const latest = seasons.data?.[0]?.id ?? null;
  return (
    <div className="e2-home">
      <div className="e2-main">
        {seasons.error ? (
          <section className="panel e2-panel">
            <ApiError error={seasons.error} />
          </section>
        ) : latest === null ? (
          <section className="panel e2-panel">
            <Loading>{t("Loading the season…")}</Loading>
          </section>
        ) : (
          <SeasonView id={latest} onTeam={onTeam} more={() => go({ at: "seasons" })} />
        )}
      </div>
      <aside className="e2-rail">
        {steamid && <YourRgl steamid={steamid} onTeam={onTeam} onPlayer={onPlayer} />}
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
  );
}

/** You on RGL, from the app's SteamID: your last season and team. */
function YourRgl({ steamid, onTeam, onPlayer }: { steamid: string; onTeam: (id: number) => void; onPlayer: (s: string) => void }) {
  const q = usePlayer(steamid);
  if (q.isPending) return null;
  const p = q.data;
  const last = p?.seasons[0];
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("You on RGL")}</h2>
        {p && (
          <button className="e2-more" onClick={() => onPlayer(steamid)}>
            {t("Profile")} ›
          </button>
        )}
      </div>
      {q.error ? (
        <ApiError error={q.error} />
      ) : !p ? (
        <p className="hint">{t("No RGL profile for your SteamID.")}</p>
      ) : !last ? (
        <p className="hint">{t("{0} has no NA Highlander seasons.", { "0": p.name })}</p>
      ) : (
        <div className="rgl-you">
          <span className="muted">{last.seasonName}</span>
          <button className="linkish" onClick={() => onTeam(last.teamId)}>
            {last.teamName}
          </button>
          <span className="muted">
            {last.division}
            {last.tier && ` ≈ ${t(last.tier)}`} · {last.wins}-{last.losses}
          </span>
        </div>
      )}
    </section>
  );
}

// ---- Seasons ---------------------------------------------------------------

function SeasonList({ onSeason }: { onSeason: (id: number) => void }) {
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
      ) : (
        <div className="rgl-season-grid">
          {q.data.map((s, i) => (
            <button key={s.id} className="rgl-season-tile" onClick={() => onSeason(s.id)}>
              <strong>{s.name}</strong>
              {i === 0 && <span className="e2-live-chip">{t("Latest")}</span>}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

function SeasonView({ id, onTeam, more }: { id: number; onTeam: (id: number) => void; more?: () => void }) {
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
              <h2>{s.name}</h2>
              <span className="muted">{t("{0} teams", { "0": s.divisions.reduce((n, d) => n + d.teams.length, 0) })}</span>
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
                            <span className="rgl-rank">{out ? "" : placing(tm.finalRank, tm.seeding)}</span>
                            <span className="rgl-name">{tm.name}</span>
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

// ---- A team ----------------------------------------------------------------

function TeamView({ id, onTeam, onPlayer }: { id: number; onTeam: (id: number) => void; onPlayer: (s: string) => void }) {
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
            </div>
            <span className="muted">
              {[v.seasonName, v.division && (v.tier ? `${v.division} ≈ ${t(v.tier)}` : v.division), placing(v.finalRank), played > 0 && `${won}-${played - won}`].filter(Boolean).join(" · ")}
            </span>
          </div>
        </div>
      </div>
      <div className="rgl-cols">
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <h2>{t("Results")}</h2>
          </div>
          {v.matches.length === 0 ? <p className="hint">{t("No matches yet.")}</p> : <Results team={v} onTeam={onTeam} />}
        </section>
        <div className="e2-rail">
          <Roster team={v} onPlayer={onPlayer} />
          {v.linked.length > 0 && <OtherSeasons ids={v.linked} onTeam={onTeam} />}
        </div>
      </div>
    </>
  );
}

function Results({ team, onTeam }: { team: RglTeamPage; onTeam: (id: number) => void }) {
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
            <ResultRow key={m.matchId} m={m} us={team.id} onTeam={onTeam} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ResultRow({ m, us, onTeam }: { m: RglMatch; us: number; onTeam: (id: number) => void }) {
  const them = m.teams.find((x) => x.teamId !== us);
  const home = m.teams.find((x) => x.teamId === us)?.isHome ?? true;
  const ours = m.maps.reduce((n, x) => n + ((home ? x.homeScore : x.awayScore) ?? 0), 0);
  const theirs = m.maps.reduce((n, x) => n + ((home ? x.awayScore : x.homeScore) ?? 0), 0);
  const cls = m.winner === null ? "num" : m.winner === us ? "num team-won" : "num team-lost";
  return (
    <tr>
      <td className="nowrap muted">{day(m.matchDate)}</td>
      <td className="nowrap muted">{m.matchName ?? "–"}</td>
      <td>
        {them ? (
          <button className="linkish" onClick={() => onTeam(them.teamId)}>
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

function Roster({ team, onPlayer }: { team: RglTeamPage; onPlayer: (s: string) => void }) {
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

function OtherSeasons({ ids, onTeam }: { ids: number[]; onTeam: (id: number) => void }) {
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

function PlayersLanding({ steamid, onPlayer }: { steamid: string | null; onPlayer: (s: string) => void }) {
  const [q, setQ] = useState("");
  const ok = looksLikeSteamId(q.trim());
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Players")}</h2>
      </div>
      <p className="hint">{t("RGL's name search is broken on its side for now: paste a SteamID, a Steam profile link or an rgl.gg profile link.")}</p>
      <form
        className="rgl-lookup"
        onSubmit={(e) => {
          e.preventDefault();
          if (ok) onPlayer(q.trim());
        }}
      >
        <input type="search" value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("SteamID or profile link")} aria-label={t("SteamID or profile link")} />
        <button type="submit" disabled={!ok}>
          {t("Open")}
        </button>
        {steamid && (
          <button type="button" onClick={() => onPlayer(steamid)}>
            {t("You")}
          </button>
        )}
      </form>
    </section>
  );
}

function PlayerView({ steamId, onTeam }: { steamId: string; onTeam: (id: number) => void }) {
  const q = usePlayer(steamId);
  const p: RglPlayer | null | undefined = q.data;
  if (q.error) return <ApiError error={q.error} />;
  if (q.isPending) return <Loading>{t("Loading the player…")}</Loading>;
  if (!p) return <p className="hint">{t("No RGL profile for this SteamID.")}</p>;
  const wins = p.seasons.reduce((n, s) => n + s.wins, 0);
  const losses = p.seasons.reduce((n, s) => n + s.losses, 0);
  return (
    <>
      <div className="panel pp-head">
        <div className="pp-top">
          <div className="pp-avatar">{p.avatar ? <img src={p.avatar} alt="" /> : <span>{p.name.slice(0, 1).toUpperCase()}</span>}</div>
          <div className="pp-who">
            <div className="pp-name-row">
              <h2 className="pp-name">{p.name}</h2>
              {p.banned && <span className="badge rgl-banned">{t("Banned")}</span>}
              {p.probation && <span className="badge">{t("On probation")}</span>}
            </div>
            <span className="muted">
              {t("{0} NA Highlander seasons", { "0": p.seasons.length })} · {wins}-{losses}
            </span>
          </div>
        </div>
      </div>
      <section className="panel e2-panel">
        <div className="e2-panel-head">
          <h2>{t("Seasons")}</h2>
        </div>
        {p.seasons.length === 0 ? (
          <p className="hint">{t("No NA Highlander seasons.")}</p>
        ) : (
          <div className="table-wrap">
            <table className="match-table">
              <thead>
                <tr>
                  <th>{t("Season")}</th>
                  <th>{t("Division")}</th>
                  <th>{t("Team")}</th>
                  <th>{t("Joined")}</th>
                  <th className="num">{t("W-L")}</th>
                </tr>
              </thead>
              <tbody>
                {p.seasons.map((s) => (
                  <tr key={`${s.teamId}-${s.startedAt}`}>
                    <td className="nowrap">{s.seasonName}</td>
                    <td className="nowrap muted">
                      {s.division ?? "–"}
                      {s.tier && ` ≈ ${t(s.tier)}`}
                    </td>
                    <td>
                      <button className="linkish" onClick={() => onTeam(s.teamId)}>
                        {s.teamName}
                      </button>
                    </td>
                    <td className="nowrap muted">{day(s.startedAt)}</td>
                    <td className="num">
                      {s.wins}-{s.losses}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </>
  );
}
