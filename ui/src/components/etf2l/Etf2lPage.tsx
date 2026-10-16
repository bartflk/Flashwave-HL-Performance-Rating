import { useEffect, useMemo, useRef, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { Fixture, ResultRow } from "../../api/types";
import { formatDate } from "../../lib/format";
import { hasPendingPlayer, onOpenSeason, onOpenTeam, onPlayerAsked, openPlayer, takePendingSeason, takePendingTeam } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { DivisionBadge } from "../players/PlayerProfile";
import { PlayersPage } from "../players/PlayersPage";
import { SeasonScreen, SeasonsGrid, TeamAvatar, TeamScreen, seasonTitle, useFixtures, wl } from "../teams/TeamsPage";
import { YourTeamsCompact, YourTeamsStrip, useYourTeams } from "../teams/YourTeams";
import { Countdown, clockTime, fullDate } from "./Countdown";
import { NewsCard, NewsList, NewsReader, useNews } from "./News";
import { FixtureRow, LeagueMatchPage, MatchesPage } from "./Matches";
import "../teams/teams.css";
import "./etf2l.css";

/**
 * The ETF2L tab (Q74, Flashy; PLAN §27): Teams and Players as one tab laid
 * out like etf2l.org -- its own menu, a search over players and teams, and a
 * front page with today's officials, the news and the latest results --
 * in the app's own style, Highlander only.
 *
 * Where you are is a stack, like a browser's: the menu, a breadcrumb, a team
 * or a season each go on top, and the mouse's back button (or Alt+Left)
 * takes the top off while there is more than one, before the app's own
 * history sees it.
 */

type Section = "home" | "matches" | "seasons" | "teams" | "players" | "news";

type Loc =
  | { at: "home" }
  | { at: "matches" }
  | { at: "match"; id: number; fixture?: Fixture; from: Loc }
  | { at: "seasons" }
  | { at: "season"; season: number }
  | { at: "team"; teamId: number; from: Loc }
  | { at: "teams" }
  | { at: "players" }
  | { at: "news" }
  | { at: "post"; id: number };

/** The menu entry a place belongs to: a team under whatever it was opened from. */
function sectionOf(l: Loc): Section {
  if (l.at === "season") return "seasons";
  if (l.at === "post") return "news";
  if (l.at === "team" || l.at === "match") return sectionOf(l.from);
  return l.at;
}

/** What a new team or match page goes back to: never another of those. */
function base(l: Loc): Loc {
  return l.at === "team" || l.at === "match" ? l.from : l;
}

const same = (a: Loc, b: Loc) => JSON.stringify(a) === JSON.stringify(b);

export function Etf2lPage({ active, onOpenMatch }: { active: boolean; onOpenMatch: (logId: number) => void }) {
  const [stack, setStack] = useState<Loc[]>(() => {
    const team = takePendingTeam();
    if (team !== null) return [{ at: "team", teamId: team, from: { at: "home" } }];
    const season = takePendingSeason();
    if (season !== null) return [{ at: "season", season }];
    return [hasPendingPlayer() ? { at: "players" } : { at: "home" }];
  });
  const loc = stack[stack.length - 1];
  const go = (l: Loc) => {
    setStack((s) => (same(s[s.length - 1], l) ? s : [...s, l].slice(-50)));
    window.scrollTo({ top: 0 });
  };
  const openTeam = (teamId: number) => setStack((s) => [...s, { at: "team", teamId, from: base(s[s.length - 1]) }]);
  const openMatch = (id: number, fixture?: Fixture) => {
    setStack((s) => [...s, { at: "match", id, fixture, from: base(s[s.length - 1]) }]);
    window.scrollTo({ top: 0 });
  };
  useEffect(() => onOpenTeam((teamId) => setStack((s) => [...s, { at: "team", teamId, from: base(s[s.length - 1]) }])), []);
  useEffect(() => onOpenSeason((season) => setStack((s) => [...s, { at: "season", season }])), []);
  useEffect(() => onPlayerAsked(() => setStack((s) => (s[s.length - 1].at === "players" ? s : [...s, { at: "players" }]))), []);

  // Mouse 4 / Alt+Left step back within the tab first (Flashy's Teams rule,
  // kept): capture phase on window runs before the app's history, and
  // stopPropagation keeps it from also going back a tab.
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
  // Players keeps its search and profile while you look elsewhere.
  const [playersSeen, setPlayersSeen] = useState(section === "players");
  if (section === "players" && !playersSeen) setPlayersSeen(true);

  const menu: Array<[Section, string]> = [
    ["home", t("Home")],
    ["matches", t("Matches")],
    ["seasons", t("Seasons")],
    ["teams", t("Teams")],
    ["players", t("Players")],
    ["news", t("News")],
  ];

  return (
    <div className="content e2-page">
      <header className="e2-head">
        <div className="e2-brand">
          <span className="e2-eyebrow">{t("ETF2L Highlander")}</span>
          <nav className="e2-menu" aria-label={t("ETF2L")}>
            {menu.map(([id, label]) => (
              <button key={id} className={section === id ? "on" : undefined} aria-current={section === id ? "page" : undefined} onClick={() => go({ at: id })}>
                {label}
              </button>
            ))}
          </nav>
        </div>
        <LeagueSearch onTeam={openTeam} />
      </header>
      <Crumbs loc={loc} go={go} />

      {loc.at === "home" && <Home onTeam={openTeam} onMatch={openMatch} go={go} />}
      {loc.at === "matches" && <MatchesPage onTeam={openTeam} onMatch={openMatch} />}
      {loc.at === "match" && <LeagueMatchPage key={loc.id} matchId={loc.id} fixture={loc.fixture} onTeam={openTeam} onOpenLog={onOpenMatch} />}
      {loc.at === "news" && <NewsList onPost={(id) => go({ at: "post", id })} />}
      {loc.at === "post" && <NewsReader key={loc.id} id={loc.id} />}
      {loc.at === "seasons" && <SeasonsGrid onSeason={(season) => go({ at: "season", season })} />}
      {loc.at === "season" && <SeasonScreen season={loc.season} onTeam={openTeam} />}
      {loc.at === "team" && <TeamScreen key={loc.teamId} teamId={loc.teamId} onTeam={openTeam} />}
      {loc.at === "teams" && <TeamsLanding onTeam={openTeam} />}
      {playersSeen && (
        <div hidden={loc.at !== "players"}>
          <PlayersPage onOpenMatch={onOpenMatch} />
        </div>
      )}
    </div>
  );
}

// ---- Breadcrumbs ---------------------------------------------------------

function Crumbs({ loc, go }: { loc: Loc; go: (l: Loc) => void }) {
  const tiles = useQuery({ queryKey: ["seasons_overview"], queryFn: api.getSeasonsOverview, staleTime: 5 * 60_000 }).data;
  const teamId = loc.at === "team" ? loc.teamId : null;
  const team = useQuery({ queryKey: ["team", teamId], queryFn: () => api.getTeam(teamId!), enabled: teamId !== null }).data;
  const matchId = loc.at === "match" ? loc.id : null;
  const lm = useQuery({ queryKey: ["league_match", matchId], queryFn: () => api.getLeagueMatch(matchId!), enabled: matchId !== null }).data;
  const mf = loc.at === "match" ? loc.fixture : undefined;
  const matchTitle = lm ? `${lm.clan1.name} vs ${lm.clan2.name}` : mf ? `${mf.clan1.name} vs ${mf.clan2.name}` : null;
  const postId = loc.at === "post" ? loc.id : null;
  const post = useQuery({ queryKey: ["etf2l_news_post", postId], queryFn: () => api.getNewsPost(postId!), enabled: postId !== null, staleTime: 30 * 60_000 }).data;
  const trail: Array<[string, Loc]> = [];
  const add = (l: Loc) => {
    if (l.at === "team") {
      add(l.from);
      trail.push([team?.name ?? t("Team"), l]);
    } else if (l.at === "season") {
      trail.push([t("Seasons"), { at: "seasons" }]);
      const tile = tiles?.find((s) => s.season === l.season);
      trail.push([tile ? seasonTitle(tile) : t("Season {0}", { "0": l.season }), l]);
    } else if (l.at === "match") {
      add(l.from);
      trail.push([matchTitle ?? t("Match"), l]);
    } else if (l.at === "post") {
      trail.push([t("News"), { at: "news" }]);
      trail.push([post?.title ?? t("Post"), l]);
    } else if (l.at !== "home") {
      trail.push([{ matches: t("Matches"), seasons: t("Seasons"), teams: t("Teams"), players: t("Players"), news: t("News") }[l.at], l]);
    }
  };
  add(loc);
  if (trail.length === 0) return null;
  return (
    <nav className="e2-crumbs" aria-label={t("Where you are")}>
      <button onClick={() => go({ at: "home" })}>{t("ETF2L")}</button>
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

// ---- Search over players and teams -------------------------------------

function LeagueSearch({ onTeam }: { onTeam: (id: number) => void }) {
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const term = q.trim();
  const players = useQuery({ queryKey: ["search_catalogue", term], queryFn: () => api.searchCatalogue(term), enabled: term.length >= 2, placeholderData: keepPreviousData });
  const teams = useQuery({ queryKey: ["search_teams", term], queryFn: () => api.searchTeams(term), enabled: term.length >= 2, placeholderData: keepPreviousData });
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
  const ps = term.length >= 2 ? (players.data ?? []).slice(0, 6) : [];
  const ts = term.length >= 2 ? (teams.data ?? []).slice(0, 6) : [];
  return (
    <div className="e2-search" ref={box}>
      <input
        type="search"
        value={q}
        placeholder={t("Player, team or SteamID")}
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={(e) => {
          if (e.key === "Escape") done();
        }}
        aria-label={t("Search ETF2L players and teams")}
      />
      {open && term.length >= 2 && (
        <div className="e2-search-panel">
          {ts.length > 0 && <h4>{t("Teams")}</h4>}
          {ts.map((x) => (
            <button
              key={`t${x.teamId}`}
              className="e2-hit"
              onClick={() => {
                done();
                onTeam(x.teamId);
              }}
            >
              <TeamAvatar src={x.avatar} />
              <span className="e2-hit-name">{x.name}</span>
              <span className="e2-hit-meta">{x.lastSeason !== null ? t("Season {0}", { "0": x.lastSeason }) : tx("{0} officials", { "0": x.officials })}</span>
            </button>
          ))}
          {ps.length > 0 && <h4>{t("Players")}</h4>}
          {ps.map((x) => (
            // Someone from your logs who never played an official: still
            // there to open, greyed (Flashy).
            <button
              key={`p${x.accountId}`}
              className={x.officials === 0 ? "e2-hit e2-hit-out" : "e2-hit"}
              title={x.officials === 0 ? t("Not in the league: only in your matches") : undefined}
              onClick={() => {
                done();
                openPlayer(x.accountId);
              }}
            >
              <span className="e2-hit-name">{x.name}</span>
              {x.highest && <DivisionBadge d={x.highest} />}
              <span className="e2-hit-meta">{x.officials > 0 ? tx("{0} officials", { "0": x.officials }) : t("not in the league")}</span>
            </button>
          ))}
          {ts.length === 0 && ps.length === 0 && !players.isFetching && !teams.isFetching && <p className="hint">{t("Nothing found.")}</p>}
        </div>
      )}
    </div>
  );
}

// ---- Home: etf2l.org's front page ----------------------------------------

type When = "today" | "tomorrow" | "week";

/** The day a fixture falls on, in local time, counted from today. */
function dayOffset(time: number): number {
  const d = new Date(time * 1000);
  const today = new Date();
  const a = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const b = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  return Math.round((a - b) / 86_400_000);
}

function Home({ onTeam, onMatch, go }: { onTeam: (id: number) => void; onMatch: (id: number, f?: Fixture) => void; go: (l: Loc) => void }) {
  const fixtures = useFixtures();
  const mine = new Set(useYourTeams().map((e) => e.teamId));
  // Your next official, if one of your teams has one coming.
  const next = fixtures.find((f) => mine.has(f.clan1.id) || mine.has(f.clan2.id)) ?? null;
  const [when, setWhen] = useState<When>(() => (fixtures.some((f) => dayOffset(f.time) <= 0) ? "today" : "week"));
  const shown = fixtures.filter((f) => {
    const d = dayOffset(f.time);
    return when === "today" ? d <= 0 : when === "tomorrow" ? d === 1 : d <= 7;
  });
  return (
    <div className="e2-home">
      <div className="e2-main">
        {next && <NextMatch f={next} side={mine.has(next.clan1.id) ? next.clan1.id : next.clan2.id} onTeam={onTeam} onOpen={() => onMatch(next.matchId, next)} />}
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <h2>{t("Officials")}</h2>
            <button className="e2-more e2-more-left" onClick={() => go({ at: "matches" })}>
              {t("All matches")} ›
            </button>
            <div className="km-seg" role="radiogroup" aria-label={t("When")}>
              {(
                [
                  ["today", t("Today")],
                  ["tomorrow", t("Tomorrow")],
                  ["week", t("This week")],
                ] as Array<[When, string]>
              ).map(([id, label]) => (
                <button key={id} role="radio" aria-checked={when === id} className={when === id ? "on" : undefined} onClick={() => setWhen(id)}>
                  {label}
                </button>
              ))}
            </div>
          </div>
          {shown.length === 0 ? (
            <p className="hint">{when === "today" ? t("No officials today.") : when === "tomorrow" ? t("No officials tomorrow.") : t("No officials in the next week.")}</p>
          ) : (
            <ol className="e2-fixtures">
              {shown.map((f) => (
                <FixtureRow key={f.matchId} f={f} mine={mine} onTeam={onTeam} onOpen={() => onMatch(f.matchId, f)} showDay={when === "week"} />
              ))}
            </ol>
          )}
        </section>
        <LatestNews go={go} />
      </div>
      <aside className="e2-rail">
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <h2>{t("Your teams")}</h2>
          </div>
          <YourTeamsCompact onTeam={onTeam} />
        </section>
        <LiveSeason go={go} fixtures={fixtures} />
        <RecentResults onTeam={onTeam} onMatch={onMatch} go={go} />
      </aside>
    </div>
  );
}

/** Your team's next official, large: the two sides, the countdown between
 *  them, and what is known of it. */
function NextMatch({ f, side, onTeam, onOpen }: { f: Fixture; side: number; onTeam: (id: number) => void; onOpen: () => void }) {
  const flip = f.clan2.id === side;
  const [a, b] = flip ? [f.clan2, f.clan1] : [f.clan1, f.clan2];
  const r = flip ? { ...f.h2h, won: f.h2h.lost, lost: f.h2h.won } : f.h2h;
  const team = (x: Fixture["clan1"], end: "l" | "r") => (
    <button className={`e2-nm-team ${end}`} onClick={() => onTeam(x.id)} title={t("Open {0}", { "0": x.name })}>
      <TeamAvatar src={x.avatar} />
      <span>{x.name}</span>
    </button>
  );
  return (
    <section className="e2-nm">
      <div className="e2-nm-top">
        <span className="e2-label">{t("Your next match")}</span>
        <span className="e2-nm-what">
          {f.division && f.tier !== null && <DivisionBadge d={{ name: f.division, tier: f.tier }} />}
          {f.round && <span className="muted">{f.round}</span>}
        </span>
      </div>
      <div className="e2-nm-row">
        {team(a, "l")}
        <div className="e2-nm-mid">
          <Countdown time={f.time} big />
          <span className="muted" title={fullDate(f.time)}>
            {formatDate(f.time)} · {clockTime(f.time)}
          </span>
        </div>
        {team(b, "r")}
      </div>
      <div className="e2-nm-foot">
        <span>{r.played === 0 ? t("First meeting in an official") : <>{t("Head-to-head")} <strong>{wl(r)}</strong></>}</span>
        <span className="muted">{f.maps.length > 0 ? f.maps.join(" · ") : t("Maps still to be picked")}</span>
        <button className="e2-more" onClick={onOpen}>
          {t("Match page")} ›
        </button>
      </div>
    </section>
  );
}

function LatestNews({ go }: { go: (l: Loc) => void }) {
  const q = useNews();
  const items = (q.data ?? []).slice(0, 3);
  if (q.isPending) return <p className="hint">{t("Loading the news…")}</p>;
  if (items.length === 0) return null;
  return (
    <section className="e2-block">
      <div className="e2-block-head">
        <h2 className="ts-section">{t("Latest news")}</h2>
        <button className="e2-more" onClick={() => go({ at: "news" })}>
          {t("All news")} ›
        </button>
      </div>
      <div className="np-grid np-grid-home">
        {items.map((n) => (
          <NewsCard key={n.id} n={n} onOpen={() => go({ at: "post", id: n.id })} />
        ))}
      </div>
    </section>
  );
}

function LiveSeason({ go, fixtures }: { go: (l: Loc) => void; fixtures: Fixture[] }) {
  const tiles = useQuery({ queryKey: ["seasons_overview"], queryFn: api.getSeasonsOverview, staleTime: 5 * 60_000 }).data;
  const live = tiles?.[0] && !tiles[0].champion ? tiles[0] : null;
  const week = useMemo(() => {
    const weeks = fixtures.filter((f) => f.season === live?.season && f.week !== null).map((f) => f.week!);
    return weeks.length > 0 ? Math.min(...weeks) : null;
  }, [fixtures, live]);
  if (!live) return null;
  return (
    <section className="panel e2-panel e2-live">
      <div className="e2-panel-head">
        <h2>{seasonTitle(live)}</h2>
        <span className="e2-live-chip">{t("Live")}</span>
      </div>
      <p className="hint">
        {week !== null ? tx("Week {0} next", { "0": week }) : null}
        {week !== null && " · "}
        {tx("{0} teams", { "0": live.teams })}
      </p>
      <button className="km-chip" onClick={() => go({ at: "season", season: live.season })}>
        {t("Open season")} ›
      </button>
    </section>
  );
}

function RecentResults({ onTeam, onMatch, go }: { onTeam: (id: number) => void; onMatch: (id: number) => void; go: (l: Loc) => void }) {
  const q = useQuery({ queryKey: ["recent_results"], queryFn: () => api.getRecentResults(8), staleTime: 5 * 60_000 });
  const rows = q.data ?? [];
  if (rows.length === 0) return null;
  const side = (r: ResultRow, x: ResultRow["clan1"], score: number | null, won: boolean) => (
    <button
      className={won ? "e2-res-team won" : "e2-res-team"}
      onClick={(e) => {
        e.stopPropagation();
        onTeam(x.id);
      }}
      title={t("Open {0}", { "0": x.name })}
    >
      <span>{x.name}</span>
      <span className="e2-res-score">{r.defaultWin && score === null ? "–" : (score ?? "–")}</span>
    </button>
  );
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Recent results")}</h2>
        <button className="e2-more" onClick={() => go({ at: "matches" })}>
          {t("All results")} ›
        </button>
      </div>
      <ol className="e2-results">
        {rows.map((r) => {
          const a = r.r1 ?? 0;
          const b = r.r2 ?? 0;
          return (
            <li key={r.matchId} title={fullDate(r.time)} className="e2-res-row" onClick={() => onMatch(r.matchId)}>
              {side(r, r.clan1, r.r1, a > b)}
              {side(r, r.clan2, r.r2, b > a)}
              <span className="e2-res-meta">
                {r.division && r.tier !== null && <DivisionBadge d={{ name: r.division, tier: r.tier }} />}
                {r.defaultWin && <span className="muted">{t("default")}</span>}
              </span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}

// ---- Teams: search, then your own -----------------------------------------

function TeamsLanding({ onTeam }: { onTeam: (id: number) => void }) {
  const [q, setQ] = useState("");
  const term = q.trim();
  const hits = useQuery({ queryKey: ["search_teams", term], queryFn: () => api.searchTeams(term), enabled: term.length >= 2, placeholderData: keepPreviousData });
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">{t("ETF2L Highlander")}</span>
        <h1>{t("Teams")}</h1>
      </header>
      <div className="e2-search e2-search-big">
        <input type="search" value={q} placeholder={t("A team's name")} onChange={(e) => setQ(e.target.value)} aria-label={t("Search teams")} />
      </div>
      {term.length >= 2 && (
        <section className="panel e2-panel">
          {(hits.data ?? []).length === 0 && !hits.isFetching ? (
            <p className="hint">{t("No team by that name.")}</p>
          ) : (
            <ol className="e2-teamhits">
              {(hits.data ?? []).map((x) => (
                <li key={x.teamId}>
                  <button className="e2-hit" onClick={() => onTeam(x.teamId)}>
                    <TeamAvatar src={x.avatar} />
                    <span className="e2-hit-name">{x.name}</span>
                    <span className="e2-hit-meta">
                      {x.lastSeason !== null && t("last in Season {0}", { "0": x.lastSeason })}
                      {x.lastSeason !== null && " · "}
                      {tx("{0} officials", { "0": x.officials })}
                    </span>
                  </button>
                </li>
              ))}
            </ol>
          )}
        </section>
      )}
      <YourTeamsStrip onTeam={onTeam} />
    </>
  );
}
