import type { DeathRow } from "../../api/types";

/**
 * Where the player who killed you was, seen from above.
 *
 * You are the middle, facing up the page. The angle round the circle is how
 * far you would have had to turn to look at them, so the bottom of the circle
 * is directly behind you. The distance out is how far away they were.
 *
 * A dartboard was the first attempt, but deaths almost never differ
 * vertically — everyone stands on the same floor — so every dot landed on one
 * line. Direction and distance are the two things that actually vary.
 *
 * The shaded wedge is roughly what a TF2 screen shows: inside it they were on
 * your screen when they killed you, outside it they were not.
 */

const SEEN = "#5791c8";
const SCOPED = "#d6763a";

/** Half of TF2's default field of view, near enough for a wedge. */
const HALF_FOV = 45;
/** Rings, in map units. A Sniper sightline runs about 1,500. */
const RINGS = [500, 1000, 1500, 2000];

export function DeathBoard({ deaths }: { deaths: DeathRow[] }) {
  const placed = deaths.filter((d) => d.killerDxDeg !== null);
  if (placed.length === 0) return null;

  const S = 320;
  const c = S / 2;
  const R = c - 26;
  const max = Math.max(1200, ...placed.map((d) => d.killerRange ?? 0));
  // Unknown distances sit just outside the last ring rather than vanishing.
  const radius = (units: number | null) => (units === null ? R + 6 : (Math.min(units, max) / max) * R);
  const place = (d: DeathRow) => {
    const bearing = ((d.killerDxDeg ?? 0) * Math.PI) / 180;
    const r = radius(d.killerRange);
    return [c + Math.sin(bearing) * r, c - Math.cos(bearing) * r] as const;
  };

  const behind = placed.filter((d) => Math.abs(d.killerDxDeg ?? 0) > HALF_FOV);
  const scopedBehind = behind.filter((d) => d.scoped).length;
  const unknown = placed.filter((d) => d.killerRange === null).length;
  const wedge = [
    `M ${c} ${c}`,
    `L ${c + Math.sin((-HALF_FOV * Math.PI) / 180) * R} ${c - Math.cos((-HALF_FOV * Math.PI) / 180) * R}`,
    `A ${R} ${R} 0 0 1 ${c + Math.sin((HALF_FOV * Math.PI) / 180) * R} ${c - Math.cos((HALF_FOV * Math.PI) / 180) * R}`,
    "Z",
  ].join(" ");

  return (
    <figure className="aim-fig">
      <figcaption>
        Where the player who killed you was
        <span className="aim-fig-sub">seen from above: you are in the middle, facing up</span>
      </figcaption>

      <div className="board-wrap">
        <svg
          viewBox={`0 0 ${S} ${S}`}
          className="aim-board death-board"
          role="img"
          aria-label={`${behind.length} of ${placed.length} deaths came from outside your view`}
        >
          <path d={wedge} className="board-fov" />
          {RINGS.filter((u) => u <= max).map((u) => (
            <g key={u}>
              <circle cx={c} cy={c} r={radius(u)} className="board-ring" />
              <text x={c + 3} y={c - radius(u) + 11} className="aim-axis">
                {u.toLocaleString()}
              </text>
            </g>
          ))}
          <line x1={c} x2={c} y1={c - R} y2={c + R} className="board-cross" />
          <line x1={c - R} x2={c + R} y1={c} y2={c} className="board-cross" />
          <text x={c} y={12} className="aim-axis" textAnchor="middle">
            where you were looking
          </text>
          <text x={c} y={S - 3} className="aim-axis" textAnchor="middle">
            behind you
          </text>

          {placed.map((d) => {
            const [x, y] = place(d);
            const turn = Math.abs(d.killerDxDeg ?? 0);
            return (
              <circle key={d.tick} cx={x} cy={y} r={5} fill={d.scoped ? SCOPED : SEEN} fillOpacity={0.85}>
                <title>
                  {turn < 1 ? "straight ahead" : `${turn.toFixed(0)}° to your ${(d.killerDxDeg ?? 0) > 0 ? "right" : "left"}`}
                  {d.killerRange === null ? ", distance not recorded" : `, ${d.killerRange.toFixed(0)} units away`}
                  {d.scoped ? ", you were scoped" : ""}
                  {d.matesNear === 0 ? ", nobody near you" : `, ${d.matesNear} teammate${d.matesNear === 1 ? "" : "s"} near`}
                </title>
              </circle>
            );
          })}
          <circle cx={c} cy={c} r={3} className="board-you" />
        </svg>

        <dl className="board-read">
          <dt>Off your screen</dt>
          <dd>
            {behind.length} of {placed.length} deaths came from more than {HALF_FOV}° to a side, so they were never in
            front of you
          </dd>
          <dt>Scoped at the time</dt>
          <dd>
            {placed.filter((d) => d.scoped).length} of {placed.length}
            {scopedBehind > 0 && `, and ${scopedBehind} of those came from outside your view`}
          </dd>
          <dt>Usual distance</dt>
          <dd>
            {median(placed)}
            {unknown > 0 && (
              <span className="muted">
                {" "}
                · {unknown} on the rim: the demo never carried them, which is what a Spy behind you looks like
              </span>
            )}
          </dd>
        </dl>
      </div>

      <ul className="aim-legend">
        <li>
          <span className="aim-dot" style={{ background: SCOPED }} aria-hidden /> you were scoped
        </li>
        <li>
          <span className="aim-dot" style={{ background: SEEN }} aria-hidden /> you were not
        </li>
        <li>
          <span className="aim-dot board-dot-fov" aria-hidden /> roughly what your screen showed
        </li>
      </ul>
    </figure>
  );
}

/** The middle distance of the deaths the demo could measure. */
function median(rows: DeathRow[]): string {
  const known = rows
    .map((d) => d.killerRange)
    .filter((v): v is number => v !== null)
    .sort((a, b) => a - b);
  if (known.length === 0) return "not recorded";
  return `${known[Math.floor(known.length / 2)].toFixed(0)} units`;
}
