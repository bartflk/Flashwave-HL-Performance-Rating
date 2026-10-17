import { useState } from "react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type ContextKind, type MatchSummary } from "../api/types";
import { capitalize, formatDate, rating, splitMap } from "../lib/format";
import { startSync, useSyncStatus } from "../lib/sync";
import { bounds, usePeriod } from "../lib/period";
import { KindPicker } from "./ContextBadge";
import { ClassIcon } from "./ClassIcon";
import { PeriodPicker } from "./PeriodPicker";
import { t, k } from "../lib/i18n";
import { Refreshing, Spinner } from "./Spinner";

const PAGE = 50;

type View = "highlander" | ContextKind | "all";

const VIEWS: Array<{ id: View; label: string; hint?: string }> = [
  { id: "highlander", label: k("Highlander") },
  { id: "official", label: k("Officials"), hint: k("ETF2L officials") },
  { id: "scrim", label: k("Scrims"), hint: k("Team games: most of your side are regular teammates or your ETF2L roster") },
  { id: "pug", label: k("Pugs"), hint: k("Pugs, lobbies and mixes: a different team every game") },
  { id: "all", label: k("All formats") },
];

/** The sortable columns, in table order. `null` sorts by date. */
const SORTS: Array<{ key: string; label: string; num?: boolean; title?: string }> = [
  { key: "date", label: k("Date") },
  { key: "kills", label: "K / D / A", num: true, title: k("Sort by kills") },
  { key: "dmg", label: k("Dmg"), num: true, title: k("Sort by damage") },
  { key: "dpm", label: k("DPM"), num: true, title: k("Sort by damage per minute") },
  { key: "rating", label: k("Rating"), num: true, title: k("Sort by your rating on your main class") },
];

