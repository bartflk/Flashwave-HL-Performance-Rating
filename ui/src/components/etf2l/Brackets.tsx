import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { Fixture, ResultRow } from "../../api/types";
import { formatDate } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { DivisionBadge } from "../players/PlayerProfile";
import { MedalGlyph } from "../players/MedalGlyph";
import { useMeasuredWidth } from "../../lib/measure";
import { TeamAvatar, useFixtures } from "../teams/TeamsPage";
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
  const fixtures = useFixtures();
  const rows = useMemo(() => {
    const known = new Set((q.data ?? []).map((m) => m.matchId));
    // The same two teams already meeting in that round is the same match.
    const pair = (m: ResultRow) => `${m.division}|${laneOf(m)}|${[m.clan1.id, m.clan2.id].sort((x, y) => x - y).join("-")}`;
    const pairs = new Set((q.data ?? []).map(pair));
    const extra = fixtures
      .filter((f) => f.season === season && !known.has(f.matchId))
      .map(fromFixture)
      .filter((m) => !pairs.has(pair(m)));
    return [...(q.data ?? []), ...extra].filter((m) => m.stage !== "Cup" && m.division);
  }, [q.data, fixtures, season]);
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
 *  it leads to: worked back from the last round. */
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

// The bracket's measures, in pixels: every card is placed by hand rather
// than left to the page's flow, so the lines can be drawn exactly.
// The card's width follows the panel (Flashy: "make the bracket bigger"):
// the columns share the room, between these two widths.
const MIN_W = 230;
const MAX_W = 440;
const ROW_H = 36;
const CARD_H = ROW_H * 2 + 3;
const COL_GAP = 64;
const V_GAP = 40;
const HEAD_H = 34;
const LANE_GAP = 36;

type Placed = { m: ResultRow; col: number; x: number; y: number };

const shares = (a: ResultRow, b: ResultRow) => [a.clan1.id, a.clan2.id].some((id) => id === b.clan1.id || id === b.clan2.id);

/**
 * Where every match goes. Each lane -- the upper bracket (or the whole of a
 * single elimination), the lower bracket -- runs left to right and ends in
 * the column before the final, as Challonge draws it; a match after the
 * first round sits level with the matches that feed it.
 */
function layout(rounds: Round[], CARD_W: number) {
  const finals = rounds.filter((r) => r.lane === "final");
  const top = rounds.filter((r) => r.lane === "upper" || r.lane === "main");
  const bottom = rounds.filter((r) => r.lane === "lower");
  const span = Math.max(top.length, bottom.length);
  const placed: Placed[] = [];
  const heads: Array<{ x: number; y: number; label: string }> = [];
  const bands: number[] = [];

  const lane = (list: Round[], y0: number, ordered: ResultRow[][]): number => {
    if (list.length === 0) return y0;
    bands.push(y0);
    let bottomEdge = y0 + HEAD_H;
    list.forEach((r, i) => {
      const col = span - list.length + i;
      const x = col * (CARD_W + COL_GAP);
      heads.push({ x, y: y0, label: r.label });
      const prev = placed.filter((p) => p.col === col - 1 && p.y >= y0);
      const want = ordered[i].map((m, k) => {
        const feeders = prev.filter((p) => shares(p.m, m));
        const y = feeders.length > 0 ? feeders.reduce((s, p) => s + p.y, 0) / feeders.length : y0 + HEAD_H + 8 + k * (CARD_H + V_GAP);
        return { m, y };
      });
      // Keep the column's order and spacing where the averages would overlap.
      let floor = y0 + HEAD_H + 8;
      for (const w of want) {
        const y = Math.max(w.y, floor);
        placed.push({ m: w.m, col, x, y });
        floor = y + CARD_H + V_GAP;
        bottomEdge = Math.max(bottomEdge, y + CARD_H);
      }
    });
    return bottomEdge;
  };

  const topEnd = lane(top, 0, orderColumns(top.map((r) => r.matches)));
  const bottomEnd = lane(bottom, bottom.length > 0 ? topEnd + LANE_GAP : topEnd, orderColumns(bottom.map((r) => r.matches)));
  // The final, level with what feeds it, else between the two lanes.
  finals.forEach((r, i) => {
    const col = span + i;
    const x = col * (CARD_W + COL_GAP);
    heads.push({ x, y: 0, label: r.label });
    let floor = HEAD_H + 8;
    for (const m of r.matches) {
      const feeders = placed.filter((p) => p.col < col && shares(p.m, m) && !placed.some((q) => q.col > p.col && q.col < col && shares(q.m, p.m)));
      const mid = feeders.length > 0 ? feeders.reduce((s, p) => s + p.y, 0) / feeders.length : (HEAD_H + bottomEnd) / 2 - CARD_H / 2;
      const y = Math.max(mid, floor);
      placed.push({ m, col, x, y });
      floor = y + CARD_H + V_GAP;
    }
  });
  const cols = span + finals.length;
  const width = cols * CARD_W + Math.max(0, cols - 1) * COL_GAP;
  const height = Math.max(bottomEnd, ...placed.map((p) => p.y + CARD_H)) + 22;

  // A line from each played match to the next match its winner played.
  const lines: Array<{ d: string; mine: number }> = [];
  for (const p of placed) {
    const o = outcome(p.m);
    if (!o) continue;
    const next = placed
      .filter((q) => q.col > p.col && (q.m.clan1.id === o.win || q.m.clan2.id === o.win))
      .sort((a, b) => a.col - b.col)[0];
    if (!next) continue;
    const x1 = p.x + CARD_W;
    const y1 = p.y + CARD_H / 2;
    const x2 = next.x;
    const y2 = next.y + (next.m.clan1.id === o.win ? ROW_H / 2 + 1 : ROW_H + 2 + ROW_H / 2);
    const mid = x2 - COL_GAP / 2;
    lines.push({ d: `M${x1},${y1} H${mid} V${y2} H${x2}`, mine: o.win });
  }
  const finalIds = new Set(finals.flatMap((r) => r.matches.map((m) => m.matchId)));
  return { placed, heads, bands, width, height, lines, finalIds };
}

