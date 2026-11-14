import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { ModelShift } from "../../api/types";
import { diagnosticsText, jobName, logTail, sourceName, sqliteTime } from "../../lib/diagnostics";
import { copy } from "../../lib/toast";
import { t, tx } from "../../lib/i18n";
import { formatDate, rating, signed, splitMap } from "../../lib/format";
import { startRerate, startRereadDemos, startSync } from "../../lib/sync";
import { ClassIcon } from "../ClassIcon";
import { ApiError } from "../ApiError";
import { Spinner } from "../Spinner";
import { SortTh, useSort } from "../../lib/sort";

/**
 * The tools a rebuild was the only one of (audit, 12 Nov 2026): the rating
 * pass alone, which takes seconds where a rebuild re-reads every log first,
 * and a sync through every source's whole history, which the backend could
 * always do and nothing in the window asked for.
 */
export function DataJobs({ busy }: { busy: boolean }) {
  const [armed, setArmed] = useState(false);
  return (
    <>
      <button onClick={() => void startRerate()} disabled={busy}>
        {t("Re-rate only")}
      </button>
      {armed ? (
        <>
          <button
            className="bk-danger"
            onClick={() => {
              setArmed(false);
              void startSync(true);
            }}
          >
            {t("Sync the whole history now")}
          </button>
          <button className="linkish" onClick={() => setArmed(false)}>{t("Not now")}</button>
        </>
      ) : (
        <button onClick={() => setArmed(true)} disabled={busy}>
          {t("Full sync")}
        </button>
      )}
    </>
  );
}

/** Settings > Demos: every demo read again, not only the ones from an older version. */
export function RereadDemosButton({ busy }: { busy: boolean }) {
  const [armed, setArmed] = useState(false);
  if (!armed) {
    return (
      <button onClick={() => setArmed(true)} disabled={busy}>
        {t("Read every demo again")}
      </button>
    );
  }
  return (
    <>
      <button
        className="bk-danger"
        onClick={() => {
          setArmed(false);
          void startRereadDemos();
        }}
      >
        {t("Read them all now")}
      </button>
      <button className="linkish" onClick={() => setArmed(false)}>{t("Not now")}</button>
      <span className="hint">{t("About 16 seconds a demo. You can stop it from the card in the corner.")}</span>
    </>
  );
}

/**
 * Settings > Diagnostics (audit, 12 Nov 2026): what a tester can send when
 * something is off. A release build has no console, and every warning the
 * backend wrote went to it; they go to a log file now, and this is where to
 * find it, with each source's last sync and the last twenty runs, which used
 * to be gone once their card was dismissed.
 */
