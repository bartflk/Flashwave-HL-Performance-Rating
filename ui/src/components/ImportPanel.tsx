import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type Imported } from "../api/types";
import { formatDate } from "../lib/format";

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
      <h2>Logs that didn&apos;t import</h2>

      {rows.length === 0 ? (
        <p className="hint" style={{ marginTop: 6 }}>
          Nothing has failed. A log that will not download after three tries appears here with the
          reason, so a failed sync is something you can look at rather than a number.
        </p>
      ) : (
        <>
          <p className="hint" style={{ marginTop: 6 }}>
            {rows.length === 1 ? "One log" : `${rows.length} logs`} would not download. Most of the
            time logs.tf was busy and trying again is enough; a log it does not have will keep
            failing, and that is logs.tf missing it rather than anything here.
          </p>
          <div className="table-wrap" style={{ marginTop: 12 }}>
            <table className="match-table">
              <thead>
                <tr>
                  <th>Played</th>
                  <th>Map</th>
                  <th className="num">Log</th>
                  <th className="num">Tries</th>
                  <th>Why</th>
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
                      <button className="linkish" disabled={busy} onClick={() => void add(String(r.logId))}>
                        Try now
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <button className="linkish" style={{ marginTop: 12 }} disabled={busy} onClick={() => void retry()}>
            Let the next sync try all of them again
          </button>
        </>
      )}

      <h3 style={{ marginTop: 22 }}>Add a log by hand</h3>
      <p className="hint" style={{ marginTop: 6 }}>
        A log id or a logs.tf link. Useful for a match no index ever listed — a pug, or a log
        uploaded under a second id. It is fetched now, not at the next sync.
      </p>
      <div className="row" style={{ marginTop: 10, gap: 8 }}>
        <input
          value={text}
          placeholder="https://logs.tf/4042136  or  4042136"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && text.trim() && void add(text)}
          style={{ flex: 1, minWidth: 0 }}
        />
        <button disabled={busy || !text.trim()} onClick={() => void add(text)}>
          {busy ? "Fetching…" : "Add it"}
        </button>
      </div>

      {got && (
        <p className="hint" style={{ marginTop: 10 }}>
          Added <strong>{got.map ?? got.title ?? `log ${got.logId}`}</strong>, {got.players} players.
          {got.yours ? " It is in your matches now." : " You are not in this one, so it joins the pool everyone is rated against rather than your match list."}
        </p>
      )}
      {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}
    </div>
  );
}
