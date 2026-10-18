import { useQuery } from "@tanstack/react-query";
import { useLayoutEffect, useState } from "react";
import { STATE_ROWS, STRIP_H, StateStrip } from "../analysis/StateStrip";
import { StateLine } from "../analysis/TimelineChart";
import { api } from "../../api/client";
import type { Analysis, EventRow, Jump, MatchDetail, RoundRow, Team } from "../../api/types";
import { copy } from "../../lib/toast";
import { clock, teamLabel } from "../../lib/format";
import { t as tr, tx } from "../../lib/i18n";

/**
 * Every round on its own track, with caps, ubers, drops and medic deaths
 * placed where they happened. Medic deaths are the only kills logs.tf
 * timestamps, so they are the only picks that can appear here.
 *
 * Teams are stable teams throughout: stopwatch swaps colours between halves,
 * and the backend has already mapped every round back to who was who. So a
 * team keeps one colour down the whole page, and each round says which colour
 * it actually wore.
 */
export function RoundTimeline({ d }: { d: MatchDetail }) {
  // Same key as the kill-by-kill panel, so this is the cache, not a fetch.
  const raw = useQuery({
    queryKey: ["analysis", d.logId],
    queryFn: () => api.getMatchAnalysis(d.logId),
    staleTime: 5 * 60_000,
  });
  if (d.rounds.length === 0) {
    return (
      <section className="panel">
        <h2>{tr("Rounds")}</h2>
        <p className="hint" style={{ marginTop: 6 }}>{tr("This log has no round data.")}</p>
      </section>
    );
  }

  const left = d.leftTeam;
  const right: Team = left === "Red" ? "Blue" : "Red";
  const us = d.myTeam !== null;
  const anySwapped = d.rounds.some((r) => r.coloursSwapped);
  // Only worth naming the demo in a copy confirmation when there are several.
  const demoName = (j: Jump) =>
    d.demos.length > 1 ? d.demos.find((x) => x.demoId === j.demoId)?.fileName : undefined;
  const hasJumps = d.rounds.some((r) => r.jump !== null);
  const hasMine = d.rounds.some((r) => r.events.some((e) => MINE.has(e.kind)));
  const won = d.rounds.filter((r) => r.winner === left).length;
  const lost = d.rounds.filter((r) => r.winner === right).length;
  const leftName = us ? tr("Us") : teamLabel(left);
  const rightName = us ? tr("Them") : teamLabel(right);

  return (
    <section className="panel rounds">
      <header className="rounds-head">
        <div>
          <h2>{tr("Rounds")}</h2>
          <p className="hint" style={{ marginTop: 4 }}>
            {tx("{0}{1}Each round has a lane per team: that team's caps and ubers, and its Medic going down.{2}", { "0": us ? tr("You won {won} of {rounds} rounds.", { won: won, rounds: d.rounds.length }) : `${teamLabel(left)} ${won}, ${teamLabel(right)} ${lost}.`, "1": " ", "2": anySwapped && tr(" Sides swap between stopwatch halves; colours follow the team, not the side.") })}
          </p>
        </div>
        <Legend jumps={hasJumps} mine={hasMine} />
      </header>
      <div className="round-list">
        {d.rounds.map((r) => (
          <Round
            key={r.roundNum}
            r={r}
            a={raw.data ?? null}
            myTeam={d.myTeam}
            left={left}
            right={right}
            us={us}
            names={[leftName, rightName]}
            showMine={hasMine}
            demoName={demoName}
          />
        ))}
      </div>
    </section>
  );
}

/** Events about the owner alone: their kills, deaths and demo killstreaks. */
const MINE = new Set(["my_kill", "my_death", "killstreak"]);

