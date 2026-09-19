import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type AppStatus, type DemoIndexSummary } from "../api/types";

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
      <Etf2lPanel />
      <RawlogPanel />
      <DemosPanel />
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

/** Demo index counts, and a rescan for when you have just recorded. */
function DemosPanel() {
  const qc = useQueryClient();
  const stats = useQuery({ queryKey: ["demo_stats"], queryFn: api.demoStats });
  const [scan, setScan] = useState<{ busy: boolean; result: DemoIndexSummary | null; error: string | null }>({
    busy: false,
    result: null,
    error: null,
  });

  async function rescan() {
    setScan({ busy: true, result: null, error: null });
    try {
      const result = await api.scanDemos();
      setScan({ busy: false, result, error: null });
      void qc.invalidateQueries({ queryKey: ["demo_stats"] });
      void qc.invalidateQueries({ queryKey: ["matches"] });
      void qc.invalidateQueries({ queryKey: ["match"] });
    } catch (e) {
      setScan({ busy: false, result: null, error: errorMessage(e) });
    }
  }

  const s = stats.data;
  return (
    <div className="panel">
      <h2>Demos</h2>
      <p className="hint" style={{ marginTop: 6 }}>
        Your recordings in <code>tf</code>, <code>tf/demos</code> and <code>tf/demos/stv</code> are
        matched to logs by map and time. Scanned at startup and after every sync.
      </p>
      {s && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>Demos found</dt>
          <dd>
            {s.demos}
            {s.stv > 0 && ` (${s.stv} STV)`}
          </dd>
          <dt>Linked</dt>
          <dd>
            {s.linked} demos, covering {s.matchesWithDemo} matches
          </dd>
          <dt>Markers</dt>
          <dd>{s.markers} killstreak markers from Demo Support</dd>
        </dl>
      )}
      <p className="hint" style={{ marginTop: 10 }}>
        Unlinked demos are usually pubs, MvM, reviews of other people&apos;s games, or matches
        with no logs.tf log that includes you.
      </p>
      <div className="row" style={{ marginTop: 14 }}>
        <button onClick={() => void rescan()} disabled={scan.busy}>
          {scan.busy ? "Scanning…" : "Rescan demos"}
        </button>
        {scan.result && (
          <span className="hint">
            Scanned {scan.result.scanned}, linked {scan.result.demosLinked}.
          </span>
        )}
      </div>
      {scan.error && <p className="error" style={{ marginTop: 10 }}>{scan.error}</p>}
    </div>
  );
}

/** What ETF2L added: officials, and how every match was classified. */
function Etf2lPanel() {
  const q = useQuery({ queryKey: ["context_counts"], queryFn: api.contextCounts });
  const c = q.data;
  return (
    <div className="panel">
      <h2>ETF2L and match types</h2>
      <p className="hint" style={{ marginTop: 6 }}>
        Your ETF2L results are fetched on every sync. Each Highlander match you played is then sorted into an
        official, a scrim (most of your side are regular teammates or your ETF2L roster) or a pug.
      </p>
      {c && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>ETF2L player</dt>
          <dd>
            {c.etf2lPlayer ? (
              <button className="linkish" onClick={() => void api.openExternal(`https://etf2l.org/forum/user/${c.etf2lPlayer}/`)}>
                #{c.etf2lPlayer} ↗
              </button>
            ) : (
              <span className="muted">not found yet — sync to look it up</span>
            )}
          </dd>
          <dt>Officials</dt>
          <dd>
            {c.officials} logs
            {c.rosterOfficials > 0 && (
              <span className="muted"> · {c.rosterOfficials} found by roster that trends.tf had not tagged</span>
            )}
          </dd>
          <dt>Scrims</dt>
          <dd>{c.scrims}</dd>
          <dt>Pugs</dt>
          <dd>{c.pugs}</dd>
          <dt>Last fetched</dt>
          <dd>
            {c.lastFetch ? (
              <>
                {new Date(c.lastFetch * 1000).toLocaleString()}{" "}
                <span className="muted">({c.etf2lMatches} ETF2L matches stored)</span>
              </>
            ) : (
              <span className="muted">never</span>
            )}
          </dd>
        </dl>
      )}
    </div>
  );
}

/** Raw server logs: where every kill, with its time and position, comes from. */
function RawlogPanel() {
  const q = useQuery({ queryKey: ["rawlog_stats"], queryFn: api.rawlogStats });
  const s = q.data;
  return (
    <div className="panel">
      <h2>Raw logs</h2>
      <p className="hint" style={{ marginTop: 6 }}>
        The server log behind each logs.tf page. It has every kill with its time, both classes and where both
        players stood, so kills are valued one by one: by the victim&apos;s class, the map, and whether they were
        defending. Fetched on every sync.
      </p>
      {s && (
        <dl className="kv" style={{ marginTop: 14 }}>
          <dt>Stored</dt>
          <dd>
            {s.stored.toLocaleString()} matches <span className="muted">({(s.bytes / 1e6).toFixed(0)} MB)</span>
          </dd>
          <dt>Kills</dt>
          <dd>{s.kills.toLocaleString()}</dd>
          <dt>Still to fetch</dt>
          <dd>
            {s.pending}
            {s.pending > 0 && <span className="muted"> · retried on the next sync</span>}
          </dd>
          {s.missing > 0 && (
            <>
              <dt>Not on logs.tf</dt>
              <dd>
                {s.missing} <span className="muted">· these use logs.tf&apos;s totals instead</span>
              </dd>
            </>
          )}
        </dl>
      )}
    </div>
  );
}
