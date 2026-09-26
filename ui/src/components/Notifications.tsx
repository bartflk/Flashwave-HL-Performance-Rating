import { dismissDownload, useDownloads, type Download } from "../lib/downloads";
import { dismissSync, fractionOf, labelOf, useSyncStatus } from "../lib/sync";

/**
 * The corner: everything running in the background, one card each.
 *
 * A sync and a demo download both take minutes in the backend whatever the
 * window is showing, and you should be able to walk away and keep reading
 * other matches while they run. So they live here rather than in the page
 * that started them, and say what they are doing and when they are done.
 */
export function Notifications({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
  const downloads = useDownloads();
  const sync = useSyncStatus();
  if (downloads.length === 0 && sync.state === "idle") return null;

  return (
    <div className="downloads" role="status" aria-live="polite">
      <SyncCard />
      {downloads.map((d) => (
        <DownloadCard key={d.logId} d={d} onOpenMatch={onOpenMatch} />
      ))}
    </div>
  );
}

function SyncCard() {
  const sync = useSyncStatus();
  if (sync.state === "idle") return null;

  const done = sync.state === "done";
  const failed = sync.state === "error";
  const fraction = sync.state === "running" ? fractionOf(sync.progress) : null;

  return (
    <div className={`dl ${done ? "dl-done" : failed ? "dl-failed" : "dl-running"}`}>
      <div className="dl-head">
        <span className="dl-title">
          {/* A rebuild sends the same events as a sync and only names itself
              at the end, so the phase it is in says which one this is. */}
          {sync.state === "running" &&
            (sync.progress?.kind === "reprocessing" ? "Rebuilding" : "Syncing")}
          {done && (sync.result.kind === "reprocess" ? "Rebuilt" : "Sync finished")}
          {failed && "Sync failed"}
        </span>
        {/* A running sync has no close button: stopping it is not something
            this card can do, and a card that hides itself would only make
            the progress harder to find. */}
        {sync.state !== "running" && (
          <button className="dl-close" onClick={dismissSync} title="Dismiss">
            ×
          </button>
        )}
      </div>

      {sync.state === "running" && (
        <>
          <p className="dl-label">{labelOf(sync.progress)}</p>
          <div className="dl-bar" aria-hidden>
            <span
              className={fraction === null ? "dl-fill dl-unknown" : "dl-fill"}
              style={fraction === null ? undefined : { width: `${Math.round(fraction * 100)}%` }}
            />
          </div>
          {sync.failures > 0 && <p className="dl-sub dl-error">{sync.failures} failed</p>}
        </>
      )}

      {done && (
        <p className="dl-sub">
          {sync.result.kind === "reprocess"
            ? "Every match rebuilt from stored data."
            : sync.result.fetched === 0
              ? "Up to date — no new matches."
              : `${sync.result.fetched} new match${sync.result.fetched === 1 ? "" : "es"}, rated and in the list.`}
          {sync.result.failed > 0 && (
            <span className="dl-error"> {sync.result.failed} failed; next sync retries them.</span>
          )}
        </p>
      )}

      {/* What a source could not give us. The sync still finished, so this
          is a note rather than a failure — but it is why a count may look
          short, and it should not need the log file to find out. */}
      {(done || sync.state === "running") &&
        sync.notes.map((n) => (
          <p key={n} className="dl-sub dl-note">
            {n}
          </p>
        ))}

      {failed && <p className="dl-sub dl-error">{sync.message}</p>}
    </div>
  );
}

function DownloadCard({ d, onOpenMatch }: { d: Download; onOpenMatch: (logId: number) => void }) {
  const pct = d.total ? Math.min(100, (d.bytes / d.total) * 100) : null;
  return (
    <div className={`dl dl-${d.state}`}>
      <div className="dl-head">
        <span className="dl-title">
          {d.state === "queued" && "Waiting to download"}
          {d.state === "running" && "Downloading demo"}
          {d.state === "done" && "Demo ready"}
          {d.state === "failed" && "Download failed"}
        </span>
        <button className="dl-close" onClick={() => dismissDownload(d.logId)} title="Dismiss">
          ×
        </button>
      </div>
      <p className="dl-label">{d.label}</p>

      {d.state === "queued" && (
        <p className="dl-sub">
          {d.position === 1 ? "Next, once the one before it finishes." : `${d.position ?? 1} ahead of it in the queue.`}
        </p>
      )}

      {d.state === "running" && (
        <>
          <div className="dl-bar" aria-hidden>
            {/* Without a total the server never said how big it is, so the bar
                slides instead of filling. */}
            <span className={pct === null ? "dl-fill dl-unknown" : "dl-fill"} style={pct === null ? undefined : { width: `${pct}%` }} />
          </div>
          <p className="dl-sub">
            {mb(d.bytes)}
            {d.total ? ` of ${mb(d.total)} · ${pct!.toFixed(0)}%` : " so far"}
          </p>
        </>
      )}

      {d.state === "done" && (
        <>
          <p className="dl-sub">Read and linked: every player&apos;s movement is on the match now.</p>
          <button
            className="dl-go"
            onClick={() => {
              onOpenMatch(d.logId);
              dismissDownload(d.logId);
            }}
          >
            Open the match
          </button>
        </>
      )}

      {d.state === "failed" && <p className="dl-sub dl-error">{d.error ?? "Something went wrong."}</p>}
    </div>
  );
}

function mb(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(0)} MB`;
}
