import { useEffect, useMemo, useState } from "react";
import type { TrendPoint } from "../../api/types";
import { formatDate, rating, splitMap } from "../../lib/format";
import { useMeasuredWidth } from "../../lib/measure";
import { t as tr, tx, k } from "../../lib/i18n";
import { SortTh, useSort } from "../../lib/sort";
import { tip } from "../../lib/glossary";

/**
 * Rating over time, as an emphasis chart: the rolling average is the story
 * (accent line), single games are context (grey dots), and officials stand
 * out from the grey as light dots. Games sit at equal
 * spacing, oldest to newest — this account has years with a handful of games
 * and years with hundreds, and a date axis would crush the busy years flat.
 *
 * Hover or arrow keys move a crosshair; click or Enter opens the match.
 */

type Range = "50" | "200" | "all";
const RANGES: Array<{ id: Range; label: string }> = [
  { id: "50", label: k("Last 50") },
  { id: "200", label: k("Last 200") },
  { id: "all", label: k("All") },
];

const H = 260;
const M = { top: 14, right: 64, bottom: 30, left: 36 };
/** The band a rating actually lives in: 0.60 is a poor game, 1.40 a strong
 *  one, and 1.00 the line everything is read against. */
const Y_TICKS = [0.4, 0.7, 1.0, 1.3, 1.6];
const Y_MIN = 0.25;
const Y_MAX = 1.75;

