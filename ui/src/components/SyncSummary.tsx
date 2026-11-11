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
        // Said out loud too, except in a narrow window, where only this is left.
        [
          s.highlander === 1 ? t("1 match") : t("{0} matches", { "0": s.highlander.toLocaleString() }),
          s.officials === 1 ? t("1 official") : t("{0} officials", { "0": s.officials }),
        ].join(" · "),
        s.pending > 0 ? t("{0} not fetched yet (about {1})", { "0": s.pending.toLocaleString(), "1": eta(s.pending) }) : null,
        s.outsideWindow > 0
          ? t("{0} older matches kept out — Settings, How far back", { "0": s.outsideWindow.toLocaleString() })
          : null,
      ]
        .filter(Boolean)
        .join("\n") || undefined;

  const more = (s?.pending ?? 0) > 0 || (s?.outsideWindow ?? 0) > 0 ? !running && <span className="ss-more">•</span> : false;

  return (
    <div className="sync-summary" title={note}>
      {s ? (
        <span className="ss-counts">
          {s.highlander === 1 ? tx("{0} match", { "0": <strong>1</strong> }) : tx("{0} matches", { "0": <strong>{s.highlander.toLocaleString()}</strong> })}
          <span className="sep">·</span>
          {s.officials === 1 ? tx("{0} official", { "0": <strong>1</strong> }) : tx("{0} officials", { "0": <strong>{s.officials}</strong> })}
          {more}
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
