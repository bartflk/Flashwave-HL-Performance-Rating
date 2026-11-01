import { useEffect, useMemo, useRef, useState } from "react";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglBan, RglMatch, RglPlayer, RglSeason, RglTeam, RglTeamPage } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { RglMark } from "../RglMark";
import { LeagueHead } from "../LeagueHead";
import "../players/players.css";
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

type Section = "home" | "seasons" | "players" | "bans";

type Loc =
  | { at: "home" }
  | { at: "seasons" }
  | { at: "season"; id: number }
  | { at: "team"; id: number; from: Loc }
  | { at: "players" }
  | { at: "bans" }
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
  [t("Matches"), t("The league's schedule week by week, and each match's logs.")],
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

const SEEN_KEY = "rgl-in-development-seen";

function seen(): boolean {
  try {
    return localStorage.getItem(SEEN_KEY) === "1";
  } catch {
    return false;
  }
}

/**
 * Said once, the first time the tab is opened (Flashy: "put a disclaimer rgl
 * tab is in development, a modal/dialog when you first click it, transparent
 * darker background so it focuses you").
 */
function InDevelopment() {
  const [open, setOpen] = useState(() => !seen());
  const ok = useRef<HTMLButtonElement>(null);
  const close = () => {
    try {
      localStorage.setItem(SEEN_KEY, "1");
    } catch {
      // It says so again next time, which is harmless.
    }
    setOpen(false);
  };
  useEffect(() => {
    if (!open) return;
    ok.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);
  if (!open) return null;
  return (
    <div className="modal-scrim focus" role="dialog" aria-modal="true" aria-labelledby="rgl-dev-title" onClick={(e) => e.target === e.currentTarget && close()}>
      <div className="modal rgl-dev">
        <span className="e2-eyebrow">
          <RglMark size={16} /> {t("RGL Highlander")}
        </span>
        <h3 id="rgl-dev-title">{t("The RGL tab is in development")}</h3>
        <p>{t("This is an early look. Seasons, teams and players are read live from RGL's public API; standings, bans and the name search are not here yet.")}</p>
        <p className="muted">{t("Things may change, be missing or not work as they should. What is still to come is listed under Coming soon.")}</p>
        <div className="modal-actions">
          <button ref={ok} className="primary" onClick={close}>
            {t("Got it")}
          </button>
        </div>
      </div>
    </div>
  );
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
    ["bans", t("Bans")],
  ];

  return (
    <div className="content e2-page">
      {active && <InDevelopment />}
      <LeagueHead>
        <div className="e2-brand">
          <span className="e2-eyebrow" data-fit="">
            <RglMark size={16} /> {t("RGL Highlander")}
          </span>
          <nav className="e2-menu" data-fit="" aria-label={t("RGL")}>
            {menu.map(([id, label]) => (
              <button key={id} className={section === id ? "on" : undefined} aria-current={section === id ? "page" : undefined} onClick={() => go({ at: id })}>
                {label}
              </button>
            ))}
          </nav>
        </div>
        <div className="e2-head-tools" data-fit="300">
          <RglSearch onTeam={openTeam} onPlayer={openPlayer} />
        </div>
      </LeagueHead>
      <Crumbs loc={loc} go={go} />

      {loc.at === "home" && <Home steamid={steamid} onTeam={openTeam} onPlayer={openPlayer} go={go} />}
      {loc.at === "seasons" && <SeasonList onSeason={(id) => go({ at: "season", id })} />}
      {loc.at === "season" && (
        <>
          <SeasonView id={loc.id} onTeam={openTeam} />
          <SeasonPlay id={loc.id} onTeam={openTeam} />
        </>
      )}
      {loc.at === "bans" && <BansPage onPlayer={openPlayer} />}
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
      trail.push([{ seasons: t("Seasons"), players: t("Players"), bans: t("Bans") }[l.at], l]);
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
              <span className="e2-hit-name">
                <RglMark size={12} /> {t("Open this player's RGL profile")}
              </span>
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

/** The page on rgl.gg, in a header's links column as on a profile. */
function OnRgl({ href }: { href: string }) {
  return (
    <nav className="pp-links" aria-label={t("Elsewhere")}>
      <a href={href} target="_blank" rel="noreferrer">
        <RglMark size={12} /> {t("Open on RGL")} <span aria-hidden>↗</span>
      </a>
    </nav>
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
        <h2>
          <RglMark size={14} /> {t("You on RGL")}
        </h2>
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

// ---- A season's standings and results ----------------------------------------

/** "Week 1 - Vigil" is week "Week 1"; a playoff round keeps its own name. */
const weekOf = (m: RglMatch) => (m.matchName ?? "").split(" - ")[0].trim() || t("Other");

type Standing = { id: number; name: string; tag: string | null; played: number; won: number; lost: number; for: number; against: number };

/** Each division's table from its matches: wins, then losses, then rounds. */
function standings(matches: RglMatch[]): Map<string, Standing[]> {
  const by = new Map<string, Map<number, Standing>>();
  for (const m of matches) {
    if (m.winner === null || m.teams.length < 2) continue;
    const div = m.divisionName ?? "";
    const rows = by.get(div) ?? new Map<number, Standing>();
    by.set(div, rows);
    for (const tm of m.teams) {
      const r = rows.get(tm.teamId) ?? { id: tm.teamId, name: tm.teamName, tag: tm.teamTag, played: 0, won: 0, lost: 0, for: 0, against: 0 };
      rows.set(tm.teamId, r);
      r.played += 1;
      if (m.winner === tm.teamId) r.won += 1;
      else r.lost += 1;
      for (const x of m.maps) {
        r.for += (tm.isHome ? x.homeScore : x.awayScore) ?? 0;
        r.against += (tm.isHome ? x.awayScore : x.homeScore) ?? 0;
      }
    }
  }
  const out = new Map<string, Standing[]>();
  for (const [div, rows] of by)
    out.set(
      div,
      [...rows.values()].sort((a, b) => b.won - a.won || a.lost - b.lost || b.for - b.against - (a.for - a.against) || a.name.localeCompare(b.name)),
    );
  return out;
}

/**
 * A season's standings and results (Flashy: "continue work on the RGL
 * page"). RGL gives only a final rank, so the tables are worked out from
 * the matches, each read on its own at RGL's pace: about 40 s the first
 * time, then kept for half an hour.
 */
function SeasonPlay({ id, onTeam }: { id: number; onTeam: (id: number) => void }) {
  const season = useSeason(id).data;
  const q = useQuery({ queryKey: ["rgl_season_matches", id], queryFn: () => api.getRglSeasonMatches(id), staleTime: KEEP, retry: false });
  const matches = useMemo(() => q.data ?? [], [q.data]);
  const tables = useMemo(() => standings(matches), [matches]);
  const weeks = useMemo(() => [...new Set(matches.map(weekOf))], [matches]);
  // Open on the week being played: the first with a match not yet decided.
  const live = weeks.find((w) => matches.some((m) => weekOf(m) === w && m.winner === null));
  const current = live ?? weeks[weeks.length - 1];
  const [picked, setPicked] = useState<string | null>(null);
  const week = picked && weeks.includes(picked) ? picked : current;
  const order = (season?.divisions ?? []).map((d) => d.name);
  const divs = [...new Set([...order, ...matches.map((m) => m.divisionName ?? "")])].filter((d) => d && (tables.has(d) || matches.some((m) => m.divisionName === d)));
  if (q.error) return <ApiError error={q.error} />;
  if (q.isPending) return <Loading>{t("Reading the season's matches from RGL, one by one: about 40 seconds the first time…")}</Loading>;
  if (matches.length === 0) return null;
  const inWeek = matches.filter((m) => weekOf(m) === week);
  return (
    <>
      <h2 className="ts-section">{t("Standings")}</h2>
      <div className="rgl-standings">
        {divs
          .filter((d) => tables.has(d))
          .map((d) => (
            <section key={d} className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{d}</h2>
                <span className="muted">{t("from the results")}</span>
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
                    <WeekLine key={m.matchId} m={m} onTeam={onTeam} />
                  ))}
                </ol>
              </section>
            );
          })}
      </div>
    </>
  );
}

