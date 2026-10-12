import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { PathRow } from "../../api/types";
import { t, tx } from "../../lib/i18n";

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

/** "1st", "2nd", "3rd", then plain numbers: a life rarely takes four. */
function ordinal(n: number): string {
  return ["1st", "2nd", "3rd"][n - 1] ?? `${n}th`;
}

/** Seconds into the life, as a clock. */
function clock(s: number): string {
  return `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, "0")}`;
}

export function LifeList(props: {
  rows: PathRow[];
  /** Ticks per second, to turn a route's length into time. */
  tickRate: number;
  focus: PathRow | null;
  onFocus: (r: PathRow | null) => void;
  /** True when the routes belong to someone other than the recorder. */
  partial: boolean;
  /** Whether this match has an STV demo: linked, fetchable, or neither. */
  stv: "linked" | "available" | "none";
  onFetchStv: () => void;
  fetching: boolean;
}) {
  const { rows, tickRate, focus, onFocus, partial, stv, onFetchStv, fetching } = props;
  // No scroll bar: an arrow at the top or bottom says there is more that
  // way, and scrolls a page when clicked (Flashy's UX pass).
  const list = useRef<HTMLOListElement>(null);
  const [more, setMore] = useState({ up: false, down: false });
  const measure = useCallback(() => {
    const el = list.current;
    if (!el) return;
    const up = el.scrollTop > 2;
    const down = el.scrollTop + el.clientHeight < el.scrollHeight - 2;
    setMore((m) => (m.up === up && m.down === down ? m : { up, down }));
  }, []);
  useLayoutEffect(() => {
    measure();
    const el = list.current;
    if (!el) return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [measure, rows]);
  const page = (dir: 1 | -1) => {
    const el = list.current;
    if (el) el.scrollBy({ top: dir * el.clientHeight * 0.8, behavior: "smooth" });
  };
  const offer =
    stv === "available" ? (
      <div className="km-stv">
        <p className="hint">{t("This is a POV demo, so other players are only in it while the recorder could see them. The SourceTV demo on demos.tf carries all eighteen.")}</p>
        <button onClick={onFetchStv} disabled={fetching}>
          {fetching ? t("Downloading and reading…") : t("Download the STV demo")}
        </button>
      </div>
    ) : null;

  if (rows.length === 0) {
    return (
      <div className="km-lives">
        <p className="hint">{t("No movement stored for this player here.")}</p>
        {offer}
      </div>
    );
  }

  const seconds = (r: PathRow) => Math.max(1, Math.round((r.toTick - r.fromTick) / tickRate));
  // Everyone's routes in a long match run to four figures; the list shows the
  // longest of them, which are the ones worth following.
  const shown = rows.length > MAX_ROWS ? [...rows].sort((a, b) => b.toTick - b.fromTick - (a.toTick - a.fromTick)).slice(0, MAX_ROWS) : rows;

  return (
    <div className="km-lives">
      <div className="km-lives-head">
        <h3>{partial ? t("Stretches the demo saw") : t("Lives")}</h3>
        <span className="hint">
          {rows.length > MAX_ROWS ? t("longest {MAX_ROWS} of {rows}", { MAX_ROWS: MAX_ROWS, rows: rows.length }) : t("{rows} in view", { rows: rows.length })}
        </span>
      </div>
      <div className={`km-lives-scroll${more.up ? " more-up" : ""}${more.down ? " more-down" : ""}`}>
      <button className="km-dorito up" onClick={() => page(-1)} hidden={!more.up} aria-label={t("Scroll up")} tabIndex={-1}>
      </button>
      <ol className="km-lives-list" ref={list} onScroll={measure}>
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
                <span className="km-life-end">
                  {r.died ? t("died") : partial ? t("lost sight") : t("survived")}
                  {r.caps.length > 0 && (
                    <span className="km-life-caps" title={t("{caps} points taken while alive", { caps: r.caps.length })}>
                      {tx("{0}· {caps} cap{2}", { "0": " ", "2": r.caps.length === 1 ? "" : "s", caps: r.caps.length })}
                    </span>
                  )}
                </span>
              </button>
              {r.caps.length > 0 && (
                <ol className="km-caps">
                  {r.caps.map(([at, point], i) => (
                    <li key={`${at}-${i}`}>
                      <span className="km-cap-n">{tx("{0} cap", { "0": ordinal(i + 1) })}</span>
                      <span className="km-cap-point">{tx("point {point}", { point: point })}</span>
                      <span className="km-cap-at" title={t("{0} into the life", { "0": clock(at) })}>{clock(at)}</span>
                    </li>
                  ))}
                </ol>
              )}
            </li>
          );
        })}
      </ol>
      <button className="km-dorito down" onClick={() => page(1)} hidden={!more.down} aria-label={t("Scroll down")} tabIndex={-1}>
      </button>
      </div>
      {offer}
      {focus && (
        <p className="hint">{tx("Showing one {0}. Click it again, or press Escape, for all of them.", { "0": focus.died ? t("life that ended in a death") : t("life") })}</p>
      )}
    </div>
  );
}
