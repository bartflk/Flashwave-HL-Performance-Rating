import { useEffect, useState } from "react";
import { api } from "../../api/client";
import { errorMessage, type MatchDetail } from "../../api/types";
import { beginDownload, useDownload } from "../../lib/downloads";

/**
 * "There is a SourceTV demo for this match. Want it?"
 *
 * Half of what this app can say about a match is locked inside the demo:
 * where all eighteen players walked, what every life was worth, and — on a
 * POV demo — only the recorder's aim. Without one the analysis tabs are a
 * scoreboard with extra steps, and nothing on the page said so.
 *
 * Asked once per match, and never again if you say so. A dialog that appears
 * every time you open a log you have already decided about is not a prompt,
 * it is a toll gate.
 */

/** Matches already answered, this session. */
const asked = new Set<number>();

const NEVER_KEY = "stv-prompt-off";

function neverAsk(): boolean {
  try {
    return localStorage.getItem(NEVER_KEY) === "1";
  } catch {
    // Private windows and cleared site data: ask, rather than assume.
    return false;
  }
}

export function StvPrompt({ d }: { d: MatchDetail }) {
  const hasStv = d.demos.some((x) => x.kind === "stv");
  const download = useDownload(d.logId);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (hasStv || d.demosTfId === null || asked.has(d.logId) || neverAsk()) return;
    asked.add(d.logId);
    setOpen(true);
  }, [d.logId, d.demosTfId, hasStv]);

  if (!open || download) return null;

  async function fetchIt() {
    setError(null);
    beginDownload(d.logId, `${d.map ?? "this match"}, log ${d.logId}`);
    setOpen(false);
    try {
      await api.fetchStv(d.logId);
    } catch (e) {
      setError(errorMessage(e));
      setOpen(true);
    }
  }

  function stopAsking() {
    try {
      localStorage.setItem(NEVER_KEY, "1");
    } catch {
      // Nothing to do: it will ask again next time, which is the safe way to fail.
    }
    setOpen(false);
  }

  return (
    <div className="modal-scrim" role="dialog" aria-modal="true" aria-label="Download the SourceTV demo">
      <div className="modal">
        <h3>There is a SourceTV demo for this match</h3>
        <p>
          It holds what the scoreboard cannot: where all eighteen players walked, what each life was
          worth, and every kill with the angle behind it. Downloading it fills in the Movement and
          Aim tabs for this match.
        </p>
        <p className="muted">
          About a hundred megabytes from demos.tf, half a minute, straight into{" "}
          <code>tf/demos/stv</code>. It carries on while you read other matches.
        </p>
        {error && <p className="error">{error}</p>}
        <div className="modal-actions">
          <button className="primary" onClick={() => void fetchIt()}>
            Download it
          </button>
          <button onClick={() => setOpen(false)}>Not now</button>
          <button className="linkish" onClick={stopAsking}>
            Never ask
          </button>
        </div>
      </div>
    </div>
  );
}