/** One match of a week: home and away, the score between, the map. */
function WeekLine({ m, onTeam }: { m: RglMatch; onTeam: (id: number) => void }) {
  const home = m.teams.find((x) => x.isHome) ?? m.teams[0];
  const away = m.teams.find((x) => x !== home);
  const hs = m.maps.reduce((n, x) => n + (x.homeScore ?? 0), 0);
  const as = m.maps.reduce((n, x) => n + (x.awayScore ?? 0), 0);
  const side = (x: RglMatch["teams"][number] | undefined, cls: string) =>
    x ? (
      <span className={`${cls}${m.winner === x.teamId ? " won" : ""}`}>
        <button className="linkish" onClick={() => onTeam(x.teamId)}>
          {x.teamName}
        </button>
      </span>
    ) : (
      <span className={cls}>–</span>
    );
  return (
    <li className="rgl-wl">
      <span className="muted rgl-wl-day">{day(m.matchDate)}</span>
      {side(home, "rgl-wl-side l")}
      <span className="rgl-wl-score">{m.isForfeit ? t("forfeit") : m.winner === null ? t("vs") : `${hs}:${as}`}</span>
      {side(away, "rgl-wl-side r")}
      <span className="muted rgl-wl-map">{m.maps.map((x) => x.mapName).join(", ")}</span>
    </li>
  );
}