export function Matches({ onOpen }: { onOpen: (logId: number) => void }) {
  const [view, setView] = useState<View>("highlander");
  const [pages, setPages] = useState(1);
  // What you played on, and where: both narrow the whole history, not the
  // rows already on screen.
  const [cls, setCls] = useState<string | null>(null);
  const [map, setMap] = useState<string | null>(null);
  const [sort, setSort] = useState<{ key: string; ascending: boolean }>({ key: "date", ascending: false });
  // Q56 (Emiel): straight to the matches with everyone's movement and aim.
  const [stvOnly, setStvOnly] = useState(false);
  const classSort = parseClassSort(sort.key);

  const kind = view === "official" || view === "scrim" || view === "pug" ? view : null;
  const period = usePeriod();
  // Q59 (Emiel): each class's and map's count is of what the other filters
  // show, not of every log.
  const counted = { format: view === "all" ? null : "highlander", kind, ...bounds(period), class: cls, map, stvOnly };
  const filters = useQuery({
    queryKey: ["played_filters", counted],
    queryFn: () => api.playedFilters(counted),
    staleTime: 5 * 60_000,
    placeholderData: keepPreviousData,
  });
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
    stvOnly,
  };

  const matches = useQuery({
    queryKey: ["matches", query],
    queryFn: () => api.listMatches(query),
    // Keep showing the current rows while a refresh is in flight, so a live
    // sync does not blank the table every ten fetches.
    placeholderData: keepPreviousData,
  });

  // Refresh means "is the game I just played here yet": a sync, which asks
  // logs.tf for new logs. The list refreshes itself as they land and again
  // when it finishes.
  const syncing = useSyncStatus().state === "running";
  const refresh = async () => {
    if (!(await api.syncBusy())) await startSync(false);
  };

  // What is picked stays on offer when the other filters leave it at none,
  // so it can still be seen and unpicked.
  const keep = (rows: Array<[string, number]> | undefined, picked: string | null) =>
    !rows ? rows : picked && !rows.some(([n]) => n === picked) ? [...rows, [picked, 0] as [string, number]] : rows;
  const mapRows = keep(filters.data?.maps, map) ?? [];
  const classRows = keep(filters.data?.classes, cls) ?? [];

  const items = matches.data?.items ?? [];
  // Q70: the previous rows stay up while a new sort or filter loads; dimmed
  // and labelled, so they are not taken for the result.
  const stale = matches.isPlaceholderData;
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
              title={t(v.hint)}
              className={view === v.id ? "seg active" : "seg"}
              onClick={() => {
                setView(v.id);
                setPages(1);
              }}
            >
              {t(v.label)}
            </button>
          ))}
        </div>
        <PeriodPicker />
        {mapRows.length > 0 && (
          <label className="an-field fi-map">
            <span className="an-label">{t("Map")}</span>
            <select
              value={map ?? ""}
              onChange={(e) => {
                setMap(e.target.value === "" ? null : e.target.value);
                setPages(1);
              }}
            >
              <option value="">{t("Every map")}</option>
              {mapRows.map(([name, n]) => (
                <option key={name} value={name}>
                  {capitalize(name)} ({n})
                </option>
              ))}
            </select>
          </label>
        )}
        <span className="fi-count">
          {matches.isPending ? <><Spinner /> {t("Loading…")}</> : stale ? <><Spinner /> {t("Sorting…")}</> : t("{0} match{1}", { "0": total.toLocaleString(), "1": total === 1 ? "" : "es" })}
        </span>
        <div className="fi-actions">
        <button
          className={syncing ? "km-chip fi-refresh busy" : "km-chip fi-refresh"}
          onClick={() => void refresh()}
          disabled={syncing}
          title={t("Look for new matches now. A log can take a minute or two to show up on logs.tf after the game.")}
        >
          <span className="fi-refresh-icon" aria-hidden>↻</span>
          {syncing ? t("Syncing…") : t("Refresh")}
        </button>
        <ImportLog onOpen={onOpen} />
        </div>
        </div>

      {classRows.length > 0 && (
        <div className="class-filter" role="tablist" aria-label={t("Class")}>
          <button
            role="tab"
            aria-selected={cls === null}
            className={cls === null ? "cf active" : "cf"}
            onClick={() => {
              setCls(null);
              setPages(1);
            }}
          >{t("All classes")}</button>
          {classRows.map(([name, n]) => (
            <button
              key={name}
              role="tab"
              aria-selected={cls === name}
              className={cls === name ? "cf active" : "cf"}
              title={t("{0}: {n} match{2}", { "0": capitalize(name), "2": n === 1 ? "" : "es", n: n })}
              onClick={() => {
                setCls(cls === name ? null : name);
                setPages(1);
              }}
            >
              <ClassIcon cls={name} size={20} />
              <span className="cf-n">{n}</span>
            </button>
          ))}
          <ClassSortPicker sort={sort} onSort={setSort} />
          <button
            className={stvOnly ? "km-chip on cf-stv" : "km-chip cf-stv"}
            aria-pressed={stvOnly}
            title={t("Only matches with their SourceTV demo on this machine: everyone's movement, aim and spychecks")}
            onClick={() => {
              setStvOnly((s) => !s);
              setPages(1);
            }}
          >
            {t("With STV")}
          </button>
        </div>
      )}
      </div>

      {matches.isError && <p className="error">{errorMessage(matches.error)}</p>}

      {!matches.isPending && items.length === 0 && !matches.isError && (
        <div className="empty">
          {stvOnly ? (
            <>
              <p>{t("No match here has its SourceTV demo on this machine.")}</p>
              <p className="hint">{t("Download one from a match's Demo linking panel, or turn off With STV.")}</p>
            </>
          ) : (
            <>
              <p>{t("No matches yet.")}</p>
              <p className="hint">{t("Press Sync to pull your history from trends.tf and logs.tf.")}</p>
            </>
          )}
        </div>
      )}

      {items.length > 0 && (
        <div className={stale ? "table-wrap stale has-refresh" : "table-wrap has-refresh"} aria-busy={stale}>
          <Refreshing on={stale} />
          <table className="match-table">
            <thead>
              <tr>
                <SortHead col={SORTS[0]} sort={sort} onSort={setSort} />
                <th>{t("Map")}</th>
                <th>{t("Class")}</th>
                <th>{t("Result")}</th>
                {classSort && <SortHead col={{ key: sort.key, label: classSortLabel(classSort), num: true, title: classSortLabel(classSort) }} sort={sort} onSort={setSort} />}
                {SORTS.slice(1).map((c) => (
                  <SortHead key={c.key} col={c} sort={sort} onSort={setSort} />
                ))}
                <th>{t("Match")}</th>
              </tr>
            </thead>
            <tbody>
              {items.map((m) => (
                <MatchRow key={m.logId} m={m} onOpen={onOpen} classCol={!!classSort} />
              ))}
            </tbody>
          </table>
        </div>
      )}

      {items.length < total && (
        <button className="load-more" onClick={() => setPages((p) => p + 1)} disabled={matches.isFetching}>
          {matches.isFetching ? <><Spinner /> {t("Loading…")}</> : t("Show {0} more", { "0": Math.min(PAGE, total - items.length) })}
        </button>
      )}
    </section>
  );
}

