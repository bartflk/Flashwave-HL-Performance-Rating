import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type LifeOnMap } from "../../api/types";
import { capitalize, formatDate } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";

/**
 * A player's lives on one map (Q57, Emiel): pick a map and a class, and
 * every route the demos on this machine followed them through is drawn on
 * it -- every Spy cross on Product, split by playing RED and BLU. Only the
 * matches with an STV, or with their own recording, have routes; the
 * counts say how many that is.
 */

type Side = "both" | "Red" | "Blue";

/** A step longer than this went through a teleporter or a respawn: the line
 *  breaks rather than crossing the map. Points are about four a second, so
 *  this is well past running speed. */
const JUMP = 900;

export function LivesOnMapTab({ accountId, name }: { accountId: number; name: string }) {
  const mapsQ = useQuery({ queryKey: ["lives_maps", accountId], queryFn: () => api.livesMaps(accountId), staleTime: 5 * 60_000 });
  const maps = mapsQ.data ?? [];
  const [picked, setPicked] = useState<string | null>(null);
  const map = picked ?? maps[0]?.map ?? null;
  const fullName = maps.find((m) => m.map === map)?.name ?? map;

  const livesQ = useQuery({
    queryKey: ["lives_on_map", accountId, map],
    queryFn: () => api.livesOnMap(accountId, map!),
    enabled: map !== null,
    staleTime: 5 * 60_000,
  });
  const overviewQ = useQuery({
    queryKey: ["overview", fullName],
    queryFn: () => api.getMapOverview(fullName ?? ""),
    enabled: fullName !== null,
    staleTime: Infinity,
  });

  const [cls, setCls] = useState<string | null>(null);
  const [side, setSide] = useState<Side>("both");
  const [deathsOnly, setDeathsOnly] = useState(false);
  const [hover, setHover] = useState<LifeOnMap | null>(null);

  const all = useMemo(() => livesQ.data?.lives ?? [], [livesQ.data]);
  // The classes they played here, most lives first; the side filter applies.
  const classes = useMemo(() => {
    const n = new Map<string, number>();
    for (const l of all) if (l.class && (side === "both" || l.team === side)) n.set(l.class, (n.get(l.class) ?? 0) + 1);
    return [...n.entries()].sort((a, b) => b[1] - a[1]);
  }, [all, side]);
  // The same array until a filter changes (Q64): hovering re-renders this
  // component, and a new array would redraw every route on each move.
  const shown = useMemo(
    () => all.filter((l) => (cls === null || l.class === cls) && (side === "both" || l.team === side) && (!deathsOnly || l.died)),
    [all, cls, side, deathsOnly],
  );
  const sides = { Red: all.filter((l) => (cls === null || l.class === cls) && l.team === "Red").length, Blue: all.filter((l) => (cls === null || l.class === cls) && l.team === "Blue").length };

  if (mapsQ.isPending) return <p className="hint">{t("Loading…")}</p>;
  if (mapsQ.isError) return <p className="error">{errorMessage(mapsQ.error)}</p>;
  if (maps.length === 0) {
    return (
      <p className="hint lm-empty">
        {tx("No demo on this machine followed {0}. Routes come from SourceTV demos (download one from a match's Demo linking panel) and from their own recordings.", { "0": <strong>{name}</strong> })}
      </p>
    );
  }

  const v = livesQ.data;
  return (
    <div className="lm">
      <div className="lm-controls">
        <label className="an-field">
          <span className="an-label">{t("Map")}</span>
          <select
            value={map ?? ""}
            onChange={(e) => {
              setPicked(e.target.value);
              setCls(null);
              setHover(null);
            }}
          >
            {maps.map((m) => (
              <option key={m.map} value={m.map}>
                {capitalize(m.map)} ({tx("{0} match{1}", { "0": m.matches, "1": m.matches === 1 ? "" : "es" })})
              </option>
            ))}
          </select>
        </label>
        <div className="segmented" role="tablist" aria-label={t("Side")}>
          {(["both", "Red", "Blue"] as Side[]).map((s) => (
            <button key={s} role="tab" aria-selected={side === s} className={side === s ? "seg active" : "seg"} onClick={() => setSide(s)}>
              {s === "both" ? t("Both sides") : s === "Red" ? tx("RED ({0})", { "0": sides.Red }) : tx("BLU ({0})", { "0": sides.Blue })}
            </button>
          ))}
        </div>
        <label className="lm-check">
          <input type="checkbox" checked={deathsOnly} onChange={(e) => setDeathsOnly(e.target.checked)} /> {t("Only lives that ended in a death")}
        </label>
      </div>

      {classes.length > 0 && (
        <div className="class-filter" role="tablist" aria-label={t("Class")}>
          <button role="tab" aria-selected={cls === null} className={cls === null ? "cf active" : "cf"} onClick={() => setCls(null)}>
            {t("All classes")}
          </button>
          {classes.map(([c, n]) => (
            <button key={c} role="tab" aria-selected={cls === c} className={cls === c ? "cf active" : "cf"} onClick={() => setCls(cls === c ? null : c)} title={`${capitalize(c)}: ${n}`}>
              <ClassIcon cls={c} size={20} />
              <span className="cf-n">{n}</span>
            </button>
          ))}
        </div>
      )}

      {v && (
        <p className="hint lm-count">
          {tx("{0} lives drawn, from {1} match{2} on {3}: {4} with a SourceTV demo, {5} from their own recordings. Matches without either have no routes.", {
            "0": shown.length,
            "1": v.matches,
            "2": v.matches === 1 ? "" : "es",
            "3": capitalize(v.map),
            "4": v.stvMatches,
            "5": v.matches - v.stvMatches,
          })}
        </p>
      )}
      {livesQ.isPending && <p className="hint">{t("Reading the routes…")}</p>}
      {livesQ.isError && <p className="error">{errorMessage(livesQ.error)}</p>}

      {v && <Drawing lives={shown} overview={overviewQ.data ?? null} hover={hover} onHover={setHover} />}

      {hover && (
        <p className="lm-hover">
          <span className={`team-${(hover.team ?? "").toLowerCase()}`}>{hover.team === "Red" ? "RED" : hover.team === "Blue" ? "BLU" : "?"}</span>{" "}
          {hover.class ? capitalize(hover.class) : t("unknown class")} · {formatDate(hover.playedAt)}
          {hover.roundNum !== null && <> · {tx("round {0}", { "0": hover.roundNum })}</>} · {tx("{0} s", { "0": Math.round(hover.seconds) })}
          {hover.died ? <> · {t("died")}</> : null} · {hover.stv ? t("STV") : t("POV")}
        </p>
      )}
    </div>
  );
}

