import { Fragment, useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type AppStatus, type DemoIndexSummary } from "../api/types";
import { formatDate } from "../lib/format";
import { HistoryPanel } from "./HistoryPanel";
import { ImportPanel } from "./ImportPanel";
import { startRebuild, useSyncStatus } from "../lib/sync";
import { setTheme, THEMES, useTheme } from "../lib/theme";
import { clearProblems, markProblemsSeen, report, useProblems } from "../lib/problems";

export function Settings({
  status,
  onReconfigure,
}: {
  status: AppStatus;
  onReconfigure: () => void;
}) {
  // Rebuilding reports from the corner like a sync, because it is one: the
  // same events, the same minutes of work.
  const sync = useSyncStatus();
  const busy = sync.state === "running";

  return (
    <div className="content">
      <ProblemsPanel version={status.version} />
      <ThemePanel />
      <HistoryPanel />
      <ImportPanel />
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
            {status.config.tfPath ? (
              <code>{status.config.tfPath}</code>
            ) : (
              <span className="muted">Not set: demo jumps and downloads are off. Everything else works.</span>
            )}
          </dd>
        </dl>
        <button className="linkish" style={{ marginTop: 14 }} onClick={onReconfigure}>
          Change these
        </button>
      </div>

      <div className="panel">
        <h2>Data</h2>
        <p className="hint" style={{ marginTop: 6 }}>
          Rebuilds every match from stored logs. No downloading.
        </p>
        <div className="row" style={{ marginTop: 14 }}>
          <button onClick={() => void startRebuild()} disabled={busy}>
            {busy ? "Working…" : "Rebuild from stored data"}
          </button>
        </div>
        <dl className="kv" style={{ marginTop: 16 }}>
          <dt>Database</dt>
          <dd className="path-row">
            <code>{status.dbPath}</code>
            {/* Every log, rating and demo link is in this one file, and it is
                the only thing here that cannot be fetched again. */}
            <button className="linkish" onClick={() => void api.revealPath(status.dbPath)}>
              Show in Explorer
            </button>
          </dd>
          <dt>Version</dt>
          <dd>{status.version}</dd>
        </dl>
      </div>

      <BackupsPanel />
    </div>
  );
}

/**
 * Everything that went wrong, and a button that turns it into a message.
 *
 * A tester with a problem had nothing to send but a screenshot of a black
 * window. This is the thing to paste instead.
 */
function ProblemsPanel({ version }: { version: string }) {
  const problems = useProblems();
  const [copied, setCopied] = useState(false);
  useEffect(() => markProblemsSeen(), [problems.length]);

  if (problems.length === 0) {
    return (
      <div className="panel">
        <h2>Problems</h2>
        <p className="hint" style={{ marginTop: 6 }}>
          Nothing has gone wrong since the app started.
        </p>
      </div>
    );
  }
  return (
    <div className="panel">
      <h2>Problems</h2>
      <p className="hint" style={{ marginTop: 6 }}>
        {problems.length} since the app started. Copy this into Discord if you are reporting something.
      </p>
      <div className="row" style={{ marginTop: 12 }}>
        <button
          onClick={() => {
            void navigator.clipboard?.writeText(report({ version }));
            setCopied(true);
            window.setTimeout(() => setCopied(false), 2000);
          }}
        >
          {copied ? "Copied" : "Copy report"}
        </button>
        <button className="linkish" onClick={() => clearProblems()}>
          Clear
        </button>
      </div>
      <div className="table-wrap" style={{ marginTop: 14 }}>
        <table className="match-table">
          <thead>
            <tr>
              <th>When</th>
              <th>What</th>
              <th>Why</th>
            </tr>
          </thead>
          <tbody>
            {problems.slice(0, 50).map((p) => (
              <tr key={p.id}>
                <td className="muted nowrap">{new Date(p.at).toLocaleTimeString()}</td>
                <td className="nowrap">
                  {p.what}
                  {p.count > 1 && <span className="muted"> ×{p.count}</span>}
                </td>
                <td className="muted">{p.message}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/**
 * Pick a palette (Q10).
 *
 * Every theme is dark. The app is built from translucent light tints over
 * dark surfaces, so a light theme is not a swap of this list — it is its
 * own piece of work, and half-doing it would look worse than not offering
 * it. RED and BLU are never themed: they mean something in TF2, and a
 * scoreboard that recolours them is lying about which team is which.
 */
function ThemePanel() {
  const theme = useTheme();
  return (
    <div className="panel">
      <h2>Theme</h2>
      <div className="theme-grid">
        {THEMES.map((t) => (
          <button
            key={t.id}
            className={theme === t.id ? "theme-card active" : "theme-card"}
            onClick={() => setTheme(t.id)}
            aria-pressed={theme === t.id}
          >
            <span className={`theme-swatch theme-${t.id}`} aria-hidden>
              <i className="sw-bg" />
              <i className="sw-panel" />
              <i className="sw-accent" />
            </span>
            <span className="theme-name">{t.name}</span>
            <span className="theme-hint muted">{t.hint}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/**
 * Copies of the database. One is taken before every sync and rebuild, five
 * are kept, and this is where to check they exist — the file holds every log,
 * demo index and rating, and nothing else here can rebuild it from nothing.
 */
function BackupsPanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["backups"], queryFn: api.listBackups });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  /** A name that says what it is and when, so a folder of them sorts. */
  function suggestedName() {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    return `flashwave-${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}.sqlite3`;
  }

  async function saveElsewhere() {
    setBusy(true);
    setError(null);
    setSaved(null);
    try {
      const b = await api.saveBackupAs(suggestedName());
      if (b) setSaved(b.path);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function backupNow() {
    setBusy(true);
    setError(null);
    try {
      await api.backupNow();
      await qc.invalidateQueries({ queryKey: ["backups"] });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const items = q.data?.items ?? [];
  return (
    <div className="panel">
      <h2>Backups</h2>
      <p className="hint">
        Taken before every sync and rebuild; the newest five are kept. To restore one, close the app and rename it
        over <code>hl.sqlite3</code>.
      </p>
      <p className="hint">
        <strong>Uninstalling can delete these.</strong> Keep a copy on another drive.
      </p>
      <div className="row" style={{ marginTop: 14 }}>
        <button onClick={() => void backupNow()} disabled={busy}>
          {busy ? "Copying…" : "Back up now"}
        </button>
        <button onClick={() => void saveElsewhere()} disabled={busy}>
          {busy ? "Copying…" : "Save a copy elsewhere…"}
        </button>
      </div>
      {saved && (
        <p className="hint" style={{ marginTop: 10 }}>
          Written to <code>{saved}</code>.
        </p>
      )}
      {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}
      {items.length === 0 ? (
        <p className="hint" style={{ marginTop: 14 }}>No copies yet. The next sync makes one.</p>
      ) : (
        <dl className="kv" style={{ marginTop: 16 }}>
          <dt>Folder</dt>
          <dd className="path-row">
            <code>{q.data?.dir}</code>
            <button className="linkish" onClick={() => void api.revealPath(q.data!.dir)}>
              Show in Explorer
            </button>
          </dd>
          {items.map((b) => (
            <Fragment key={b.path}>
              <dt>{formatDate(b.madeAt, true)}</dt>
              <dd>{(b.bytes / 1_000_000).toFixed(0)} MB</dd>
            </Fragment>
          ))}
        </dl>
      )}
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
        Demos in <code>tf</code>, <code>tf/demos</code> and <code>tf/demos/stv</code>, matched to logs by map and time.
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
