import { useMemo, useState } from "react";
import type { Analysis, AnalysisPlayer } from "../../api/types";
import { clock } from "../../lib/format";
import { useMeasuredWidth } from "../../lib/measure";
import { inSlice, roundClock, type Slice } from "./common";
import { chargeLabel, sides, StateStrip } from "./StateStrip";

type Metric = "kills" | "deaths" | "damage";

const METRICS: Array<[Metric, string]> = [
  ["kills", "Kills"],
  ["deaths", "Deaths"],
  ["damage", "Damage"],
];

const H = 300;
const M = { top: 16, right: 150, bottom: 30, left: 48 };

/**
 * Running totals over game time, one stepped line per player. An emphasis
 * chart: the chosen player is the accent line, their team is light grey and
 * the other team darker grey, so the story is one line against the field.
 * Hover for everyone's total at that moment; click near a line to pick that
 * player for every view.
 */
export function TimelineChart(props: {
  a: Analysis;
  player: number;
  slice: Slice;
  onPick: (id: number) => void;
}) {
  const { a, player, slice, onPick } = props;
  const [metric, setMetric] = useState<Metric>("kills");
  const [asTable, setAsTable] = useState(false);
  const [hoverT, setHoverT] = useState<number | null>(null);

  const [width, wrap] = useMeasuredWidth(320, 900);

  const t0 = slice.startS;
  const t1 = Math.max(slice.endS, t0 + 1);

  // Per player: a step function, as sorted (time, running total) pairs.
  const series = useMemo(() => {
    return a.players.map((p) => {
      let steps: Array<[number, number]> = [];
      if (metric === "damage") {
        const b = a.damageSeries.find((s) => s.accountId === p.accountId)?.buckets ?? [];
        let sum = 0;
        b.forEach((v, i) => {
          const t = (i + 1) * a.bucketS;
          if (t <= t0) return;
          sum += v;
          steps.push([Math.min(t, t1), sum]);
        });
        steps = steps.filter(([t]) => t <= t1);
      } else {
        const times = a.kills
          .filter((k) => (metric === "kills" ? k.killer === p.accountId && k.victim !== p.accountId : k.victim === p.accountId))
          .map((k) => k.t)
          .filter((t) => t >= t0 && t <= t1)
          .sort((x, y) => x - y);
        steps = times.map((t, i) => [t, i + 1]);
      }
      return { p, steps, final: steps.length ? steps[steps.length - 1][1] : 0 };
    });
  }, [a, metric, t0, t1]);

  const valueAt = (steps: Array<[number, number]>, t: number) => {
    let v = 0;
    for (const [st, sv] of steps) {
      if (st > t) break;
      v = sv;
    }
    return v;
  };

  const maxV = Math.max(1, ...series.map((s) => s.final));
  const ticks = niceTicks(maxV, metric !== "damage");
  const top = ticks[ticks.length - 1];
  const plotW = width - M.left - M.right;
  const plotH = H - M.top - M.bottom;
  const x = (t: number) => M.left + ((t - t0) / (t1 - t0)) * plotW;
  const y = (v: number) => M.top + (1 - v / top) * plotH;

  const path = (steps: Array<[number, number]>) => {
    let d = `M${x(t0)},${y(0)}`;
    let prev = 0;
    for (const [t, v] of steps) {
      d += `H${x(t).toFixed(1)}V${y(v).toFixed(1)}`;
      prev = v;
    }
    return d + `H${x(t1)}V${y(prev)}`;
  };

  const me = a.players.find((p) => p.accountId === player);
  const tone = (p: AnalysisPlayer) =>
    p.accountId === player ? "tl-sel" : me && p.team === me.team ? "tl-mate" : "tl-foe";
  // The chosen line draws last, on top.
  const order = [...series].sort((s1, s2) => (s1.p.accountId === player ? 1 : 0) - (s2.p.accountId === player ? 1 : 0));
  const sel = series.find((s) => s.p.accountId === player);

  const tFromEvent = (e: React.MouseEvent<SVGSVGElement>) => {
    const mx = e.clientX - e.currentTarget.getBoundingClientRect().left;
    return Math.max(t0, Math.min(t1, t0 + ((mx - M.left) / plotW) * (t1 - t0)));
  };

  const ranked =
    hoverT === null
      ? []
      : series.map((s) => ({ p: s.p, v: valueAt(s.steps, hoverT) })).sort((r1, r2) => r2.v - r1.v);

  const fmt = (v: number) => (metric === "damage" ? v.toLocaleString() : String(v));

  return (
    <div className="tlc">
      <div className="tlc-controls">
        <div className="segmented" role="tablist" aria-label="Measure">
          {METRICS.map(([m, label]) => (
            <button key={m} role="tab" aria-selected={metric === m} className={metric === m ? "seg active" : "seg"} onClick={() => setMetric(m)}>
              {label}
            </button>
          ))}
        </div>
        <span className="legend">
          <span>
            <svg width="18" height="8" aria-hidden>
              <line x1="1" y1="4" x2="17" y2="4" className="tl-sel" />
            </svg>
            {me?.name ?? "chosen player"}
          </span>
          <span>
            <svg width="18" height="8" aria-hidden>
              <line x1="1" y1="4" x2="17" y2="4" className="tl-mate" />
            </svg>
            their team
          </span>
          <span>
            <svg width="18" height="8" aria-hidden>
              <line x1="1" y1="4" x2="17" y2="4" className="tl-foe" />
            </svg>
            other team
          </span>
        </span>
        <button className="linkish" onClick={() => setAsTable((t) => !t)}>
          {asTable ? "Show chart" : "Show as table"}
        </button>
      </div>

      {asTable ? (
        <div className="table-wrap">
          <table className="match-table">
            <thead>
              <tr>
                <th>Player</th>
                <th className="num">{METRICS.find(([m]) => m === metric)![1]}</th>
              </tr>
            </thead>
            <tbody>
              {[...series].sort((s1, s2) => s2.final - s1.final).map((s) => (
                <tr key={s.p.accountId} className="clickable" onClick={() => onPick(s.p.accountId)}>
                  <td className={`team-${s.p.team.toLowerCase()}`}>
                    {s.p.name}
                    {s.p.accountId === player && <span className="muted"> · chosen</span>}
                  </td>
                  <td className="num">{fmt(s.final)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="tlc-wrap" ref={wrap}>
          <svg
            width={width}
            height={H}
            role="img"
            aria-label={`${METRICS.find(([m]) => m === metric)![1]} over the match, one line per player`}
            onMouseMove={(e) => setHoverT(tFromEvent(e))}
            onMouseLeave={() => setHoverT(null)}
            onClick={(e) => {
              // Pick the line nearest the pointer at that moment.
              const t = tFromEvent(e);
              const my = e.clientY - e.currentTarget.getBoundingClientRect().top;
              let best: number | null = null;
              let bestD = 12;
              for (const s of series) {
                const d = Math.abs(y(valueAt(s.steps, t)) - my);
                if (d < bestD) {
                  bestD = d;
                  best = s.p.accountId;
                }
              }
              if (best !== null) onPick(best);
            }}
          >
            {ticks.map((v) => (
              <g key={v}>
                <line x1={M.left} x2={M.left + plotW} y1={y(v)} y2={y(v)} className="tl-grid" />
                <text x={M.left - 8} y={y(v)} className="tl-axis" textAnchor="end" dominantBaseline="middle">
                  {metric === "damage" && v >= 1000 ? `${Math.round(v / 1000)}k` : v}
                </text>
              </g>
            ))}
            {a.rounds
              .filter((r) => inSlice(r.roundNum, slice) && r.startS > t0 && r.startS < t1)
              .map((r) => (
                <line key={r.roundNum} x1={x(r.startS)} x2={x(r.startS)} y1={M.top} y2={M.top + plotH} className="tlc-round" />
              ))}
            {a.rounds
              .filter((r) => inSlice(r.roundNum, slice) && r.endS > t0 && r.startS < t1)
              .map((r) => (
                <text key={r.roundNum} x={x(Math.max(r.startS, t0)) + 4} y={H - 10} className="tl-axis">
                  R{r.roundNum}
                </text>
              ))}

            {order.map((s) => (
              <path key={s.p.accountId} d={path(s.steps)} className={`tlc-line ${tone(s.p)}`} />
            ))}

            {sel && (
              <text x={x(t1) + 8} y={y(sel.final)} className="tlc-end" dominantBaseline="middle">
                {sel.p.name} {fmt(sel.final)}
              </text>
            )}

            {hoverT !== null && <line x1={x(hoverT)} x2={x(hoverT)} y1={M.top} y2={M.top + plotH} className="tl-cross" />}
          </svg>

          {me && (
            <StateStrip
              a={a}
              mine={me.team}
              t0={t0}
              t1={t1}
              x={x}
              left={M.left}
              plotW={plotW}
              width={width}
              hoverT={hoverT}
              onHover={setHoverT}
            />
          )}

          {hoverT !== null && (
            <div className="tl-tip tlc-tip" style={{ left: Math.min(width - 230, x(hoverT) + 12), top: M.top }}>
              <div className="tip-meta">
                {slice.oneRound ? clock(hoverT - t0) : roundClock(hoverT, a.rounds)}
              </div>
              {me && <StateLine a={a} mine={me.team} t={hoverT} />}
              {ranked.map((r) => (
                <div key={r.p.accountId} className={r.p.accountId === player ? "tlc-tip-row sel" : "tlc-tip-row"}>
                  <span className={`team-${r.p.team.toLowerCase()}`}>{r.p.name}</span>
                  <span className="num">{fmt(r.v)}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** Round axis steps: 0 up to a round number at or above `max`, about 4 steps.
 *  Counts step by whole numbers. */
function niceTicks(max: number, whole: boolean): number[] {
  const raw = max / 4;
  const mag = 10 ** Math.floor(Math.log10(raw));
  let step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
  if (whole) step = Math.max(1, Math.round(step));
  const out: number[] = [];
  for (let v = 0; v < max + step; v += step) out.push(Math.round(v * 100) / 100);
  return out;
}

/** The game state at one moment, for the hover tip. */
function StateLine({ a, mine, t }: { a: Analysis; mine: "Red" | "Blue"; t: number }) {
  const s = sides(a.state, mine);
  const i = Math.min(Math.floor(t), s.alive.length - 1);
  if (i < 0) return null;
  const ad = s.advantage[i] > 0 ? " · uber advantage yours" : s.advantage[i] < 0 ? " · uber advantage theirs" : "";
  return (
    <div className="tip-meta">
      {s.alive[i]} v {s.theirAlive[i]} alive · uber {chargeLabel(s.charge[i])} v {chargeLabel(s.theirCharge[i])}
      {ad}
    </div>
  );
}
