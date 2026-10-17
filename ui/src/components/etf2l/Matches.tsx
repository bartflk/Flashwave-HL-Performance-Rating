import { useMemo, useState, type KeyboardEvent } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { Fixture, LeagueMatch, ResultRow, TeamRosterRow } from "../../api/types";
import { errorMessage } from "../../api/types";
import { formatDate, rating } from "../../lib/format";
import { openPlayer } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { CLASS_ORDER } from "../analysis/common";
import { ClassIcon } from "../ClassIcon";
import { Seg } from "../analysis/MapChrome";
import { DivisionBadge } from "../players/PlayerProfile";
import { TeamAvatar, useFixtures, wl } from "../teams/TeamsPage";
import { useYourTeams } from "../teams/YourTeams";
import { Countdown, clockTime, fullDate } from "./Countdown";

/**
 * The ETF2L tab's Matches (Q74, Flashy; PLAN §27): what is coming, with a
 * countdown each, and what was just played; and a page for one official --
 * its score or countdown, maps, lineups with ratings, head-to-head, logs.
 */

/** A row the whole of which opens the match; a team's name opens the team. */
function rowKeys(open: () => void) {
  return {
    role: "link",
    tabIndex: 0,
    onClick: open,
    onKeyDown: (e: KeyboardEvent) => {
      if (e.key === "Enter") open();
    },
  } as const;
}

function TeamLink({ x, onTeam }: { x: { id: number; name: string; avatar: string | null }; onTeam: (id: number) => void }) {
  return (
    <button
      className="e2-team"
      onClick={(e) => {
        e.stopPropagation();
        onTeam(x.id);
      }}
      title={t("Open {0}", { "0": x.name })}
    >
      <TeamAvatar src={x.avatar} />
      <span>{x.name}</span>
    </button>
  );
}

export function FixtureRow(props: { f: Fixture; mine: Set<number>; onTeam: (id: number) => void; onOpen: () => void; showDay?: boolean; showRound?: boolean }) {
  const { f, mine, onTeam, onOpen, showDay, showRound } = props;
  const ours = mine.has(f.clan1.id) || mine.has(f.clan2.id);
  return (
    <li className={`e2-fx${ours ? " ours" : ""}${showRound ? " with-round" : ""}`} {...rowKeys(onOpen)} title={t("Open the match")}>
      <span className="e2-fx-time" title={fullDate(f.time)}>
        {showDay && <span className="muted">{formatDate(f.time)} </span>}
        {clockTime(f.time)}
      </span>
      <Countdown time={f.time} />
      <span className="e2-fx-div">{f.division && f.tier !== null && <DivisionBadge d={{ name: f.division, tier: f.tier }} />}</span>
      <span className="e2-fx-teams">
        <TeamLink x={f.clan1} onTeam={onTeam} />
        <span className="e2-vs">{t("vs")}</span>
        <TeamLink x={f.clan2} onTeam={onTeam} />
      </span>
      {showRound && <span className="e2-fx-round muted">{f.week !== null ? t("Week {0}", { "0": f.week }) : (f.round ?? "")}</span>}
      <span className="e2-fx-h2h muted" title={t("Head-to-head in officials")}>
        {f.h2h.played > 0 ? wl(f.h2h) : ""}
      </span>
    </li>
  );
}

