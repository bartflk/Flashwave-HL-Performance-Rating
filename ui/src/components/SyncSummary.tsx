import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import { eta, startSync, useSyncStatus } from "../lib/sync";
import { t, tx } from "../lib/i18n";
import { Spinner } from "./Spinner";

/**
 * What is stored, and the button that fetches more — in the top bar, beside
 * who you are.
 *
 * The progress itself is not here: a sync runs for minutes whatever the
 * window is showing, so it reports from the corner (see `Notifications`)
 * rather than from a bar above whichever page happened to start it. That left
 * a whole strip holding two numbers and a button, which is what the header is
 * for.
 */
export function SyncSummary() {
  const stats = useQuery({ queryKey: ["index_stats"], queryFn: api.indexStats });
  const sync = useSyncStatus();
  const running = sync.state === "running";
  const s = stats.data;

  // Everything the counts do not say out loud, on hover.
  const note = !s
    ? undefined
    : [
        s.pending > 0 ? t("{0} not fetched yet (about {1})", { "0": s.pending.toLocaleString(), "1": eta(s.pending) }) : null,
        s.outsideWindow > 0
          ? t("{0} older matches kept out — Settings, How far back", { "0": s.outsideWindow.toLocaleString() })
          : null,
      ]
        .filter(Boolean)
        .join("\n") || undefined;

  return (
    <div className="sync-summary" title={note}>
      {s ? (
        <span className="ss-counts">
          {tx("{0} matches{1}{2} official{3}{4}", { "0": <strong>{s.highlander.toLocaleString()}</strong>, "1": <span className="sep">·</span>, "2": <strong>{s.officials}</strong>, "3": s.officials === 1 ? "" : "s", "4": (s.pending > 0 || s.outsideWindow > 0) && !running && <span className="ss-more">•</span> })}
        </span>
      ) : (
        <span className="ss-counts muted"><Spinner /> {t("Loading…")}</span>
      )}
      <button className="primary ss-sync" onClick={() => void startSync()} disabled={running}>
        {running ? t("Syncing…") : t("Sync")}
      </button>
    </div>
  );
}
