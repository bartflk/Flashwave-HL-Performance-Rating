import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type Progress, type SyncDone } from "../api/types";

/** Newest logs are fetched first, so refreshing the list during a sync shows
 *  recent matches appearing while older history is still downloading. */
const REFRESH_EVERY_N_FETCHES = 10;

/** Rough seconds per log: the 1 req/s throttle plus logs.tf response time. */
const SECONDS_PER_LOG = 2.5;

type Status =
  | { state: "idle" }
  | { state: "running"; progress: Progress | null }
  | { state: "done"; result: SyncDone }
  | { state: "error"; message: string };

export function SyncStrip() {
  const qc = useQueryClient();
  const [status, setStatus] = useState<Status>({ state: "idle" });
  const [failures, setFailures] = useState(0);

  const stats = useQuery({ queryKey: ["index_stats"], queryFn: api.indexStats });

  // A sync may already be running (started before a reload). Reflect that.
  useEffect(() => {
    void api.syncBusy().then((busy) => {
      if (busy) setStatus({ state: "running", progress: null });
    });
  }, []);

  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void api
      .onSync({
        onProgress: (p) => {
          setStatus({ state: "running", progress: p });
          if (p.kind === "fetchFailed") setFailures((n) => n + 1);
          if (p.kind === "fetching" && p.done > 0 && p.done % REFRESH_EVERY_N_FETCHES === 0) {
            void qc.invalidateQueries({ queryKey: ["matches"] });
          }
        },
        onDone: (result) => {
          setStatus({ state: "done", result });
          void qc.invalidateQueries({ queryKey: ["matches"] });
          void qc.invalidateQueries({ queryKey: ["index_stats"] });
        },
        onError: (e) => setStatus({ state: "error", message: e.message }),
      })
      .then((unlisten) => {
        // StrictMode mounts effects twice; drop the listener if already torn down.
        if (cancelled) unlisten();
        else off = unlisten;
      });
    return () => {
      cancelled = true;
      off?.();
    };
  }, [qc]);

  async function start() {
    setFailures(0);
    setStatus({ state: "running", progress: null });
    try {
      await api.syncStart(false);
    } catch (e) {
      setStatus({ state: "error", message: errorMessage(e) });
    }
  }

  const s = stats.data;
  const running = status.state === "running";

  return (
    <section className="sync-strip">
      <div className="sync-row">
        <div className="sync-summary">
          {s ? (
            <>
              <span>
                <strong>{s.highlander.toLocaleString()}</strong> Highlander matches
              </span>
              <span className="sep">·</span>
              <span>
                <strong>{s.officials}</strong> ETF2L official
              </span>
              {s.pending > 0 && !running && (
                <>
                  <span className="sep">·</span>
                  <span className="warn-text">
                    {s.pending.toLocaleString()} not fetched yet (~{eta(s.pending)})
                  </span>
                </>
              )}
            </>
          ) : (
            <span className="hint">Loading…</span>
          )}
        </div>
        <button className="primary" onClick={() => void start()} disabled={running}>
          {running ? "Syncing…" : "Sync"}
        </button>
      </div>

      {running && <ProgressLine progress={status.progress} failures={failures} />}

      {status.state === "done" && (
        <p className="sync-note">
          {status.result.kind === "reprocess"
            ? "Rebuilt every match from stored data."
            : status.result.fetched === 0
              ? "Up to date — no new matches."
              : `Fetched ${status.result.fetched} match${status.result.fetched === 1 ? "" : "es"}.`}
          {status.result.failed > 0 && (
            <span className="error"> {status.result.failed} failed and will be retried next sync.</span>
          )}
        </p>
      )}

      {status.state === "error" && <p className="error sync-note">{status.message}</p>}
    </section>
  );
}

function ProgressLine({ progress, failures }: { progress: Progress | null; failures: number }) {
  let label = "Starting…";
  let fraction: number | null = null;

  if (progress) {
    switch (progress.kind) {
      case "indexing":
        label =
          progress.rows > 0
            ? `Indexing ${progress.source} — ${progress.rows.toLocaleString()} rows`
            : `Indexing ${progress.source}…`;
        break;
      case "indexed":
        label = `Indexed. ${progress.superseded} per-round logs folded into their combined match.`;
        break;
      case "fetching":
        fraction = progress.total > 0 ? progress.done / progress.total : 1;
        label =
          progress.total === 0
            ? "Nothing new to fetch."
            : `Fetching ${progress.done.toLocaleString()} of ${progress.total.toLocaleString()}` +
              (progress.done < progress.total ? ` — about ${eta(progress.total - progress.done)} left` : "");
        break;
      case "fetchFailed":
        label = `Log ${progress.logId} failed; continuing.`;
        break;
      case "reprocessing":
        fraction = progress.total > 0 ? progress.done / progress.total : 1;
        label = `Rebuilding ${progress.done.toLocaleString()} of ${progress.total.toLocaleString()}`;
        break;
    }
  }

  return (
    <div className="progress">
      <div className="progress-track">
        <div
          className={fraction === null ? "progress-fill indeterminate" : "progress-fill"}
          style={fraction === null ? undefined : { width: `${Math.round(fraction * 100)}%` }}
        />
      </div>
      <div className="progress-label">
        <span>{label}</span>
        {failures > 0 && <span className="error">{failures} failed</span>}
      </div>
    </div>
  );
}

function eta(logs: number): string {
  const mins = Math.ceil((logs * SECONDS_PER_LOG) / 60);
  return mins <= 1 ? "1 min" : `${mins} min`;
}
