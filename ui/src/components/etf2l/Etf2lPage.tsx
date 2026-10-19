import { useEffect, useRef, useState } from "react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type CatTeam, type Fixture, type LeagueTableRow, type RecruitPost, type ResultRow, type SeasonTile } from "../../api/types";
import { formatDate } from "../../lib/format";
import { hasPendingPlayer, onOpenLeagueMatch, onOpenSeason, onOpenTeam, onPlayerAsked, openPlayer, takePendingLeagueMatch, takePendingSeason, takePendingTeam } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { DivisionBadge } from "../players/PlayerProfile";
import { MedalGlyph } from "../players/MedalGlyph";
import { PlayersPage } from "../players/PlayersPage";
import { SeasonScreen, SeasonsGrid, TeamAvatar, TeamScreen, seasonTitle, useBanner, useFixtures, wl } from "../teams/TeamsPage";
import { YourTeamsCompact, useYourTeams } from "../teams/YourTeams";
import { Countdown, clockTime, fullDate } from "./Countdown";
import { NewsCard, NewsList, NewsReader, useNews } from "./News";
import { FixtureRow, LeagueMatchPage, MatchesPage, SeasonFixtures } from "./Matches";
import { SeasonDivisions, laneOf } from "./Brackets";
import { TeamsLanding } from "./Teams";
import { RecruitPostPage, RecruitPreview, RecruitmentPage, type Kind as RecruitKind } from "./Recruitment";
import "../teams/teams.css";
import "./etf2l.css";
import { checkEtf2lDaily } from "../../lib/etf2lcheck";
import { Etf2lMark } from "../Etf2lMark";
import { Loading, Refreshing, Spinner } from "../Spinner";

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

type Section = "home" | "matches" | "seasons" | "teams" | "players" | "news" | "recruitment";

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
  | { at: "recruitment" }
  | { at: "recruit"; post: RecruitPost; kind: RecruitKind }
  | { at: "post"; id: number };

/** The menu entry a place belongs to: a team under whatever it was opened from. */
function sectionOf(l: Loc): Section {
  if (l.at === "season") return "seasons";
  if (l.at === "post") return "news";
  if (l.at === "recruit") return "recruitment";
  if (l.at === "team" || l.at === "match") return sectionOf(l.from);
  return l.at;
}

/** What a new team or match page goes back to: never another of those. */
function base(l: Loc): Loc {
  return l.at === "team" || l.at === "match" ? l.from : l;
}

const same = (a: Loc, b: Loc) => JSON.stringify(a) === JSON.stringify(b);

/**
 * Read ETF2L again now (Flashy: "dd14 just won their season today ... want
 * the teams and players with medals to be updated with this, like a
 * refresh"): seasons, results and tables, then every page that shows a
 * medal, a table or a result is asked again.
 */
