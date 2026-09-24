import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type ContextKind, type MatchSummary } from "../api/types";
import { capitalize, formatDate, rating, splitMap } from "../lib/format";
import { bounds, usePeriod } from "../lib/period";
import { ContextBadge } from "./ContextBadge";
import { ClassIcon } from "./ClassIcon";
import { PeriodPicker } from "./PeriodPicker";

const PAGE = 50;

type View = "highlander" | ContextKind | "all";

const VIEWS: Array<{ id: View; label: string; hint?: string }> = [
  { id: "highlander", label: "Highlander" },
  { id: "official", label: "Officials", hint: "ETF2L officials" },
  { id: "scrim", label: "Scrims", hint: "Team games: most of your side are regular teammates or your ETF2L roster" },
  { id: "pug", label: "Pugs", hint: "Pugs, lobbies and mixes: a different team every game" },
  { id: "all", label: "All formats" },
];

/** The sortable columns, in table order. `null` sorts by date. */
const SORTS: Array<{ key: string; label: string; num?: boolean; title?: string }> = [
  { key: "date", label: "Date" },
  { key: "kills", label: "K / D / A", num: true, title: "Sort by kills" },
  { key: "dmg", label: "Dmg", num: true, title: "Sort by damage" },
  { key: "dpm", label: "DPM", num: true, title: "Sort by damage per minute" },
  { key: "rating", label: "Rating", num: true, title: "Sort by your rating on your main class" },
];

export function Matches({ onOpen }: { onOpen: (logId: number) => void }) {
  const [view, setView] = useState<View>("highlander");
  const [pages, setPages] = useState(1);
  // What you played on, and where: both narrow the whole history, not the
  // rows already on screen.
  const [cls, setCls] = useState<string | null>(null);
  const [map, setMap] = useState<string | null>(null);
  const filters = useQuery({ queryKey: ["played_filters"], queryFn: api.playedFilters, staleTime: 5 * 60_000 });
  const [sort, setSort] = useState<{ key: string; ascending: boolean }>({ key: "date", ascending: false });

  const kind = view === "official" || view === "scrim" || view === "pug" ? view : null;
  const period = usePeriod();
  const query = {
    format: view === "all" ? null : "highlander",
    kind,
    ...bounds(period),
    limit: PAGE * pages,
    offset: 0,
    sort: sort.key,
    ascending: sort.ascending,
    class: cls,
    map,
  };

  const matches = useQuery({
    queryKey: ["matches", query],
    queryFn: () => api.listMatches(query),
    // Keep showing the current rows while a refresh is in flight, so a live
    // sync does not blank the table every ten fetches.
    placeholderData: keepPreviousData,
  });

  const items = matches.data?.items ?? [];
  const total = matches.data?.total ?? 0;

  return (
    <section className="matches">
      {/* Every filter has a place and keeps it: the format on the left, the
          two dropdowns in the middle at a fixed width, the count on the
          right, and the classes on their own line underneath. Nothing moves
          when one of them is missing. */}
      <div className="filters">
        <div className="filter-row">
        <div className="segmented" role="tablist">
          {VIEWS.map((v) => (
            <button
              key={v.id}
              role="tab"
              aria-selected={view === v.id}
              title={v.hint}
              className={view === v.id ? "seg active" : "seg"}
              onClick={() => {
                setView(v.id);
                setPages(1);
              }}
            >
              {v.label}
            </button>
          ))}
        </div>
        <PeriodPicker />
        {(filters.data?.maps.length ?? 0) > 0 && (
          <label className="an-field fi-map">
            <span className="an-label">Map</span>
            <select
              value={map ?? ""}
              onChange={(e) => {
                setMap(e.target.value === "" ? null : e.target.value);
                setPages(1);
              }}
            >
              <option value="">Every map</option>
              {filters.data!.maps.map(([name, n]) => (
                <option key={name} value={name}>
                  {capitalize(name)} ({n})
                </option>
              ))}
            </select>
          </label>
        )}
        <span className="fi-count">
          {matches.isPending ? "Loading…" : `${total.toLocaleString()} match${total === 1 ? "" : "es"}`}
        </span>
        </div>

      {(filters.data?.classes.length ?? 0) > 0 && (
        <div className="class-filter" role="tablist" aria-label="Class">
          <button
            role="tab"
            aria-selected={cls === null}
            className={cls === null ? "cf active" : "cf"}
            onClick={() => {
              setCls(null);
              setPages(1);
            }}
          >
            All classes
          </button>
          {filters.data!.classes.map(([name, n]) => (
            <button
              key={name}
              role="tab"
              aria-selected={cls === name}
              className={cls === name ? "cf active" : "cf"}
              title={`${capitalize(name)}: ${n} match${n === 1 ? "" : "es"}`}
              onClick={() => {
                setCls(cls === name ? null : name);
                setPages(1);
              }}
            >
              <ClassIcon cls={name} size={20} />
              <span className="cf-n">{n}</span>
            </button>
          ))}
        </div>
      )}
      </div>

      {matches.isError && <p className="error">{errorMessage(matches.error)}</p>}

      {!matches.isPending && items.length === 0 && !matches.isError && (
        <div className="empty">
          <p>No matches yet.</p>
          <p className="hint">Press Sync to pull your history from trends.tf and logs.tf.</p>
        </div>
      )}

      {items.length > 0 && (
        <div className="table-wrap">
          <table className="match-table">
            <thead>
              <tr>
                <SortHead col={SORTS[0]} sort={sort} onSort={setSort} />
                <th>Map</th>
                <th>Class</th>
                <th>Result</th>
                {SORTS.slice(1).map((c) => (
                  <SortHead key={c.key} col={c} sort={sort} onSort={setSort} />
                ))}
                <th>Match</th>
              </tr>
            </thead>
            <tbody>
              {items.map((m) => (
                <MatchRow key={m.logId} m={m} onOpen={onOpen} />
              ))}
            </tbody>
          </table>
        </div>
      )}

      {items.length < total && (
        <button className="load-more" onClick={() => setPages((p) => p + 1)} disabled={matches.isFetching}>
          {matches.isFetching ? "Loading…" : `Show ${Math.min(PAGE, total - items.length)} more`}
        </button>
      )}
    </section>
  );
}

