import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type Imported } from "../api/types";
import { formatDate } from "../lib/format";
import { t, tx } from "../lib/i18n";
import { DemoImport } from "./DemoImport";

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

  const rows = failed.data ?? [];

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
        <p className="hint" style={{ marginTop: 6 }}>{t("Nothing has failed.")}</p>
      ) : (
        <>
          <p className="hint" style={{ marginTop: 6 }}>
            {tx("{0} would not download. Usually logs.tf was busy — try again.", { "0": rows.length === 1 ? t("One log") : t("{rows} logs", { rows: rows.length }) })}</p>
          <div className="table-wrap" style={{ marginTop: 12 }}>
            <table className="match-table">
              <thead>
                <tr>
                  <th>{t("Played")}</th>
                  <th>{t("Map")}</th>
                  <th className="num">{t("Log")}</th>
                  <th className="num">{t("Tries")}</th>
                  <th>{t("Why")}</th>
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
                    <td className="num">{r.attempts}</td>
                    <td className="muted">{r.error}</td>
                    <td className="num">
                      <button className="linkish" disabled={busy} onClick={() => void add(String(r.logId))}>{t("Try now")}</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <button className="linkish" style={{ marginTop: 12 }} disabled={busy} onClick={() => void retry()}>{t("Let the next sync try all of them again")}</button>
        </>
      )}

      <h3 style={{ marginTop: 22 }}>{t("Add a log by hand")}</h3>
      <p className="hint" style={{ marginTop: 6 }}>{t("A log id or logs.tf link. Fetched now, not at the next sync.")}</p>
      <div className="row" style={{ marginTop: 10, gap: 8 }}>
        <input
          value={text}
          placeholder="https://logs.tf/4042136  or  4042136"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && text.trim() && void add(text)}
          style={{ flex: 1, minWidth: 0 }}
        />
        <button disabled={busy || !text.trim()} onClick={() => void add(text)}>
          {busy ? t("Fetching…") : t("Add it")}
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
