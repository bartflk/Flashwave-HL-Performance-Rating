import { useState } from "react";
import { api } from "../api/client";
import { errorMessage, type AppStatus } from "../api/types";

export function Settings({
  status,
  onReconfigure,
}: {
  status: AppStatus;
  onReconfigure: () => void;
}) {
  const [rebuild, setRebuild] = useState<{ busy: boolean; error: string | null }>({
    busy: false,
    error: null,
  });

  async function startRebuild() {
    setRebuild({ busy: true, error: null });
    try {
      // Progress and completion arrive through the sync strip's listeners.
      await api.reprocessStart();
    } catch (e) {
      setRebuild({ busy: false, error: errorMessage(e) });
      return;
    }
    setRebuild({ busy: false, error: null });
  }

  return (
    <div className="content">
      <div className="panel">
        <h2>Setup</h2>
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>SteamID</dt>
          <dd>
            <code>{status.config.steamid}</code>
          </dd>
          <dt>TF2 folder</dt>
          <dd>
            <code>{status.config.tfPath}</code>
          </dd>
        </dl>
        <button className="linkish" style={{ marginTop: 14 }} onClick={onReconfigure}>
          Change these
        </button>
      </div>

      <div className="panel">
        <h2>Data</h2>
        <p className="hint" style={{ marginTop: 6 }}>
          Every match is rebuilt from the logs already stored on this machine — no downloading.
          Use this after an update changes how logs are read.
        </p>
        <div className="row" style={{ marginTop: 14 }}>
          <button onClick={() => void startRebuild()} disabled={rebuild.busy}>
            Rebuild from stored data
          </button>
        </div>
        {rebuild.error && <p className="error" style={{ marginTop: 10 }}>{rebuild.error}</p>}
        <dl className="kv" style={{ marginTop: 16 }}>
          <dt>Database</dt>
          <dd>
            <code>{status.dbPath}</code>
          </dd>
          <dt>Version</dt>
          <dd>{status.version}</dd>
        </dl>
      </div>
    </div>
  );
}
