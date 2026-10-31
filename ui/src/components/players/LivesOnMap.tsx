import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ApiError } from "../ApiError";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { type LifeOnMap } from "../../api/types";
import { capitalize, formatDate } from "../../lib/format";
import { k, t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";
import { MassImportButton } from "./MassImport";
import { Loading } from "../Spinner";
import { CtlGroup, Icon, Seg } from "../analysis/MapChrome";

/**
 * A player's lives on one map (Q57, Emiel): pick a map and a class, and
 * every route the demos on this machine followed them through is drawn on
 * it -- every Spy cross on Product, split by playing RED and BLU. Only the
 * matches with an STV, or with their own recording, have routes; the
 * counts say how many that is.
 */

type Side = "both" | "Red" | "Blue";
type Ending = "any" | "died" | "survived";
type View = "auto" | "routes" | "density" | "opening";
type Situation = "capped" | "afterCap" | "midFight";

/** The situation filters (Q67): `[key, label, what it means]`. */
const SITUATIONS: Array<[Situation, string, string]> = [
  ["capped", k("With a cap"), k("Their team took a point during the life")],
  ["afterCap", k("After the other team capped"), k("The life began within 20 s of the other team taking a point in the same round")],
  ["midFight", k("Spawned mid-fight"), k("Others were killing each other both in the 5 s before the life began and the 10 s after")],
];

/** Each situation's icon. */
const SITUATION_ICON: Record<Situation, "cap" | "afterCap" | "swords"> = { capped: "cap", afterCap: "afterCap", midFight: "swords" };

/** "Auto" shows where they usually go from this many lives up (Q65):
 *  past it, separate lines are a solid smear. */
const DENSITY_FROM = 300;

/** The density grid's cells across the frame. */
const CELLS = 140;

/** The round-openings view's hexes across the frame. */
const HEXES = 70;

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
  const [ending, setEnding] = useState<Ending>("any");
  // Q67: situations, each narrowing the lives further.
  const [situations, setSituations] = useState<Set<Situation>>(new Set());
  // Q66: only the most recent N matches (of this class and side); null is all.
  const [lastN, setLastN] = useState<number | null>(null);
  // Q65: every route, or where they usually go; "auto" picks by how many.
  const [view, setView] = useState<View>("auto");
  const [hover, setHover] = useState<LifeOnMap | null>(null);

  const all = useMemo(() => livesQ.data?.lives ?? [], [livesQ.data]);
  // The classes they played here, most lives first; the side filter applies.
  const classes = useMemo(() => {
    const n = new Map<string, number>();
    for (const l of all) if (l.class && (side === "both" || l.team === side)) n.set(l.class, (n.get(l.class) ?? 0) + 1);
    return [...n.entries()].sort((a, b) => b[1] - a[1]);
  }, [all, side]);
  // The class and side asked for, then the most recent N of their matches.
  const recent = useMemo(() => {
    const mine = all.filter((l) => (cls === null || l.class === cls) && (side === "both" || l.team === side));
    if (lastN === null) return { lives: mine, matches: new Set(mine.map((l) => l.logId)).size };
    const byDate = [...new Map(mine.map((l) => [l.logId, l.playedAt ?? 0])).entries()].sort((a, b) => b[1] - a[1]);
    const keep = new Set(byDate.slice(0, lastN).map(([id]) => id));
    return { lives: mine.filter((l) => keep.has(l.logId)), matches: keep.size };
  }, [all, cls, side, lastN]);
  // The same array until a filter changes (Q64): hovering re-renders this
  // component, and a new array would redraw every route on each move.
  const shown = useMemo(
    () =>
      recent.lives.filter(
        (l) =>
          (ending === "any" || (ending === "died") === l.died) &&
          (!situations.has("capped") || l.capped) &&
          (!situations.has("afterCap") || l.afterEnemyCap) &&
          (!situations.has("midFight") || l.midFight),
      ),
    [recent, ending, situations],
  );
  // The round openings (PGL's heatmaps): only the lives that began with a
  // round, cut at its first kill or 40 s in. The end and situation filters
  // are about the rest of the life, so they do not apply.
  const openings = useMemo(
    () => recent.lives.filter((l) => l.opening > 0).map((l) => ({ ...l, points: l.points.slice(0, l.opening) })),
    [recent],
  );
  const sides = { Red: all.filter((l) => (cls === null || l.class === cls) && l.team === "Red").length, Blue: all.filter((l) => (cls === null || l.class === cls) && l.team === "Blue").length };
  const mode: Mode = view === "auto" ? (shown.length > DENSITY_FROM ? "density" : "routes") : view;
  const toggle = (s: Situation) =>
    setSituations((was) => {
      const next = new Set(was);
      if (next.has(s)) next.delete(s);
      else next.add(s);
      return next;
    });

  if (mapsQ.isPending) return <Loading>{t("Loading…")}</Loading>;
  if (mapsQ.isError) return <ApiError error={mapsQ.error} />;
  if (maps.length === 0) {
    return (
      <div className="lm-empty">
        <p className="hint">
          {tx("No demo on this machine followed {0}. Routes come from SourceTV demos (download one from a match's Demo linking panel) and from their own recordings.", { "0": <strong>{name}</strong> })}
        </p>
        <MassImportButton accountId={accountId} name={name} map={null} cls={null} />
      </div>
    );
  }

  const v = livesQ.data;
  return (
    <div className="lm">
      {/* Every control in a labelled group, each option with its icon
          (Flashy: "add some icons to differentiate what buttons do what"). */}
      <div className="ctl-bar">
        <CtlGroup label={t("Map")}>
          <select
            className="lm-select"
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
        </CtlGroup>
        <CtlGroup label={t("Side")}>
          <Seg
            label={t("Side")}
            value={side}
            onChange={setSide}
            options={[
              { v: "both", label: <><Icon name="both" size={14} /> {t("Both")}</> },
              { v: "Red", label: <><i className="lm-dot red" /> RED <span className="lm-n">{sides.Red}</span></> },
              { v: "Blue", label: <><i className="lm-dot blu" /> BLU <span className="lm-n">{sides.Blue}</span></> },
            ]}
          />
        </CtlGroup>
        <CtlGroup label={t("Matches")}>
          <label className="lm-lastn" title={t("Only their most recent matches with this class and side; empty for all")}>
            <Icon name="history" size={14} />
            <span>{t("Last")}</span>
            <input
              type="number"
              min={1}
              step={1}
              inputMode="numeric"
              placeholder={t("all")}
              value={lastN ?? ""}
              onChange={(e) => {
                const n = Math.floor(Number(e.target.value));
                setLastN(e.target.value === "" || !Number.isFinite(n) || n < 1 ? null : n);
              }}
            />
            {lastN !== null ? (
              <button className="lm-clear" onClick={() => setLastN(null)} title={t("Every match")} aria-label={t("Every match")}>
                <Icon name="close" size={12} />
              </button>
            ) : (
              <span className="muted">{t("matches")}</span>
            )}
          </label>
        </CtlGroup>
        <CtlGroup label={t("View")}>
          <Seg
            label={t("View")}
            value={view}
            onChange={setView}
            options={[
              { v: "auto", label: <><Icon name="auto" size={14} /> {t("Auto")}</>, title: t("Most used from {0} lives, every route below that", { "0": DENSITY_FROM }) },
              { v: "routes", label: <><Icon name="routes" size={14} /> {t("Every route")}</>, title: t("One line per life") },
              { v: "density", label: <><Icon name="grid" size={14} /> {t("Most used")}</>, title: t("How many lives passed each spot: the busier, the stronger") },
              { v: "opening", label: <><Icon name="hex" size={14} /> {t("Round openings")}</>, title: t("Where they go in the first 40 seconds of each round, or until its first kill") },
            ]}
          />
        </CtlGroup>
        {/* Q69: far more than this machine recorded, from demos.tf. */}
        <span className="lm-head-actions">
          <MassImportButton accountId={accountId} name={name} map={map} cls={cls} />
        </span>
      </div>

      <div className="ctl-bar">
        {mode !== "opening" && (
          <>
            <CtlGroup label={t("How the life ended")}>
              <Seg
                label={t("How the life ended")}
                value={ending}
                onChange={setEnding}
                options={[
                  { v: "any", label: <><Icon name="any" size={14} /> {t("Any end")}</> },
                  { v: "died", label: <><Icon name="skull" size={14} /> {t("Ended in a death")}</> },
                  { v: "survived", label: <><Icon name="flag" size={14} /> {t("Lived to the round's end")}</> },
                ]}
              />
            </CtlGroup>
            <CtlGroup label={t("Situation")}>
              <div className="lm-toggles" role="group" aria-label={t("Situation")}>
                {SITUATIONS.map(([s, label, hint]) => (
                  <button key={s} className={situations.has(s) ? "lm-toggle on" : "lm-toggle"} aria-pressed={situations.has(s)} onClick={() => toggle(s)} title={t(hint)}>
                    <Icon name={SITUATION_ICON[s]} size={14} />
                    {t(label)}
                    <span className="lm-check" aria-hidden>
                      {situations.has(s) && <Icon name="check" size={12} />}
                    </span>
                  </button>
                ))}
              </div>
            </CtlGroup>
          </>
        )}
        {classes.length > 0 && (
          <CtlGroup label={t("Class")}>
            <div className="class-filter" role="tablist" aria-label={t("Class")}>
              <button role="tab" aria-selected={cls === null} className={cls === null ? "cf active" : "cf"} onClick={() => setCls(null)}>
                {t("All")}
              </button>
              {classes.map(([c, n]) => (
                <button key={c} role="tab" aria-selected={cls === c} className={cls === c ? "cf active" : "cf"} onClick={() => setCls(cls === c ? null : c)} title={`${capitalize(c)}: ${n}`}>
                  <ClassIcon cls={c} size={20} />
                  <span className="cf-n">{n}</span>
                </button>
              ))}
            </div>
          </CtlGroup>
        )}
      </div>

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
          {lastN !== null && <> {tx("Only the last {0} of them with this class and side.", { "0": recent.matches })}</>}
        </p>
      )}
      {livesQ.isPending && <Loading>{t("Reading the routes…")}</Loading>}
      {livesQ.isError && <ApiError error={livesQ.error} />}

      {v && <Drawing lives={mode === "opening" ? openings : shown} overview={overviewQ.data ?? null} hover={hover} onHover={setHover} mode={mode} />}

      {hover && mode === "routes" && (
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
  mode,
}: {
  lives: LifeOnMap[];
  overview: { minX: number; maxY: number; size: number; aspect: number; image: string } | null;
  hover: LifeOnMap | null;
  onHover: (l: LifeOnMap | null) => void;
  mode: Mode;
}) {
  // Q65: the cell under the mouse in the density view, and its counts.
  const [cellHover, setCellHover] = useState<{ red: number; blue: number; other: number } | null>(null);
  // The round-openings view's hex under the mouse.
  const [hexHover, setHexHover] = useState<{ red: number; blue: number } | null>(null);
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

  // Q65: how many lives passed through each cell, per side. Each life counts
  // once per cell, so a camper does not outweigh a hundred passers-by.
  const density = useMemo(() => (frame && mode === "density" ? densityOf(lives, frame) : null), [lives, frame, mode]);

  // The round openings, in hexes.
  const hexes = useMemo(() => (frame && mode === "opening" ? hexesOf(lives, frame) : null), [lives, frame, mode]);

  // Each hex its side's colour, stronger the more rounds went through it;
  // where both sides went, outlined in white. Each side's main position is
  // ringed.
  useEffect(() => {
    if (mode !== "opening") return;
    const ctx = prepare(base.current, width, height);
    if (!ctx || !proj || !hexes || !frame) return;
    const col = colours();
    const r = hexes.size * proj.scale;
    const at = (q: number, rr: number): [number, number] => {
      const [x, y] = hexCentre(q, rr, hexes.size);
      return [proj.ox + x * proj.scale, proj.oy + y * proj.scale];
    };
    const hexPath = (cx: number, cy: number, radius: number) => {
      ctx.beginPath();
      for (let i = 0; i < 6; i++) {
        const a = (Math.PI / 3) * i + Math.PI / 6;
        const x = cx + radius * Math.cos(a);
        const y = cy + radius * Math.sin(a);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.closePath();
    };
    for (const [key, n] of hexes.cells) {
      const [q, rr] = key.split(",").map(Number);
      const [cx, cy] = at(q, rr);
      for (const [team, count, max] of [["Red", n.red, hexes.maxRed], ["Blue", n.blue, hexes.maxBlue]] as const) {
        if (count === 0) continue;
        ctx.globalAlpha = (0.2 + 0.75 * Math.sqrt(count / max)) * (n.red > 0 && n.blue > 0 ? 0.7 : 1);
        ctx.fillStyle = col[team];
        hexPath(cx, cy, r * 0.9);
        ctx.fill();
      }
      if (n.red > 0 && n.blue > 0) {
        ctx.globalAlpha = 0.85;
        ctx.strokeStyle = "#fff";
        ctx.lineWidth = 1.2;
        hexPath(cx, cy, r * 0.82);
        ctx.stroke();
      }
    }
    ctx.globalAlpha = 1;
    for (const [team, main] of [["Red", hexes.mainRed], ["Blue", hexes.mainBlue]] as const) {
      if (!main) continue;
      const [cx, cy] = at(main.q, main.r);
      ctx.lineWidth = 2.5;
      ctx.strokeStyle = "#fff";
      hexPath(cx, cy, r * 1.15);
      ctx.stroke();
      ctx.strokeStyle = col[team];
      ctx.lineWidth = 1.5;
      hexPath(cx, cy, r * 1.45);
      ctx.stroke();
    }
  }, [mode, hexes, proj, frame, width, height]);

  // The density view: each cell its side's colour, stronger the more lives
  // passed through it. Square root, so the quieter lanes still show.
  useEffect(() => {
    if (mode !== "density") return;
    const ctx = prepare(base.current, width, height);
    if (!ctx || !proj || !density || !frame) return;
    const col = colours();
    const size = density.cell * proj.scale;
    for (const [team, counts] of [["Red", density.red], ["Blue", density.blue], ["none", density.other]] as const) {
      const max = counts.reduce((m, n) => Math.max(m, n), 0);
      if (max === 0) continue;
      ctx.fillStyle = col[team];
      for (let i = 0; i < counts.length; i++) {
        const n = counts[i];
        if (n === 0) continue;
        const cx = i % density.cols;
        const cy = Math.floor(i / density.cols);
        ctx.globalAlpha = 0.12 + 0.83 * Math.sqrt(n / max);
        ctx.fillRect(proj.ox + cx * size, proj.oy + cy * size, size + 0.5, size + 0.5);
      }
    }
  }, [mode, density, proj, frame, width, height]);

  // All the routes. Each life is stroked on its own, so where many overlap
  // the line builds up and the usual lanes stand out (one path per side
  // would paint every overlap once, flat).
  useEffect(() => {
    if (mode !== "routes") return;
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
  }, [mode, lives, proj, opacity, width, height]);

  // The hovered route on the canvas above, at full strength.
  useEffect(() => {
    const ctx = prepare(top.current, width, height);
    if (!ctx || !proj || !hover || mode !== "routes") return;
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
  }, [hover, proj, width, height, mode]);

  if (!frame) return <p className="hint">{t("Nothing to draw with these filters.")}</p>;

  // The nearest point under the mouse, within HOVER_PX, picks the route; in
  // the density view, the cell under it says how many lives went through.
  function onMove(e: React.MouseEvent<HTMLCanvasElement>) {
    if (!proj || !grid || !frame) return;
    const r = e.currentTarget.getBoundingClientRect();
    const gx = frame.minX + (e.clientX - r.left - proj.ox) / proj.scale;
    const gy = frame.maxY - (e.clientY - r.top - proj.oy) / proj.scale;
    if (mode === "opening") {
      if (!hexes) return;
      const [q, r] = hexAt(gx - frame.minX, frame.maxY - gy, hexes.size);
      const n = hexes.cells.get(`${q},${r}`);
      setHexHover(n ? { red: n.red, blue: n.blue } : null);
      return;
    }
    if (mode === "density") {
      if (!density) return;
      const cx = Math.floor((gx - frame.minX) / density.cell);
      const cy = Math.floor((frame.maxY - gy) / density.cell);
      const i = cy * density.cols + cx;
      const inside = cx >= 0 && cy >= 0 && cx < density.cols && cy < density.rows;
      const at = inside ? { red: density.red[i], blue: density.blue[i], other: density.other[i] } : null;
      setCellHover(at && at.red + at.blue + at.other > 0 ? at : null);
      return;
    }
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
    <>
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
          onMouseLeave={() => {
            onHover(null);
            setCellHover(null);
            setHexHover(null);
          }}
        />
        {mode === "density" && cellHover && density && (
          <p className="lm-cell">
            {tx("{0} of the {1} lives passed here ({2}%)", {
              "0": cellHover.red + cellHover.blue + cellHover.other,
              "1": lives.length,
              "2": Math.round(((cellHover.red + cellHover.blue + cellHover.other) / Math.max(1, lives.length)) * 100),
            })}
            {cellHover.red > 0 && cellHover.blue > 0 && <> · <span className="team-red">RED {cellHover.red}</span> · <span className="team-blue">BLU {cellHover.blue}</span></>}
          </p>
        )}
        {mode === "opening" && hexHover && hexes && (
          <p className="lm-cell">
            {hexHover.red > 0 && <span className="team-red">{tx("RED: {0} of {1} rounds", { "0": hexHover.red, "1": hexes.rounds.Red })}</span>}
            {hexHover.red > 0 && hexHover.blue > 0 && " · "}
            {hexHover.blue > 0 && <span className="team-blue">{tx("BLU: {0} of {1} rounds", { "0": hexHover.blue, "1": hexes.rounds.Blue })}</span>}
          </p>
        )}
      </div>
      {mode === "opening" && hexes && <OpeningCards hexes={hexes} />}
    </>
  );
}

/** Under the round-openings view: each side's rounds and main position, the
 *  scale, and what the view counts (as PGL's heatmaps put it). */
function OpeningCards({ hexes }: { hexes: Hexes }) {
  const card = (team: "Red" | "Blue") => {
    const rounds = hexes.rounds[team];
    const main = team === "Red" ? hexes.mainRed : hexes.mainBlue;
    const pct = main && rounds > 0 ? Math.round((main.rounds / rounds) * 100) : null;
    return (
      <div className={`lm-op-card ${team === "Red" ? "red" : "blu"}`}>
        <div className="lm-op-head">
          <span className="lm-op-side">{team === "Red" ? t("RED side") : t("BLU side")}</span>
          <span className="lm-op-rounds">
            <strong>{rounds}</strong> {t("rounds")}
          </span>
        </div>
        <div className="lm-op-main">
          <span>{t("Main position")}</span>
          <span className="lm-op-pct">
            <span>{pct === null ? "–" : <><strong>{pct}</strong>%</>}</span>
            <small>{t("of rounds")}</small>
          </span>
        </div>
      </div>
    );
  };
  return (
    <div className="lm-op">
      {card("Red")}
      <div className="lm-op-mid">
        <div className="lm-op-scale">
          <span>{t("Rare")}</span>
          {[0.25, 0.45, 0.65, 0.85, 1].map((a) => (
            <i key={a} style={{ opacity: a }} />
          ))}
          <span>{t("Often")}</span>
        </div>
        <div className="lm-op-scale">
          <i className="both" />
          <span>{t("Both sides")}</span>
        </div>
        <p className="lm-op-note">{t("First 40 seconds of each round, or until the round's first kill")}</p>
      </div>
      {card("Blue")}
    </div>
  );
}

type Mode = "routes" | "density" | "opening";

/** The hex a point falls in (pointy-top, `size` the corner radius), from
 *  the frame's top-left in game units, y down. */
function hexAt(x: number, y: number, size: number): [number, number] {
  const fq = ((Math.sqrt(3) / 3) * x - y / 3) / size;
  const fr = ((2 / 3) * y) / size;
  const fs = -fq - fr;
  let q = Math.round(fq);
  let r = Math.round(fr);
  const s = Math.round(fs);
  const [dq, dr, ds] = [Math.abs(q - fq), Math.abs(r - fr), Math.abs(s - fs)];
  if (dq > dr && dq > ds) q = -r - s;
  else if (dr > ds) r = -q - s;
  return [q, r];
}

/** A hex's centre, from the frame's top-left in game units, y down. */
function hexCentre(q: number, r: number, size: number): [number, number] {
  return [size * Math.sqrt(3) * (q + r / 2), size * 1.5 * r];
}

/** The six neighbours' offsets. */
const AROUND: Array<[number, number]> = [[1, 0], [-1, 0], [0, 1], [0, -1], [1, -1], [-1, 1]];

type Hexes = ReturnType<typeof hexesOf>;

/** Rounds through each hex, per side, each counted once per hex it crossed;
 *  and each side's main position: the hex whose neighbourhood (it and the
 *  six around it) the most rounds went through, so a spot rather than one
 *  tile of it. Not the spawn room, which every round starts in. */
function hexesOf(lives: LifeOnMap[], frame: { minX: number; maxY: number; w: number; h: number }) {
  const size = Math.max(frame.w, frame.h) / HEXES / Math.sqrt(3);
  const cells = new Map<string, { red: number; blue: number }>();
  const near: Record<"Red" | "Blue", Map<string, number>> = { Red: new Map(), Blue: new Map() };
  // Where each round began, so the spawn room is not taken for a position.
  const spawn: Record<"Red" | "Blue", Set<string>> = { Red: new Set(), Blue: new Set() };
  const rounds = { Red: 0, Blue: 0 };
  const seen = new Set<string>();
  const around = new Set<string>();
  for (const l of lives) {
    if (l.team !== "Red" && l.team !== "Blue") continue;
    rounds[l.team] += 1;
    seen.clear();
    around.clear();
    let prev: [number, number] | null = null;
    for (const p of l.points) {
      const from: [number, number] = prev !== null && Math.hypot(p[0] - prev[0], p[1] - prev[1]) <= JUMP ? prev : p;
      const steps = Math.max(1, Math.ceil(Math.hypot(p[0] - from[0], p[1] - from[1]) / (size / 2)));
      for (let s = 0; s <= steps; s++) {
        const x = from[0] + ((p[0] - from[0]) * s) / steps;
        const y = from[1] + ((p[1] - from[1]) * s) / steps;
        const [q, r] = hexAt(x - frame.minX, frame.maxY - y, size);
        seen.add(`${q},${r}`);
      }
      prev = p;
    }
    const first = l.points[0];
    if (first) {
      const [q, r] = hexAt(first[0] - frame.minX, frame.maxY - first[1], size);
      for (let dq = -2; dq <= 2; dq++) for (let dr = -2; dr <= 2; dr++) spawn[l.team].add(`${q + dq},${r + dr}`);
    }
    for (const key of seen) {
      const c = cells.get(key) ?? { red: 0, blue: 0 };
      if (l.team === "Red") c.red += 1;
      else c.blue += 1;
      cells.set(key, c);
      const [q, r] = key.split(",").map(Number);
      around.add(key);
      for (const [dq, dr] of AROUND) around.add(`${q + dq},${r + dr}`);
    }
    for (const key of around) near[l.team].set(key, (near[l.team].get(key) ?? 0) + 1);
  }
  const main = (team: "Red" | "Blue") => {
    let best: { q: number; r: number; rounds: number } | null = null;
    for (const [key, n] of near[team]) {
      if (spawn[team].has(key)) continue;
      const own = cells.get(key);
      if (!own || (team === "Red" ? own.red : own.blue) === 0) continue;
      if (!best || n > best.rounds) {
        const [q, r] = key.split(",").map(Number);
        best = { q, r, rounds: n };
      }
    }
    return best;
  };
  let maxRed = 0;
  let maxBlue = 0;
  for (const c of cells.values()) {
    maxRed = Math.max(maxRed, c.red);
    maxBlue = Math.max(maxBlue, c.blue);
  }
  return { size, cells, rounds, maxRed, maxBlue, mainRed: main("Red"), mainBlue: main("Blue") };
}

/** Lives through each cell of a `CELLS`-wide grid over the frame, per side:
 *  each life counted once per cell it crossed (Q65). Steps between points are
 *  filled in so a fast move does not skip cells; a jump (a teleporter, a
 *  respawn) is not. */
function densityOf(lives: LifeOnMap[], frame: { minX: number; maxY: number; w: number; h: number }) {
  const cell = Math.max(frame.w, frame.h) / CELLS;
  const cols = Math.ceil(frame.w / cell);
  const rows = Math.ceil(frame.h / cell);
  const red = new Uint32Array(cols * rows);
  const blue = new Uint32Array(cols * rows);
  const other = new Uint32Array(cols * rows);
  const seen = new Set<number>();
  for (const l of lives) {
    const into = l.team === "Red" ? red : l.team === "Blue" ? blue : other;
    seen.clear();
    let prev: [number, number] | null = null;
    for (const p of l.points) {
      const from: [number, number] = prev !== null && Math.hypot(p[0] - prev[0], p[1] - prev[1]) <= JUMP ? prev : p;
      const steps = Math.max(1, Math.ceil(Math.hypot(p[0] - from[0], p[1] - from[1]) / (cell / 2)));
      for (let s = 0; s <= steps; s++) {
        const x = from[0] + ((p[0] - from[0]) * s) / steps;
        const y = from[1] + ((p[1] - from[1]) * s) / steps;
        const cx = Math.floor((x - frame.minX) / cell);
        const cy = Math.floor((frame.maxY - y) / cell);
        if (cx < 0 || cy < 0 || cx >= cols || cy >= rows) continue;
        seen.add(cy * cols + cx);
      }
      prev = p;
    }
    for (const i of seen) into[i] += 1;
  }
  return { cell, cols, rows, red, blue, other };
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