function RefreshEtf2l() {
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  async function go() {
    setBusy(true);
    setMsg(null);
    try {
      const r = await api.refreshEtf2l();
      setMsg(t("{0} results read", { "0": r.results.toLocaleString() }));
      void qc.invalidateQueries();
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <span className="e2-refresh">
      <button className="km-chip" onClick={() => void go()} disabled={busy} title={t("Read ETF2L's seasons, results and final standings again, for medals and tables")}>
        {busy ? <Spinner /> : <span aria-hidden>⟳</span>} {busy ? t("Reading ETF2L…") : t("Refresh ETF2L")}
      </button>
      {msg && <span className="hint">{msg}</span>}
    </span>
  );
}

export function Etf2lPage({ active, onOpenMatch }: { active: boolean; onOpenMatch: (logId: number) => void }) {
  const [stack, setStack] = useState<Loc[]>(() => {
    const match = takePendingLeagueMatch();
    if (match !== null) return [{ at: "match", id: match, from: { at: "home" } }];
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
  useEffect(
    () =>
      onOpenLeagueMatch((id) => {
        setStack((s) => [...s, { at: "match", id, from: base(s[s.length - 1]) }]);
        window.scrollTo({ top: 0 });
      }),
    [],
  );
  useEffect(() => onPlayerAsked(() => setStack((s) => (s[s.length - 1].at === "players" ? s : [...s, { at: "players" }]))), []);

  // Mouse 4 / Alt+Left step back within the tab first (Flashy's Teams rule,
  // kept): capture phase on window runs before the app's history, and
  // stopPropagation keeps it from also going back a tab.
  const depth = useRef(stack.length);
  const qcDaily = useQueryClient();
  // The daily check, whenever the tab is opened (Flashy: "whenever this
  // info is checked"): skipped by the backend if it ran in the last day.
  useEffect(() => {
    if (active) void checkEtf2lDaily(qcDaily);
  }, [active, qcDaily]);
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
    ["recruitment", t("Recruitment")],
  ];

  return (
    <div className="content e2-page">
      <header className="e2-head">
        <div className="e2-brand">
          <span className="e2-eyebrow">
            <Etf2lMark size={16} /> {t("ETF2L Highlander")}
          </span>
          <nav className="e2-menu" aria-label={t("ETF2L")}>
            {menu.map(([id, label]) => (
              <button key={id} className={section === id ? "on" : undefined} aria-current={section === id ? "page" : undefined} onClick={() => go({ at: id })}>
                {label}
              </button>
            ))}
          </nav>
        </div>
        <div className="e2-head-tools">
          <RefreshEtf2l />
          <LeagueSearch onTeam={openTeam} />
        </div>
      </header>
      <Crumbs loc={loc} go={go} />

      {loc.at === "home" && <Home onTeam={openTeam} onMatch={openMatch} go={go} />}
      {loc.at === "matches" && <MatchesPage onTeam={openTeam} onMatch={openMatch} />}
      {loc.at === "match" && <LeagueMatchPage key={loc.id} matchId={loc.id} fixture={loc.fixture} onTeam={openTeam} onOpenLog={onOpenMatch} />}
      {loc.at === "news" && <NewsList onPost={(id) => go({ at: "post", id })} />}
      {loc.at === "post" && <NewsReader key={loc.id} id={loc.id} />}
      {loc.at === "seasons" && <SeasonsGrid onSeason={(season) => go({ at: "season", season })} />}
      {loc.at === "season" && (
        <>
          <SeasonScreen season={loc.season} onTeam={openTeam} tables={false} />
          <SeasonDivisions season={loc.season} onTeam={openTeam} onMatch={(id) => openMatch(id)} />
          <SeasonFixtures season={loc.season} onTeam={openTeam} onMatch={(id) => openMatch(id)} />
        </>
      )}
      {loc.at === "recruitment" && <RecruitmentPage onOpen={(post, kind) => go({ at: "recruit", post, kind })} />}
      {loc.at === "recruit" && <RecruitPostPage key={loc.post.id} p={loc.post} kind={loc.kind} onTeam={openTeam} />}
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
  const mp = useQuery({ queryKey: ["match_page", matchId], queryFn: () => api.getMatchPage(matchId!), enabled: matchId !== null, staleTime: 2 * 60_000 }).data;
  const matchTitle = lm
    ? `${lm.clan1.name} vs ${lm.clan2.name}`
    : mf
      ? `${mf.clan1.name} vs ${mf.clan2.name}`
      : mp && mp.teams.length >= 2
        ? `${mp.teams[0].name} vs ${mp.teams[1].name}`
        : null;
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
    } else if (l.at === "recruit") {
      trail.push([t("Recruitment"), { at: "recruitment" }]);
      trail.push([l.post.name, l]);
    } else if (l.at === "post") {
      trail.push([t("News"), { at: "news" }]);
      trail.push([post?.title ?? t("Post"), l]);
    } else if (l.at !== "home") {
      trail.push([{ matches: t("Matches"), seasons: t("Seasons"), teams: t("Teams"), players: t("Players"), news: t("News"), recruitment: t("Recruitment") }[l.at], l]);
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
    <div className="e2-search has-refresh" ref={box}>
      <Refreshing on={term.length >= 2 && (players.isFetching || teams.isFetching)} />
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

function useSeasonTiles() {
  return useQuery({ queryKey: ["seasons_overview"], queryFn: api.getSeasonsOverview, staleTime: 5 * 60_000 }).data;
}

function useLeagueTables(season: number | null) {
  return useQuery({ queryKey: ["leagues", season], queryFn: () => api.getLeagues(season!), enabled: season !== null, staleTime: 5 * 60_000 }).data;
}

/** "1st", "2nd", "3rd", "4th". */
function place(n: number): string {
  return n === 1 ? t("1st") : n === 2 ? t("2nd") : n === 3 ? t("3rd") : t("{0}th", { "0": n });
}

/** Where the season is (Flashy: group stage or playoffs?): read from the
 *  coming week's officials -- their round names and competitions, as the
 *  brackets read them. A season whose lower divisions are in their playoffs
 *  while the top ones still play groups gives both, playoffs first. */
function stagesOf(fixtures: Fixture[], season: number): Array<{ phase: string; round: string | null }> {
  const soon = fixtures.filter((f) => f.season === season && dayOffset(f.time) <= 7);
  const lane = (f: Fixture) =>
    laneOf({ round: f.round, stage: /3rd place/i.test(f.competition) ? "3rd Place" : /playoffs/i.test(f.competition) ? "Playoffs" : "regular" });
  const label = (f: Fixture) => (lane(f) === "group" ? (f.week !== null ? t("Week {0}", { "0": f.week }) : f.round) : f.round);
  const top = (list: Fixture[]) => {
    const count = new Map<string, number>();
    for (const f of list) {
      const k = label(f);
      if (k) count.set(k, (count.get(k) ?? 0) + 1);
    }
    return [...count.entries()].sort((x, y) => y[1] - x[1])[0]?.[0] ?? null;
  };
  const playoffs = soon.filter((f) => lane(f) !== "group");
  const groups = soon.filter((f) => lane(f) === "group");
  const out: Array<{ phase: string; round: string | null }> = [];
  if (playoffs.length > 0) out.push({ phase: t("Playoffs"), round: top(playoffs) });
  if (groups.length > 0) out.push({ phase: t("Group stage"), round: top(groups) });
  return out;
}

function Home({ onTeam, onMatch, go }: { onTeam: (id: number) => void; onMatch: (id: number, f?: Fixture) => void; go: (l: Loc) => void }) {
  const fixtures = useFixtures();
  const mine = new Set(useYourTeams().map((e) => e.teamId));
  const tiles = useSeasonTiles();
  const latest = tiles?.[0] ?? null;
  // Your next official, if one of your teams has one coming.
  const next = fixtures.find((f) => mine.has(f.clan1.id) || mine.has(f.clan2.id)) ?? null;
  const [when, setWhen] = useState<When>(() => (fixtures.some((f) => dayOffset(f.time) <= 0) ? "today" : "week"));
  const shown = fixtures.filter((f) => {
    const d = dayOffset(f.time);
    return when === "today" ? d <= 0 : when === "tomorrow" ? d === 1 : d <= 7;
  });
  // The season the listed officials are in, said once when they share one.
  const seasons = [...new Set(shown.map((f) => f.season))];
  const seasonLine = seasons.length === 1 && seasons[0] !== null && latest?.season === seasons[0] ? seasonTitle(latest) : null;
  return (
    <div className="e2-home">
      {latest && <SeasonStrip tile={latest} fixtures={fixtures} go={go} />}
      <div className="e2-main">
        {next && <NextMatch f={next} side={mine.has(next.clan1.id) ? next.clan1.id : next.clan2.id} onTeam={onTeam} onOpen={() => onMatch(next.matchId, next)} />}
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <div className="e2-titles">
              <h2>{t("Officials")}</h2>
              {seasonLine && <span className="muted">{seasonLine}</span>}
            </div>
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
                <FixtureRow key={f.matchId} f={f} mine={mine} onTeam={onTeam} onOpen={() => onMatch(f.matchId, f)} showDay={when === "week"} showRound />
              ))}
            </ol>
          )}
        </section>
        <LatestNews go={go} />
      </div>
      <aside className="e2-rail">
        <YourSeason season={latest?.season ?? null} fixtures={fixtures} onTeam={onTeam} onMatch={onMatch} />
        <DivisionLeaders season={latest?.season ?? null} onTeam={onTeam} go={go} />
        <RecentResults onTeam={onTeam} onMatch={onMatch} go={go} />
        <RecruitPreview onAll={() => go({ at: "recruitment" })} onOpen={(post, kind) => go({ at: "recruit", post, kind })} />
      </aside>
    </div>
  );
}

