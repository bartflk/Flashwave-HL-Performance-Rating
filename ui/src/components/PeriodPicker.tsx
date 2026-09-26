import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import { setPeriod, usePeriod } from "../lib/period";

/** A day as the date input wants it, `YYYY-MM-DD`, from unix seconds (UTC). */
const toInput = (t: number | null) =>
  // `new Date(NaN).toISOString()` throws a RangeError, and this value comes
  // from storage, so one bad number would blank the window on every load.
  t === null || !Number.isFinite(t) ? "" : new Date(t * 1000).toISOString().slice(0, 10);
/** The start (or end) of a picked day, in unix seconds. */
const fromInput = (v: string, end: boolean) => {
  if (!v) return null;
  const t = Date.parse(`${v}T00:00:00Z`);
  // A half-typed year parses to NaN in some browsers rather than "".
  return Number.isFinite(t) ? t / 1000 + (end ? 86_399 : 0) : null;
};

const shortDate = (t: number) =>
  new Date(t * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });

/**
 * All time, one season, or two dates of your choosing. Seasons come from
 * your ETF2L officials; each runs from the week before its first official
 * to the day after its last, and counts every game in it.
 */
export function PeriodPicker() {
  const period = usePeriod();
  const seasons = useQuery({ queryKey: ["seasons"], queryFn: api.listSeasons, staleTime: 60_000 });
  const list = seasons.data ?? [];

  const value = period.kind === "season" ? `s:${period.key}` : period.kind;

  return (
    <div className="period-picker">
      <label className="period-field">
        <span className="an-label">Period</span>
        <select
          value={value}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "all") setPeriod({ kind: "all" });
            else if (v === "custom") setPeriod({ kind: "custom", from: null, to: null });
            else {
              const s = list.find((x) => `s:${x.key}` === v);
              if (s) setPeriod({ kind: "season", key: s.key, name: s.name, from: s.from, to: s.to });
            }
          }}
        >
          <option value="all">All time</option>
          {list.length > 0 && (
            <optgroup label="Seasons">
              {list.map((s) => (
                <option key={s.key} value={`s:${s.key}`}>
                  {s.name}
                  {s.ongoing ? " (now)" : ""}
                </option>
              ))}
            </optgroup>
          )}
          <option value="custom">Custom dates…</option>
        </select>
      </label>
      {period.kind === "season" && (
        <span className="period-range hint">
          {shortDate(period.from)} – {shortDate(period.to)}
        </span>
      )}
      {period.kind === "custom" && (
        <span className="period-dates">
          <input
            type="date"
            aria-label="From"
            value={toInput(period.from)}
            onChange={(e) => setPeriod({ ...period, from: fromInput(e.target.value, false) })}
          />
          <span className="hint">to</span>
          <input
            type="date"
            aria-label="To"
            value={toInput(period.to)}
            onChange={(e) => setPeriod({ ...period, to: fromInput(e.target.value, true) })}
          />
        </span>
      )}
    </div>
  );
}
