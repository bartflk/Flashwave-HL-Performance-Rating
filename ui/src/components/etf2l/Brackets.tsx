import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { ResultRow } from "../../api/types";
import { formatDate } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { DivisionBadge } from "../players/PlayerProfile";
import { TeamAvatar } from "../teams/TeamsPage";
import { useYourTeams } from "../teams/YourTeams";
import { clockTime, Countdown, fullDate } from "./Countdown";

/**
 * Each division of a season, as ETF2L runs it (Flashy: "bracket and group
 * stage for every div per season"): the group stage as a table with every
 * week's result, then the playoffs drawn as a bracket.
 *
 * ETF2L gives no bracket, only matches with a round name ("Week 3", "Upper
 * Bracket Final", "Semi-Finals", "3rd Place"), so the bracket is rebuilt
 * from those: the round name says which part a match belongs to, the times
 * put the rounds in order, and the teams say which match leads to which --
 * a team's next match after a win is where the line goes, after a loss
 * (into a lower bracket, or the 3rd place match) the line is dashed. A
 * format the names do not describe is drawn as plain rounds in order,
 * never guessed.
 */

type Lane = "group" | "upper" | "lower" | "main" | "final" | "third";

/** Which part of the season a match is, from its round's name and stage. */
export function laneOf(m: Pick<ResultRow, "round" | "stage">): Lane {
  const r = (m.round ?? "").toLowerCase();
  if (m.stage === "3rd Place" || /3rd|third/.test(r)) return "third";
  if (/grand.?final/.test(r)) return "final";
  if (/upper/.test(r)) return "upper";
  if (/lower/.test(r)) return "lower";
  if (m.stage === "Playoffs" || /quarter|semi|final|round of \d/.test(r)) return "main";
  return "group";
}

/** The ladder's tier for a division name, for ordering and badge colour. */
export function tierOf(division: string): number {
  const d = division.toLowerCase();
  if (d.startsWith("prem")) return d.includes("qual") ? 0.5 : 0;
  if (d.startsWith("high") || d.startsWith("division 1") || d.startsWith("division 2")) return 1;
  if (d.startsWith("mid") || d.startsWith("division 3") || d.startsWith("division 4")) return 2;
  if (d.startsWith("low") || d.startsWith("division 5")) return 3;
  if (d.startsWith("open") || d.startsWith("division 6")) return 4;
  return 5;
}

const played = (m: ResultRow) => m.r1 !== null || m.defaultWin;

/** The winner and loser of a played match, or null. */
function outcome(m: ResultRow): { win: number; lose: number } | null {
  if (!played(m) || m.r1 === m.r2) return null;
  return (m.r1 ?? 0) > (m.r2 ?? 0) ? { win: m.clan1.id, lose: m.clan2.id } : { win: m.clan2.id, lose: m.clan1.id };
}

type Round = { label: string; lane: Lane; time: number; matches: ResultRow[] };

/** Matches grouped by round, in the order the rounds were played. */
function roundsOf(rows: ResultRow[]): Round[] {
  const by = new Map<string, Round>();
  for (const m of rows) {
    const lane = laneOf(m);
    const label = m.round ?? (m.week !== null ? t("Week {0}", { "0": m.week }) : t("Unscheduled"));
    // "Quarter Final" and "Quarter Finals", "Semi-Finals" and "Semi Finals":
    // ETF2L names one round several ways within a season (S35 Low).
    const key = `${lane}|${label.toLowerCase().replace(/[^a-z0-9]/g, "").replace(/s$/, "")}`;
    const r = by.get(key) ?? { label, lane, time: Number.POSITIVE_INFINITY, matches: [] };
    r.matches.push(m);
    if (m.time > 0) r.time = Math.min(r.time, m.time);
    by.set(key, r);
  }
  return [...by.values()].sort((a, b) => a.time - b.time);
}

// ---- The section on a season's page ----------------------------------------