/** The routes over the map's top-down picture, or over their own extent. */
function Drawing({
  lives,
  overview,
  hover,
  onHover,
}: {
  lives: LifeOnMap[];
  overview: { minX: number; maxY: number; size: number; aspect: number; image: string } | null;
  hover: LifeOnMap | null;
  onHover: (l: LifeOnMap | null) => void;
}) {
  // The frame in game units: the picture's, else the routes' own bounds.
  const frame = useMemo(() => {
    if (overview) return { minX: overview.minX, maxY: overview.maxY, w: overview.size, h: overview.size * overview.aspect };
    let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
    for (const l of lives)
      for (const [x, y] of l.points) {
        x0 = Math.min(x0, x);
        x1 = Math.max(x1, x);
        y0 = Math.min(y0, y);
        y1 = Math.max(y1, y);
      }
    if (!Number.isFinite(x0)) return null;
    const pad = 200;
    const size = Math.max(x1 - x0, y1 - y0) + pad * 2;
    return { minX: x0 - pad, maxY: y1 + pad, w: size, h: size };
  }, [lives, overview]);

  const box = useRef<HTMLDivElement>(null);
  const base = useRef<HTMLCanvasElement>(null);
  const top = useRef<HTMLCanvasElement>(null);
  // The stage's width in CSS pixels; the height follows the frame's aspect.
  const [width, setWidth] = useState(0);
  const framed = frame !== null;
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    // Measured at once, not only when the observer first reports: a window
    // that is not painting (minimised, or behind another) delays that.
    setWidth(el.clientWidth);
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, [framed]);
  const height = frame && width > 0 ? Math.min((width * frame.h) / frame.w, window.innerHeight * 0.78) : 0;
  // Fainter the more there are, so a hundred routes still read as a pattern.
  const opacity = Math.max(0.12, Math.min(0.7, 9 / Math.sqrt(lives.length + 1)));

  // Game units to CSS pixels at the current size, the frame centred.
  const proj = useMemo(() => {
    if (!frame || width === 0 || height === 0) return null;
    const scale = Math.min(width / frame.w, height / frame.h);
    const ox = (width - frame.w * scale) / 2;
    const oy = (height - frame.h * scale) / 2;
    return { scale, ox, oy, px: (x: number) => ox + (x - frame.minX) * scale, py: (y: number) => oy + (frame.maxY - y) * scale };
  }, [frame, width, height]);

  // Every point by grid cell, for hover: a cell is 1% of the frame.
  const grid = useMemo(() => {
    if (!frame) return null;
    const cell = Math.max(frame.w, frame.h) / 100;
    const cells = new Map<string, Array<[number, number, number]>>();
    lives.forEach((l, i) => {
      for (const [x, y] of l.points) {
        const k = `${Math.floor(x / cell)},${Math.floor(y / cell)}`;
        let c = cells.get(k);
        if (!c) cells.set(k, (c = []));
        c.push([x, y, i]);
      }
    });
    return { cell, cells };
  }, [lives, frame]);

  // All the routes. Each life is stroked on its own, so where many overlap
  // the line builds up and the usual lanes stand out (one path per side
  // would paint every overlap once, flat).
  useEffect(() => {
    const ctx = prepare(base.current, width, height);
    if (!ctx || !proj) return;
    const col = colours();
    ctx.lineWidth = 1.6;
    for (const team of ["Red", "Blue", "none"] as const) {
      const mine = lives.filter((l) => (l.team ?? "none") === team);
      if (mine.length === 0) continue;
      ctx.globalAlpha = opacity;
      ctx.strokeStyle = col[team];
      for (const l of mine) {
        ctx.beginPath();
        trace(ctx, l, proj);
        ctx.stroke();
      }
      // Where a life ended in a death.
      ctx.globalAlpha = Math.min(1, opacity * 1.6);
      ctx.fillStyle = col[team];
      ctx.beginPath();
      for (const l of mine) {
        const last = l.points[l.points.length - 1];
        if (!l.died || !last) continue;
        ctx.moveTo(proj.px(last[0]) + 3, proj.py(last[1]));
        ctx.arc(proj.px(last[0]), proj.py(last[1]), 3, 0, Math.PI * 2);
      }
      ctx.fill();
    }
  }, [lives, proj, opacity, width, height]);

  // The hovered route on the canvas above, at full strength.
  useEffect(() => {
    const ctx = prepare(top.current, width, height);
    if (!ctx || !proj || !hover) return;
    const col = colours()[hover.team ?? "none"];
    ctx.strokeStyle = col;
    ctx.fillStyle = col;
    ctx.lineWidth = 3.5;
    ctx.beginPath();
    trace(ctx, hover, proj);
    ctx.stroke();
    const last = hover.points[hover.points.length - 1];
    if (hover.died && last) {
      ctx.beginPath();
      ctx.arc(proj.px(last[0]), proj.py(last[1]), 6, 0, Math.PI * 2);
      ctx.fill();
    }
  }, [hover, proj, width, height]);

  if (!frame) return <p className="hint">{t("Nothing to draw with these filters.")}</p>;

  // The nearest point under the mouse, within HOVER_PX, picks the route.
  function onMove(e: React.MouseEvent<HTMLCanvasElement>) {
    if (!proj || !grid || !frame) return;
    const r = e.currentTarget.getBoundingClientRect();
    const gx = frame.minX + (e.clientX - r.left - proj.ox) / proj.scale;
    const gy = frame.maxY - (e.clientY - r.top - proj.oy) / proj.scale;
    const cx = Math.floor(gx / grid.cell);
    const cy = Math.floor(gy / grid.cell);
    const reach = HOVER_PX / proj.scale;
    let best: [number, number] | null = null; // [distance, life]
    for (let dx = -1; dx <= 1; dx++)
      for (let dy = -1; dy <= 1; dy++)
        for (const [x, y, i] of grid.cells.get(`${cx + dx},${cy + dy}`) ?? []) {
          const d = Math.hypot(x - gx, y - gy);
          if (d <= reach && (best === null || d < best[0])) best = [d, i];
        }
    const l = best ? lives[best[1]] : null;
    if (l !== hover) onHover(l);
  }

  return (
    <div className="lm-stage" ref={box} style={{ height: height || undefined }}>
      {overview && proj && (
        <img
          className="lm-picture"
          src={overview.image}
          alt=""
          style={{ left: proj.ox, top: proj.oy, width: frame.w * proj.scale, height: frame.h * proj.scale }}
        />
      )}
      <canvas ref={base} className="lm-canvas" style={{ width, height }} aria-hidden />
      <canvas
        ref={top}
        className="lm-canvas lm-canvas-top"
        style={{ width, height }}
        role="img"
        aria-label={t("Routes on the map")}
        onMouseMove={onMove}
        onMouseLeave={() => onHover(null)}
      />
    </div>
  );
}

