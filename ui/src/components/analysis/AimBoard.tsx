import { useState } from "react";
import type { AimRow } from "../../api/types";

/**
 * The kills on a target, seen down your own scope (PLAN §14).
 *
 * The centre is the head you killed. A dot is where your crosshair sat
 * relative to it: right of centre means you were aiming to their right, above
 * centre means high. Rings are degrees off, on a square-root scale so the
 * middle — where most kills live — stays readable.
 *
 * Two moments can be shown: the shot itself, and one second earlier, which is
 * the same kills with the crosshair still on its way.
 */

const CALM = "#5791c8";
const BUSY = "#d6763a";

/** Rings, in degrees off the head. */
const RINGS = [1, 3, 10, 30];
const MAX_DEG = RINGS[RINGS.length - 1];
/** A head is about this wide in map units, which sets the bullseye. */
const HEAD_UNITS = 16;

export function AimBoard({ kills }: { kills: AimRow[] }) {
  const [when, setWhen] = useState<"shot" | "before">("shot");
  const seen = kills.filter((k) => k.victimSeen);
  if (seen.length === 0) return null;

  const at = (k: AimRow) => (when === "shot" ? { x: k.dxDeg, y: k.dyDeg, d: k.errorDeg } : { x: k.beforeDxDeg, y: k.beforeDyDeg, d: k.beforeDeg });

  const S = 420;
  const c = S / 2;
  const R = c - 26;
  // Square root of the angle, so one degree is not a pixel at the centre.
  const radius = (deg: number) => R * Math.sqrt(Math.min(deg, MAX_DEG) / MAX_DEG);
  const place = (x: number, y: number, d: number) => {
    const len = Math.hypot(x, y) || 1;
    const r = radius(d);
    return [c + (x / len) * r, c - (y / len) * r] as const;
  };

  const inside = seen.filter((k) => at(k).d <= MAX_DEG).length;
  const bias = {
    x: seen.reduce((n, k) => n + at(k).x, 0) / seen.length,
    y: seen.reduce((n, k) => n + at(k).y, 0) / seen.length,
  };
  const medianRange = [...seen].map((k) => k.rangeUnits).sort((a, b) => a - b)[Math.floor(seen.length / 2)] ?? 1000;
  // The head's own angular size at that range: what "on target" actually is.
  const headDeg = (2 * Math.atan(HEAD_UNITS / 2 / medianRange) * 180) / Math.PI;

  return (
    <figure className="aim-fig">
      <figcaption>
        Where your crosshair sat
        <span className="aim-fig-sub">
          centre is the head you killed; right of centre means you were aiming to their right
        </span>
        <span className="board-when segmented" role="tablist" aria-label="Moment">
          <button role="tab" aria-selected={when === "shot"} className={when === "shot" ? "seg active" : "seg"} onClick={() => setWhen("shot")}>
            At the shot
          </button>
          <button role="tab" aria-selected={when === "before"} className={when === "before" ? "seg active" : "seg"} onClick={() => setWhen("before")}>
            A second before
          </button>
        </span>
      </figcaption>

      <div className="board-wrap">
        <svg viewBox={`0 0 ${S} ${S}`} className="aim-board" role="img" aria-label={`${inside} of ${seen.length} kills within ${MAX_DEG} degrees of the head`}>
          {RINGS.map((deg) => (
            <g key={deg}>
              <circle cx={c} cy={c} r={radius(deg)} className="board-ring" />
              <text x={c} y={c - radius(deg) - 4} className="aim-axis" textAnchor="middle">
                {deg}°
              </text>
            </g>
          ))}
          <line x1={c - R} x2={c + R} y1={c} y2={c} className="board-cross" />
          <line x1={c} x2={c} y1={c - R} y2={c + R} className="board-cross" />
          {/* The head itself, to scale: inside this circle the shot was on it. */}
          <circle cx={c} cy={c} r={Math.max(2, radius(headDeg))} className="board-head" />

          {seen.map((k) => {
            const p = at(k);
            const [x, y] = place(p.x, p.y, p.d);
            return (
              <circle key={k.tick} cx={x} cy={y} r={4.5} fill={k.headshot ? BUSY : "none"} stroke={k.headshot ? BUSY : CALM} strokeWidth={1.8} opacity={0.9}>
                <title>
                  {p.d.toFixed(1)}° off ({fmt(p.x)}, {fmtY(p.y)}), {k.rangeUnits.toFixed(0)} units away
                  {k.headshot ? ", headshot" : ""}
                </title>
              </circle>
            );
          })}

          {/* Where the crosshair usually sat: the habit, not the kill. */}
          {(() => {
            const d = Math.hypot(bias.x, bias.y);
            const [x, y] = place(bias.x, bias.y, d);
            return (
              <g className="board-bias">
                <line x1={x - 7} x2={x + 7} y1={y} y2={y} />
                <line x1={x} x2={x} y1={y - 7} y2={y + 7} />
                <title>
                  On average {d.toFixed(1)}° off: {fmt(bias.x)}, {fmtY(bias.y)}
                </title>
              </g>
            );
          })()}
        </svg>

        <dl className="board-read">
          <dt>Usually off by</dt>
          <dd>
            {Math.hypot(bias.x, bias.y).toFixed(1)}° — {fmt(bias.x)}, {fmtY(bias.y)}
          </dd>
          <dt>On the head</dt>
          <dd>
            {seen.filter((k) => at(k).d <= headDeg).length} of {seen.length} kills, inside the {headDeg.toFixed(1)}° the head
            covers at {medianRange.toFixed(0)} units
          </dd>
          <dt>Off the board</dt>
          <dd>
            {seen.length - inside} kills were more than {MAX_DEG}° away and sit on the rim
          </dd>
        </dl>
      </div>

      <ul className="aim-legend">
        <li>
          <span className="aim-dot" style={{ background: BUSY }} aria-hidden /> headshot
        </li>
        <li>
          <span className="aim-dot aim-dot-open" style={{ borderColor: CALM }} aria-hidden /> body shot
        </li>
        <li>
          <span className="aim-dot board-dot-bias" aria-hidden /> where you usually were
        </li>
      </ul>
    </figure>
  );
}

/** A sideways angle, said the way a player would say it. */
function fmt(deg: number): string {
  if (Math.abs(deg) < 0.05) return "dead centre";
  return `${Math.abs(deg).toFixed(1)}° ${deg > 0 ? "right" : "left"}`;
}

/** The same, up and down. */
function fmtY(deg: number): string {
  if (Math.abs(deg) < 0.05) return "level";
  return `${Math.abs(deg).toFixed(1)}° ${deg > 0 ? "high" : "low"}`;
}