/** The nine classes as the raw logs name them, in the scoreboard's order. */
const NINE = ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"];

/** Q61: a `killed:<class>` or `diedto:<class>` sort, read back. */
type ClassSort = { how: "killed" | "diedto"; cls: string };

function parseClassSort(key: string): ClassSort | null {
  const m = /^(killed|diedto):(\w+)$/.exec(key);
  return m ? { how: m[1] as ClassSort["how"], cls: m[2] } : null;
}

function classSortLabel(s: ClassSort): string {
  return s.how === "killed" ? t("Kills on {0}", { "0": capitalize(s.cls) }) : t("Deaths to {0}", { "0": capitalize(s.cls) });
}

/**
 * Q61 (Clark): sort by kills on one class or deaths to one -- "the log
 * where I killed the most Soldiers", "where a Sniper killed me most".
 */
function ClassSortPicker({ sort, onSort }: { sort: { key: string; ascending: boolean }; onSort: (s: { key: string; ascending: boolean }) => void }) {
  return (
    <label className="cf-sort" title={t("Sort by your kills on one class, or your deaths to one, from the server logs")}>
      <select value={parseClassSort(sort.key) ? sort.key : ""} onChange={(e) => onSort({ key: e.target.value || "date", ascending: false })}>
        <option value="">{t("Sort by a class…")}</option>
        <optgroup label={t("Most kills on")}>
          {NINE.map((c) => (
            <option key={c} value={`killed:${c}`}>{classSortLabel({ how: "killed", cls: c })}</option>
          ))}
        </optgroup>
        <optgroup label={t("Most deaths to")}>
          {NINE.map((c) => (
            <option key={c} value={`diedto:${c}`}>{classSortLabel({ how: "diedto", cls: c })}</option>
          ))}
        </optgroup>
      </select>
    </label>
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
      title={col.title ? t(col.title) : t("Sort by date")}
      aria-sort={on ? (sort.ascending ? "ascending" : "descending") : "none"}
      onClick={() => onSort({ key: col.key, ascending: on ? !sort.ascending : false })}
    >
      {t(col.label)}
      {on && <span className="sort-arrow">{sort.ascending ? " ▴" : " ▾"}</span>}
    </th>
  );
}

/**
 * Q58 (Flashy): import a log from here, not only from Settings. A log id
 * or logs.tf link, fetched now; a log you are in opens straight away.
 */
function ImportLog({ onOpen }: { onOpen: (logId: number) => void }) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  // Opens toward the room it has: leftward from the row's right end, or
  // rightward when a narrow window has wrapped the button to the left.
  const [toRight, setToRight] = useState(false);

  async function add() {
    if (!text.trim()) return;
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      const got = await api.importLog(text.trim());
      setText("");
      void qc.invalidateQueries({ queryKey: ["matches"] });
      void qc.invalidateQueries({ queryKey: ["index_stats"] });
      void qc.invalidateQueries({ queryKey: ["failed_logs"] });
      if (got.yours) {
        setOpen(false);
        // A part of a combined log is shown as the combined one (Q71).
        onOpen(got.partOf ?? got.logId);
      } else {
        setNote(t("Added. You are not in this one, so it joins the pool everyone is rated against rather than your match list."));
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fi-import">
      <button
        className={open ? "km-chip on" : "km-chip"}
        onClick={(e) => {
          setToRight(e.currentTarget.getBoundingClientRect().right < 420);
          setOpen((o) => !o);
        }}
        aria-expanded={open}
        title={t("Add a log by its id or logs.tf link")}
      >
        <span aria-hidden>＋</span> {t("Import")}
      </button>
      {open && (
        <div className={toRight ? "fi-import-pop to-right" : "fi-import-pop"} role="dialog" aria-label={t("Import a log")}>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void add();
            }}
          >
            <input autoFocus value={text} placeholder="https://logs.tf/4042136" onChange={(e) => setText(e.target.value)} disabled={busy} onKeyDown={(e) => e.key === "Escape" && setOpen(false)} />
            <button type="submit" className="primary" disabled={busy || !text.trim()}>{busy ? <><Spinner /> {t("Fetching…")}</> : t("Add it")}</button>
          </form>
          <span className="hint">{t("A log id or logs.tf link. Fetched now, not at the next sync.")}</span>
          {note && <span className="hint">{note}</span>}
          {error && <span className="error">{error}</span>}
        </div>
      )}
    </div>
  );
}

