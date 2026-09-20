import type { AimRow, DeathRow } from "../../api/types";

/**
 * Three pictures of the same kills, so the numbers in the table mean
 * something at a glance:
 *
 * 1. **How each kill was set up** — one bar, split into angles that were
 *    already held, small adjustments, and flicks.
 * 2. **Range against crosshair error** — one dot per kill, so a long shot
 *    that was also a scramble stands out from a held sightline.
 * 3. **Who was near when you died** — one dot per death against the distance
 *    to your nearest teammate, with the cover line marked.
 *
 * Colours: the validated pair used elsewhere in the app (blue for the calm
 * case, orange for the busy one), with a muted step between them. Every
 * series is labelled as well as coloured.
 */

/** A kill is "held" when the crosshair was already on them a second before. */
const HELD_DEG = 3;
/** Above this a second before, the crosshair had to travel a long way. */
const FLICK_DEG = 20;
/** A teammate within this is cover; it matches the pass's own distance. */
const COVER_UNITS = 900;

const CALM = "#5791c8";
const MID = "#a8998b";
const BUSY = "#d6763a";

export function AimCharts({ kills, deaths }: { kills: AimRow[]; deaths: DeathRow[] }) {
  const seen = kills.filter((k) => k.victimSeen);
  if (seen.length === 0) return null;

  return (
    <div className="aim-charts">
      <Setup kills={seen} />
      <Scatter kills={seen} />
      {deaths.length > 0 && <Cover deaths={deaths} />}
    </div>
  );
}

/** One bar: how the crosshair got there, kill by kill. */
function Setup({ kills }: { kills: AimRow[] }) {
  const bands = [
    { label: "Angle held", hint: `already within ${HELD_DEG}° a second before`, color: CALM, rows: kills.filter((k) => k.beforeDeg <= HELD_DEG) },
    { label: "Adjusted", hint: `${HELD_DEG}–${FLICK_DEG}° to travel`, color: MID, rows: kills.filter((k) => k.beforeDeg > HELD_DEG && k.beforeDeg <= FLICK_DEG) },
    { label: "Flicked", hint: `over ${FLICK_DEG}° to travel`, color: BUSY, rows: kills.filter((k) => k.beforeDeg > FLICK_DEG) },
  ];

  return (
    <figure className="aim-fig">
      <figcaption>
        How each kill was set up
        <span className="aim-fig-sub">where the crosshair was one second before the shot</span>
      </figcaption>
      <div className="setup-bar" role="img" aria-label={bands.map((b) => `${b.label}: ${b.rows.length} kills`).join(", ")}>
        {bands.map((b) =>
          b.rows.length === 0 ? null : (
            <span
              key={b.label}
              className="setup-seg"
              style={{ flexGrow: b.rows.length, background: b.color }}
              title={`${b.label}: ${b.rows.length} of ${kills.length} kills, ${b.hint}`}
            >
              {b.rows.length / kills.length > 0.12 && (
                <span className="setup-in">{Math.round((b.rows.length / kills.length) * 100)}%</span>
              )}
            </span>
          ),
        )}
      </div>
      <ul className="aim-legend">
        {bands.map((b) => (
          <li key={b.label}>
            <span className="aim-dot" style={{ background: b.color }} aria-hidden />
            <b>{b.label}</b> <span className="muted">{b.hint}</span> — {b.rows.length}
          </li>
        ))}
      </ul>
    </figure>
  );
}