/** A played official on one line: the winner bold, the score between. */
export function ResultLine({ r, mine, onTeam, onOpen }: { r: ResultRow; mine: Set<number>; onTeam: (id: number) => void; onOpen: () => void }) {
  const ours = mine.has(r.clan1.id) || mine.has(r.clan2.id);
  const a = r.r1 ?? 0;
  const b = r.r2 ?? 0;
  return (
    <li className={ours ? "e2-rl ours" : "e2-rl"} {...rowKeys(onOpen)} title={t("Open the match")}>
      <span className="e2-fx-time muted" title={fullDate(r.time)}>
        {formatDate(r.time)}
      </span>
      <span className="e2-fx-div">{r.division && r.tier !== null && <DivisionBadge d={{ name: r.division, tier: r.tier }} />}</span>
      <span className={a > b ? "e2-rl-side l won" : "e2-rl-side l"}>
        <TeamLink x={r.clan1} onTeam={onTeam} />
      </span>
      <span className="e2-rl-score">
        {r.r1 ?? "–"}
        <span className="e2-rl-dash">:</span>
        {r.r2 ?? "–"}
      </span>
      <span className={b > a ? "e2-rl-side r won" : "e2-rl-side r"}>
        <TeamLink x={r.clan2} onTeam={onTeam} />
      </span>
      <span className="e2-rl-note muted">{r.defaultWin ? t("default") : (r.round ?? "")}</span>
    </li>
  );
}

// ---- The Matches page -----------------------------------------------------

type Mode = "upcoming" | "results";

/** "Today", "Tomorrow" or the date, for a day's heading. */
function dayLabel(time: number): string {
  const d = new Date(time * 1000);
  const today = new Date();
  const off = Math.round(
    (new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() - new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime()) / 86_400_000,
  );
  if (off === 0) return t("Today");
  if (off === 1) return t("Tomorrow");
  if (off === -1) return t("Yesterday");
  return new Date(time * 1000).toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });
}

/** Rows grouped by their local day, in the order given. */
function byDay<T>(rows: T[], time: (r: T) => number): Array<[string, T[]]> {
  const out: Array<[string, T[]]> = [];
  for (const r of rows) {
    const label = dayLabel(time(r));
    const last = out[out.length - 1];
    if (last && last[0] === label) last[1].push(r);
    else out.push([label, [r]]);
  }
  return out;
}

export function MatchesPage({ onTeam, onMatch }: { onTeam: (id: number) => void; onMatch: (id: number, f?: Fixture) => void }) {
  const [mode, setMode] = useState<Mode>("upcoming");
  const [division, setDivision] = useState("");
  const [onlyMine, setOnlyMine] = useState(false);
  const [limit, setLimit] = useState(60);
  const fixtures = useFixtures();
  const results = useQuery({ queryKey: ["recent_results", limit], queryFn: () => api.getRecentResults(limit), enabled: mode === "results", placeholderData: keepPreviousData });
  const my = new Set(useYourTeams().map((e) => e.teamId));
  const keep = (x: { division: string | null; clan1: { id: number }; clan2: { id: number } }) =>
    (division === "" || x.division === division) && (!onlyMine || my.has(x.clan1.id) || my.has(x.clan2.id));
  const divisions = useMemo(() => {
    const all = [...fixtures.map((f) => [f.division, f.tier] as const), ...(results.data ?? []).map((r) => [r.division, r.tier] as const)];
    const seen = new Map<string, number>();
    for (const [d, tier] of all) if (d && !seen.has(d)) seen.set(d, tier ?? 9);
    return [...seen.entries()].sort((a, b) => a[1] - b[1]).map(([d]) => d);
  }, [fixtures, results.data]);
  const upcoming = fixtures.filter(keep);
  const played = (results.data ?? []).filter(keep);
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">{t("ETF2L Highlander")}</span>
        <h1>{t("Matches")}</h1>
      </header>
      <div className="e2-filters">
        <Seg
          label={t("Matches")}
          value={mode}
          onChange={setMode}
          options={[
            { v: "upcoming", label: tx("Upcoming ({0})", { "0": fixtures.length }) },
            { v: "results", label: t("Results") },
          ]}
        />
        <label className="km-field">
          <span>{t("Division")}</span>
          <select value={division} onChange={(e) => setDivision(e.target.value)}>
            <option value="">{t("All divisions")}</option>
            {divisions.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </label>
        {my.size > 0 && (
          <label className="check">
            <input type="checkbox" checked={onlyMine} onChange={(e) => setOnlyMine(e.target.checked)} />
            {t("Only my teams")}
          </label>
        )}
      </div>

      {mode === "upcoming" &&
        (upcoming.length === 0 ? (
          <section className="panel e2-panel">
            <p className="hint">{fixtures.length === 0 ? t("No officials are scheduled on ETF2L right now.") : t("No officials match these filters.")}</p>
          </section>
        ) : (
          byDay(upcoming, (f) => f.time).map(([day, rows]) => (
            <section key={day} className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{day}</h2>
                <span className="muted">{rows.length === 1 ? t("1 match") : tx("{0} matches", { "0": rows.length })}</span>
              </div>
              <ol className="e2-fixtures">
                {rows.map((f) => (
                  <FixtureRow key={f.matchId} f={f} mine={my} onTeam={onTeam} onOpen={() => onMatch(f.matchId, f)} />
                ))}
              </ol>
            </section>
          ))
        ))}

      {mode === "results" && (
        <>
          {results.isPending && <p className="hint">{t("Loading results…")}</p>}
          {results.isError && <p className="error">{errorMessage(results.error)}</p>}
          {results.data && played.length === 0 && (
            <section className="panel e2-panel">
              <p className="hint">{t("No results match these filters.")}</p>
            </section>
          )}
          {byDay(played, (r) => r.time).map(([day, rows]) => (
            <section key={day} className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{day}</h2>
                <span className="muted">{rows.length === 1 ? t("1 match") : tx("{0} matches", { "0": rows.length })}</span>
              </div>
              <ol className="e2-results-list">
                {rows.map((r) => (
                  <ResultLine key={r.matchId} r={r} mine={my} onTeam={onTeam} onOpen={() => onMatch(r.matchId)} />
                ))}
              </ol>
            </section>
          ))}
          {results.data && results.data.length >= limit && (
            <button className="km-chip e2-loadmore" onClick={() => setLimit((l) => l + 60)} disabled={results.isFetching}>
              {results.isFetching ? t("Loading…") : t("Show older results")}
            </button>
          )}
        </>
      )}
    </>
  );
}