export function SeasonDivisions({ season, onTeam, onMatch }: { season: number; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const q = useQuery({ queryKey: ["season_results", season], queryFn: () => api.getSeasonResults(season), staleTime: 5 * 60_000 });
  const tables = useQuery({ queryKey: ["leagues", season], queryFn: () => api.getLeagues(season), staleTime: 5 * 60_000 }).data;
  const mine = new Set(useYourTeams().map((e) => e.teamId));
  const rows = useMemo(() => (q.data ?? []).filter((m) => m.stage !== "Cup" && m.division), [q.data]);
  const divisions = useMemo(() => [...new Set(rows.map((m) => m.division!))].sort((a, b) => tierOf(a) - tierOf(b) || a.localeCompare(b)), [rows]);
  // Open on your team's division when you have one in the season.
  const yours = divisions.find((d) => rows.some((m) => m.division === d && (mine.has(m.clan1.id) || mine.has(m.clan2.id))));
  const [picked, setPicked] = useState<string | null>(null);
  const division = picked && divisions.includes(picked) ? picked : (yours ?? divisions[0]);
  if (q.isPending) return <p className="hint">{t("Loading the divisions…")}</p>;
  if (divisions.length === 0) return null;
  const here = rows.filter((m) => m.division === division);
  const group = here.filter((m) => laneOf(m) === "group");
  const bracket = here.filter((m) => laneOf(m) !== "group");
  const order = tables?.divisions.find((d) => d.division === division)?.teams.map((x) => x.teamId) ?? [];
  return (
    <>
      <h2 className="ts-section">{t("Divisions")}</h2>
      <div className="e2-rounds br-divs" role="tablist" aria-label={t("Division")}>
        {divisions.map((d) => (
          <button key={d} role="tab" aria-selected={d === division} className={d === division ? "on" : undefined} onClick={() => setPicked(d)}>
            <DivisionBadge d={{ name: d, tier: Math.floor(tierOf(d)) }} />
          </button>
        ))}
      </div>
      {group.length > 0 && <GroupStage rows={group} order={order} mine={mine} onTeam={onTeam} onMatch={onMatch} />}
      {bracket.length > 0 ? (
        <Bracket rows={bracket} mine={mine} onTeam={onTeam} onMatch={onMatch} />
      ) : (
        group.length > 0 && <p className="hint">{t("No playoffs for this division yet.")}</p>
      )}
    </>
  );
}

// ---- The group stage: standings with every week's result -------------------

type Line = { id: number; name: string; avatar: string | null; played: number; won: number; lost: number; drawn: number; for: number; against: number };

function GroupStage({ rows, order, mine, onTeam, onMatch }: { rows: ResultRow[]; order: number[]; mine: Set<number>; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const rounds = roundsOf(rows);
  const teams = new Map<number, Line>();
  const line = (x: ResultRow["clan1"]) => {
    const l = teams.get(x.id) ?? { id: x.id, name: x.name, avatar: x.avatar, played: 0, won: 0, lost: 0, drawn: 0, for: 0, against: 0 };
    teams.set(x.id, l);
    return l;
  };
  for (const m of rows) {
    const a = line(m.clan1);
    const b = line(m.clan2);
    if (!played(m)) continue;
    const [s1, s2] = [m.r1 ?? 0, m.r2 ?? 0];
    a.played++;
    b.played++;
    a.for += s1;
    a.against += s2;
    b.for += s2;
    b.against += s1;
    if (s1 > s2) {
      a.won++;
      b.lost++;
    } else if (s2 > s1) {
      b.won++;
      a.lost++;
    } else {
      a.drawn++;
      b.drawn++;
    }
  }
  // ETF2L's own table order where the league view has it; else wins, then
  // rounds won minus lost.
  const rank = (id: number) => {
    const i = order.indexOf(id);
    return i < 0 ? Number.POSITIVE_INFINITY : i;
  };
  const list = [...teams.values()].sort((a, b) => rank(a.id) - rank(b.id) || b.won - a.won || b.for - b.against - (a.for - a.against) || a.name.localeCompare(b.name));
  const cell = (team: number, r: Round): ReactNode => {
    const m = r.matches.find((x) => x.clan1.id === team || x.clan2.id === team);
    if (!m) return <span className="br-cell none">·</span>;
    const first = m.clan1.id === team;
    const opp = first ? m.clan2 : m.clan1;
    const us = first ? m.r1 : m.r2;
    const them = first ? m.r2 : m.r1;
    const kind = !played(m) ? "todo" : (us ?? 0) > (them ?? 0) ? "won" : (us ?? 0) < (them ?? 0) ? "lost" : "drawn";
    return (
      <button className={`br-cell ${kind}`} onClick={() => onMatch(m.matchId)} title={`${t("vs {0}", { "0": opp.name })}${m.time > 0 ? ` · ${fullDate(m.time)}` : ""}`}>
        {kind === "todo" ? (m.time > 0 ? formatDate(m.time) : "–") : m.defaultWin && us === null ? t("def.") : `${us ?? "–"}–${them ?? "–"}`}
      </button>
    );
  };
  return (
    <section className="panel e2-panel br-group">
      <div className="e2-panel-head">
        <h2>{t("Group stage")}</h2>
        <span className="muted">{tx("{0} teams", { "0": list.length })}</span>
      </div>
      <div className="table-wrap">
        <table className="match-table br-table">
          <thead>
            <tr>
              <th className="num">#</th>
              <th>{t("Team")}</th>
              <th className="num">{t("Played")}</th>
              <th className="num">{t("W-L")}</th>
              <th className="num">{t("Score")}</th>
              {rounds.map((r) => (
                <th key={r.label} className="num br-week" title={r.label}>
                  {r.label.replace(/^(Week|Round) /, "")}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {list.map((l, i) => (
              <tr key={l.id} className={mine.has(l.id) ? "br-mine" : undefined}>
                <td className="num muted">{i + 1}</td>
                <td>
                  <button className="e2-team" onClick={() => onTeam(l.id)} title={t("Open {0}", { "0": l.name })}>
                    <TeamAvatar src={l.avatar} />
                    <span>{l.name}</span>
                  </button>
                </td>
                <td className="num">{l.played}</td>
                <td className="num nowrap">
                  {l.won}–{l.lost}
                  {l.drawn > 0 ? `–${l.drawn}` : ""}
                </td>
                <td className="num nowrap">
                  {l.for}:{l.against}
                </td>
                {rounds.map((r) => (
                  <td key={r.label} className="num">
                    {cell(l.id, r)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

// ---- The playoffs, drawn as a bracket ---------------------------------------

/** A single-elimination column ordered so each pair sits beside the match
 *  it leads to: worked back from the final. */
function orderColumns(cols: ResultRow[][]): ResultRow[][] {
  const out = cols.map((c) => [...c]);
  for (let i = out.length - 2; i >= 0; i--) {
    const next = out[i + 1];
    const slot = (m: ResultRow) => {
      for (const [j, n] of next.entries()) {
        for (const id of [m.clan1.id, m.clan2.id]) {
          if (n.clan1.id === id) return j * 2;
          if (n.clan2.id === id) return j * 2 + 1;
        }
      }
      return Number.POSITIVE_INFINITY;
    };
    out[i].sort((a, b) => slot(a) - slot(b));
  }
  return out;
}

function Bracket({ rows, mine, onTeam, onMatch }: { rows: ResultRow[]; mine: Set<number>; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const rounds = roundsOf(rows);
  const third = rounds.filter((r) => r.lane === "third");
  const main = rounds.filter((r) => r.lane !== "third");
  const double = main.some((r) => r.lane === "upper" || r.lane === "lower");
  const box = useRef<HTMLDivElement>(null);
  const cards = useRef(new Map<number, HTMLElement>());
  const [lines, setLines] = useState<Array<{ d: string; loser: boolean; mine: boolean }>>([]);

  // Where each team went next: a line from a match to the next one its
  // winner played (solid) and its loser played (dashed), measured from the
  // drawn cards so any layout gets its lines.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => {
      const base = el.getBoundingClientRect();
      const out: Array<{ d: string; loser: boolean; mine: boolean }> = [];
      const sorted = [...rows].sort((a, b) => a.time - b.time);
      for (const m of sorted) {
        const o = outcome(m);
        const from = cards.current.get(m.matchId);
        if (!o || !from) continue;
        for (const [team, loser] of [
          [o.win, false],
          [o.lose, true],
        ] as Array<[number, boolean]>) {
          const next = sorted.find((n) => n.time > m.time && n.matchId !== m.matchId && (n.clan1.id === team || n.clan2.id === team));
          const to = next && cards.current.get(next.matchId);
          if (!to) continue;
          const a = from.getBoundingClientRect();
          const b = to.getBoundingClientRect();
          if (b.left <= a.left + 4) continue;
          const x1 = a.right - base.left;
          const y1 = a.top + a.height / 2 - base.top;
          const x2 = b.left - base.left;
          // Into the half of the card where the team is listed.
          const top = next!.clan1.id === team;
          const y2 = b.top + b.height * (top ? 0.33 : 0.67) - base.top;
          const mid = x1 + Math.max(10, (x2 - x1) / 2);
          out.push({ d: `M${x1},${y1} H${mid} V${y2} H${x2}`, loser, mine: mine.has(team) });
        }
      }
      setLines(out);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [rows, mine]);

  const card = (m: ResultRow) => (
    <MatchCard
      key={m.matchId}
      m={m}
      mine={mine}
      onTeam={onTeam}
      onOpen={() => onMatch(m.matchId)}
      refEl={(e) => {
        if (e) cards.current.set(m.matchId, e);
        else cards.current.delete(m.matchId);
      }}
    />
  );

  let body: ReactNode;
  if (double) {
    // Upper bracket on top, lower below, the final to the right of both.
    const cols = main.filter((r) => r.lane !== "final");
    const finals = main.filter((r) => r.lane === "final");
    body = (
      <div className="br-grid" style={{ gridTemplateColumns: `repeat(${cols.length + finals.length}, minmax(220px, 1fr))` }}>
        {cols.map((r, i) => (
          <div key={r.label} className={`br-col ${r.lane === "lower" ? "br-lower" : "br-upper"}`} style={{ gridColumn: i + 1, gridRow: r.lane === "lower" ? 3 : 2 }}>
            {r.matches.map(card)}
          </div>
        ))}
        {cols.map((r, i) => (
          <span key={`h${r.label}`} className="br-head" style={{ gridColumn: i + 1, gridRow: 1 }}>
            {r.label}
          </span>
        ))}
        {finals.map((r, i) => (
          <div key={r.label} className="br-col br-final" style={{ gridColumn: cols.length + i + 1, gridRow: "2 / 4" }}>
            {r.matches.map(card)}
          </div>
        ))}
        {finals.map((r, i) => (
          <span key={`h${r.label}`} className="br-head" style={{ gridColumn: cols.length + i + 1, gridRow: 1 }}>
            {r.label}
          </span>
        ))}
      </div>
    );
  } else {
    const cols = orderColumns(main.map((r) => r.matches));
    body = (
      <div className="br-grid br-single" style={{ gridTemplateColumns: `repeat(${cols.length}, minmax(220px, 1fr))` }}>
        {main.map((r, i) => (
          <span key={`h${r.label}`} className="br-head" style={{ gridColumn: i + 1, gridRow: 1 }}>
            {r.label}
          </span>
        ))}
        {cols.map((c, i) => (
          <div key={main[i].label} className="br-col" style={{ gridColumn: i + 1, gridRow: 2 }}>
            {c.map(card)}
          </div>
        ))}
      </div>
    );
  }

  return (
    <section className="panel e2-panel br-bracket">
      <div className="e2-panel-head">
        <h2>{t("Playoffs")}</h2>
        <span className="br-key muted">
          <span className="br-key-line" /> {t("winner goes on")} <span className="br-key-line loser" /> {t("loser drops down")}
        </span>
      </div>
      <div className="br-scroll">
        <div className="br-box" ref={box}>
          <svg className="br-lines" aria-hidden>
            {lines.map((l, i) => (
              <path key={i} d={l.d} className={`${l.loser ? "loser" : ""}${l.mine ? " mine" : ""}`} />
            ))}
          </svg>
          {body}
        </div>
      </div>
      {third.length > 0 && (
        <div className="br-third">
          <span className="br-head">{third[0].label}</span>
          <div className="br-third-row">{third.flatMap((r) => r.matches).map(card)}</div>
        </div>
      )}
    </section>
  );
}

function MatchCard({ m, mine, onTeam, onOpen, refEl }: { m: ResultRow; mine: Set<number>; onTeam: (id: number) => void; onOpen: () => void; refEl: (e: HTMLElement | null) => void }) {
  const o = outcome(m);
  const side = (x: ResultRow["clan1"], score: number | null) => (
    <div className={`br-side${o?.win === x.id ? " won" : o?.lose === x.id ? " lost" : ""}${mine.has(x.id) ? " mine" : ""}`}>
      <button
        className="br-team"
        onClick={(e) => {
          e.stopPropagation();
          onTeam(x.id);
        }}
        title={t("Open {0}", { "0": x.name })}
      >
        <TeamAvatar src={x.avatar} />
        <span>{x.name}</span>
      </button>
      <span className="br-score">{played(m) ? (score ?? "–") : ""}</span>
    </div>
  );
  return (
    <article className="br-card" ref={refEl} role="link" tabIndex={0} onClick={onOpen} onKeyDown={(e) => e.key === "Enter" && onOpen()} title={t("Open the match")}>
      {side(m.clan1, m.r1)}
      {side(m.clan2, m.r2)}
      {!played(m) && (
        <div className="br-when">
          {m.time > 0 ? (
            <>
              <span>
                {formatDate(m.time)} · {clockTime(m.time)}
              </span>
              <Countdown time={m.time} />
            </>
          ) : (
            <span className="muted">{t("Not scheduled yet")}</span>
          )}
        </div>
      )}
    </article>
  );
}
