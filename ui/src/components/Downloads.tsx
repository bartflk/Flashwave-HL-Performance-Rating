import { dismissDownload, useDownloads, type Download } from "../lib/downloads";

/**
 * Demo downloads, bottom right, one card each.
 *
 * A SourceTV demo takes half a minute and a hundred megabytes, and you should
 * be able to walk away and keep reading other matches while it runs. The card
 * says how far along it is, and when it lands it offers to take you back to
 * the match it belongs to.
 */
export function Downloads({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
  const items = useDownloads();
  if (items.length === 0) return null;

  return (
    <div className="downloads" role="status" aria-live="polite">
      {items.map((d) => (
        <Card key={d.logId} d={d} onOpenMatch={onOpenMatch} />
      ))}
    </div>
  );
}

function Card({ d, onOpenMatch }: { d: Download; onOpenMatch: (logId: number) => void }) {
  const pct = d.total ? Math.min(100, (d.bytes / d.total) * 100) : null;
  return (
    <div className={`dl dl-${d.state}`}>
      <div className="dl-head">
        <span className="dl-title">
          {d.state === "running" && "Downloading demo"}
          {d.state === "done" && "Demo ready"}
          {d.state === "failed" && "Download failed"}
        </span>
        <button className="dl-close" onClick={() => dismissDownload(d.logId)} title="Dismiss">
          ×
        </button>
      </div>
      <p className="dl-label">{d.label}</p>

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
