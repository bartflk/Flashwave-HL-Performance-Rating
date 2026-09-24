import { useState } from "react";
import { api } from "../api/client";
import { errorMessage, type RestoreOffer } from "../api/types";
import { formatDate } from "../lib/format";

/**
 * The database is empty and a backup beside it is not.
 *
 * This is what a wipe looks like from the inside: an uninstaller that took the
 * app data, a profile that moved, a file deleted by something else. The app
 * used to say nothing — it opened blank, asked for a SteamID as if it had
 * never run, and started re-downloading twelve years of logs while a 200 MB
 * copy sat in the folder next door. So this goes above everything, including
 * the setup screen, and asks before anything else does.
 */
export function RestoreBanner({ offer, onSettled }: { offer: RestoreOffer; onSettled: () => void }) {
  const [busy, setBusy] = useState<"restore" | "decline" | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function restore() {
    setBusy("restore");
    setError(null);
    try {
      // The app restarts into the restored database; this never returns.
      await api.restoreBackup(offer.path);
    } catch (e) {
      setError(errorMessage(e));
      setBusy(null);
    }
  }

  async function decline() {
    setBusy("decline");
    setError(null);
    try {
      await api.declineRestore();
      onSettled();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(null);
    }
  }

  return (
    <div className="restore" role="alert">
      <div className="restore-body">
        <h2>This database is empty, but a backup is not.</h2>
        <p>
          There are no matches here, and a copy made{" "}
          <strong>{formatDate(offer.madeAt, true)}</strong> holds{" "}
          <strong>{offer.matches.toLocaleString()}</strong> of them
          <span className="muted"> ({(offer.bytes / 1_000_000).toFixed(0)} MB)</span>. Putting it
          back takes a second and restarts the app; downloading it all again takes an hour.
        </p>
        <p className="restore-path">
          <code>{offer.path}</code>
          <button className="linkish" onClick={() => void api.revealPath(offer.path)}>
            Show me
          </button>
        </p>
        {error && <p className="error">{error}</p>}
      </div>
      <div className="restore-actions">
        <button className="primary" onClick={() => void restore()} disabled={busy !== null}>
          {busy === "restore" ? "Restarting…" : "Put it back"}
        </button>
        {/* The empty database is kept either way — declining only stops the
            asking, so this is never the irreversible choice. */}
        <button onClick={() => void decline()} disabled={busy !== null}>
          Start fresh
        </button>
      </div>
    </div>
  );
}