function MatchRow({ m, onOpen, classCol }: { m: MatchSummary; onOpen: (logId: number) => void; classCol: boolean }) {
  const me = m.me;
  const [rMine, rTheirs] =
    me?.team === "Blue" ? [m.blueScore, m.redScore] : [m.redScore, m.blueScore];
  // Q55 (Clark): an official shows ETF2L's result; the logs' rounds, which
  // in stopwatch are not the same, are in its tooltip.
  const etf2l = m.context?.kind === "official" ? m.context.official?.score ?? null : null;
  const [mine, theirs] = etf2l ?? [rMine, rTheirs];
  const result = etf2l ? (etf2l[0] > etf2l[1] ? "W" : etf2l[0] < etf2l[1] ? "L" : "T") : me?.result;
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
      <td className="nowrap" title={maps.length > 0 ? maps.join(", ") : m.map ?? t("Map not recorded in the log")}>
        {maps.length > 1 ? (
          <span className="multi-map">
            {maps.map((x) => splitMap(x).name).join(" · ")}
          </span>
        ) : (
          <>
            {mode && <span className={`mode mode-${mode}`}>{mode}</span>}
            <span className={name ? "" : "muted"}>{name ?? t("unknown")}</span>
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
          <span className={`result result-${result}`}>
            {result}{" "}
            <span className="score" title={etf2l ? t("ETF2L's result; rounds in the logs {0}–{1}", { "0": rMine ?? "?", "1": rTheirs ?? "?" }) : undefined}>
              {mine ?? "?"}–{theirs ?? "?"}
            </span>
          </span>
        ) : (
          <span className="muted">{t("not in log")}</span>
        )}
      </td>
      {classCol && (
        <td className="num" title={m.classCount === null ? t("No server log for this match yet") : undefined}>
          {m.classCount ?? <span className="muted">—</span>}
        </td>
      )}
      <td className="num nowrap">{me ? `${me.kills} / ${me.deaths} / ${me.assists}` : ""}</td>
      <td className="num">{me ? me.dmg.toLocaleString() : ""}</td>
      <td className="num">{dpm ?? ""}</td>
      <td className="num">
        {m.rating === null ? <span className="muted">–</span> : <span className="sb-rating">{rating(m.rating)}</span>}
      </td>
      <td className="title-cell">
        {m.context ? (
          <KindPicker logId={m.logId} c={m.context} />
        ) : (
          m.league && <span className="badge badge-league">{m.league.toUpperCase()}</span>
        )}
        {m.parts > 0 && (
          <span className="badge badge-parts" title={t("Combined from {parts} logs; open the match to see them", { parts: m.parts })}>
            {m.parts} {m.parts === 1 ? t("log") : t("logs")}
          </span>
        )}
        {m.hasDemo && (
          <span className="badge badge-pov" title={t("Your recording of this match is on this machine")}>{t("POV")}</span>
        )}
        {m.demosTfId && (
          <span className="badge badge-demo" title={t("STV demo on demos.tf (#{demosTfId})", { demosTfId: m.demosTfId })}>{t("STV")}</span>
        )}
        {m.context?.oppName ? (
          <span title={m.title ?? undefined}>
            <span className="muted">{t("vs")}</span> {m.context.oppName}
          </span>
        ) : (
          <span className="muted">{m.title ?? t("log {logId}", { logId: m.logId })}</span>
        )}
      </td>
    </tr>
  );
}
