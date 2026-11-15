import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { SixesDivision, SixesRow } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { drawn, mapMode } from "../../lib/gameformat";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { Etf2lMark } from "../Etf2lMark";
import { LeagueHead } from "../LeagueHead";
import { DivisionBadge } from "../players/PlayerProfile";
import { tierOf } from "../etf2l/Brackets";
import "../etf2l/etf2l.css";
import "./sixes.css";
import { SixesMatches } from "./SixesMatches";
import { SixesTeam, SixesTeamsList } from "./SixesTeams";
import { SixesSeasons } from "./SixesSeasons";
import { SixesSample } from "./SixesSample";
import { SixesPlayers } from "./SixesPlayers";
import { SixesHome } from "./SixesHome";
import { NewsList, NewsReader } from "../etf2l/News";
import { RecruitmentPage, RecruitPostPage, type Kind } from "../etf2l/Recruitment";
import type { RecruitPost } from "../../api/types";

/**
 * The ETF2L tab in 6v6 (the 6v6 plan, P2). The current season's tables,
 * read live from ETF2L's API: the round-robin groups (Premiership to
 * Division 3, and Fresh) and the Swiss divisions (Division 4, Low, Open)
 * since Season 53, with the map pool and the match rules that make 6v6's
 * tables read differently from Highlander's (two maps a week, 5CP maps that
 * can be drawn, golden caps); its matches by week, its teams, ETF2L's 6v6
 * news and recruitment. What is still to come is listed in the rail.
 */
