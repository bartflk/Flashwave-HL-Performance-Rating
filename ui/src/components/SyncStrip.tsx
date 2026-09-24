import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import { eta, startSync, useSyncStatus } from "../lib/sync";

/**
 * What is stored, and the button that fetches more.
 *
 * The progress itself is not here: a sync runs for minutes whatever the
 * window is showing, so it reports from the corner (see `Notifications`)
 * rather than from a bar above whichever page happened to start it.
 */
export function SyncStrip() {
  const stats = useQuery({ queryKey: ["index_stats"], queryFn: api.indexStats });
  const sync = useSyncStatus();
  const running = sync.state === "running";
  const s = stats.data;

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
                <strong>{s.officials}</strong> official{s.officials === 1 ? "" : "s"}
              </span>
              {s.pending > 0 && !running && (
                <>
                  <span className="sep">·</span>
                  <span className="warn-text">
                    {s.pending.toLocaleString()} not fetched yet (~{eta(s.pending)})
                  </span>
                </>
              )}
              {s.outsideWindow > 0 && !running && (
                <>
                  <span className="sep">·</span>
                  {/* Not a warning: these are left alone on purpose, and
                      Settings says how to ask for them. */}
                  <span className="muted" title="Scrims and pugs older than two years. Settings › How far back">
                    {s.outsideWindow.toLocaleString()} older kept out
                  </span>
                </>
              )}
            </>
          ) : (
            <span className="hint">Loading…</span>
          )}
        </div>
        <button className="primary" onClick={() => void startSync()} disabled={running}>
          {running ? "Syncing…" : "Sync"}
        </button>
      </div>
    </section>
  );
}