/** Hover picks the nearest point within this many CSS pixels. */
const HOVER_PX = 12;

type Proj = { px: (x: number) => number; py: (y: number) => number };

/** One life's route into the current path, broken where it jumped. */
function trace(ctx: CanvasRenderingContext2D, l: LifeOnMap, p: Proj) {
  let prev: [number, number] | null = null;
  for (const pt of l.points) {
    const far = prev !== null && Math.hypot(pt[0] - prev[0], pt[1] - prev[1]) > JUMP;
    if (prev === null || far) ctx.moveTo(p.px(pt[0]), p.py(pt[1]));
    else ctx.lineTo(p.px(pt[0]), p.py(pt[1]));
    prev = pt;
  }
}

/** A canvas sized for the screen's pixel density, cleared. */
function prepare(c: HTMLCanvasElement | null, width: number, height: number): CanvasRenderingContext2D | null {
  if (!c || width === 0 || height === 0) return null;
  const dpr = window.devicePixelRatio || 1;
  c.width = Math.round(width * dpr);
  c.height = Math.round(height * dpr);
  const ctx = c.getContext("2d");
  if (!ctx) return null;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  return ctx;
}

/** The team colours from the theme, so light and dark both hold. */
function colours(): Record<"Red" | "Blue" | "none", string> {
  const cs = getComputedStyle(document.documentElement);
  const v = (n: string) => cs.getPropertyValue(n).trim() || "#999";
  return { Red: v("--red"), Blue: v("--blu"), none: v("--muted") };
}