export function SixesPage({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
  const q = useQuery({ queryKey: ["sixes_season"], queryFn: api.getSixesSeason, staleTime: 30 * 60_000, retry: false });
  const s = q.data;
  const groups = useMemo(() => (s?.divisions ?? []).filter((d) => d.system === "roundRobin"), [s]);
  const swiss = useMemo(() => (s?.divisions ?? []).filter((d) => d.system === "swiss"), [s]);
  // The sections built so far: the season's tables, and its matches by week
  // (the 6v6 plan, P2). The rest of the menu waits for its step.
  type Section = "home" | "season" | "matches" | "teams" | "seasons" | "players" | "news" | "recruitment";
  // Home first, as Highlander's (Flashy: "the etf2l 6's page doesnt have a
  // home, please match this with the highlander").
  const [section, setSection] = useState<Section>("home");
  // A page opened over the section: a team (from a table, a match or the
  // Teams list), a news post, or a recruitment post.
  const [teamId, setTeamIdRaw] = useState<number | null>(null);
  const [postId, setPostId] = useState<number | null>(null);
  const [recruit, setRecruit] = useState<{ p: RecruitPost; kind: Kind } | null>(null);
  const setTeamId = (id: number | null) => {
    setTeamIdRaw(id);
    setPostId(null);
    setRecruit(null);
  };
  const open = (sec: Section) => {
    setSection(sec);
    setTeamId(null);
  };
  const tab = (sec: Section, label: string) => (
    <button className={section === sec ? "on" : undefined} aria-current={section === sec ? "page" : undefined} onClick={() => open(sec)}>
      {label}
    </button>
  );

  return (
    <div className="content e2-page sx-page">
      <LeagueHead>
        <div className="e2-brand">
          <span className="e2-eyebrow" data-fit="">
            <Etf2lMark size={16} /> {t("ETF2L 6v6")}
          </span>
          <nav className="e2-menu" data-fit="" aria-label={t("ETF2L 6v6")}>
            {tab("home", t("Home"))}
            {tab("season", t("Season"))}
            {tab("matches", t("Matches"))}
            {tab("teams", t("Teams"))}
            {tab("seasons", t("Seasons"))}
            {tab("players", t("Players"))}
            {tab("news", t("News"))}
            {tab("recruitment", t("Recruitment"))}
          </nav>
        </div>
        <span className="sx-proto" data-fit="" title={t("6v6 is in preview: it ships once its rating is ready")}>
          {t("Preview")}
        </span>
      </LeagueHead>

      {q.error ? (
        <ApiError error={q.error} />
      ) : q.isPending ? (
        <Loading>{t("Reading ETF2L's 6v6 season…")}</Loading>
      ) : !s ? (
        <p className="hint">{t("ETF2L lists no 6v6 season.")}</p>
      ) : section === "players" && teamId === null ? (
        // Full width, as Highlander's Players tab: a player's page is the profile.
        <SixesPlayers onOpenMatch={onOpenMatch} />
      ) : section === "home" && teamId === null && postId === null && recruit === null ? (
        <SixesHome s={s} onTeam={setTeamId} onPost={setPostId} onRecruit={(p, kind) => setRecruit({ p, kind })} go={(to) => open(to)} />
      ) : (
        <div className="e2-match-grid">
          <div className="e2-main">
            {teamId !== null ? (
              <SixesTeam teamId={teamId} onTeam={setTeamId} onBack={() => setTeamId(null)} />
            ) : postId !== null ? (
              <div className="sx-team-page">
                <button className="linkish sx-back" onClick={() => setPostId(null)}>
                  {t("‹ Back to the news")}
                </button>
                <NewsReader id={postId} />
              </div>
            ) : recruit !== null ? (
              <div className="sx-team-page">
                <button className="linkish sx-back" onClick={() => setRecruit(null)}>
                  {t("‹ Back to recruitment")}
                </button>
                <RecruitPostPage p={recruit.p} kind={recruit.kind} onTeam={setTeamId} />
              </div>
            ) : section === "news" ? (
              <>
                <NewsList format="sixes" onPost={setPostId} />
              </>
            ) : section === "recruitment" ? (
              <>
                <RecruitmentPage format="sixes" onOpen={(p, kind) => setRecruit({ p, kind })} />
              </>
            ) : section === "players" ? (
              <SixesPlayers onOpenMatch={onOpenMatch} />
            ) : section === "seasons" ? (
              <SixesSeasons onTeam={setTeamId} />
            ) : section === "teams" ? (
              <>
                <h2 className="ts-section">{tx("{0}: teams", { "0": s.name })}</h2>
                <SixesTeamsList s={s} onTeam={setTeamId} />
              </>
            ) : section === "matches" ? (
              <>
                <h2 className="ts-section">{tx("{0}: matches", { "0": s.name })}</h2>
                <SixesMatches onTeam={setTeamId} />
              </>
            ) : (
            <>
            <header className="panel pp-head sx-head">
              <span className="ts-eyebrow">{t("ETF2L 6v6")}</span>
              <h1 className="sx-title">{s.name}</h1>
              <div className="sx-facts">
                <Fact label={t("Divisions")} value={String(s.divisions.length)} sub={tx("{0} round robin · {1} Swiss", { "0": groups.length, "1": swiss.length })} />
                <Fact label={t("Teams")} value={String(s.divisions.reduce((n, d) => n + d.teams.length, 0))} sub={t("signed up")} />
                <Fact label={t("Map pool")} value={String(s.pool.length)} sub={tx("{0} 5CP · {1} KOTH", { "0": s.pool.filter((m) => mapMode(m) === "5cp").length, "1": s.pool.filter((m) => mapMode(m) === "koth").length })} />
                <Fact label={t("A match")} value={t("2 maps")} sub={t("a week, fixed rotation")} />
              </div>
              <div className="sx-pool">
                {s.pool.map((m) => (
                  <span key={m} className={`sx-map sx-${mapMode(m)}`}>
                    {m}
                  </span>
                ))}
              </div>
            </header>

            {groups.length > 0 && (
              <>
                <h2 className="ts-section">{t("Round robin")}</h2>
                <p className="hint sx-explain">{t("Groups of eight: every team plays every other once over the seven weeks.")}</p>
                <div className="sx-grid">
                  {groups.map((d) => (
                    <DivisionTable key={d.name} d={d} onTeam={setTeamId} />
                  ))}
                </div>
              </>
            )}
            {swiss.length > 0 && (
              <>
                <h2 className="ts-section">{t("Swiss")}</h2>
                <p className="hint sx-explain">{t("Each week a team plays one with a similar record, so the big divisions need no groups. Teams level on points are split by head-to-head, then the Sonneborn-Berger score, then rounds.")}</p>
                <div className="sx-stack">
                  {swiss.map((d) => (
                    <DivisionTable key={d.name} d={d} wide onTeam={setTeamId} />
                  ))}
                </div>
              </>
            )}
            </>
            )}
          </div>

          <aside className="e2-rail">
            <section className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{t("How 6v6 is played")}</h2>
              </div>
              <ul className="sx-rules">
                <li>
                  <strong>{t("Six a side")}</strong>
                  <span className="muted">{t("Two Scouts, two Soldiers (pocket and roamer), a Demoman and a Medic; other classes are off-classes for a moment.")}</span>
                </li>
                <li>
                  <strong>{t("5CP")}</strong>
                  <span className="muted">{t("First to 5 rounds in 30 minutes, 4 minutes a round. Level at the limit is a draw in the regular season.")}</span>
                </li>
                <li>
                  <strong>{t("KOTH")}</strong>
                  <span className="muted">{t("First to 3 rounds, no time limit.")}</span>
                </li>
                <li>
                  <strong>{t("Points")}</strong>
                  <span className="muted">{t("As ETF2L counts them, per map. ETF2L changed them for this season, so the app shows its numbers rather than working them out.")}</span>
                </li>
              </ul>
              <p className="hint">{t("From ETF2L's 6v6 rules and the Season 53 home page.")}</p>
            </section>

            <SixesSample />

            <section className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{t("Coming in 6v6")}</h2>
              </div>
              <ul className="sx-rules">
                <li>
                  <strong>{t("The 6v6 rating")}</strong>
                  <span className="muted">{t("Fitted on every ETF2L 6v6 official, with round impact. Until then, your preview ratings by role are on the Profile tab in 6v6.")}</span>
                </li>
                <li>
                  <strong>{t("The rest of this tab")}</strong>
                  <span className="muted">{t("6v6 player pages with seasons and teams, from the 6v6 league sample.")}</span>
                </li>
              </ul>
            </section>
          </aside>
        </div>
      )}
    </div>
  );
}

function Fact({ label, value, sub }: { label: string; value: string; sub: ReactNode }) {
  return (
    <div className="sx-fact">
      <span className="sx-fact-label">{label}</span>
      <strong className="sx-fact-value">{value}</strong>
      <span className="muted">{sub}</span>
    </div>
  );
}

type SortKey = "place" | "won" | "score";

/** One division's table: maps won, drawn and lost, golden caps, points. */
function DivisionTable({ d, wide, onTeam }: { d: SixesDivision; wide?: boolean; onTeam: (id: number) => void }) {
  const [open, setOpen] = useState(!wide || d.teams.length <= 12);
  const [sort, setSort] = useState<SortKey>("place");
  const rows = useMemo(() => {
    const placed = d.teams.map((r, i) => ({ r, place: r.dropped ? null : i + 1 }));
    if (sort === "won") return [...placed].sort((a, b) => b.r.mapsWon - a.r.mapsWon || (a.place ?? 99) - (b.place ?? 99));
    if (sort === "score") return [...placed].sort((a, b) => b.r.score - a.r.score || (a.place ?? 99) - (b.place ?? 99));
    return placed;
  }, [d.teams, sort]);
  const shown = open ? rows : rows.slice(0, 10);
  const hasPen = d.teams.some((r) => r.penalty !== 0);
  const hasBye = d.teams.some((r) => r.byes !== 0);
  const hasSeed = d.teams.some((r) => r.seededPoints !== 0);
  const th = (k: SortKey, label: string, title: string) => (
    <th className="num">
      <button className={sort === k ? "sx-sort on" : "sx-sort"} onClick={() => setSort(sort === k ? "place" : k)} title={title}>
        {label}
      </button>
    </th>
  );
  return (
    <section className="panel e2-panel sx-div">
      <div className="e2-panel-head">
        <h2 className="sx-div-name">
          <DivisionBadge d={{ name: d.name, tier: Math.floor(tierOf(d.name)) }} />
        </h2>
        <span className="muted">{d.teams.length === 1 ? t("1 team") : tx("{0} teams", { "0": d.teams.length })}</span>
      </div>
      <div className="table-wrap">
        <table className="match-table sx-table">
          <thead>
            <tr>
              <th className="num">#</th>
              <th>{t("Team")}</th>
              <th className="num" title={t("Maps played")}>{t("P")}</th>
              {th("won", t("W"), t("Maps won"))}
              <th className="num" title={t("Maps drawn")}>{t("D")}</th>
              <th className="num" title={t("Maps lost")}>{t("L")}</th>
              <th className="num" title={t("Golden caps won and lost")}>{t("GC")}</th>
              {hasSeed && <th className="num" title={t("Seeded points")}>{t("SP")}</th>}
              {hasBye && <th className="num" title={t("Byes")}>{t("B")}</th>}
              {hasPen && <th className="num" title={t("Penalty points")}>{t("Pen")}</th>}
              {th("score", t("Pts"), t("League points, as ETF2L counts them"))}
            </tr>
          </thead>
          <tbody>
            {shown.map(({ r, place }) => (
              <Row key={r.id} r={r} place={place} hasPen={hasPen} hasBye={hasBye} hasSeed={hasSeed} onTeam={onTeam} />
            ))}
          </tbody>
        </table>
      </div>
      {!open && (
        <button className="linkish sx-more" onClick={() => setOpen(true)}>
          {tx("Show all {0}", { "0": d.teams.length })}
        </button>
      )}
    </section>
  );
}

function Row({ r, place, hasPen, hasBye, hasSeed, onTeam }: { r: SixesRow; place: number | null; hasPen: boolean; hasBye: boolean; hasSeed: boolean; onTeam: (id: number) => void }) {
  return (
    <tr className={r.dropped ? "sx-dropped" : undefined}>
      <td className="num muted">{place ?? "–"}</td>
      <td className="sx-team">
        <button className="linkish sx-team-link" onClick={() => onTeam(r.id)} title={t("Open the team's page")}>
          {r.name}
        </button>
        {r.dropped && <span className="muted"> {t("dropped")}</span>}
      </td>
      <td className="num muted">{r.mapsPlayed}</td>
      <td className="num team-won">{r.mapsWon}</td>
      <td className="num muted">{drawn(r)}</td>
      <td className="num team-lost">{r.mapsLost}</td>
      <td className="num muted">{r.gcWon || r.gcLost ? `${r.gcWon}-${r.gcLost}` : "–"}</td>
      {hasSeed && <td className="num muted">{r.seededPoints || ""}</td>}
      {hasBye && <td className="num muted">{r.byes || ""}</td>}
      {hasPen && <td className="num muted">{r.penalty || ""}</td>}
      <td className="num sx-pts">{r.score}</td>
    </tr>
  );
}
