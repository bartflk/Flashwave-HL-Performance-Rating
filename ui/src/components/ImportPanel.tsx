import { useState } from "react";
import { Help } from "./StatTip";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type FailedLog, type Imported } from "../api/types";
import { formatDate } from "../lib/format";
import { t, tx } from "../lib/i18n";
import { DemoImport } from "./DemoImport";
import { Spinner } from "./Spinner";
import { SortTh, useSort } from "../lib/sort";

/**
 * Logs that would not import, and a way to add one by hand.
 *
 * A sync ends with "2 failed; next sync retries them" and used to stop there:
 * no way to see which two, why, or do anything but wait. Asked for by
 * KamikaZe, September 2026, after two maps of a season would not sync.
 *
 * Three things are wanted, and they are all here: see what failed, make the
 * next sync try again, and — when logs.tf has the log under an id no index
 * ever listed — paste the link in and have it now.
 */
export function ImportPanel() {
  const qc = useQueryClient();
  const failed = useQuery({ queryKey: ["failed_logs"], queryFn: api.failedLogs });
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [got, setGot] = useState<Imported | null>(null);

  // Still being tried, and given up (Q77, Flashy): a log deleted from
  // logs.tf, or out of tries, is no longer asked for and waits out of the way.
  const rows = (failed.data ?? []).filter((r) => !r.givenUp);
  const givenUp = (failed.data ?? []).filter((r) => r.givenUp);
  const deleted = givenUp.filter((r) => r.gone).length;

  /** Everything that could have changed after a log arrives or is requeued. */
  async function refresh() {
    await Promise.all([
      qc.invalidateQueries({ queryKey: ["failed_logs"] }),
      qc.invalidateQueries({ queryKey: ["index_stats"] }),
      qc.invalidateQueries({ queryKey: ["matches"] }),
    ]);
  }

  async function add(what: string) {
    setBusy(true);
    setError(null);
    setGot(null);
    try {
      const imported = await api.importLog(what);
      setGot(imported);
      setText("");
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function retry(logId?: number) {
    setBusy(true);
    setError(null);
    try {
      await api.retryFailed(logId);
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel">
      <h2>{t("Logs that didn't import")}</h2>

      {rows.length === 0 ? (
        <p className="hint" style={{ marginTop: 6 }}>{givenUp.length === 0 ? t("Nothing has failed.") : t("Nothing is waiting to be tried again.")}</p>
      ) : (
        <>
          <p className="hint" style={{ marginTop: 6 }}>
            {tx("{0} would not download. Usually logs.tf was busy: the next syncs try again, then once a week, {1} times in all.", {
              "0": rows.length === 1 ? t("One log") : t("{rows} logs", { rows: rows.length }),
              "1": MAX_TRIES,
            })}
          </p>
          <FailedTable rows={rows} busy={busy} action={t("Try now")} onTry={(id) => void add(String(id))} />
          <button className="linkish" style={{ marginTop: 12 }} disabled={busy} onClick={() => void retry()}>{t("Let the next sync try these again")}</button>
        </>
      )}

      {givenUp.length > 0 && (
        <details className="failed-given-up">
          <summary>
            {tx("Given up: {0}", { "0": givenUp.length })}
            <span className="muted">
              {" · "}
              {deleted > 0 && t("{0} deleted on logs.tf", { "0": deleted })}
              {deleted > 0 && givenUp.length > deleted && ", "}
              {givenUp.length > deleted && t("{0} kept failing", { "0": givenUp.length - deleted })}
            </span>
          </summary>
          <p className="hint" style={{ marginTop: 8 }}>
            {t("Neither logs.tf nor drops.tf has a deleted log any more, and more.tf was asked for its copy once. These are not tried again; try one by hand if you know it is back.")}
          </p>
          <FailedTable rows={givenUp} busy={busy} action={t("Try anyway")} onTry={(id) => void add(String(id))} />
        </details>
      )}

      <h3 style={{ marginTop: 22 }}>{t("Add a log by hand")}<Help>{t("A log id or logs.tf link. Fetched now, not at the next sync.")}</Help></h3>
      <div className="row" style={{ marginTop: 10, gap: 8 }}>
        <input
          value={text}
          placeholder="https://logs.tf/4042136  or  4042136"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && text.trim() && void add(text)}
          style={{ flex: 1, minWidth: 0 }}
        />
        <button disabled={busy || !text.trim()} onClick={() => void add(text)}>
          {busy ? <><Spinner /> {t("Fetching…")}</> : t("Add it")}
        </button>
      </div>

      {got && (
        <p className="hint" style={{ marginTop: 10 }}>{tx("Added {0}, {players} players.{2}", { "0": <strong>{got.map ?? got.title ?? t("log {logId}", { logId: got.logId })}</strong>, "2": got.yours ? t(" It is in your matches now.") : t(" You are not in this one, so it joins the pool everyone is rated against rather than your match list."), players: got.players })}
          {got.partOf !== null && <> {t("It is part of combined log {0}, which is what your match list shows.", { "0": got.partOf })}</>}
        </p>
      )}
      {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}

      <DemoImport />
    </div>
  );
}

/** Every try a failing log gets, as the database counts them (MAX_FETCH_TRIES). */
const MAX_TRIES = 6;

function FailedTable({ rows: all, busy, action, onTry }: { rows: FailedLog[]; busy: boolean; action: string; onTry: (logId: number) => void }) {
  const sort = useSort("failed-logs", all, { played: (r) => r.playedAt, map: (r) => r.map ?? r.title ?? null, log: (r) => r.logId, tries: (r) => r.attempts, why: (r) => (r.gone ? "deleted" : r.error) });
  const rows = sort.sorted;
  return (
    <div className="table-wrap" style={{ marginTop: 12 }}>
      <table className="match-table">
        <thead>
          <tr>
            <SortTh k="played" s={sort}>{t("Played")}</SortTh>
            <SortTh k="map" s={sort}>{t("Map")}</SortTh>
            <SortTh k="log" s={sort} className="num">{t("Log")}</SortTh>
            <SortTh k="tries" s={sort} className="num">{t("Tries")}</SortTh>
            <SortTh k="why" s={sort}>{t("Why")}</SortTh>
            <th />
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.logId}>
              <td className="muted nowrap">{r.playedAt ? formatDate(r.playedAt, true) : "—"}</td>
              <td className="nowrap">{r.map ?? r.title ?? "—"}</td>
              <td className="num">
                <a href={`https://logs.tf/${r.logId}`} target="_blank" rel="noreferrer">
                  {r.logId}
                </a>
              </td>
              <td className="num" title={!r.givenUp && r.attempts >= 3 ? t("Tried again once a week") : undefined}>
                {r.givenUp ? r.attempts : `${r.attempts} / ${MAX_TRIES}`}
              </td>
              <td className="muted">{r.gone ? <span className="failed-gone">{t("Deleted on logs.tf")}</span> : r.error}</td>
              <td className="num">
                <button className="linkish" disabled={busy} onClick={() => onTry(r.logId)}>
                  {action}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
