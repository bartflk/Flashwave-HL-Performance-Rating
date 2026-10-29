import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type StorageStray } from "../../api/types";
import { formatDate } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { Spinner } from "../Spinner";

/** Bytes as people read them: 812 MB, 2.3 GB. */
export function size(bytes: number): string {
  return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(bytes / 1e6))} MB`;
}

const STRAY: Record<StorageStray["kind"], string> = {
  partial: "A copy a sync was cut off writing",
  unreadable: "A database that would not open, set aside",
  replaced: "The database a restore replaced",
};

/** A button that asks once more before it acts (and forgets after a few seconds). */
function Twice({ label, confirm, onConfirm, disabled }: { label: string; confirm: string; onConfirm: () => void; disabled?: boolean }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const id = window.setTimeout(() => setArmed(false), 4000);
    return () => window.clearTimeout(id);
  }, [armed]);
  return (
    <button
      className={armed ? "bk-danger" : undefined}
      disabled={disabled}
      onClick={() => {
        if (armed) {
          setArmed(false);
          onConfirm();
        } else setArmed(true);
      }}
    >
      {armed ? confirm : label}
    </button>
  );
}

/**
 * Copies of the database, and what everything the app keeps takes (Flashy,
 * October 2026: a tester's folder had reached 14 GB, and nothing here said
 * so). A copy is taken before every sync and rebuild, without the league
 * officials' logs, which download again; the newest and one a day before it
 * are kept, within a budget; what an interrupted copy left behind is listed
 * and cleared.
 */
export function BackupsPanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["storage_overview"], queryFn: api.getStorageOverview });
  const [measure, setMeasure] = useState(false);
  const parts = useQuery({ queryKey: ["storage_breakdown"], queryFn: api.getStorageBreakdown, enabled: measure, staleTime: 10 * 60_000 });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  async function act(work: () => Promise<unknown>, done?: (r: unknown) => string | null) {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      const r = await work();
      setNote(done ? done(r) : null);
      await qc.invalidateQueries({ queryKey: ["storage_overview"] });
      await qc.invalidateQueries({ queryKey: ["backups"] });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  /** A name that says what it is and when, so a folder of them sorts. */
  function suggestedName() {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    return `flashwave-${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}.sqlite3`;
  }

  const o = q.data;
  const total = o ? o.database + o.backupsBytes + o.straysBytes : 0;
  const share = (n: number) => `${total > 0 ? Math.max(0.5, (n / total) * 100) : 0}%`;
  const partsTotal = (parts.data ?? []).reduce((n, p) => n + p.bytes, 0);

  return (
    <div className="panel bk">
      <h2>{t("Backups")}</h2>
      <p className="hint">
        {t(
          "A copy is taken before every sync and rebuild: the whole database except the league officials' logs, which download again on their own. The newest copy is kept, and one a day before it, five at most and {0} together.",
          { "0": o ? size(o.budget) : "3 GB" },
        )}
      </p>
      <p className="hint">{tx("{0} Keep a copy on another drive.", { "0": <strong>{t("Uninstalling can delete these.")}</strong> })}</p>

      {q.isPending && <Spinner />}
      {o && (
        <>
          <div className="bk-total">
            <strong>{size(total)}</strong> <span className="muted">{t("on this computer")}</span>
          </div>
          <div className="bk-bar" role="img" aria-label={t("Disk space: the database, its copies and what was left behind")}>
            <span className="bk-db" style={{ width: share(o.database) }} />
            <span className="bk-copies" style={{ width: share(o.backupsBytes) }} />
            {o.straysBytes > 0 && <span className="bk-strays" style={{ width: share(o.straysBytes) }} />}
          </div>
          <ul className="bk-legend">
            <li>
              <i className="bk-db" /> {t("Database")} <strong>{size(o.database)}</strong>
            </li>
            <li>
              <i className="bk-copies" /> {t("{0} copies", { "0": o.backups.length })} <strong>{size(o.backupsBytes)}</strong>
            </li>
            {o.strays.length > 0 && (
              <li>
                <i className="bk-strays" /> {t("Left behind")} <strong>{size(o.straysBytes)}</strong>
              </li>
            )}
          </ul>
        </>
      )}

      <div className="row bk-actions">
        <button onClick={() => void act(() => api.backupNow())} disabled={busy}>
          {busy ? t("Working…") : t("Back up now")}
        </button>
        <button onClick={() => void act(() => api.saveBackupAs(suggestedName()), (b) => (b ? t("Written to {0}, whole, the league's logs included.", { "0": (b as { path: string }).path }) : null))} disabled={busy}>
          {t("Save a whole copy elsewhere…")}
        </button>
        {o && (
          <button className="linkish" onClick={() => void api.revealPath(o.dir)}>
            {t("Show in Explorer")}
          </button>
        )}
      </div>
      {note && <p className="hint bk-note">{note}</p>}
      {error && <p className="error bk-note">{error}</p>}

      {o && o.backups.length === 0 && <p className="hint bk-note">{t("No copies yet. The next sync makes one.")}</p>}
      {o && o.backups.length > 0 && (
        <table className="bk-table">
          <thead>
            <tr>
              <th>{t("Copy")}</th>
              <th className="num">{t("Size")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {o.backups.map((b, i) => (
              <tr key={b.path}>
                <td>
                  {formatDate(b.madeAt, true)} {i === 0 && <span className="bk-tag">{t("newest")}</span>}
                </td>
                <td className="num">{size(b.bytes)}</td>
                <td className="bk-row-actions">
                  <Twice
                    label={t("Restore…")}
                    confirm={t("Restart and restore")}
                    disabled={busy}
                    onConfirm={() => void act(() => api.restoreBackup(b.path))}
                  />
                  <Twice label={t("Delete")} confirm={t("Delete for good?")} disabled={busy} onConfirm={() => void act(() => api.deleteBackup(b.path), (n) => t("{0} freed.", { "0": size(n as number) }))} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {o && o.strays.length > 0 && (
        <>
          <h3 className="bk-sub">{t("Left behind")}</h3>
          <p className="hint">{t("Not copies the app keeps. A half-written copy is cleared an hour after it was left; a database set aside stays until you delete it.")}</p>
          <table className="bk-table">
            <tbody>
              {o.strays.map((s) => (
                <tr key={s.path}>
                  <td>
                    {t(STRAY[s.kind])} <span className="muted">{formatDate(s.madeAt, true)}</span>
                  </td>
                  <td className="num">{size(s.bytes)}</td>
                  <td className="bk-row-actions">
                    <Twice label={t("Delete")} confirm={t("Delete for good?")} disabled={busy} onConfirm={() => void act(() => api.deleteBackup(s.path), (n) => t("{0} freed.", { "0": size(n as number) }))} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}

      <h3 className="bk-sub">{t("What the database holds")}</h3>
      {!measure ? (
        <button onClick={() => setMeasure(true)}>{t("Measure it")}</button>
      ) : parts.isPending ? (
        <p className="hint">
          <Spinner /> {t("Reading the whole database: a few seconds.")}
        </p>
      ) : parts.isError ? (
        <p className="error">{errorMessage(parts.error)}</p>
      ) : (
        <table className="bk-table bk-parts">
          <tbody>
            {(parts.data ?? []).map((p) => (
              <tr key={p.key}>
                <td>
                  <div>{t(p.label)}</div>
                  <div className="muted bk-tables">{p.tables.slice(0, 3).map((x) => x[0]).join(", ")}</div>
                </td>
                <td className="bk-share">
                  <span className="bk-meter">
                    <span style={{ width: `${partsTotal > 0 ? (p.bytes / partsTotal) * 100 : 0}%` }} className={p.inBackups ? "" : "out"} />
                  </span>
                </td>
                <td className="num">{size(p.bytes)}</td>
                <td>
                  {p.inBackups ? (
                    <span className="bk-tag">{t("in copies")}</span>
                  ) : (
                    <span className="bk-tag out" title={t("Left out of the copies: after a restore it downloads again over the next syncs.")}>
                      {t("left out, downloads again")}
                    </span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