// ---- The league's bans ---------------------------------------------------------

/** A ban that ends after this year is for good. */
const forGood = (iso: string | null) => !!iso && new Date(iso).getFullYear() > 2100;

function BansPage({ onPlayer }: { onPlayer: (s: string) => void }) {
  const q = useInfiniteQuery({
    queryKey: ["rgl_bans"],
    queryFn: ({ pageParam }) => api.getRglBans(pageParam),
    initialPageParam: 0,
    getNextPageParam: (last, all) => (last.length < 50 ? undefined : all.length * 50),
    staleTime: KEEP,
  });
  const bans: RglBan[] = q.data?.pages.flat() ?? [];
  const now = Date.now();
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">{t("RGL")}</span>
        <h1>{t("Bans")}</h1>
        <p className="hint">{t("The league's bans, newest first, every format's: RGL keeps one list.")}</p>
      </header>
      <section className="panel e2-panel">
        {q.error ? (
          <ApiError error={q.error} />
        ) : q.isPending ? (
          <Loading>{t("Loading the bans…")}</Loading>
        ) : bans.length === 0 ? (
          <p className="hint">{t("No bans.")}</p>
        ) : (
          <div className="table-wrap">
            <table className="match-table rgl-bans">
              <thead>
                <tr>
                  <th>{t("Player")}</th>
                  <th>{t("Reason")}</th>
                  <th>{t("Banned")}</th>
                  <th>{t("Until")}</th>
                </tr>
              </thead>
              <tbody>
                {bans.map((b, i) => {
                  const over = !!b.expiresAt && !forGood(b.expiresAt) && new Date(b.expiresAt).getTime() < now;
                  return (
                    <tr key={`${b.steamId}-${b.createdAt ?? i}`} className={over ? "rgl-ban-over" : undefined}>
                      <td>
                        <button className="linkish" onClick={() => onPlayer(b.steamId)}>
                          {b.alias ?? b.steamId}
                        </button>
                      </td>
                      <td>{b.reason ?? "–"}</td>
                      <td className="nowrap muted">{day(b.createdAt)}</td>
                      <td className="nowrap">
                        {forGood(b.expiresAt) ? <span className="rgl-ban-perm">{t("Permanent")}</span> : over ? <span className="muted">{tx("ended {0}", { "0": day(b.expiresAt) })}</span> : day(b.expiresAt)}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
        {q.hasNextPage && (
          <button className="e2-more rgl-bans-more" onClick={() => void q.fetchNextPage()} disabled={q.isFetchingNextPage}>
            {q.isFetchingNextPage ? t("Loading…") : t("Show more")}
          </button>
        )}
      </section>
    </>
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
          <OnRgl href={`https://rgl.gg/Public/Team?t=${v.id}`} />
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
          <OnRgl href={`https://rgl.gg/Public/PlayerProfile?p=${p.steamId}`} />
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
