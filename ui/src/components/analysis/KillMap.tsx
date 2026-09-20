import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { Analysis, KillView, MapView, Overview, PathRow, Vec3 } from "../../api/types";
import { capitalize, splitMap } from "../../lib/format";
import { DEATH, KILL, inSlice, jumpTo, playerMap, roundClock, type Slice } from "./common";
import { LifeList } from "./LifeList";

/**
 * Where the player's kills and deaths happened, top-down.
 *
 * The map is an overview image where one is saved locally (see
 * `overview.rs`), and otherwise drawn from data: every kill stored on this
 * map records where both players stood, and the density of those positions
 * traces the playable space (see `mapview.rs`). A kill is a blue dot where the victim
 * fell, a death an orange cross where the player fell; a hollow ring marks
 * where the shooter stood, joined by a line. Click any of them to copy the
 * demo tick.
 */

type Layer = "dots" | "paths" | "heat";
type HeatOf = "kills" | "deaths";
type Scope = "match" | "career";

/** The grid everything is drawn on, in game units. */
interface Frame {
  minX: number;
  maxY: number;
  cell: number;
  width: number;
  height: number;
}

interface Mark {
  k: KillView;
  kind: "kill" | "death";
  /** Where it happened (the victim) and where the shot came from. */
  at: [number, number];
  from: [number, number] | null;
}

const MAX_H = 640;
/** Room left for the controls and the note when the map fills the window. */
const FULL_CHROME = 150;