// ---- One official ---------------------------------------------------------

const order = (c: string | null) => {
  const i = c ? CLASS_ORDER.indexOf(c === "heavyweapons" ? "heavy" : c) : -1;
  return i < 0 ? 99 : i;
};

export function LeagueMatchPage({ matchId, fixture, onTeam, onOpenLog }: { matchId: number; fixture?: Fixture; onTeam: (id: number) => void; onOpenLog: (logId: number) => void }) {
  const q = useQuery({ queryKey: ["league_match", matchId], queryFn: () => api.getLeagueMatch(matchId) });
  const fromList = useFixtures().find((f) => f.matchId === matchId);
  const fx = fixture ?? fromList;
  if (q.isPending) return <p className="hint">{t("Loading the match…")}</p>;
  if (q.isError) return <p className="error">{errorMessage(q.error)}</p>;
  const m = q.data;
  // Known to the database (played or listed in its season), or only to
  // ETF2L's list of scheduled matches.
  const head = m ?? (fx ? fromFixture(fx) : null);
  if (!head) return <p className="hint">{t("This match is not in the officials read so far.")}</p>;
  const played = head.r1 !== null || head.defaultWin;
  const time = fx?.time ?? head.time;
  const maps = m?.maps ?? [];
  const players = m?.players ?? [];
  const url = m?.url ?? `https://etf2l.org/matches/${matchId}/`;
  const a = head.r1 ?? 0;
  const b = head.r2 ?? 0;

  const side = (x: ResultRow["clan1"], end: "l" | "r", won: boolean) => (
    <button className={`e2-nm-team ${end}${played && won ? " won" : ""}`} onClick={() => onTeam(x.id)} title={t("Open {0}", { "0": x.name })}>
      <TeamAvatar src={x.avatar} />
      <span>{x.name}</span>
    </button>
  );

  return (
    <article className="e2-match">
      <section className="e2-nm e2-match-hero">
        <div className="e2-nm-top">
          <span className="e2-nm-what">
            {head.division && head.tier !== null && <DivisionBadge d={{ name: head.division, tier: head.tier }} />}
            <span className="muted">{[t("Season {0}", { "0": head.season }), head.round].filter(Boolean).join(" · ")}</span>
          </span>
          {time > 0 && (
            <span className="muted" title={fullDate(time)}>
              {formatDate(time, true)} · {clockTime(time)}
            </span>
          )}
        </div>
        <div className="e2-nm-row">
          {side(head.clan1, "l", a > b)}
          <div className="e2-nm-mid">
            {played ? (
              <span className="e2-score">
                <span className={a > b ? "won" : undefined}>{head.r1 ?? "–"}</span>
                <span className="e2-rl-dash">:</span>
                <span className={b > a ? "won" : undefined}>{head.r2 ?? "–"}</span>
              </span>
            ) : time > 0 ? (
              <Countdown time={time} big />
            ) : (
              <span className="muted">{t("Not scheduled yet")}</span>
            )}
            {head.defaultWin && <span className="muted">{t("won by default")}</span>}
          </div>
          {side(head.clan2, "r", b > a)}
        </div>
        {fx && !played && (
          <div className="e2-nm-foot">
            <span className="muted">{fx.maps.length > 0 ? fx.maps.join(" · ") : t("Maps still to be picked")}</span>
          </div>
        )}
      </section>

      <div className="e2-match-grid">
        <div className="e2-main">
          {maps.length > 0 && (
            <section className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{t("Maps")}</h2>
              </div>
              <ol className="e2-maps">
                {maps.map((x, i) => (
                  <li key={i}>
                    <span className="e2-map-name">{x.map}</span>
                    <span className="e2-map-score">
                      <span className={x.clan1 > x.clan2 ? "won" : undefined}>{x.clan1}</span>
                      <span className="e2-rl-dash">:</span>
                      <span className={x.clan2 > x.clan1 ? "won" : undefined}>{x.clan2}</span>
                    </span>
                    {x.goldenCap && <span className="muted">{t("golden cap")}</span>}
                  </li>
                ))}
              </ol>
            </section>
          )}
          <section className="panel e2-panel">
            <div className="e2-panel-head">
              <h2>{played && players.length > 0 ? t("Lineups") : t("Recent lineups")}</h2>
              {!(played && players.length > 0) && <span className="muted">{t("who played each team's latest officials")}</span>}
            </div>
            <div className="e2-lineups">
              {played && players.length > 0 ? (
                <>
                  <div>
                    <h3 className="e2-lu-team">{head.clan1.name}</h3>
                    <Lineup rows={players.filter((p) => p.teamId === head.clan1.id)} />
                  </div>
                  <div>
                    <h3 className="e2-lu-team">{head.clan2.name}</h3>
                    <Lineup rows={players.filter((p) => p.teamId === head.clan2.id)} />
                  </div>
                </>
              ) : (
                <>
                  <div>
                    <h3 className="e2-lu-team">{head.clan1.name}</h3>
                    <RecentLineup teamId={head.clan1.id} />
                  </div>
                  <div>
                    <h3 className="e2-lu-team">{head.clan2.name}</h3>
                    <RecentLineup teamId={head.clan2.id} />
                  </div>
                </>
              )}
            </div>
          </section>
        </div>
        <aside className="e2-rail">
          <Meetings m={m} fx={fx} head={head} />
          {m && m.logs.length > 0 && (
            <section className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{t("Logs")}</h2>
              </div>
              <div className="e2-logs">
                {m.logs.map((l) => (
                  <button
                    key={l.logId}
                    className="km-chip"
                    onClick={() => (l.stored ? onOpenLog(l.logId) : void api.openExternal(`https://logs.tf/${l.logId}`))}
                    title={l.stored ? t("Open in the app") : t("Open on logs.tf")}
                  >
                    {l.map ?? `#${l.logId}`} {l.stored ? "›" : "↗"}
                  </button>
                ))}
              </div>
            </section>
          )}
          <button className="km-chip e2-etf2l-link" onClick={() => void api.openExternal(url)}>
            {t("Open on ETF2L")} ↗
          </button>
        </aside>
      </div>
    </article>
  );
}

