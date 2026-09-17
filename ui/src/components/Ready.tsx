import type { AppStatus } from "../api/types";

/**
 * Placeholder for the real dashboard. M0 ends here: the app boots, the database
 * is live and configuration persists. M1 fills this with the match list.
 */
export function Ready({ status, onReconfigure }: { status: AppStatus; onReconfigure: () => void }) {
  return (
    <>
      <header className="topbar">
        <h1>HL Rating</h1>
        <span className="who">
          <code>{status.config.steamid}</code>
        </span>
      </header>

      <div className="content">
        <div className="panel">
          <h2>Setup complete</h2>
          <p className="hint" style={{ marginTop: 6, marginBottom: 16 }}>
            Nothing is being tracked yet — this is the M0 skeleton.
          </p>
          <dl className="kv">
            <dt>SteamID</dt>
            <dd>
              <code>{status.config.steamid}</code>
            </dd>
            <dt>TF2 folder</dt>
            <dd>
              <code>{status.config.tfPath}</code>
            </dd>
            <dt>Database</dt>
            <dd>
              <code>{status.dbPath}</code>
            </dd>
            <dt>Version</dt>
            <dd>{status.version}</dd>
          </dl>
          <button className="linkish" style={{ marginTop: 16 }} onClick={onReconfigure}>
            Change these settings
          </button>
        </div>

        <div className="panel">
          <h2>Next: M1 — logs.tf pipeline</h2>
          <ul className="next">
            <li>Sync your match history from logs.tf and store the raw JSON</li>
            <li>Normalise it into match, round and per-class tables</li>
            <li>List every Highlander match you have played</li>
            <li>Rebuild every derived table from stored blobs, with no refetching</li>
          </ul>
        </div>
      </div>
    </>
  );
}