export function KillMap({ a, player, slice }: { a: Analysis; player: number; slice: Slice }) {
  // Full screen: the window's own where the webview allows it, and otherwise
  // the map fills the app over everything else. Escape leaves either way.
  const box = useRef<HTMLDivElement>(null);
  const [full, setFull] = useState(false);
  useEffect(() => {
    const onChange = () => {
      if (!document.fullscreenElement) setFull(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // Escape drops a picked route first, and only then leaves full screen:
      // losing the whole view to un-pick one line is a surprise.
      setFocus((f) => {
        if (f) return null;
        if (!document.fullscreenElement) setFull(false);
        return null;
      });
    };
    document.addEventListener("fullscreenchange", onChange);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("fullscreenchange", onChange);
      document.removeEventListener("keydown", onKey);
    };
  }, []);
  const toggleFull = () => {
    if (full) {
      setFull(false);
      if (document.fullscreenElement) void document.exitFullscreen();
      return;
    }
    setFull(true);
    void box.current?.requestFullscreen().catch(() => {
      // The webview refused it; the in-app overlay covers the window instead.
    });
  };
  // The slice's map: in a combined log, the map of the chosen segment.
  const mapName = slice.map;
  const mapQ = useQuery({
    queryKey: ["mapview", mapName],
    queryFn: () => api.getMapView(mapName ?? ""),
    enabled: mapName !== null,
    staleTime: 5 * 60_000,
  });
  const view = mapQ.data ?? null;
  const overviewQ = useQuery({
    queryKey: ["overview", mapName],
    queryFn: () => api.getMapOverview(mapName ?? ""),
    enabled: mapName !== null,
    staleTime: Infinity,
  });
  const overview = overviewQ.data ?? null;
  const players = useMemo(() => playerMap(a), [a]);
  const me = players.get(player);
  const enemies = a.players.filter((p) => me && p.team !== me.team);

  const [enemy, setEnemy] = useState<number | null>(null);
  const [showKills, setShowKills] = useState(true);
  const [showDeaths, setShowDeaths] = useState(true);
  const [layer, setLayer] = useState<Layer>("dots");
  // Routes are only fetched once the layer is asked for: most views never
  // want them, and a match is a few hundred kilobytes of points.
  const pathQ = useQuery({
    queryKey: ["paths", a.logId],
    queryFn: () => api.getPaths(a.logId),
    enabled: layer === "paths",
    staleTime: 5 * 60_000,
  });
  // Whose routes: the player the panel is on, or everyone the demo saw.
  const [pathWho, setPathWho] = useState<"player" | "all">("player");
  const [focus, setFocus] = useState<PathRow | null>(null);
  const [heatOf, setHeatOf] = useState<HeatOf>("deaths");
  const [scope, setScope] = useState<Scope>("match");
  const [asTable, setAsTable] = useState(false);
  const [hover, setHover] = useState<Mark | null>(null);

  // A new player invalidates the enemy filter; career scope is the owner's only.
  useEffect(() => {
    setEnemy(null);
    setHover(null);
    if (!players.get(player)?.isMe) setScope("match");
  }, [player, players]);

  const sliceKills = useMemo(() => a.kills.filter((k) => inSlice(k.roundNum, slice)), [a.kills, slice]);
  const frame: Frame | null = useMemo(() => (view ? view : frameFromKills(sliceKills)), [view, sliceKills]);
  const heat = useMemo(
    () =>
      layer === "heat" && frame
        ? heatGrid(frame, view, sliceKills, player, heatOf, scope, enemy)
        : null,
    [layer, frame, view, sliceKills, player, heatOf, scope, enemy],
  );

  // Routes in view: this round or map, and whose the layer is set to.
  const routes = useMemo(
    () =>
      (pathQ.data ?? []).filter(
        (r) =>
          (slice.rounds === null || (r.roundNum !== null && slice.rounds.has(r.roundNum))) &&
          (pathWho === "all" || r.accountId === player),
      ),
    [pathQ.data, slice.rounds, pathWho, player],
  );
  // The routes are stored in demo ticks; TF2 servers run at 66.67 a second,
  // which is close enough to turn a route's length into seconds.
  const tickRate = 66.67;

  const kills = sliceKills;
  const marks: Mark[] = [];
  for (const k of kills) {
    if (!k.victimPos) continue;
    const isKill = k.killer === player && k.victim !== player && (enemy === null || k.victim === enemy);
    const isDeath = k.victim === player && (enemy === null || k.killer === enemy);
    if ((isKill && showKills) || (isDeath && showDeaths)) {
      marks.push({
        k,
        kind: isKill ? "kill" : "death",
        at: [k.victimPos[0], k.victimPos[1]],
        from: k.killerPos && k.killer !== k.victim ? [k.killerPos[0], k.killerPos[1]] : null,
      });
    }
  }
  const nKills = marks.filter((m) => m.kind === "kill").length;
  const nDeaths = marks.length - nKills;
  const name = me?.name ?? "player";

  if (slice.map === null && slice.multiMap) {
    return (
      <p className="hint an-empty">
        This match covers {new Set(a.segments.map((x) => x.map)).size} maps. Pick one above to see where its kills
        happened: positions on different maps cannot share one drawing.
      </p>
    );
  }

  if (!a.hasPositions || !frame) {
    return <p className="hint an-empty">This log recorded no positions, so there is no map to draw.</p>;
  }

  const { name: shortMap } = splitMap(mapName);
  const careerOk = me?.isMe && view !== null && view.myGames > 0;

  return (
    <div className={full ? "killmap full" : "killmap"} ref={box}>
      <div className="km-controls">
        <label className="an-field">
          <span className="an-label">Against</span>
          <select value={enemy ?? ""} onChange={(e) => setEnemy(e.target.value === "" ? null : Number(e.target.value))}>
            <option value="">Everyone</option>
            {enemies.map((p) => (
              <option key={p.accountId} value={p.accountId}>
                {p.name}
                {p.mainClass ? ` · ${capitalize(p.mainClass)}` : ""}
              </option>
            ))}
          </select>
        </label>
        <div className="segmented" role="tablist" aria-label="Layer">
          <button role="tab" aria-selected={layer === "dots"} className={layer === "dots" ? "seg active" : "seg"} onClick={() => setLayer("dots")}>
            Each kill
          </button>
          <button
            role="tab"
            aria-selected={layer === "paths"}
            className={layer === "paths" ? "seg active" : "seg"}
            title="Where you walked, one line per life, from your own demo"
            onClick={() => setLayer("paths")}
          >
            Movement
          </button>
          <button role="tab" aria-selected={layer === "heat"} className={layer === "heat" ? "seg active" : "seg"} onClick={() => setLayer("heat")}>
            Heatmap
          </button>
        </div>
        {layer === "paths" && (
          <div className="segmented" role="tablist" aria-label="Whose movement">
            <button
              role="tab"
              aria-selected={pathWho === "player"}
              className={pathWho === "player" ? "seg active" : "seg"}
              onClick={() => {
                setPathWho("player");
                setFocus(null);
              }}
            >
              {name}
            </button>
            <button
              role="tab"
              aria-selected={pathWho === "all"}
              className={pathWho === "all" ? "seg active" : "seg"}
              title="Everyone the demo could see, which is patchy for players other than the recorder"
              onClick={() => {
                setPathWho("all");
                setFocus(null);
              }}
            >
              Everyone
            </button>
          </div>
        )}
        {layer === "dots" ? (
          <>
            <label className="check">
              <input type="checkbox" checked={showKills} onChange={(e) => setShowKills(e.target.checked)} />
              <span className="km-key km-key-kill" aria-hidden /> Kills {nKills}
            </label>
            <label className="check">
              <input type="checkbox" checked={showDeaths} onChange={(e) => setShowDeaths(e.target.checked)} />
              <span className="km-key km-key-death" aria-hidden /> Deaths {nDeaths}
            </label>
          </>
        ) : (
          <>
            <div className="segmented" role="tablist" aria-label="Heatmap of">
              <button role="tab" aria-selected={heatOf === "kills"} className={heatOf === "kills" ? "seg active" : "seg"} onClick={() => setHeatOf("kills")}>
                Where {name} got kills
              </button>
              <button role="tab" aria-selected={heatOf === "deaths"} className={heatOf === "deaths" ? "seg active" : "seg"} onClick={() => setHeatOf("deaths")}>
                Where {name} died
              </button>
            </div>
            {careerOk && (
              <div className="segmented" role="tablist" aria-label="Over">
                <button role="tab" aria-selected={scope === "match"} className={scope === "match" ? "seg active" : "seg"} onClick={() => setScope("match")}>
                  This match
                </button>
                <button role="tab" aria-selected={scope === "career"} className={scope === "career" ? "seg active" : "seg"} onClick={() => setScope("career")}>
                  All {view!.myGames} of your {shortMap ?? ""} matches
                </button>
              </div>
            )}
          </>
        )}
        <button className="linkish km-table-toggle" onClick={() => setAsTable((t) => !t)}>
          {asTable ? "Show map" : "Show as table"}
        </button>
        <button
          className="linkish km-full-toggle"
          onClick={toggleFull}
          title={full ? "Leave full screen (Escape)" : "Fill the window with the map"}
        >
          {full ? "Exit full screen" : "Full screen"}
        </button>
      </div>

      {asTable ? (
        <MarkTable marks={marks} a={a} />
      ) : (
        <>
          <div className={layer === "paths" ? "km-split" : undefined}>
          <Canvas
            frame={frame}
            display={overview ? overviewFrame(overview) : frame}
            image={overview?.image ?? null}
            view={view}
            marks={layer === "dots" ? marks : []}
            paths={layer === "paths" ? routes : []}
            focus={focus}
            maxHeight={full ? Math.max(360, window.innerHeight - FULL_CHROME) : MAX_H}
            heat={heat}
            heatColor={heatOf === "kills" ? KILL : DEATH}
            hover={hover}
            onHover={setHover}
            a={a}
          />
          {layer === "paths" && (
            <LifeList
              rows={routes}
              tickRate={tickRate}
              focus={focus}
              onFocus={setFocus}
              partial={pathWho === "all" || !me?.isMe}
            />
          )}
          </div>
          {layer === "paths" && (
            <div className="km-scale" aria-hidden>
              <span className="km-key km-key-path" /> a life
              <span className="km-key km-key-path-died" /> one that ended in a death
            </div>
          )}
          {layer === "heat" && (
            <div className="km-scale" aria-hidden>
              <span>fewer</span>
              <span className="km-scale-bar" style={{ background: `linear-gradient(90deg, transparent, ${heatOf === "kills" ? KILL : DEATH})` }} />
              <span>more {heatOf === "kills" ? "kills" : "deaths"}</span>
            </div>
          )}
          <p className="hint km-note">
            {overview
              ? "Map image from more.tf."
              : view
                ? `Map drawn from ${view.points.toLocaleString()} positions in ${view.games} stored ${shortMap ?? ""} matches; brighter is busier.`
                : "Too few matches on this map to draw it; only this match's positions are shown."}
            {layer === "heat" && heatOf === "kills" && " The heatmap marks where the player stood when they got the kill."}
            {layer === "paths" &&
              (pathQ.isPending
                ? " Reading the demo's routes…"
                : pathQ.data && pathQ.data.length > 0
                  ? pathWho === "all"
                    ? " One line per life, four positions a second. A POV demo only carries other players while its recorder could see them, so their lines break where the demo lost them."
                    : " One line per life, four positions a second, read from the demo."
                  : " No demo is linked to this match, so there is no movement to draw.")}
          </p>
          {layer === "dots" && <TimeStrip a={a} slice={slice} marks={marks} hover={hover} onHover={setHover} />}
        </>
      )}
    </div>
  );
}