export function DiagnosticsPanel() {
  const q = useQuery({ queryKey: ["diagnostics"], queryFn: api.getDiagnostics });
  const [copied, setCopied] = useState(false);
  const d = q.data;
  const runSort = useSort("diag-runs", d?.runs ?? [], { when: (r) => r.at, what: (r) => jobName(r.kind), took: (r) => r.seconds });

  if (q.error) return <div className="panel"><ApiError error={q.error} /></div>;
  if (!d) return <div className="panel"><p className="hint loading"><Spinner /> {t("Loading…")}</p></div>;

  return (
    <div className="panel">
      <h2>{t("Diagnostics")}</h2>
      <div className="row" style={{ marginTop: 4, gap: 10, flexWrap: "wrap" }}>
        <button
          onClick={() => {
            void logTail().then((tail) => {
              const text = diagnosticsText(d) + (tail ? `\n\nLog (last lines):\n${tail}` : "");
              void navigator.clipboard?.writeText(text);
              setCopied(true);
              window.setTimeout(() => setCopied(false), 2000);
            });
          }}
        >
          {copied ? t("Copied") : t("Copy diagnostics")}
        </button>
        <button onClick={() => void api.revealPath(d.logFile ?? d.logDir)}>{t("Open the log folder")}</button>
        <button className="linkish" onClick={() => void q.refetch()}>{t("Refresh")}</button>
      </div>

      <dl className="kv" style={{ marginTop: 16 }}>
        <dt>{t("Version")}</dt>
        <dd>{d.version}</dd>
        <dt>{t("Rating model")}</dt>
        <dd>
          {d.model}
          {d.rated.length > 0 && (
            <span className="muted"> · {d.rated.map(([v, n]) => t("{0}: {1} games", { "0": v, "1": n.toLocaleString() })).join(" · ")}</span>
          )}
        </dd>
        <dt>{t("Matches")}</dt>
        <dd>
          {tx("{0} stored, {1} waiting, {2} failed, {3} given up", {
            "0": d.stats.normalized.toLocaleString(),
            "1": d.stats.pending.toLocaleString(),
            "2": d.stats.failed.toLocaleString(),
            "3": (d.stats.givenUp ?? 0).toLocaleString(),
          })}
        </dd>
        <dt>{t("Log file")}</dt>
        <dd>
          {d.logFile ? (
            <>
              <code>{d.logFile}</code>{" "}
              <button className="linkish" onClick={() => void copy(d.logFile!, t("the log file's location"))}>
                {t("Copy path")}
              </button>
            </>
          ) : (
            <span className="muted">{t("Could not be opened; the folder may be read-only.")}</span>
          )}
        </dd>
      </dl>

      <h3 style={{ marginTop: 22 }}>{t("Sources")}</h3>
      {d.sources.length === 0 ? (
        <p className="hint" style={{ marginTop: 6 }}>{t("No sync has run yet.")}</p>
      ) : (
        <div className="table-wrap" style={{ marginTop: 10 }}>
          <table className="match-table">
            <thead>
              <tr>
                <th>{t("Source")}</th>
                <th>{t("Last asked")}</th>
                <th>{t("Last error")}</th>
              </tr>
            </thead>
            <tbody>
              {d.sources.map((s) => (
                <tr key={s.source}>
                  <td className="nowrap">{sourceName(s.source)}</td>
                  <td className="muted nowrap">{sqliteTime(s.lastRunAt)}</td>
                  <td className={s.lastError ? "error" : "muted"}>{s.lastError ?? t("None")}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <h3 style={{ marginTop: 22 }}>{t("Recent runs")}</h3>
      {d.runs.length === 0 ? (
        <p className="hint" style={{ marginTop: 6 }}>{t("Syncs, rebuilds and re-rates are listed here once they finish.")}</p>
      ) : (
        <div className="table-wrap" style={{ marginTop: 10 }}>
          <table className="match-table">
            <thead>
              <tr>
                <SortTh k="when" s={runSort}>{t("When")}</SortTh>
                <SortTh k="what" s={runSort}>{t("What")}</SortTh>
                <SortTh k="took" s={runSort} className="num">{t("Took")}</SortTh>
                <th>{t("Result")}</th>
              </tr>
            </thead>
            <tbody>
              {runSort.sorted.map((r) => (
                <tr key={`${r.at}-${r.kind}`}>
                  <td className="muted nowrap">{new Date(r.at * 1000).toLocaleString()}</td>
                  <td className="nowrap">{jobName(r.kind)}</td>
                  <td className="num nowrap">{r.seconds < 60 ? `${r.seconds} s` : `${Math.round(r.seconds / 60)} min`}</td>
                  <td>
                    {r.error ? (
                      <span className="error">{r.error}</span>
                    ) : r.kind === "sync" ? (
                      <span className="muted">{tx("{0} new, {1} failed", { "0": r.fetched, "1": r.failed })}</span>
                    ) : (
                      <span className="muted">{t("Done")}</span>
                    )}
                    {r.problems.map((p) => (
                      <div key={p} className="warn-text">{p}</div>
                    ))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/**
 * Settings > Rating model: how your games moved from the last model to this
 * one. Older ratings stay in the database when the model changes, so this
 * is the same games scored twice: each class's average, and the games that
 * moved the most.
 */
export function ModelShiftPanel({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
  const q = useQuery({ queryKey: ["model_shift"], queryFn: api.getModelShift });
  if (q.error) return <div className="panel"><ApiError error={q.error} /></div>;
  if (q.isLoading) return <div className="panel"><p className="hint loading"><Spinner /> {t("Loading…")}</p></div>;
  const s: ModelShift | null | undefined = q.data;
  return (
    <div className="panel">
      <h2>{t("What the new model changed")}</h2>
      {!s ? (
        <p className="hint" style={{ marginTop: 6 }}>{t("Nothing to compare yet: your games have been rated on one model only.")}</p>
      ) : (
        <>
          <p className="hint" style={{ marginTop: 6 }}>
            {tx("Your games rated on both {0} and {1}.", { "0": <strong>{s.before}</strong>, "1": <strong>{s.after}</strong> })}
          </p>
          <div className="table-wrap" style={{ marginTop: 12 }}>
            <table className="match-table">
              <thead>
                <tr>
                  <th>{t("Class")}</th>
                  <th className="num">{t("Games")}</th>
                  <th className="num">{s.before}</th>
                  <th className="num">{s.after}</th>
                  <th className="num">{t("Change")}</th>
                </tr>
              </thead>
              <tbody>
                {s.classes.map((c) => (
                  <tr key={c.class}>
                    <td className="nowrap"><ClassIcon cls={c.class} size={18} /></td>
                    <td className="num">{c.games.toLocaleString()}</td>
                    <td className="num">{rating(c.before)}</td>
                    <td className="num">{rating(c.after)}</td>
                    <td className="num">{signed(c.after - c.before, 2)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {s.games.length > 0 && (
            <>
              <h3 style={{ marginTop: 22 }}>{t("Moved the most")}</h3>
              <div className="table-wrap" style={{ marginTop: 10 }}>
                <table className="match-table">
                  <thead>
                    <tr>
                      <th>{t("Played")}</th>
                      <th>{t("Map")}</th>
                      <th>{t("Class")}</th>
                      <th className="num">{s.before}</th>
                      <th className="num">{s.after}</th>
                      <th className="num">{t("Change")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {s.games.map((g) => (
                      <tr key={`${g.logId}-${g.class}`} className="clickable" tabIndex={0} onClick={() => onOpenMatch(g.logId)} onKeyDown={(e) => e.key === "Enter" && onOpenMatch(g.logId)}>
                        <td className="muted nowrap">{g.playedAt ? formatDate(g.playedAt, true) : "—"}</td>
                        <td className="nowrap">{splitMap(g.map).name ?? t("log {logId}", { logId: g.logId })}</td>
                        <td className="nowrap"><ClassIcon cls={g.class} size={18} /></td>
                        <td className="num">{rating(g.before)}</td>
                        <td className="num">{rating(g.after)}</td>
                        <td className="num">{signed(g.after - g.before, 2)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}