function Round(props: {
  r: RoundRow;
  /** The raw log, for the uber and numbers strip. Null until it loads. */
  a: Analysis | null;
  myTeam: Team | null;
  left: Team;
  right: Team;
  us: boolean;
  names: [string, string];
  showMine: boolean;
  demoName: (j: Jump) => string | undefined;
}) {
  const { r, a, myTeam, left, right, us, names, showMine, demoName } = props;
  const len = r.lengthS ?? Math.max(1, ...r.events.map((e) => e.atS));
  const events = r.events.filter((e) => e.kind !== "round_win");
  // An event with no team (rare) goes in the second lane rather than nowhere.
  const lane = (team: Team) =>
    events.filter((e) => !MINE.has(e.kind) && (e.team === team || (team === right && e.team === null)));
  const mine = events.filter((e) => MINE.has(e.kind));

  const result = r.winner === null ? "–" : !us ? `${teamLabel(r.winner)}` : r.winner === left ? tr("Won") : tr("Lost");
  const resultClass = r.winner === null || !us ? "round-result" : r.winner === left ? "round-result result-W" : "round-result result-L";

  // The colour the left team actually wore this round.
  const leftWore: Team = r.coloursSwapped ? right : left;

  const ticks = Array.from({ length: Math.floor(len / 60) }, (_, i) => (i + 1) * 60);
  const stat = (t: Team, red: number | null, blue: number | null) => (t === "Red" ? red : blue);
  // Where the pointer is on the time axis, as seconds into the round.
  const [cursor, setCursor] = useState<number | null>(null);
  const track = (ev: React.MouseEvent<HTMLDivElement>) => {
    const ticksEl = ev.currentTarget.querySelector<HTMLElement>(".lane-ticks");
    if (!ticksEl) return;
    const box = ticksEl.getBoundingClientRect();
    const f = (ev.clientX - box.left) / box.width;
    setCursor(f < 0 || f > 1 ? null : f * len);
  };
  const markers = (list: EventRow[]) =>
    list.map((e, i) => <Marker key={i} e={e} len={len} left={left} us={us} demoName={demoName} />);

  return (
    <div className="round">
      <div className="round-meta">
        {r.jump ? (
          <button
            className="round-num jumpable"
            title={tr("Copy demo_gototick {tick} (round start)", { tick: r.jump.tick })}
            onClick={() => jumpTo(r.jump!, `round ${r.roundNum} start`, demoName)}
          >
            R{r.roundNum}
          </button>
        ) : (
          <span className="round-num">R{r.roundNum}</span>
        )}
        <span className={resultClass}>{result}</span>
        <span className="round-sub">
          {clock(r.lengthS)}
          {us && (
            <span className={`wore wore-${leftWore.toLowerCase()}`} title={r.coloursSwapped ? tr("Sides swapped this half") : undefined}>
              {teamLabel(leftWore)}
            </span>
          )}
        </span>
      </div>

      <div className="round-lanes" onMouseMove={track} onMouseLeave={() => setCursor(null)}>
        <span className={`lane-label team-${left.toLowerCase()}`}>{names[0]}</span>
        <div className={`lane lane-team lane-${left.toLowerCase()}`}>{markers(lane(left))}</div>
        {showMine && (
          <>
            <span className="lane-label">{tr("You")}</span>
            <div className="lane lane-mine">{markers(mine)}</div>
          </>
        )}
        <span className={`lane-label team-${right.toLowerCase()}`}>{names[1]}</span>
        <div className={`lane lane-team lane-${right.toLowerCase()}`}>{markers(lane(right))}</div>
        <div className="lane-ticks" aria-hidden>
          {ticks.map((t) => (
            <span key={t} className="tick" style={{ left: `${(t / len) * 100}%` }}>
              {/* Every minute labelled on short rounds, every other on long ones. */}
              {(len <= 600 || (t / 60) % 2 === 0) && <span className="tick-label">{t / 60}:00</span>}
            </span>
          ))}
          {cursor !== null && (
            <span className="lane-cursor" style={{ left: `${(cursor / len) * 100}%` }}>
              <span>{clock(cursor)}</span>
            </span>
          )}
        </div>
        {a && <RoundStrip a={a} roundNum={r.roundNum} len={len} mine={myTeam ?? left} />}
      </div>

      <div className="round-stats">
        <StatRow label="Kills" a={stat(left, r.redKills, r.blueKills)} b={stat(right, r.redKills, r.blueKills)} left={left} right={right} />
        <StatRow label="Ubers" a={stat(left, r.redUbers, r.blueUbers)} b={stat(right, r.redUbers, r.blueUbers)} left={left} right={right} />
        {r.firstcap && (
          <div className="rs-row">
            <span className="rs-label">{tr("First cap")}</span>
            <span className={`team-${r.firstcap.toLowerCase()}`}>{names[r.firstcap === left ? 0 : 1]}</span>
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * The kill-by-kill timeline's game state, under one round's lanes: players up
 * or down, each side's uber building, used and ready, and who holds the
 * advantage. On the lanes' own time axis, so an uber pop sits directly under
 * its marker and a round lost a player down for most of it reads as that.
 *
 * The state series is one sample a game second, laid out with the rounds end
 * to end, so a round is the slice from its own start.
 */
function RoundStrip({ a, roundNum, len, mine }: { a: Analysis; roundNum: number; len: number; mine: Team }) {
  // A callback ref, not `useRef`: the box can appear after the first render
  // (the round's span arrives with the analysis), and the watcher has to
  // attach to it whenever it does.
  const [el, setEl] = useState<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(0);
  const [hoverT, setHoverT] = useState<number | null>(null);
  useLayoutEffect(() => {
    if (!el) return;
    // A ResizeObserver reports the size once as soon as it starts watching,
    // so this also takes the first measurement.
    const ro = new ResizeObserver(([e]) => setWidth(Math.floor(e.contentRect.width)));
    ro.observe(el);
    return () => ro.disconnect();
  }, [el]);

  const span = a.rounds.find((x) => x.roundNum === roundNum);
  if (!span || span.endS <= span.startS) return null;
  const t0 = span.startS;
  // The lanes' scale, so markers above and state below line up.
  const t1 = span.startS + len;
  const x = (t: number) => ((t - t0) / (t1 - t0)) * width;

  return (
    <>
      <span className="lane-label ss-lane-labels" style={{ height: STRIP_H }}>
        {STATE_ROWS.map((r) => (
          <span key={r.label} style={{ top: r.top, height: r.height }}>
            {tr(r.label)}
          </span>
        ))}
      </span>
      <div className="round-strip" ref={setEl}>
        {width > 0 && (
          <StateStrip
            a={a}
            mine={mine}
            t0={t0}
            t1={t1}
            x={x}
            left={0}
            plotW={width}
            width={width}
            hoverT={hoverT}
            onHover={setHoverT}
            labels={false}
            summary={false}
          />
        )}
        {hoverT !== null && (
          <div className="tl-tip round-strip-tip" style={{ left: Math.min(Math.max(0, width - 260), x(hoverT) + 10) }}>
            <div className="tip-meta">{clock(hoverT - t0)}</div>
            <StateLine a={a} mine={mine} t={hoverT} />
          </div>
        )}
      </div>
    </>
  );
}

function Marker(props: {
  e: EventRow;
  len: number;
  left: Team;
  us: boolean;
  demoName: (j: Jump) => string | undefined;
}) {
  const { e, len, left, us, demoName } = props;
  const pos = `${Math.min(100, (e.atS / len) * 100)}%`;
  const team = e.team?.toLowerCase() ?? "none";
  const at = clock(e.atS);
  const who = e.team === null ? "" : us ? (e.team === left ? "Our" : "Their") : teamLabel(e.team);

  // A marker with a jump is a button: clicking it copies the tick.
  const jump = e.jump;
  const act = jump
    ? {
        role: "button" as const,
        tabIndex: 0,
        onClick: () => jumpTo(jump, `${e.kind.replace("_", " ")} at ${at}`, demoName),
        onKeyDown: (k: React.KeyboardEvent) => {
          if (k.key === "Enter" || k.key === " ") {
            k.preventDefault();
            jumpTo(jump, `${e.kind.replace("_", " ")} at ${at}`, demoName);
          }
        },
      }
    : {};
  const hint = jump ? " — click to copy demo_gototick" : "";
  const jumpCls = jump ? " jumpable" : "";

  switch (e.kind) {
    case "my_kill":
      return (
        <span
          className={`mk mk-mine mk-my-kill${jumpCls}`}
          style={{ left: pos }}
          title={tr("{at} — you killed {1}{2}{hint}", { "1": e.player ?? "?", "2": e.value ? ` (${e.value})` : "", at: at, hint: tr(hint) })}
          {...act}
        />
      );
    case "my_death":
      return (
        <span
          className={`mk mk-mine mk-my-death${jumpCls}`}
          style={{ left: pos }}
          title={tr("{at} — {1} killed you{2}{hint}", { "1": e.killer ?? "?", "2": e.value ? ` (${e.value})` : "", at: at, hint: tr(hint) })}
          {...act}
        />
      );
    case "killstreak":
      return (
        <span
          className={`mk mk-streak${jumpCls}`}
          style={{ left: pos }}
          title={tr("{at} — killstreak of {1} (from your demo){hint}", { "1": e.value ?? "?", at: at, hint: tr(hint) })}
          {...act}
        >
          {e.value ?? "K"}
        </span>
      );
    case "pointcap":
      return (
        <span
          className={`mk mk-cap team-bg-${team}${jumpCls}`}
          style={{ left: pos }}
          title={tr("{at} — {who} cap{2}{hint}", { "2": e.point !== null ? tr(", point {0}", { "0": e.point }) : "", at: at, who: who, hint: tr(hint) })}
          {...act}
        />
      );
    case "charge":
      return (
        <span
          className={`mk mk-uber team-border-${team}${jumpCls}`}
          style={{ left: pos }}
          title={tr("{at} — {who} uber{2}{3}{hint}", { "2": e.medigun && e.medigun !== "medigun" ? ` (${e.medigun})` : "", "3": e.player ? `, ${e.player}` : "", at: at, who: who, hint: tr(hint) })}
          {...act}
        >
          <MarkGlyph kind="uber" />
        </span>
      );
    case "drop":
      return (
        <span
          className={`mk mk-drop${jumpCls}`}
          style={{ left: pos }}
          title={tr("{at} — {who} drop: {2} died with uber ready{hint}", { "2": e.player ?? "medic", at: at, who: who, hint: tr(hint) })}
          {...act}
        >
          D
        </span>
      );
    case "medic_death":
      return (
        <span
          className={`mk mk-pick ${e.killerIsMe ? "by-me" : ""} team-border-${team}${jumpCls}`}
          style={{ left: pos }}
          title={tr("{at} — {who} Medic {2} killed{3}{4}{hint}", { "2": e.player ?? "", "3": e.killer ? tr(" by {0}", { "0": e.killer }) : "", "4": e.killerIsMe ? tr(" (you)") : "", at: at, who: who, hint: tr(hint) })}
          {...act}
        >
          <MarkGlyph kind="pick" />
        </span>
      );
    default:
      return null;
  }
}

function StatRow(props: { label: string; a: number | null; b: number | null; left: Team; right: Team }) {
  const { label, a, b, left, right } = props;
  if (a === null && b === null) return null;
  return (
    <div className="rs-row">
      <span className="rs-label">{tr(label)}</span>
      <span>
        <strong className={`team-${left.toLowerCase()}`}>{a ?? "–"}</strong>
        <span className="sep"> – </span>
        <strong className={`team-${right.toLowerCase()}`}>{b ?? "–"}</strong>
      </span>
    </div>
  );
}

function jumpTo(j: Jump, what: string, demoName: (j: Jump) => string | undefined) {
  const name = demoName(j);
  void copy(`demo_gototick ${j.tick}`, name ? `${what} (in ${name})` : what);
}

function Legend({ jumps, mine }: { jumps: boolean; mine: boolean }) {
  return (
    <div className="legend">
      {jumps && <span className="legend-note">{tr("click any marker to copy its tick")}</span>}
      <span>
        {tx("{0} cap", { "0": <span className="mk-demo mk-cap team-bg-none" /> })}</span>
      <span>
        {tx("{0} uber", { "0": <span className="mk-demo mk-uber"><MarkGlyph kind="uber" /></span> })}</span>
      <span>
        {tx("{0} drop", { "0": <span className="mk-demo mk-drop">D</span> })}</span>
      <span>
        {tx("{0} Medic down", { "0": <span className="mk-demo mk-pick"><MarkGlyph kind="pick" /></span> })}</span>
      <span>
        {tx("{0} killed by you", { "0": <span className="mk-demo mk-pick by-me"><MarkGlyph kind="pick" /></span> })}</span>
      {jumps && (
        <span>
          {tx("{0} your killstreak", { "0": <span className="mk-demo mk-streak">4</span> })}</span>
      )}
      {mine && (
        <>
          <span>
            {tx("{0} your kill", { "0": <span className="mk-demo mk-mine mk-my-kill" /> })}</span>
          <span>
            {tx("{0} your death", { "0": <span className="mk-demo mk-mine mk-my-death" /> })}</span>
        </>
      )}
    </div>
  );
}

/**
 * A marker's letter or cross drawn, not typed: a font's "U" and "✚" sit a
 * pixel or two off centre, and which way depended on the font and where the
 * marker landed (Flashy: "sometimes they're centered sometimes leaning left
 * or right"). On a 16-unit grid, centred by construction.
 */
function MarkGlyph({ kind }: { kind: "uber" | "pick" }) {
  return (
    <svg className="mk-glyph" viewBox="0 0 16 16" aria-hidden>
      {kind === "uber" ? (
        <path d="M5 4v4.5a3 3 0 0 0 6 0V4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      ) : (
        <path d="M8 3.5v9M3.5 8h9" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" />
      )}
    </svg>
  );
}