/** A fixture from ETF2L's scheduled list, read as a match not yet in the database. */
function fromFixture(f: Fixture): ResultRow {
  return {
    matchId: f.matchId,
    time: f.time,
    season: f.season ?? 0,
    division: f.division,
    tier: f.tier,
    round: f.round,
    week: f.week,
    clan1: f.clan1,
    clan2: f.clan2,
    r1: null,
    r2: null,
    defaultWin: false,
  };
}

function Lineup({ rows }: { rows: LeagueMatch["players"] }) {
  const sorted = [...rows].sort((x, y) => order(x.class) - order(y.class) || x.name.localeCompare(y.name));
  if (sorted.length === 0) return <p className="hint">{t("Not read yet.")}</p>;
  return (
    <ol className="e2-lineup">
      {sorted.map((p) => (
        <li key={p.accountId}>
          <button onClick={() => openPlayer(p.accountId)} title={t("Open profile")}>
            <ClassIcon cls={p.class} size={20} />
            <span className="e2-lu-name">{p.name}</span>
            <span className="e2-lu-rating">{rating(p.rating)}</span>
          </button>
        </li>
      ))}
    </ol>
  );
}

/** Who played a team's latest officials: its own page's roster, the last nine seen. */
function RecentLineup({ teamId }: { teamId: number }) {
  const q = useQuery({ queryKey: ["team", teamId], queryFn: () => api.getTeam(teamId) });
  const rows: TeamRosterRow[] = (q.data?.roster ?? []).slice().sort((x, y) => y.lastPlayed - x.lastPlayed);
  if (q.isPending) return <p className="hint">{t("Loading…")}</p>;
  const last = rows[0]?.lastPlayed ?? 0;
  // Everyone in the latest official, else the nine seen most recently.
  const lineup = rows.filter((r) => r.lastPlayed === last).length >= 6 ? rows.filter((r) => r.lastPlayed === last) : rows.slice(0, 9);
  return <Lineup rows={lineup.map((r) => ({ accountId: r.accountId, name: r.name, teamId, class: r.class, rating: r.rating }))} />;
}