/** The map, a heat layer or the kill marks, and the hover card. */
function Canvas(props: {
  /** The grid the heat and outline are counted on. */
  frame: Frame;
  /** What the canvas shows: the image's square when there is one. */
  display: Frame;
  image: string | null;
  view: MapView | null;
  marks: Mark[];
  /** Routes to draw under the marks, one per life. */
  paths: PathRow[];
  /** One route to pick out, with the rest faded. */
  focus: PathRow | null;
  /** How tall the map may be; the window's height in full screen. */
  maxHeight: number;
  heat: number[] | null;
  heatColor: string;
  hover: Mark | null;
  onHover: (m: Mark | null) => void;
  a: Analysis;
}) {
  const { frame, display, image, view, marks, paths, focus, maxHeight, heat, heatColor, hover, onHover, a } = props;
  const [img, setImg] = useState<HTMLImageElement | null>(null);
  useEffect(() => {
    setImg(null);
    if (!image) return;
    const el = new Image();
    el.onload = () => setImg(el);
    el.src = image;
  }, [image]);
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [boxW, setBoxW] = useState(800);

  useLayoutEffect(() => {
    const el = wrap.current;
    if (!el) return;
    // Measure now as well: an observer only reports once the page renders.
    setBoxW(el.clientWidth - 20);
    const ro = new ResizeObserver((e) => setBoxW(e[0].contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const scale = Math.min(boxW / display.width, maxHeight / display.height);
  const W = Math.floor(display.width * scale);
  const H = Math.floor(display.height * scale);
  const px = ([x, y]: [number, number]): [number, number] => [
    ((x - display.minX) / display.cell) * scale,
    ((display.maxY - y) / display.cell) * scale,
  ];
  // A cell of the counting grid, in canvas pixels.
  const cellRect = (i: number): [number, number, number] => {
    const [x, y] = px([frame.minX + (i % frame.width) * frame.cell, frame.maxY - Math.floor(i / frame.width) * frame.cell]);
    return [x, y, (frame.cell / display.cell) * scale + 0.5];
  };

  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = W * dpr;
    c.height = H * dpr;
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, W, H);

    if (img) {
      // The image, under a dark veil: the marks' blue and orange have to read
      // on sand and stone, and the veil keeps the map recessive.
      g.drawImage(img, 0, 0, W, H);
      g.fillStyle = "rgba(27, 23, 20, 0.45)";
      g.fillRect(0, 0, W, H);
    } else if (view) {
      // The map: occupancy on a log scale, in the muted ink, so the marks own
      // the colour. A cell seen once is a stray (a rocket jump, an old map
      // version) and would speckle the outline, so it is left out.
      const max = Math.log1p(Math.max(2, ...view.occupancy));
      for (let i = 0; i < view.occupancy.length; i++) {
        const n = view.occupancy[i];
        if (n < 2) continue;
        const v = Math.log1p(n) / max;
        g.fillStyle = `rgba(168, 153, 139, ${(0.06 + 0.6 * v ** 1.4).toFixed(3)})`;
        const [x, y, s] = cellRect(i);
        g.fillRect(x, y, s, s);
      }
    }
    // Routes: one line per life, thin and translucent so a busy match reads
    // as traffic rather than spaghetti. A life that ended in a death is drawn
    // in the death colour, and every line ends in a dot where it stopped.
    for (const route of paths) {
      if (route.points.length < 2) continue;
      const picked = focus !== null && focus.seq === route.seq && focus.demoId === route.demoId;
      const dim = focus !== null && !picked;
      g.strokeStyle = route.died
        ? `rgba(232, 106, 98, ${dim ? 0.12 : picked ? 0.95 : 0.55})`
        : `rgba(134, 171, 201, ${dim ? 0.1 : picked ? 0.95 : 0.5})`;
      g.lineWidth = picked ? 2.5 : 1.5;
      g.lineJoin = "round";
      g.beginPath();
      route.points.forEach(([, x, y], i) => {
        const [cx, cy] = px([x, y]);
        if (i === 0) g.moveTo(cx, cy);
        else g.lineTo(cx, cy);
      });
      g.stroke();
      if (dim) continue;
      // Where it started and where it stopped: a ring for the spawn, a solid
      // dot for the end, so a route reads in one direction.
      const first = route.points[0];
      const last = route.points[route.points.length - 1];
      const [sx, sy] = px([first[1], first[2]]);
      const [ex, ey] = px([last[1], last[2]]);
      if (picked) {
        g.strokeStyle = "rgba(242, 230, 217, 0.9)";
        g.lineWidth = 1.5;
        g.beginPath();
        g.arc(sx, sy, 4, 0, Math.PI * 2);
        g.stroke();
      }
      g.fillStyle = route.died ? DEATH : KILL;
      g.beginPath();
      g.arc(ex, ey, picked ? 4 : 2.5, 0, Math.PI * 2);
      g.fill();
    }

    // Heat: one hue, transparent to full, so more is brighter. No floor:
    // with a floor every cell anyone ever died in lights up and the hot spots
    // drown; below 6% a cell stays clear.
    if (heat) {
      const max = Math.max(...heat);
      if (max > 0) {
        for (let i = 0; i < heat.length; i++) {
          const alpha = 0.92 * (heat[i] / max) ** 0.9;
          if (alpha < 0.06) continue;
          g.globalAlpha = alpha;
          g.fillStyle = heatColor;
          const [x, y, s] = cellRect(i);
          g.fillRect(x, y, s, s);
        }
        g.globalAlpha = 1;
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [img, view, heat, heatColor, frame, display, W, H, scale, paths, focus]);

  const nearest = (e: React.MouseEvent<SVGSVGElement>): Mark | null => {
    const r = e.currentTarget.getBoundingClientRect();
    const mx = e.clientX - r.left;
    const my = e.clientY - r.top;
    let best: Mark | null = null;
    let bestD = 14 * 14;
    for (const m of marks) {
      const [x, y] = px(m.at);
      const d = (x - mx) ** 2 + (y - my) ** 2;
      if (d < bestD) {
        bestD = d;
        best = m;
      }
    }
    return best;
  };

  return (
    <div className="km-canvas" ref={wrap}>
      <div className="km-stage" style={{ width: W, height: H }}>
        <canvas ref={canvas} style={{ width: W, height: H }} aria-hidden />
        <svg
          width={W}
          height={H}
          className="km-svg"
          role="img"
          aria-label={`${marks.length} kills and deaths on the map`}
          onMouseMove={(e) => onHover(nearest(e))}
          onMouseLeave={() => onHover(null)}
          onClick={(e) => {
            const m = nearest(e);
            if (m?.k.jump) jumpTo(m.k.jump, `${m.kind} at ${roundClock(m.k.t, a.rounds)}`);
          }}
          style={{ cursor: hover?.k.jump ? "pointer" : "default" }}
        >
          {marks.map((m, i) => {
            const [x, y] = px(m.at);
            const from = m.from ? px(m.from) : null;
            const color = m.kind === "kill" ? KILL : DEATH;
            const dim = hover && hover !== m ? 0.35 : 1;
            return (
              <g key={i} opacity={dim}>
                {from && <line x1={from[0]} y1={from[1]} x2={x} y2={y} stroke={color} strokeWidth={1.25} opacity={0.7} />}
                {from && <circle cx={from[0]} cy={from[1]} r={3.5} className="km-shooter" />}
                {m.kind === "kill" ? (
                  <circle cx={x} cy={y} r={4.5} fill={color} className="km-mark" />
                ) : (
                  <>
                    {/* A dark halo first, so the cross reads on a light map. */}
                    <path d={`M${x - 4},${y - 4}L${x + 4},${y + 4}M${x - 4},${y + 4}L${x + 4},${y - 4}`} className="km-halo" />
                    <path d={`M${x - 4},${y - 4}L${x + 4},${y + 4}M${x - 4},${y + 4}L${x + 4},${y - 4}`} stroke={color} strokeWidth={2.5} strokeLinecap="round" />
                  </>
                )}
              </g>
            );
          })}
        </svg>
        {hover && <HoverCard m={hover} a={a} pos={px(hover.at)} W={W} />}
      </div>
    </div>
  );
}

function HoverCard({ m, a, pos, W }: { m: Mark; a: Analysis; pos: [number, number]; W: number }) {
  const players = playerMap(a);
  const k = m.k;
  const left = Math.min(W - 250, Math.max(0, pos[0] + 12));
  return (
    <div className="km-tip" style={{ left, top: Math.max(0, pos[1] - 10) }}>
      <div className="km-tip-head">
        <span className={m.kind === "kill" ? "km-key km-key-kill" : "km-key km-key-death"} aria-hidden />
        <strong>{m.kind === "kill" ? "Kill" : "Death"}</strong>
        <span className="muted">{roundClock(k.t, a.rounds)}</span>
      </div>
      <div>
        {players.get(k.killer)?.name ?? "?"} <span className="muted">({k.killerClass ?? "?"})</span> →{" "}
        {players.get(k.victim)?.name ?? "?"} <span className="muted">({k.victimClass ?? "?"})</span>
      </div>
      <div className="muted">
        {k.weapon}
        {k.custom && ` · ${k.custom}`}
        {k.distance !== null && ` · ${Math.round(k.distance).toLocaleString()} units`}
      </div>
      {k.jump && <div className="km-tip-jump">Click to copy demo_gototick {k.jump.tick}</div>}
    </div>
  );
}

/** Every mark along the match's game time: kills above the line, deaths below. */
function TimeStrip(props: { a: Analysis; slice: Slice; marks: Mark[]; hover: Mark | null; onHover: (m: Mark | null) => void }) {
  const { a, slice, marks, hover, onHover } = props;
  const wrap = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(800);
  useLayoutEffect(() => {
    const el = wrap.current;
    if (!el) return;
    setW(el.clientWidth);
    const ro = new ResizeObserver((e) => setW(e[0].contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const H = 44;
  const mid = H / 2;
  const span = Math.max(1, slice.endS - slice.startS);
  const x = (t: number) => ((t - slice.startS) / span) * w;

  const nearest = (e: React.MouseEvent<SVGSVGElement>) => {
    const mx = e.clientX - e.currentTarget.getBoundingClientRect().left;
    let best: Mark | null = null;
    let bestD = 8;
    for (const m of marks) {
      const d = Math.abs(x(m.k.t) - mx);
      if (d < bestD) {
        bestD = d;
        best = m;
      }
    }
    return best;
  };

  return (
    <div className="km-strip" ref={wrap}>
      <svg
        width={w}
        height={H + 16}
        role="img"
        aria-label="Kills and deaths over the match"
        onMouseMove={(e) => onHover(nearest(e))}
        onMouseLeave={() => onHover(null)}
        onClick={(e) => {
          const m = nearest(e);
          if (m?.k.jump) jumpTo(m.k.jump, `${m.kind} at ${roundClock(m.k.t, a.rounds)}`);
        }}
      >
        <line x1={0} x2={w} y1={mid} y2={mid} className="km-axis" />
        {a.rounds.filter((r) => inSlice(r.roundNum, slice)).map((r) => (
          <g key={r.roundNum}>
            {r.startS > slice.startS && <line x1={x(r.startS)} x2={x(r.startS)} y1={2} y2={H - 2} className="km-round" />}
            <text x={x(r.startS) + 4} y={H + 12} className="km-round-label">
              R{r.roundNum}
            </text>
          </g>
        ))}
        {marks.map((m, i) => (
          <rect
            key={i}
            x={x(m.k.t) - 1.5}
            y={m.kind === "kill" ? mid - 16 : mid + 2}
            width={3}
            height={14}
            rx={1}
            fill={m.kind === "kill" ? KILL : DEATH}
            opacity={hover && hover !== m ? 0.35 : 1}
          />
        ))}
      </svg>
    </div>
  );
}

/** The same marks without the map: every value reachable without hovering. */
function MarkTable({ marks, a }: { marks: Mark[]; a: Analysis }) {
  const players = playerMap(a);
  if (marks.length === 0) return <p className="hint an-empty">Nothing to show for this filter.</p>;
  return (
    <div className="table-wrap">
      <table className="match-table">
        <thead>
          <tr>
            <th>When</th>
            <th></th>
            <th>Killer</th>
            <th>Victim</th>
            <th>Weapon</th>
            <th className="num">Distance</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {marks.map((m, i) => (
            <tr key={i}>
              <td className="muted nowrap">{roundClock(m.k.t, a.rounds)}</td>
              <td>
                <span className={m.kind === "kill" ? "km-key km-key-kill" : "km-key km-key-death"} aria-hidden />{" "}
                {m.kind === "kill" ? "kill" : "death"}
              </td>
              <td className="nowrap">
                {players.get(m.k.killer)?.name} <span className="muted">{m.k.killerClass}</span>
              </td>
              <td className="nowrap">
                {players.get(m.k.victim)?.name} <span className="muted">{m.k.victimClass}</span>
              </td>
              <td className="muted nowrap">
                {m.k.weapon}
                {m.k.custom && ` · ${m.k.custom}`}
              </td>
              <td className="num">{m.k.distance === null ? "—" : Math.round(m.k.distance).toLocaleString()}</td>
              <td>
                {m.k.jump && (
                  <button className="linkish" onClick={() => jumpTo(m.k.jump!, `${m.kind} at ${roundClock(m.k.t, a.rounds)}`)}>
                    copy tick
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Without a stored outline, frame the match's own positions. */
function frameFromKills(kills: KillView[]): Frame | null {
  const pts = kills.flatMap((k) => [k.killerPos, k.victimPos]).filter((p): p is Vec3 => p !== null);
  if (pts.length === 0) return null;
  let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
  for (const [x, y] of pts) {
    x0 = Math.min(x0, x);
    x1 = Math.max(x1, x);
    y0 = Math.min(y0, y);
    y1 = Math.max(y1, y);
  }
  const pad = 0.05 * Math.max(x1 - x0, y1 - y0, 1);
  const cell = Math.max(1, (Math.max(x1 - x0, y1 - y0) + 2 * pad) / 180);
  return {
    minX: x0 - pad,
    maxY: y1 + pad,
    cell,
    width: Math.ceil((x1 - x0 + 2 * pad) / cell),
    height: Math.ceil((y1 - y0 + 2 * pad) / cell),
  };
}

/**
 * Heat on the frame's grid, smoothed with two box blurs. Kills are placed
 * where the player stood when they got them; deaths where they fell.
 */
function heatGrid(
  f: Frame,
  view: MapView | null,
  kills: KillView[],
  player: number,
  of: HeatOf,
  scope: Scope,
  enemy: number | null,
): number[] {
  let grid: number[];
  if (scope === "career" && view) {
    grid = (of === "kills" ? view.myKills : view.myDeaths).slice();
  } else {
    grid = new Array(f.width * f.height).fill(0);
    for (const k of kills) {
      const mine = of === "kills" ? k.killer === player && k.victim !== player : k.victim === player;
      const vs = of === "kills" ? k.victim : k.killer;
      const pos = of === "kills" ? k.killerPos : k.victimPos;
      if (!mine || !pos || (enemy !== null && vs !== enemy)) continue;
      const cx = Math.floor((pos[0] - f.minX) / f.cell);
      const cy = Math.floor((f.maxY - pos[1]) / f.cell);
      if (cx >= 0 && cy >= 0 && cx < f.width && cy < f.height) grid[cy * f.width + cx] += 1;
    }
  }
  return blur(blur(grid, f.width, f.height), f.width, f.height);
}

function blur(g: number[], w: number, h: number): number[] {
  const out = new Array(g.length).fill(0);
  const r = 2;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      let s = 0;
      for (let dy = -r; dy <= r; dy++) {
        const yy = y + dy;
        if (yy < 0 || yy >= h) continue;
        for (let dx = -r; dx <= r; dx++) {
          const xx = x + dx;
          if (xx >= 0 && xx < w) s += g[yy * w + xx];
        }
      }
      out[y * w + x] = s;
    }
  }
  return out;
}

/** An overview image's square, as a drawing frame of 1024 units a side. */
function overviewFrame(o: Overview): Frame {
  return { minX: o.minX, maxY: o.maxY, cell: o.size / 1024, width: 1024, height: 1024 };
}
