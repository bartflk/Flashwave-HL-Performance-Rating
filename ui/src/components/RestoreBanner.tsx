import { useState } from "react";
import { api } from "../api/client";
import { errorMessage, type RestoreOffer } from "../api/types";
import { formatDate } from "../lib/format";

/**
 * Something is wrong with the database, and it is worth saying so before the
 * app asks for anything else.
 *
 * Two ways to get here, and they are not the same:
 *
 * * **empty** — the database opened and holds nothing. A wipe looks exactly
 *   like a new install: no config, no matches, a setup screen asking for a
 *   SteamID, and a fresh download of twelve years of logs with a 200 MB copy
 *   sitting in the folder next door.
 * * **unreadable** — it would not open at all, so it was moved aside and a
 *   fresh one took its place. This used to end at "Could not start" with no
 *   way forward, which is how 25 September 2026 went.
 */
export function RestoreBanner({ offer, onSettled }: { offer: RestoreOffer; onSettled: () => void }) {
  const [busy, setBusy] = useState<"restore" | "decline" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const broken = offer.reason === "unreadable";
  // The setting holds "<where it went>|<what SQLite said>".
  const [movedTo, why] = (offer.setAside ?? "").split("|", 2);

  async function restore(path: string) {
    setBusy("restore");
    setError(null);
    try {
      // The app restarts into the restored database; this never returns.
      await api.restoreBackup(path);
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
        <h2>
          {broken
            ? "This database could not be opened."
            : "This database is empty, but a backup is not."}
        </h2>

        {broken && (
          <p>
            It has been moved aside and a new one put in its place. Nothing was
            deleted — a file this app cannot read is still the only copy of
            whatever was in it.
          </p>
        )}

        {offer.backup ? (
          <p>
            A copy made <strong>{formatDate(offer.backup.madeAt, true)}</strong> holds{" "}
            <strong>{offer.backup.matches.toLocaleString()}</strong> matches
            <span className="muted"> ({(offer.backup.bytes / 1_000_000).toFixed(0)} MB)</span>.
            Putting it back takes a second and restarts the app; downloading it all again takes
            an hour.
          </p>
        ) : (
          <p>
            There is no backup to go back to, so this starts over. Copies are made before every
            sync from now on, and Settings says where they live.
          </p>
        )}

        {offer.backup && (
          <p className="restore-path">
            <code>{offer.backup.path}</code>
            <button className="linkish" onClick={() => void api.revealPath(offer.backup!.path)}>
              Show me
            </button>
          </p>
        )}

        {broken && movedTo && (
          <p className="restore-path">
            <span className="muted">The old one:</span>
            <code title={why}>{movedTo}</code>
            <button className="linkish" onClick={() => void api.revealPath(movedTo)}>
              Show me
            </button>
          </p>
        )}

        {error && <p className="error">{error}</p>}
      </div>

      <div className="restore-actions">
        {offer.backup && (
          <button
            className="primary"
            onClick={() => void restore(offer.backup!.path)}
            disabled={busy !== null}
          >
            {busy === "restore" ? "Restarting…" : "Put it back"}
          </button>
        )}
        {/* The database in front of you is kept either way — declining only
            stops the asking, so this is never the irreversible choice. */}
        <button onClick={() => void decline()} disabled={busy !== null}>
          {offer.backup ? "Start fresh" : "Carry on"}
        </button>
      </div>
    </div>
  );
}