/** Range against crosshair error: one dot per kill. */
function Scatter({ kills }: { kills: AimRow[] }) {
  const W = 560;
  const H = 220;
  const pad = { l: 44, r: 12, t: 10, b: 34 };
  const maxRange = Math.max(1200, ...kills.map((k) => k.rangeUnits));
  const maxErr = Math.max(4, ...kills.map((k) => k.errorDeg));
  const x = (v: number) => pad.l + (v / maxRange) * (W - pad.l - pad.r);
  const y = (v: number) => H - pad.b - (v / maxErr) * (H - pad.t - pad.b);
  const xTicks = ticks(maxRange, 4);
  const yTicks = ticks(maxErr, 3);

  return (
    <figure className="aim-fig">
      <figcaption>
        Every kill: how far, and how far off
        <span className="aim-fig-sub">a dot low and right is a long shot with the crosshair already on the head</span>
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} className="aim-svg" role="img" aria-label="Range against crosshair error, one dot per kill">
        {yTicks.map((t) => (
          <g key={t}>
            <line x1={pad.l} x2={W - pad.r} y1={y(t)} y2={y(t)} className="aim-grid" />
            <text x={pad.l - 8} y={y(t) + 4} className="aim-axis" textAnchor="end">
              {t.toFixed(t < 10 ? 1 : 0)}°
            </text>
          </g>
        ))}
        {xTicks.map((t) => (
          <text key={t} x={x(t)} y={H - pad.b + 18} className="aim-axis" textAnchor="middle">
            {t.toLocaleString()}
          </text>
        ))}
        <line x1={pad.l} x2={W - pad.r} y1={H - pad.b} y2={H - pad.b} className="aim-axis-line" />
        {kills.map((k) => (
          <circle
            key={k.tick}
            cx={x(k.rangeUnits)}
            cy={y(k.errorDeg)}
            r={5}
            fill={k.headshot ? BUSY : "none"}
            stroke={k.headshot ? BUSY : CALM}
            strokeWidth={2}
          >
            <title>
              {k.rangeUnits.toFixed(0)} units away, {k.errorDeg.toFixed(1)}° off at the shot,{" "}
              {k.beforeDeg.toFixed(1)}° a second before{k.headshot ? ", headshot" : ""}
            </title>
          </circle>
        ))}
        <text x={W - pad.r} y={H - 4} className="aim-axis" textAnchor="end">
          distance in map units
        </text>
      </svg>
      <ul className="aim-legend">
        <li>
          <span className="aim-dot" style={{ background: BUSY }} aria-hidden /> headshot
        </li>
        <li>
          <span className="aim-dot aim-dot-open" style={{ borderColor: CALM }} aria-hidden /> body shot
        </li>
      </ul>
    </figure>
  );
}

/** Deaths against the distance to the nearest teammate. */
function Cover({ deaths }: { deaths: DeathRow[] }) {
  const W = 560;
  const H = 96;
  const pad = { l: 12, r: 12, t: 18, b: 28 };
  const known = deaths.filter((d) => d.nearestMate !== null);
  if (known.length === 0) return null;
  const max = Math.max(1200, ...known.map((d) => d.nearestMate ?? 0));
  const x = (v: number) => pad.l + (v / max) * (W - pad.l - pad.r);
  const alone = known.filter((d) => d.matesNear === 0).length;

  return (
    <figure className="aim-fig">
      <figcaption>
        Who was near when you died
        <span className="aim-fig-sub">distance to your nearest living teammate</span>
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} className="aim-svg" role="img" aria-label="Distance to the nearest teammate at each death">
        <line x1={pad.l} x2={W - pad.r} y1={H - pad.b} y2={H - pad.b} className="aim-axis-line" />
        <line x1={x(COVER_UNITS)} x2={x(COVER_UNITS)} y1={pad.t - 8} y2={H - pad.b} className="aim-cover-line" />
        <text x={x(COVER_UNITS) + 6} y={pad.t - 2} className="aim-axis">
          {COVER_UNITS} units: within reach
        </text>
        {known.map((d, i) => (
          <circle
            key={d.tick}
            cx={x(d.nearestMate ?? 0)}
            cy={H - pad.b - 10 - (i % 3) * 9}
            r={4.5}
            fill={d.matesNear === 0 ? BUSY : CALM}
            fillOpacity={0.85}
          >
            <title>
              nearest teammate {(d.nearestMate ?? 0).toFixed(0)} units away, {d.matesNear} within {COVER_UNITS}
              {d.scoped ? ", scoped" : ""}
            </title>
          </circle>
        ))}
        {[0, max / 2, max].map((t) => (
          <text key={t} x={x(t)} y={H - 8} className="aim-axis" textAnchor={t === 0 ? "start" : t === max ? "end" : "middle"}>
            {t.toFixed(0)}
          </text>
        ))}
      </svg>
      <ul className="aim-legend">
        <li>
          <span className="aim-dot" style={{ background: BUSY }} aria-hidden /> nobody within {COVER_UNITS} — {alone} of{" "}
          {known.length} deaths
        </li>
        <li>
          <span className="aim-dot" style={{ background: CALM }} aria-hidden /> someone close by
        </li>
      </ul>
    </figure>
  );
}

/** Round tick values up to `max`, `n` of them. */
function ticks(max: number, n: number): number[] {
  const step = max / n;
  return Array.from({ length: n + 1 }, (_, i) => Math.round(i * step));
}