/** The season across the top: its banner, where it is, what is in it. */
function SeasonStrip({ tile, fixtures, go }: { tile: SeasonTile; fixtures: Fixture[]; go: (l: Loc) => void }) {
  const banner = useBanner(tile, true);
  // Live while it still has officials coming, or ended in the last few days:
  // a lower division's final is played first and does not end the season.
  const live = fixtures.some((f) => f.season === tile.season) || tile.to * 1000 > Date.now() - 3 * 86_400_000;
  const stages = live ? stagesOf(fixtures, tile.season) : [];
  const upcoming = fixtures.filter((f) => f.season === tile.season).length;
  return (
    <section className={`e2-strip ${banner ? "" : "e2-strip-plain"}`} style={banner ? { backgroundImage: `url("${banner}")` } : undefined}>
      <div className="e2-strip-text">
        <span className="e2-strip-eyebrow">
          {t("ETF2L Highlander")}
          {live && <span className="e2-live-chip">{t("Live")}</span>}
        </span>
        <h1>{seasonTitle(tile)}</h1>
        <span className="e2-strip-dates">
          {formatDate(tile.from, true)} – {formatDate(tile.to, true)}
        </span>
        <div className="e2-strip-stats">
          {stages.map((st) => (
            <span key={st.phase}>
              <strong>{st.phase}</strong>
              <em>{st.round ?? t("now")}</em>
            </span>
          ))}
          <span>
            <strong>{tile.teams}</strong>
            <em>{t("teams")}</em>
          </span>
          <span>
            <strong>{tile.matches}</strong>
            <em>{t("officials played")}</em>
          </span>
          {live && (
            <span>
              <strong>{upcoming}</strong>
              <em>{t("scheduled")}</em>
            </span>
          )}
        </div>
        <span className="e2-strip-divs">
          {tile.divisions.map((d) => (
            <DivisionBadge key={d.name} d={d} />
          ))}
        </span>
      </div>
      <div className="e2-strip-actions">
        <button className="km-chip" onClick={() => go({ at: "season", season: tile.season })}>
          {t("Standings & fixtures")} ›
        </button>
        <button className="km-chip" onClick={() => go({ at: "seasons" })}>
          {t("All seasons")} ›
        </button>
      </div>
    </section>
  );
}

