import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type DemoView, type MatchDetail } from "../../api/types";
import { copy } from "../../lib/toast";

/**
 * The demos behind this match, and how to jump into them.
 *
 * TF2 cannot open a demo and seek in one console line (`demo_gototick` runs
 * before the demo has loaded), so the flow is two steps: open the demo once
 * with `playdemo`, then paste a `demo_gototick` for each moment. Every
 * timeline marker below copies its own tick.
 */
export function DemoPanel({ d }: { d: MatchDetail }) {
  const qc = useQueryClient();
  const [download, setDownload] = useState<{ bytes: number; total: number | null } | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void api
      .onStv({
        onProgress: (p) => p.logId === d.logId && setDownload({ bytes: p.bytes, total: p.total }),
        onDone: () => {
          setDownload(null);
          void qc.invalidateQueries({ queryKey: ["match", d.logId] });
          void qc.invalidateQueries({ queryKey: ["matches"] });
        },
        onError: (e) => {
          if (e.logId !== d.logId) return;
          setDownload(null);
          setError(e.message);
        },
      })
      .then((u) => (cancelled ? u() : (off = u)));
    return () => {
      cancelled = true;
      off?.();
    };
  }, [d.logId, qc]);

  async function fetchStv() {
    setError(null);
    setDownload({ bytes: 0, total: null });
    try {
      await api.fetchStv(d.logId);
    } catch (e) {
      setDownload(null);
      setError(errorMessage(e));
    }
  }

  const hasStv = d.demos.some((x) => x.kind === "stv");
  const canFetch = d.demosTfId !== null && !hasStv;

  if (d.demos.length === 0 && !canFetch) {
    return (
      <section className="panel demo-panel">
        <h2>Demo</h2>
        <p className="hint" style={{ marginTop: 6 }}>
          No recording of this match on this machine, and demos.tf has none either.
        </p>
      </section>
    );
  }

  return (
    <section className="panel demo-panel">
      <header className="demo-head">
        <div>
          <h2>Demo</h2>
          {d.demos.length > 0 && (
            <p className="hint">
              Open the demo once, then click any marker on the round timeline to copy its{" "}
              <code>demo_gototick</code>. Jumps land 5 seconds early, so you see the lead-up.
            </p>
          )}
        </div>
      </header>

      {d.demos.map((demo) => (
        <DemoRow key={demo.demoId} demo={demo} />
      ))}

      {canFetch && (
        <div className="stv-fetch">
          <div>
            <strong>SourceTV demo on demos.tf</strong>
            <p className="hint">
              All 18 players, not just your view. Stopwatch matches are usually split into one demo per
              half, and demos.tf links one of them, so this may cover only part of the match.
            </p>
          </div>
          {download ? (
            <div className="dl-progress">
              <div className="progress-track">
                <div
                  className={download.total ? "progress-fill" : "progress-fill indeterminate"}
                  style={download.total ? { width: `${Math.round((download.bytes / download.total) * 100)}%` } : undefined}
                />
              </div>
              <span className="hint">
                {mb(download.bytes)}
                {download.total ? ` of ${mb(download.total)}` : ""}
              </span>
            </div>
          ) : (
            <button onClick={() => void fetchStv()}>Download to tf/demos/stv</button>
          )}
        </div>
      )}
      {error && <p className="error">{error}</p>}
    </section>
  );
}

function DemoRow({ demo }: { demo: DemoView }) {
  const cmd = `playdemo ${demo.playdemoArg}`;
  return (
    <div className="demo-row">
      <div className="demo-info">
        <div className="demo-name">
          <span className={demo.kind === "stv" ? "badge badge-demo" : "badge badge-pov"}>
            {demo.kind === "stv" ? "STV" : "POV"}
          </span>
          <code>{demo.fileName}</code>
        </div>
        <span className="hint">
          {Math.round(demo.durationS / 60)} min
          {demo.recordedAt !== null && ` · recorded ${new Date(demo.recordedAt * 1000).toLocaleString()}`}
          {` · ${Math.round(demo.logShare * 100)}% of this match inside`}
          {demo.markers > 0 && ` · ${demo.markers} killstreak marker${demo.markers === 1 ? "" : "s"}`}
          {demo.approximate && " · jump positions estimated"}
        </span>
      </div>
      <button className="primary" onClick={() => void copy(cmd, "playdemo command")} title={cmd}>
        Copy playdemo
      </button>
    </div>
  );
}

function mb(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}
