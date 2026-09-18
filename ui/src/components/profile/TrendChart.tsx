import { useEffect, useMemo, useRef, useState } from "react";
import type { TrendPoint } from "../../api/types";
import { formatDate, splitMap } from "../../lib/format";

/**
 * Rating over time, as an emphasis chart: the rolling average is the story
 * (accent line), single games are context (grey dots). Games sit at equal
 * spacing, oldest to newest — this account has years with a handful of games
 * and years with hundreds, and a date axis would crush the busy years flat.
 *
 * Hover or arrow keys move a crosshair; click or Enter opens the match.
 */

type Range = "50" | "200" | "all";
const RANGES: Array<{ id: Range; label: string }> = [
  { id: "50", label: "Last 50" },
  { id: "200", label: "Last 200" },
  { id: "all", label: "All" },
];

const H = 260;
const M = { top: 14, right: 64, bottom: 30, left: 36 };
const Y_TICKS = [0, 25, 50, 75, 100];

export function TrendChart(props: {
  points: TrendPoint[];
  rollingWindow: number;
  onOpen: (logId: number) => void;
}) {
  const { points, rollingWindow, onOpen } = props;
  const [range, setRange] = useState<Range>(points.length > 200 ? "200" : "all");
  const [asTable, setAsTable] = useState(false);
  const [hover, setHover] = useState<number | null>(null);

  const wrap = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => setWidth(Math.max(320, entries[0].contentRect.width)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const shown = useMemo(
    () => (range === "all" ? points : points.slice(-Number(range))),
    [points, range],
  );

  // Selecting a new range clears the crosshair rather than leaving it on a
  // point that moved.
  useEffect(() => setHover(null), [range]);

  const plotW = width - M.left - M.right;
  const plotH = H - M.top - M.bottom;
  const x = (i: number) => M.left + (shown.length <= 1 ? plotW / 2 : (i / (shown.length - 1)) * plotW);
  const y = (v: number) => M.top + (1 - v / 100) * plotH;

  const linePath = useMemo(() => {
    let d = "";
    shown.forEach((p, i) => {
      if (p.rolling === null) return;
      d += `${d === "" ? "M" : "L"}${x(i).toFixed(1)},${y(p.rolling).toFixed(1)}`;
    });
    return d;
    // x and y close over width and length only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shown, width]);

  // A label at the first game of each year, skipping any that would collide.
  const yearTicks = useMemo(() => {
    const out: Array<{ i: number; label: string }> = [];
    let lastYear = "";
    let lastX = -Infinity;
    shown.forEach((p, i) => {
      if (p.playedAt === null) return;
      const yr = String(new Date(p.playedAt * 1000).getFullYear());
      if (yr !== lastYear) {
        lastYear = yr;
        if (x(i) - lastX >= 44) {
          out.push({ i, label: yr });
          lastX = x(i);
        }
      }
    });
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shown, width]);

  const last = [...shown].reverse().find((p) => p.rolling !== null) ?? null;
  const lastIdx = last ? shown.lastIndexOf(last) : -1;
  const dotR = shown.length > 250 ? 3 : 4;

  function nearest(clientX: number, svg: SVGSVGElement): number {
    const rect = svg.getBoundingClientRect();
    const px = clientX - rect.left;
    const t = (px - M.left) / Math.max(1, plotW);
    return Math.max(0, Math.min(shown.length - 1, Math.round(t * (shown.length - 1))));
  }

  const h = hover !== null ? shown[hover] : null;

  return (
    <section className="panel trend">
      <header className="trend-head">
        <div>
          <h2>Rating over time</h2>
          <p className="hint">
            {shown.length} games, oldest to newest. Click a game to open it.
          </p>
        </div>
        <div className="trend-controls">
          <div className="segmented" role="tablist" aria-label="Range">
            {RANGES.map((r) => (
              <button
                key={r.id}
                role="tab"
                aria-selected={range === r.id}
                className={range === r.id ? "seg active" : "seg"}
                onClick={() => setRange(r.id)}
              >
                {r.label}
              </button>
            ))}
          </div>
          <button className="linkish" onClick={() => setAsTable((t) => !t)}>
            {asTable ? "Show chart" : "Show as table"}
          </button>
        </div>
      </header>

      <div className="legend trend-legend">
        <span>
          <svg width="18" height="8" aria-hidden>
            <line x1="1" y1="4" x2="17" y2="4" className="tl-line" />
          </svg>
          {rollingWindow}-game average
        </span>
        <span>
          <svg width="10" height="10" aria-hidden>
            <circle cx="5" cy="5" r="4" className="tl-dot" />
          </svg>
          single game
        </span>
        <span>
          <svg width="18" height="8" aria-hidden>
            <line x1="1" y1="4" x2="17" y2="4" className="tl-median" />
          </svg>
          50 = the typical player you face
        </span>
      </div>

      {asTable ? (
        <TrendTable points={shown} onOpen={onOpen} />
      ) : (
        <div className="trend-wrap" ref={wrap}>
          <svg
            width={width}
            height={H}
            role="img"
            aria-label={`Rating over ${shown.length} games; latest ${rollingWindow}-game average ${last?.rolling ?? "n/a"}`}
            tabIndex={0}
            className="trend-svg"
            onPointerMove={(e) => setHover(nearest(e.clientX, e.currentTarget))}
            onPointerLeave={() => setHover(null)}
            onClick={(e) => onOpen(shown[nearest(e.clientX, e.currentTarget)].logId)}
            onKeyDown={(e) => {
              if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
                e.preventDefault();
                const step = e.key === "ArrowRight" ? 1 : -1;
                setHover((i) => Math.max(0, Math.min(shown.length - 1, (i ?? shown.length - 1) + step)));
              } else if (e.key === "Enter" && hover !== null) {
                onOpen(shown[hover].logId);
              }
            }}
            onBlur={() => setHover(null)}
          >
            {Y_TICKS.map((t) => (
              <g key={t}>
                <line
                  x1={M.left}
                  x2={M.left + plotW}
                  y1={y(t)}
                  y2={y(t)}
                  className={t === 50 ? "tl-median" : "tl-grid"}
                />
                <text x={M.left - 8} y={y(t)} className="tl-axis" textAnchor="end" dominantBaseline="middle">
                  {t}
                </text>
              </g>
            ))}

            {yearTicks.map((t) => (
              <text key={t.i} x={x(t.i)} y={H - 8} className="tl-axis" textAnchor="middle">
                {t.label}
              </text>
            ))}

            {shown.map((p, i) => (
              <circle key={p.logId} cx={x(i)} cy={y(p.score)} r={dotR} className="tl-dot" />
            ))}

            <path d={linePath} className="tl-line" />

            {last && last.rolling !== null && (
              <>
                <circle cx={x(lastIdx)} cy={y(last.rolling)} r={4.5} className="tl-end" />
                <text x={x(lastIdx) + 9} y={y(last.rolling)} className="tl-endlabel" dominantBaseline="middle">
                  {last.rolling.toFixed(0)}
                </text>
              </>
            )}

            {h && hover !== null && (
              <g pointerEvents="none">
                <line x1={x(hover)} x2={x(hover)} y1={M.top} y2={M.top + plotH} className="tl-cross" />
                <circle cx={x(hover)} cy={y(h.score)} r={5} className="tl-hover-dot" />
                {h.rolling !== null && <circle cx={x(hover)} cy={y(h.rolling)} r={4.5} className="tl-end" />}
              </g>
            )}
          </svg>

          {h && hover !== null && (
            <div
              className="tl-tip"
              style={{
                left: Math.min(width - 190, Math.max(0, x(hover) + 12)),
                top: M.top,
              }}
            >
              {h.rolling !== null && (
                <div className="tip-row">
                  <svg width="14" height="6" aria-hidden>
                    <line x1="1" y1="3" x2="13" y2="3" className="tl-line" />
                  </svg>
                  <strong>{h.rolling.toFixed(1)}</strong>
                  <span className="muted">{rollingWindow}-game avg</span>
                </div>
              )}
              <div className="tip-row">
                <svg width="14" height="8" aria-hidden>
                  <circle cx="7" cy="4" r="3.5" className="tl-dot solid" />
                </svg>
                <strong>{h.score.toFixed(0)}</strong>
                <span className="muted">this game</span>
              </div>
              <div className="tip-meta">
                {formatDate(h.playedAt, true)} · {splitMap(h.map).name ?? "unknown map"}
                {h.result && <span className={`result-${h.result}`}> · {h.result}</span>}
              </div>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

/** The same data without a chart: every value reachable without hovering. */
function TrendTable({ points, onOpen }: { points: TrendPoint[]; onOpen: (logId: number) => void }) {
  const rows = [...points].reverse();
  return (
    <div className="table-wrap trend-table">
      <table className="match-table">
        <thead>
          <tr>
            <th>Date</th>
            <th>Map</th>
            <th>Result</th>
            <th className="num">Rating</th>
            <th className="num">10-game avg</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((p) => (
            <tr key={p.logId} className="clickable" tabIndex={0} onClick={() => onOpen(p.logId)} onKeyDown={(e) => e.key === "Enter" && onOpen(p.logId)}>
              <td className="muted nowrap">{formatDate(p.playedAt, true)}</td>
              <td className="nowrap">{splitMap(p.map).name ?? <span className="muted">unknown</span>}</td>
              <td>{p.result && <span className={`result result-${p.result}`}>{p.result}</span>}</td>
              <td className="num">{p.score.toFixed(0)}</td>
              <td className="num">{p.rolling === null ? "—" : p.rolling.toFixed(1)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