/** Your teams in the season: division, place, record and next match. Without
 *  a team in it, your teams as before. */
function YourSeason({ season, fixtures, onTeam, onMatch }: { season: number | null; fixtures: Fixture[]; onTeam: (id: number) => void; onMatch: (id: number, f?: Fixture) => void }) {
  const tables = useLeagueTables(season);
  const yours = useYourTeams();
  const rows = (tables?.divisions ?? []).flatMap((d) =>
    d.teams
      .map((row, i) => ({ row, division: d.division, place: i + 1, of: d.teams.length, tier: tierOf(d.division) }))
      .filter((x) => yours.some((e) => e.teamId === x.row.teamId)),
  );
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{rows.length > 0 ? t("Your season") : t("Your teams")}</h2>
      </div>
      {rows.length === 0 ? (
        <YourTeamsCompact onTeam={onTeam} />
      ) : (
        <ul className="e2-yours">
          {rows.map(({ row, division, place: p, of, tier }) => {
            const next = fixtures.find((f) => f.clan1.id === row.teamId || f.clan2.id === row.teamId);
            return (
              <li key={row.teamId}>
                <button className="e2-yours-team" onClick={() => onTeam(row.teamId)}>
                  <TeamAvatar src={row.avatar} />
                  <span className="e2-yours-name">{row.name}</span>
                  <DivisionBadge d={{ name: division, tier }} />
                </button>
                <div className="e2-yours-stats">
                  <span>
                    <strong>{place(p)}</strong> {tx("of {0}", { "0": of })}
                  </span>
                  <span>
                    <strong>{wl(row.record)}</strong>
                  </span>
                  <span className="muted">
                    {row.scoreFor}:{row.scoreAgainst}
                  </span>
                </div>
                {next && (
                  <button className="e2-yours-next" onClick={() => onMatch(next.matchId, next)}>
                    <span className="muted">{t("Next")}</span>
                    <span className="e2-yours-opp">{next.clan1.id === row.teamId ? next.clan2.name : next.clan1.name}</span>
                    <Countdown time={next.time} />
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

/** The ladder's tier for a division name, for its badge's colour. */
function tierOf(division: string): number {
  const d = division.toLowerCase();
  if (d.startsWith("prem")) return 0;
  if (d.startsWith("high")) return 1;
  if (d.startsWith("mid")) return 2;
  if (d.startsWith("low")) return 3;
  return 4;
}

/** Each division of the season: its winner once its final is played (a gold
 *  medal), its leader until then. The champions live here rather than on
 *  the banner (Flashy). */
function DivisionLeaders({ season, onTeam, go }: { season: number | null; onTeam: (id: number) => void; go: (l: Loc) => void }) {
  const tables = useLeagueTables(season);
  const podiums = useQuery({ queryKey: ["season_podiums", season], queryFn: () => api.getSeasonPodiums(season!), enabled: season !== null, staleTime: 5 * 60_000 }).data ?? [];
  const winner = (division: string) => podiums.find((p) => p.division === division)?.medals.find((m) => m.place === 1)?.team ?? null;
  type Row = { division: string; tier: number; won: CatTeam | null; lead: LeagueTableRow | null };
  const rows: Row[] = (tables?.divisions ?? [])
    .filter((d) => d.teams.length > 0)
    .map((d) => ({ division: d.division, tier: tierOf(d.division), won: winner(d.division), lead: d.teams[0] }));
  // Divisions with a podium but no table here (a cup's, an older format's).
  for (const p of podiums) {
    const first = p.medals.find((m) => m.place === 1);
    if (first && !rows.some((r) => r.division === p.division)) rows.push({ division: p.division, tier: p.tier ?? tierOf(p.division), won: first.team, lead: null });
  }
  rows.sort((a, b) => a.tier - b.tier);
  if (season === null || rows.length === 0) return null;
  const done = rows.every((r) => r.won !== null);
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{done ? t("Division champions") : t("Divisions")}</h2>
        <button className="e2-more" onClick={() => go({ at: "season", season })}>
          {t("Standings")} ›
        </button>
      </div>
      <ul className="e2-leaders">
        {rows.map((r) => {
          const team = r.won ?? r.lead;
          if (!team) return null;
          const id = "id" in team ? team.id : team.teamId;
          return (
            <li key={r.division}>
              <DivisionBadge d={{ name: r.division, tier: r.tier }} />
              <button className="e2-team" onClick={() => onTeam(id)} title={t("Open {0}", { "0": team.name })}>
                <TeamAvatar src={team.avatar ?? null} />
                <span>{team.name}</span>
              </button>
              {r.won ? (
                <span className="e2-leader-won" title={t("Won the division")}>
                  <MedalGlyph size={16} place={1} />
                </span>
              ) : (
                <span className="e2-leader-wl" title={t("Leading so far")}>
                  {r.lead ? wl(r.lead.record) : ""}
                </span>
              )}
            </li>
          );
        })}
      </ul>
      {!done && <p className="hint e2-leaders-key">{t("A medal for a division whose final is played; the others show who leads.")}</p>}
    </section>
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
          <span className="muted">{[f.season !== null ? t("Season {0}", { "0": f.season }) : null, f.round].filter(Boolean).join(" · ")}</span>
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
  const items = (q.data ?? []).slice(0, 5);
  if (q.isPending) return <Loading>{t("Loading the news…")}</Loading>;
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
        {items.map((n, i) => (
          <NewsCard key={n.id} n={n} lead={i === 0} onOpen={() => go({ at: "post", id: n.id })} />
        ))}
      </div>
    </section>
  );
}

/** "Thu 15 Oct · 21:00". */
function when(time: number): string {
  return `${new Date(time * 1000).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })} · ${clockTime(time)}`;
}

function RecentResults({ onTeam, onMatch, go }: { onTeam: (id: number) => void; onMatch: (id: number) => void; go: (l: Loc) => void }) {
  const q = useQuery({ queryKey: ["recent_results"], queryFn: () => api.getRecentResults(8), staleTime: 5 * 60_000 });
  const rows = q.data ?? [];
  if (rows.length === 0) return null;
  const side = (x: ResultRow["clan1"], score: number | null, won: boolean) => (
    <button
      className={won ? "e2-res-team won" : "e2-res-team"}
      onClick={(e) => {
        e.stopPropagation();
        onTeam(x.id);
      }}
      title={t("Open {0}", { "0": x.name })}
    >
      <span className="e2-res-name">
        <TeamAvatar src={x.avatar} />
        <span>{x.name}</span>
      </span>
      <span className="e2-res-score">{score ?? "–"}</span>
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
              {side(r.clan1, r.r1, a > b)}
              {side(r.clan2, r.r2, b > a)}
              <span className="e2-res-meta">
                {r.division && r.tier !== null && <DivisionBadge d={{ name: r.division, tier: r.tier }} />}
                <span>{r.defaultWin ? t("default") : (r.week !== null ? t("Week {0}", { "0": r.week }) : r.round) ?? ""}</span>
                <span className="e2-res-when">{when(r.time)}</span>
              </span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}