/** A column heading that sorts: click to use it, click again to flip it. */
function SortHead(props: {
  col: { key: string; label: string; num?: boolean; title?: string };
  sort: { key: string; ascending: boolean };
  onSort: (s: { key: string; ascending: boolean }) => void;
}) {
  const { col, sort, onSort } = props;
  const on = sort.key === col.key;
  return (
    <th
      className={`sortable${col.num ? " num" : ""}${on ? " sorted" : ""}`}
      title={col.title ?? "Sort by date"}
      aria-sort={on ? (sort.ascending ? "ascending" : "descending") : "none"}
      onClick={() => onSort({ key: col.key, ascending: on ? !sort.ascending : false })}
    >
      {col.label}
      {on && <span className="sort-arrow">{sort.ascending ? " ▴" : " ▾"}</span>}
    </th>
  );
}

function MatchRow({ m, onOpen }: { m: MatchSummary; onOpen: (logId: number) => void }) {
  const me = m.me;
  const [mine, theirs] =
    me?.team === "Blue" ? [m.blueScore, m.redScore] : [m.redScore, m.blueScore];
  const dpm = me && me.timeS > 0 ? Math.round(me.dmg / (me.timeS / 60)) : null;
  // A combined log's own map field is free text; its resolved maps are not.
  const maps = [...new Set(m.maps)];
  const { mode, name } = splitMap(maps.length === 1 ? maps[0] : m.map);

  return (
    <tr
      className="clickable"
      tabIndex={0}
      onClick={() => onOpen(m.logId)}
      onKeyDown={(e) => {
        if (e.key === "Enter") onOpen(m.logId);
      }}
    >
      <td className="muted nowrap">{formatDate(m.playedAt)}</td>
      <td className="nowrap" title={maps.length > 0 ? maps.join(", ") : m.map ?? "Map not recorded in the log"}>
        {maps.length > 1 ? (
          <span className="multi-map">
            {maps.map((x) => splitMap(x).name).join(" · ")}
          </span>
        ) : (
          <>
            {mode && <span className={`mode mode-${mode}`}>{mode}</span>}
            <span className={name ? "" : "muted"}>{name ?? "unknown"}</span>
          </>
        )}
      </td>
      <td className="nowrap class-cell">
        {me?.mainClass ? (
          <>
            <ClassIcon cls={me.mainClass} size={20} /> {capitalize(me.mainClass)}
          </>
        ) : (
          <span className="muted">—</span>
        )}
      </td>
      <td className="nowrap">
        {me ? (
          <span className={`result result-${me.result}`}>
            {me.result} <span className="score">{mine ?? "?"}–{theirs ?? "?"}</span>
          </span>
        ) : (
          <span className="muted">not in log</span>
        )}
      </td>
      <td className="num nowrap">{me ? `${me.kills} / ${me.deaths} / ${me.assists}` : ""}</td>
      <td className="num">{me ? me.dmg.toLocaleString() : ""}</td>
      <td className="num">{dpm ?? ""}</td>
      <td className="num">
        {m.rating === null ? <span className="muted">–</span> : <span className="sb-rating">{rating(m.rating)}</span>}
      </td>
      <td className="title-cell">
        {m.context ? (
          <ContextBadge c={m.context} />
        ) : (
          m.league && <span className="badge badge-league">{m.league.toUpperCase()}</span>
        )}
        {m.parts > 0 && (
          <span className="badge badge-parts" title={`Combined from ${m.parts} logs; open the match to see them`}>
            {m.parts} {m.parts === 1 ? "log" : "logs"}
          </span>
        )}
        {m.hasDemo && (
          <span className="badge badge-pov" title="Your recording of this match is on this machine">
            POV
          </span>
        )}
        {m.demosTfId && (
          <span className="badge badge-demo" title={`STV demo on demos.tf (#${m.demosTfId})`}>
            STV
          </span>
        )}
        {m.context?.oppName ? (
          <span title={m.title ?? undefined}>
            <span className="muted">vs</span> {m.context.oppName}
          </span>
        ) : (
          <span className="muted">{m.title ?? `log ${m.logId}`}</span>
        )}
      </td>
    </tr>
  );
}