function Bracket({ rows, mine, onTeam, onMatch }: { rows: ResultRow[]; mine: Set<number>; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const rounds = roundsOf(rows);
  const third = rounds.filter((r) => r.lane === "third").flatMap((r) => r.matches);
  const main = rounds.filter((r) => r.lane !== "third");
  const [room, measure] = useMeasuredWidth(MIN_W);
  const cols = Math.max(main.filter((r) => r.lane === "upper" || r.lane === "main").length, main.filter((r) => r.lane === "lower").length) + main.filter((r) => r.lane === "final").length;
  const CARD_W = Math.round(Math.max(MIN_W, Math.min(MAX_W, (room - Math.max(0, cols - 1) * COL_GAP) / Math.max(1, cols))));
  const { placed, heads, bands, width, height, lines, finalIds } = useMemo(() => layout(main, CARD_W), [rows, CARD_W]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <section className="panel e2-panel br-bracket">
      <div className="e2-panel-head">
        <h2>{t("Playoffs")}</h2>
      </div>
      <div className="br-scroll" ref={measure}>
        <div className="br-box" style={{ width, height }}>
          {bands.map((y) => (
            <span key={y} className="br-band" style={{ top: y, width }} aria-hidden />
          ))}
          {heads.map((h, i) => (
            <span key={i} className="br-head" style={{ left: h.x, top: h.y, width: CARD_W }}>
              {h.label}
            </span>
          ))}
          <svg className="br-lines" width={width} height={height} aria-hidden>
            {lines.map((l, i) => (
              <path key={i} d={l.d} className={mine.has(l.mine) ? "mine" : undefined} />
            ))}
          </svg>
          {placed.map((p) => (
            <div key={p.m.matchId} className="br-at" style={{ left: p.x, top: p.y, width: CARD_W }}>
              {finalIds.has(p.m.matchId) && <Champion m={p.m} />}
              <MatchCard m={p.m} mine={mine} final={finalIds.has(p.m.matchId)} onTeam={onTeam} onOpen={() => onMatch(p.m.matchId)} />
            </div>
          ))}
        </div>
      </div>
      {third.length > 0 && (
        <div className="br-third">
          <span className="br-third-head">{t("3rd place")}</span>
          <div className="br-third-row">
            {third.map((m) => (
              <div key={m.matchId} style={{ width: CARD_W }}>
                <MatchCard m={m} mine={mine} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} />
              </div>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}

/** One match, two rows: each team with its score in a box, the winner's
 *  lit; a match to come says when under it. */
/** Over a played final: who won the division, in gold. */
function Champion({ m }: { m: ResultRow }) {
  const o = outcome(m);
  if (!o) return null;
  const team = o.win === m.clan1.id ? m.clan1 : m.clan2;
  return (
    <span className="br-champ">
      <MedalGlyph size={16} place={1} />
      <span>{t("Champion")}</span>
      <strong>{team.name}</strong>
    </span>
  );
}

function MatchCard({ m, mine, final, onTeam, onOpen }: { m: ResultRow; mine: Set<number>; final?: boolean; onTeam: (id: number) => void; onOpen: () => void }) {
  const o = outcome(m);
  const side = (x: ResultRow["clan1"], score: number | null) => (
    <div className={`br-side${o?.win === x.id ? " won" : o?.lose === x.id ? " lost" : ""}${mine.has(x.id) ? " mine" : ""}`}>
      <button
        className="br-team"
        onClick={(e) => {
          e.stopPropagation();
          onTeam(x.id);
        }}
        title={x.name}
      >
        <TeamAvatar src={x.avatar} />
        <span>{x.name}</span>
      </button>
      <span className="br-score">{played(m) ? (m.defaultWin && score === null ? "–" : (score ?? "–")) : ""}</span>
    </div>
  );
  return (
    <>
      <article
        className={`br-card${played(m) ? "" : " todo"}${final ? " final" : ""}`}
        role="link"
        tabIndex={0}
        onClick={onOpen}
        onKeyDown={(e) => e.key === "Enter" && onOpen()}
        title={m.time > 0 ? `${t("Open the match")} · ${fullDate(m.time)}` : t("Open the match")}
      >
        {side(m.clan1, m.r1)}
        {side(m.clan2, m.r2)}
      </article>
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
            <span>{t("Not scheduled yet")}</span>
          )}
        </div>
      )}
    </>
  );
}

/** A scheduled official from ETF2L's list, as a season row: its stage from
 *  its competition's name ("…: Low Playoffs"). */
function fromFixture(f: Fixture): ResultRow {
  return {
    matchId: f.matchId,
    time: f.time,
    season: f.season ?? 0,
    division: f.division,
    tier: f.tier,
    round: f.round,
    week: f.week,
    stage: /3rd place/i.test(f.competition) ? "3rd Place" : /playoffs/i.test(f.competition) ? "Playoffs" : /qualifier/i.test(f.competition) ? "Qualifiers" : /cup/i.test(f.competition) ? "Cup" : "regular",
    clan1: f.clan1,
    clan2: f.clan2,
    r1: null,
    r2: null,
    defaultWin: false,
  };
}
