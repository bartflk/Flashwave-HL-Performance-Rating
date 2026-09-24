import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage } from "../api/types";

/**
 * How far back a sync reaches.
 *
 * By default it downloads the last two years plus every official at any age:
 * that is what a rating is about, and it is most of the value for a fraction
 * of logs.tf's patience. The rest is here, behind a dialog that says what it
 * costs — because the cost is real, and it is paid by somebody else's server.
 */

/** How long a log takes, downloading at logs.tf's pace. Each kept log also
 *  pulls its raw server log, which is the slower of the two. */
const SECONDS_PER_LOG = 4;

/** Two of them, as the window is written in the backend. */
const KEEP_YEARS = 2;

export function HistoryPanel() {
  const qc = useQueryClient();
  const all = useQuery({ queryKey: ["all_history"], queryFn: api.allHistory });
  const stats = useQuery({ queryKey: ["index_stats"], queryFn: api.indexStats });
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function set(on: boolean) {
    setBusy(true);
    setError(null);
    try {
      await api.setAllHistory(on);
      await qc.invalidateQueries({ queryKey: ["all_history"] });
      await qc.invalidateQueries({ queryKey: ["index_stats"] });
      setAsking(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const on = all.data ?? false;
  const held = stats.data?.outsideWindow ?? 0;

  return (
    <div className="panel">
      <h2>How far back</h2>
      <p className="hint" style={{ marginTop: 6 }}>
        {on ? (
          <>
            <strong>Full history.</strong> Every Highlander log you have ever played is downloaded,
            back to your first one.
          </>
        ) : (
          <>
            <strong>The last {KEEP_YEARS} years, plus every official.</strong> An official counts
            whatever its age — a season from 2019 is still a game you care about. Older scrims and
            pugs are indexed but not downloaded, so they cost nothing and are one click away.
          </>
        )}
      </p>

      {!on && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>Not downloaded</dt>
          <dd>
            {held.toLocaleString()} older {held === 1 ? "match" : "matches"}
            {held > 0 && <span className="muted"> · about {eta(held)} to fetch</span>}
          </dd>
        </dl>
      )}

      {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}

      <div className="row" style={{ marginTop: 14 }}>
        {on ? (
          <button onClick={() => void set(false)} disabled={busy}>
            Go back to the last {KEEP_YEARS} years
          </button>
        ) : (
          <button onClick={() => setAsking(true)} disabled={busy || held === 0}>
            {held === 0 ? "Nothing older to download" : "Download the full history"}
          </button>
        )}
      </div>

      {asking && (
        <Dialog
          held={held}
          busy={busy}
          onCancel={() => setAsking(false)}
          onConfirm={() => void set(true)}
        />
      )}
    </div>
  );
}

/** The one screen that says what asking for everything actually costs. */
function Dialog({
  held,
  busy,
  onCancel,
  onConfirm,
}: {
  held: number;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="modal-scrim" role="dialog" aria-modal="true" aria-label="Download the full history">
      <div className="modal">
        <h3>Download the full history?</h3>
        <p>
          {held.toLocaleString()} older {held === 1 ? "match is" : "matches are"} indexed but not
          downloaded — scrims and pugs from before the last {KEEP_YEARS} years. Fetching them means
          about {held.toLocaleString()} requests to logs.tf, plus the raw server log behind each
          one: roughly <strong>{eta(held)}</strong>.
        </p>
        <p className="muted">
          logs.tf is a community server and this app waits between requests on purpose. Nothing
          downloads now — the next sync just has more to do, and you can keep using the app while
          it runs.
        </p>
        <div className="modal-actions">
          <button className="primary" onClick={onConfirm} disabled={busy}>
            {busy ? "Saving…" : "Download everything"}
          </button>
          <button onClick={onCancel} disabled={busy}>
            Not now
          </button>
        </div>
      </div>
    </div>
  );
}

function eta(logs: number): string {
  const mins = Math.round((logs * SECONDS_PER_LOG) / 60);
  if (mins < 1) return "under a minute";
  if (mins < 90) return `${mins} min`;
  return `${(mins / 60).toFixed(1)} hours`;
}