function Meetings({ m, fx, head }: { m: LeagueMatch | null | undefined; fx?: Fixture; head: ResultRow }) {
  const list = m?.meetings ?? [];
  const won = list.filter((x) => (x.scoreA ?? 0) > (x.scoreB ?? 0)).length;
  const lost = list.filter((x) => (x.scoreB ?? 0) > (x.scoreA ?? 0)).length;
  const record = list.length > 0 ? `${won}–${lost}` : fx && fx.h2h.played > 0 ? wl(fx.h2h) : null;
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Head-to-head")}</h2>
        {record && (
          <span className="e2-h2h-rec" title={t("From {0}'s side", { "0": head.clan1.name })}>
            {record}
          </span>
        )}
      </div>
      {record === null && <p className="hint">{t("First meeting in an official")}</p>}
      {list.length > 0 && (
        <ol className="e2-meetings">
          {list.slice(0, 8).map((x) => (
            <li key={x.matchId}>
              <span className="muted">{x.time ? formatDate(x.time, true) : t("Season {0}", { "0": x.season })}</span>
              <span className="e2-map-score">
                <span className={(x.scoreA ?? 0) > (x.scoreB ?? 0) ? "won" : undefined}>{x.scoreA ?? "–"}</span>
                <span className="e2-rl-dash">:</span>
                <span className={(x.scoreB ?? 0) > (x.scoreA ?? 0) ? "won" : undefined}>{x.scoreB ?? "–"}</span>
              </span>
              <span className="muted">{x.defaultWin ? t("default") : (x.division ?? "")}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

// ---- A season's fixtures and results, week by week -------------------------

/** What a round is called in the list: "Week 3", or the playoff stage. */
function roundOf(r: ResultRow): string {
  return r.week !== null ? t("Week {0}", { "0": r.week }) : (r.round ?? t("Unscheduled"));
}

export function SeasonFixtures({ season, onTeam, onMatch }: { season: number; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const q = useQuery({ queryKey: ["season_results", season], queryFn: () => api.getSeasonResults(season), staleTime: 5 * 60_000 });
  const my = new Set(useYourTeams().map((e) => e.teamId));
  const rows = q.data ?? [];
  // Rounds in the order they were played.
  const rounds = useMemo(() => {
    const out: string[] = [];
    for (const r of rows) {
      const k = roundOf(r);
      if (!out.includes(k)) out.push(k);
    }
    return out;
  }, [rows]);
  // Open on the round being played: the first with a match not yet scored.
  const current = rounds.find((k) => rows.some((r) => roundOf(r) === k && r.r1 === null && !r.defaultWin)) ?? rounds[rounds.length - 1];
  const [picked, setPicked] = useState<string | null>(null);
  const round = picked && rounds.includes(picked) ? picked : current;
  if (q.isPending) return <p className="hint">{t("Loading the fixtures…")}</p>;
  if (rows.length === 0) return null;
  const inRound = rows.filter((r) => roundOf(r) === round);
  const divisions: Array<[string, ResultRow[]]> = [];
  for (const r of [...inRound].sort((a, b) => (a.tier ?? 9) - (b.tier ?? 9))) {
    const d = r.division ?? t("Other");
    const last = divisions[divisions.length - 1];
    if (last && last[0] === d) last[1].push(r);
    else divisions.push([d, [r]]);
  }
  return (
    <>
      <h2 className="ts-section">{t("Fixtures & results")}</h2>
      <div className="e2-rounds" role="tablist" aria-label={t("Round")}>
        {rounds.map((k) => (
          <button key={k} role="tab" aria-selected={k === round} className={k === round ? "on" : undefined} onClick={() => setPicked(k)}>
            {k}
            {k === current && <span className="e2-round-now" aria-hidden />}
          </button>
        ))}
      </div>
      <div className="e2-round-grid">
        {divisions.map(([d, list]) => (
          <section key={d} className="panel e2-panel">
            <div className="e2-panel-head">
              <h2>{d}</h2>
              <span className="muted">{list.length === 1 ? t("1 match") : tx("{0} matches", { "0": list.length })}</span>
            </div>
            <ol className="e2-results-list e2-round-list">
              {list.map((r) =>
                r.r1 !== null || r.defaultWin ? (
                  <ResultLine key={r.matchId} r={r} mine={my} onTeam={onTeam} onOpen={() => onMatch(r.matchId)} />
                ) : (
                  <UpcomingLine key={r.matchId} r={r} mine={my} onTeam={onTeam} onOpen={() => onMatch(r.matchId)} />
                ),
              )}
            </ol>
          </section>
        ))}
      </div>
    </>
  );
}

/** A match of the season not played yet: its date and countdown, the two sides. */
function UpcomingLine({ r, mine, onTeam, onOpen }: { r: ResultRow; mine: Set<number>; onTeam: (id: number) => void; onOpen: () => void }) {
  const ours = mine.has(r.clan1.id) || mine.has(r.clan2.id);
  return (
    <li className={ours ? "e2-rl ours" : "e2-rl"} {...rowKeys(onOpen)} title={t("Open the match")}>
      <span className="e2-fx-time muted" title={r.time > 0 ? fullDate(r.time) : undefined}>
        {r.time > 0 ? formatDate(r.time) : "—"}
      </span>
      <span className="e2-fx-div">{r.time > 0 && <Countdown time={r.time} />}</span>
      <span className="e2-rl-side l">
        <TeamLink x={r.clan1} onTeam={onTeam} />
      </span>
      <span className="e2-rl-score e2-rl-vs">{t("vs")}</span>
      <span className="e2-rl-side r">
        <TeamLink x={r.clan2} onTeam={onTeam} />
      </span>
      <span className="e2-rl-note muted">{r.time > 0 ? clockTime(r.time) : ""}</span>
    </li>
  );
}