export function TrendChart(props: {
  points: TrendPoint[];
  rollingWindow: number;
  onOpen: (logId: number) => void;
}) {
  const { points, rollingWindow, onOpen } = props;
  const [range, setRange] = useState<Range>(points.length > 200 ? "200" : "all");
  const [asTable, setAsTable] = useState(false);
  const [hover, setHover] = useState<number | null>(null);

  // A ref callback, not an effect: "Show as table" unmounts the chart and
  // brings back a different element, and an observer attached once would be
  // left watching the old one. See useMeasuredWidth.
  const [width, wrap] = useMeasuredWidth(320, 800);

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
  const y = (v: number) => M.top + (1 - (v - Y_MIN) / (Y_MAX - Y_MIN)) * plotH;

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
  const hasOfficials = shown.some((p) => p.kind === "official");

  function nearest(clientX: number, svg: SVGSVGElement): number {
    const rect = svg.getBoundingClientRect();
    // The svg scales to its box, so a screen pixel is not always a viewBox
    // unit; between a resize and the observer reporting it, they differ.
    const scale = rect.width > 0 ? width / rect.width : 1;
    const px = (clientX - rect.left) * scale;
    const t = (px - M.left) / Math.max(1, plotW);
    return Math.max(0, Math.min(shown.length - 1, Math.round(t * (shown.length - 1))));
  }

  const h = hover !== null ? shown[hover] : null;

  return (
    <section className="panel trend">
      <header className="trend-head">
        <div>
          <h2>{tr("Rating over time")}</h2>
          <p className="hint">
            {tx("{shown} games, oldest to newest. Click a game to open it.", { shown: shown.length })}</p>
        </div>
        <div className="trend-controls">
          <div className="segmented" role="tablist" aria-label={tr("Range")}>
            {RANGES.map((r) => (
              <button
                key={r.id}
                role="tab"
                aria-selected={range === r.id}
                className={range === r.id ? "seg active" : "seg"}
                onClick={() => setRange(r.id)}
              >
                {tr(r.label)}
              </button>
            ))}
          </div>
          <button className="linkish" onClick={() => setAsTable((t) => !t)}>
            {asTable ? tr("Show chart") : tr("Show as table")}
          </button>
        </div>
      </header>

      <div className="legend trend-legend">
        <span>
          <svg width="18" height="8" aria-hidden>
            <line x1="1" y1="4" x2="17" y2="4" className="tl-line" />
          </svg>
          {tr("{rollingWindow}-game average", { rollingWindow })}</span>
        <span>
          <svg width="10" height="10" aria-hidden>
            <circle cx="5" cy="5" r="4" className="tl-dot" />
          </svg>{tr("single game")}</span>
        {hasOfficials && (
          <span>
            <svg width="10" height="10" aria-hidden>
              <circle cx="5" cy="5" r="4" className="tl-dot official" />
            </svg>{tr("ETF2L official")}</span>
        )}
        <span>
          <svg width="18" height="8" aria-hidden>
            <line x1="1" y1="4" x2="17" y2="4" className="tl-median" />
          </svg>{tr("1.00 = the typical player you face")}</span>
      </div>

      {asTable ? (
        <TrendTable points={shown} onOpen={onOpen} />
      ) : (
        <div className="trend-wrap" ref={wrap}>
          <svg
            width="100%"
            viewBox={`0 0 ${width} ${H}`}
            height={H}
            role="img"
            aria-label={tr("Rating over {shown} games; latest {rollingWindow}-game average {2}", { "2": last?.rolling ?? "n/a", shown: shown.length, rollingWindow: rollingWindow })}
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
                  className={t === 1.0 ? "tl-median" : "tl-grid"}
                />
                <text x={M.left - 8} y={y(t)} className="tl-axis" textAnchor="end" dominantBaseline="middle">
                  {t.toFixed(2)}
                </text>
              </g>
            ))}

            {yearTicks.map((t) => (
              <text key={t.i} x={x(t.i)} y={H - 8} className="tl-axis" textAnchor="middle">
                {tr(t.label)}
              </text>
            ))}

            {shown.map((p, i) =>
              p.kind === "official" ? null : (
                <circle key={p.logId} cx={x(i)} cy={y(p.score)} r={dotR} className="tl-dot" />
              ),
            )}
            {/* Drawn last so no grey dot covers one. */}
            {shown.map((p, i) =>
              p.kind === "official" ? (
                <circle key={p.logId} cx={x(i)} cy={y(p.score)} r={dotR + 0.5} className="tl-dot official" />
              ) : null,
            )}

            <path d={linePath} className="tl-line" />

            {last && last.rolling !== null && (
              <>
                <circle cx={x(lastIdx)} cy={y(last.rolling)} r={4.5} className="tl-end" />
                <text x={x(lastIdx) + 9} y={y(last.rolling)} className="tl-endlabel" dominantBaseline="middle">
                  {rating(last.rolling)}
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
                  <strong>{rating(h.rolling)}</strong>
                  <span className="muted">{tx("{rollingWindow}-game avg", { rollingWindow: rollingWindow })}</span>
                </div>
              )}
              <div className="tip-row">
                <svg width="14" height="8" aria-hidden>
                  <circle cx="7" cy="4" r="3.5" className="tl-dot solid" />
                </svg>
                <strong>{rating(h.score)}</strong>
                <span className="muted">{tr("this game")}</span>
              </div>
              <div className="tip-meta">
                {formatDate(h.playedAt, true)} · {splitMap(h.map).name ?? tr("unknown map")}
                {h.result && <span className={`result-${h.result}`}> · {h.result}</span>}
                {h.kind && <span> · {h.kind}</span>}
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
  const sort = useSort(
    "trend-table",
    points,
    { date: (p) => p.playedAt, map: (p) => splitMap(p.map).name ?? null, result: (p) => p.result ?? null, rating: (p) => p.score, avg: (p) => p.rolling },
    { key: "date", dir: "desc" },
  );
  const rows = sort.sorted;
  return (
    <div className="table-wrap trend-table">
      <table className="match-table">
        <thead>
          <tr>
            <SortTh k="date" s={sort}>{tr("Date")}</SortTh>
            <SortTh k="map" s={sort}>{tr("Map")}</SortTh>
            <SortTh k="result" s={sort}>{tr("Result")}</SortTh>
            <SortTh k="rating" s={sort} className="num" tip="rating">{tr("Rating")}</SortTh>
            <SortTh k="avg" s={sort} className="num" tip={tip(tr("10-game avg"), tr("The average of this game and the nine before it."), tr("The line on the chart: it smooths one night's luck out of the trend."), "this app")}>{tr("10-game avg")}</SortTh>
          </tr>
        </thead>
        <tbody>
          {rows.map((p) => (
            <tr key={p.logId} className="clickable" tabIndex={0} onClick={() => onOpen(p.logId)} onKeyDown={(e) => e.key === "Enter" && onOpen(p.logId)}>
              <td className="muted nowrap">{formatDate(p.playedAt, true)}</td>
              <td className="nowrap">{splitMap(p.map).name ?? <span className="muted">{tr("unknown")}</span>}</td>
              <td>{p.result && <span className={`result result-${p.result}`}>{p.result}</span>}</td>
              <td className="num">{rating(p.score)}</td>
              <td className="num">{rating(p.rolling)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
