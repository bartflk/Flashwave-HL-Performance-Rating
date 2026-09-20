import type { PathRow } from "../../api/types";

/**
 * The lives behind the Movement layer, as a list you can pick from.
 *
 * One row per route: the round it was in, when it started, how long it
 * lasted, and how it ended. Hovering or clicking a row picks that route out
 * on the map and fades the rest, which is the only way to read a busy match.
 *
 * A POV demo keeps its recorder the whole time but loses everyone else
 * whenever they leave the recorder's sight, so another player's "life" is
 * really a stretch the demo could see; the list says so.
 */
/** Rows past this are cut: the map still draws every route. */
const MAX_ROWS = 200;

export function LifeList(props: {
  rows: PathRow[];
  /** Ticks per second, to turn a route's length into time. */
  tickRate: number;
  focus: PathRow | null;
  onFocus: (r: PathRow | null) => void;
  /** True when the routes belong to someone other than the recorder. */
  partial: boolean;
}) {
  const { rows, tickRate, focus, onFocus, partial } = props;
  if (rows.length === 0) {
    return <p className="hint km-lives-empty">No movement stored for this player in this round.</p>;
  }

  const seconds = (r: PathRow) => Math.max(1, Math.round((r.toTick - r.fromTick) / tickRate));
  // Everyone's routes in a long match run to four figures; the list shows the
  // longest of them, which are the ones worth following.
  const shown = rows.length > MAX_ROWS ? [...rows].sort((a, b) => b.toTick - b.fromTick - (a.toTick - a.fromTick)).slice(0, MAX_ROWS) : rows;

  return (
    <div className="km-lives">
      <div className="km-lives-head">
        <h3>{partial ? "Stretches the demo saw" : "Lives"}</h3>
        <span className="hint">
          {rows.length > MAX_ROWS ? `longest ${MAX_ROWS} of ${rows.length}` : `${rows.length} in view`}
        </span>
      </div>
      <ol className="km-lives-list">
        {shown.map((r) => {
          const picked = focus !== null && focus.seq === r.seq && focus.demoId === r.demoId;
          return (
            <li key={`${r.demoId}-${r.seq}`}>
              <button
                className={picked ? "km-life picked" : "km-life"}
                onMouseEnter={() => onFocus(r)}
                onMouseLeave={() => focus === r && onFocus(null)}
                onClick={() => onFocus(picked ? null : r)}
                aria-pressed={picked}
              >
                <span className={`km-life-dot ${r.died ? "died" : "lived"}`} aria-hidden />
                <span className="km-life-round">R{r.roundNum ?? "?"}</span>
                <span className="km-life-len">{seconds(r)}s</span>
                <span className="km-life-end">{r.died ? "died" : partial ? "lost sight" : "survived"}</span>
              </button>
            </li>
          );
        })}
      </ol>
      {focus && (
        <p className="hint">
          Showing one {focus.died ? "life that ended in a death" : "life"}. Click it again, or press Escape, for all of
          them.
        </p>
      )}
    </div>
  );
}
