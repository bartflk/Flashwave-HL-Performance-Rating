import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type MatchSummary } from "../api/types";
import { capitalize, formatDate, splitMap } from "../lib/format";

const PAGE = 50;

type View = "highlander" | "officials" | "all";

const VIEWS: Array<{ id: View; label: string }> = [
  { id: "highlander", label: "Highlander" },
  { id: "officials", label: "ETF2L officials" },
  { id: "all", label: "All formats" },
];

export function Matches({ onOpen }: { onOpen: (logId: number) => void }) {
  const [view, setView] = useState<View>("highlander");
  const [pages, setPages] = useState(1);

  const query = {
    format: view === "all" ? null : "highlander",
    officialsOnly: view === "officials",
    limit: PAGE * pages,
    offset: 0,
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
      <div className="matches-head">
        <div className="segmented" role="tablist">
          {VIEWS.map((v) => (
            <button
              key={v.id}
              role="tab"
              aria-selected={view === v.id}
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
        <span className="hint">
          {matches.isPending ? "Loading…" : `${total.toLocaleString()} match${total === 1 ? "" : "es"}`}
        </span>
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
                <th>Date</th>
                <th>Map</th>
                <th>Class</th>
                <th>Result</th>
                <th className="num">K / D / A</th>
                <th className="num">Dmg</th>
                <th className="num">DPM</th>
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

function MatchRow({ m, onOpen }: { m: MatchSummary; onOpen: (logId: number) => void }) {
  const me = m.me;
  const [mine, theirs] =
    me?.team === "Blue" ? [m.blueScore, m.redScore] : [m.redScore, m.blueScore];
  const dpm = me && me.timeS > 0 ? Math.round(me.dmg / (me.timeS / 60)) : null;
  const { mode, name } = splitMap(m.map);

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
      <td className="nowrap" title={m.map ?? "Map not recorded in the log"}>
        {mode && <span className={`mode mode-${mode}`}>{mode}</span>}
        <span className={name ? "" : "muted"}>{name ?? "unknown"}</span>
      </td>
      <td className="nowrap">{me?.mainClass ? capitalize(me.mainClass) : <span className="muted">—</span>}</td>
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
      <td className="title-cell">
        {m.league && <span className="badge badge-league">{m.league.toUpperCase()}</span>}
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
        <span className="muted">{m.title ?? `log ${m.logId}`}</span>
      </td>
    </tr>
  );
}
