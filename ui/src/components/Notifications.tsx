import { useEffect } from "react";
import { dismissDownload, useDownloads, type Download } from "../lib/downloads";
import { cancelSync, dismissSync, fractionOf, labelOf, matchOf, phaseOf, useSyncStatus } from "../lib/sync";
import { dismissDemoSeen, useDemoSeen } from "../lib/demowatch";
import { dismissNewLogs, useNewLogs } from "../lib/newlogs";
import { dismissEtf2lCheck, useEtf2lCheck } from "../lib/etf2lcheck";
import { formatDate } from "../lib/format";
import { dismissUpdate, installUpdate, restartNow, useUpdate } from "../lib/update";
import { t, tx } from "../lib/i18n";
import { Spinner } from "./Spinner";
import type { SyncReport } from "../api/types";

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
  const demo = useDemoSeen();
  const fresh = useNewLogs();
  const update = useUpdate();
  const etf2l = useEtf2lCheck();
  const quiet = update.state === "idle" || update.state === "checking" || update.state === "current";
  if (downloads.length === 0 && sync.state === "idle" && !demo && !fresh && !etf2l && quiet) return null;

  return (
    <div className="downloads" role="status" aria-live="polite">
      <UpdateCard />
      <NewLogsCard />
      <Etf2lCheckCard />
      <DemoSeenCard />
      <SyncCard />
      {downloads.map((d) => (
        <DownloadCard key={d.logId} d={d} onOpenMatch={onOpenMatch} />
      ))}
    </div>
  );
}

/**
 * What the daily ETF2L check changed (Flashy): a medal won since the last
 * look -- "DD14 won S36 High" -- or moved to ETF2L's own final standings.
 */
function Etf2lCheckCard() {
  const c = useEtf2lCheck();
  if (!c) return null;
  const place = (p: number) => (p === 1 ? t("won") : p === 2 ? t("runner-up in") : t("3rd in"));
  const shown = c.changed.slice(0, 6);
  return (
    <div className="dl dl-done">
      <div className="dl-head">
        <span className="dl-title">{t("ETF2L checked")}</span>
        <button className="dl-close" onClick={() => dismissEtf2lCheck()} title={t("Dismiss")}>
          ×
        </button>
      </div>
      <p className="dl-label">{c.changed.length === 1 ? t("1 medal is new") : t("{0} medals are new", { "0": c.changed.length })}</p>
      <ul className="dl-report">
        {shown.map((m) => (
          <li key={`${m.season}-${m.division}-${m.place}-${m.teamId}`} title={m.how}>
            {t("{team} {placed} {division}, {season}", { team: m.team, placed: place(m.place), division: m.division, season: m.seasonName || `S${m.season}` })}
          </li>
        ))}
      </ul>
      {c.changed.length > shown.length && <p className="dl-sub">{t("and {0} more", { "0": c.changed.length - shown.length })}</p>}
      <p className="dl-sub dl-took">{t("Medals checked against ETF2L's final standings in {0} divisions.", { "0": c.confirmed })}</p>
    </div>
  );
}

/** Why a sync started by itself: new Highlander logs since the app was last open. */
function NewLogsCard() {
  const n = useNewLogs();
  if (!n) return null;
  return (
    <div className="dl dl-running">
      <div className="dl-head">
        <span className="dl-title">{t("New logs")}</span>
        <button className="dl-close" onClick={() => dismissNewLogs()} title={t("Dismiss")}>
          ×
        </button>
      </div>
      <p className="dl-label">
        {n.count === 1 ? t("1 new Highlander log") : t("{0} new Highlander logs", { "0": n.count })}
        {n.since !== null && <> {t("since {0}", { "0": formatDate(n.since) })}</>}
      </p>
      <p className="dl-sub">{tx("Syncing them now, from {0}.", { "0": n.source })}</p>
    </div>
  );
}

/**
 * A new version, and the two clicks it takes to be on it.
 *
 * Nothing happens without being asked. The app holds a lock on the
 * database while its window is open, so an installer swapping files under
 * a running process is precisely the shape of the thing that corrupted it
 * twice — download, then restart, in that order and on purpose.
 */
function UpdateCard() {
  const u = useUpdate();
  if (u.state === "idle" || u.state === "checking" || u.state === "current") return null;

  const pct =
    u.state === "downloading" && u.total ? Math.min(100, (u.got / u.total) * 100) : null;

  return (
    <div className={u.state === "failed" ? "dl dl-failed" : "dl dl-running"}>
      <div className="dl-head">
        <span className="dl-title">
          {u.state === "available" && t("Update available")}
          {u.state === "downloading" && t("Downloading update")}
          {u.state === "ready" && t("Update ready")}
          {u.state === "failed" && t("Update failed")}
        </span>
        <button className="dl-close" onClick={() => dismissUpdate()} title={t("Dismiss")}>
          ×
        </button>
      </div>

      {u.state === "available" && (
        <>
          <p className="dl-label">{tx("Version {version}", { version: u.version })}</p>
          {u.notes && <p className="dl-sub up-notes">{u.notes.replace(/\s+/g, " ").slice(0, 160)}</p>}
          <button className="dl-go" onClick={() => void installUpdate()}>{u.manual ? t("Open the download page") : t("Download and install")}</button>
        </>
      )}

      {u.state === "downloading" && (
        <>
          <p className="dl-label">{tx("Version {version}", { version: u.version })}</p>
          <div className="dl-bar" aria-hidden>
            <span
              className={pct === null ? "dl-fill dl-unknown" : "dl-fill"}
              style={pct === null ? undefined : { width: `${pct}%` }}
            />
          </div>
          <p className="dl-sub">
            {tx("{0} MB{1}", { "0": (u.got / 1_000_000).toFixed(0), "1": u.total ? t(" of {0} MB", { "0": (u.total / 1_000_000).toFixed(0) }) : t(" so far") })}
          </p>
        </>
      )}

      {u.state === "ready" && (
        <>
          <p className="dl-sub">{tx("Version {version} is installed. Restart to use it.", { version: u.version })}</p>
          <button className="dl-go" onClick={() => void restartNow()}>{t("Restart now")}</button>
        </>
      )}

      {u.state === "failed" && <p className="dl-sub dl-error">{u.message}</p>}
    </div>
  );
}

/**
 * "You just played a game." Shown from the moment TF2 finishes writing the
 * demo until the match is on the page.
 */
function DemoSeenCard() {
  const d = useDemoSeen();
  const sync = useSyncStatus();
  // Its log is fetched: the sync card beside it says the rest.
  const fetched = d?.state === "syncing" && (sync.state === "done" || sync.state === "error");
  useEffect(() => {
    if (fetched) dismissDemoSeen();
  }, [fetched]);
  if (!d || fetched) return null;
  const gaveUp = d.state === "gaveup";
  return (
    <div className={gaveUp ? "dl dl-failed" : "dl dl-running"}>
      <div className="dl-head">
        <span className="dl-title">{gaveUp ? t("No log yet") : t("New demo")}</span>
        <button className="dl-close" onClick={() => dismissDemoSeen()} title={t("Dismiss")}>
          ×
        </button>
      </div>
      <p className="dl-label">{d.fileName}</p>
      <p className="dl-sub">
        {!gaveUp && <Spinner />} {gaveUp
          ? t("logs.tf has nothing for this match yet. Press Sync once it is uploaded.")
          : d.state === "waiting"
            ? t("Giving the server a few seconds to upload the log…")
            : d.state === "syncing"
              ? t("The log is up. Fetching it…")
              : t("Looking for the log — logs.tf can take a minute (look {tries}).", { tries: d.tries })}
      </p>
    </div>
  );
}

/**
 * What each step of the sync did (Flashy: "make the sync toast be more
 * descriptive"): one line per step that brought something in, the steps
 * that failed with their reason, and how long it took. A step with nothing
 * to do is left out rather than listed as zero.
 */
function SyncReportList({ r }: { r: SyncReport }) {
  const n = (x: number) => x.toLocaleString();
  const lines: string[] = [];
  if (r.demosTfLinked > 0) lines.push(t("{0} matches linked to their demos.tf demo", { "0": n(r.demosTfLinked) }));
  if (r.combinedParts > 0) lines.push(t("{0} logs found to be parts of combined logs", { "0": n(r.combinedParts) }));
  if (r.partsFetched > 0) lines.push(t("{0} per-map logs of combined logs downloaded", { "0": n(r.partsFetched) }));
  if (r.rawFetched > 0) lines.push(t("{0} raw server logs read ({1} kills)", { "0": n(r.rawFetched), "1": n(r.rawKills) }));
  if (r.etf2lResults > 0 || r.etf2lDetails > 0) lines.push(t("{0} ETF2L results and {1} match pages read", { "0": n(r.etf2lResults), "1": n(r.etf2lDetails) }));
  if (r.demosLinked > 0) lines.push(t("{0} demos from your folder linked to matches", { "0": n(r.demosLinked) }));
  if (r.demosRead > 0) lines.push(t("{0} demos read for aim, routes and deaths", { "0": n(r.demosRead) }));
  if (r.demosCleaned > 0) lines.push(t("{0} read demos deleted, as set in Settings", { "0": n(r.demosCleaned) }));
  const took = r.seconds < 60 ? t("{0} s", { "0": r.seconds }) : t("{0} min {1} s", { "0": Math.floor(r.seconds / 60), "1": r.seconds % 60 });
  return (
    <>
      {lines.length > 0 && (
        <ul className="dl-report">
          {lines.map((l) => (
            <li key={l}>{l}</li>
          ))}
        </ul>
      )}
      {r.problems.map((p) => (
        <p key={p} className="dl-sub dl-error">
          {p}
        </p>
      ))}
      <p className="dl-sub dl-took">{t("Took {0}.", { "0": took })}</p>
    </>
  );
}

function SyncCard() {
  const sync = useSyncStatus();
  if (sync.state === "idle") return null;

  const done = sync.state === "done";
  const failed = sync.state === "error";
  const cancelled = sync.state === "cancelled";
  const rebuilding = sync.state === "running" && sync.progress?.kind === "reprocessing";
  const fraction = sync.state === "running" ? fractionOf(sync.progress) : null;
  const phase = sync.state === "running" ? phaseOf(sync.progress) : null;

  return (
    <div className={`dl ${done ? "dl-done" : failed ? "dl-failed" : "dl-running"}`}>
      <div className="dl-head">
        <span className="dl-title">
          {/* A rebuild sends the same events as a sync and only names itself
              at the end, so the phase it is in says which one this is. */}
          {sync.state === "running" &&
            (sync.progress?.kind === "reprocessing" ? t("Rebuilding") : t("Syncing"))}
          {done && (sync.result.kind === "reprocess" ? t("Rebuilt") : t("Sync finished"))}
          {failed && t("Sync failed")}
          {cancelled && t("Sync cancelled")}
        </span>
        {/* A running sync has no close button: stopping it is not something
            this card can do, and a card that hides itself would only make
            the progress harder to find. */}
        {phase && <span className="dl-step">{t("Step {0} of {1}", { "0": phase.step, "1": phase.of })}</span>}
        {sync.state !== "running" && (
          <button className="dl-close" onClick={dismissSync} title={t("Dismiss")}>
            ×
          </button>
        )}
      </div>

      {sync.state === "running" && (
        <>
          {phase && (
            <>
              <p className="dl-phase">{phase.title}</p>
              {phase.about && <p className="dl-sub dl-about">{phase.about}</p>}
            </>
          )}
          <p className="dl-label">{labelOf(sync.progress)}</p>
          {matchOf(sync.progress) && <p className="dl-sub dl-match">{matchOf(sync.progress)}</p>}
          {/* A rebuild rewrites every table and has to finish; a sync can stop. */}
          {!rebuilding && (
            <button className="dl-cancel" onClick={() => void cancelSync()}>
              {t("Cancel")}
            </button>
          )}
          <div className="dl-bar" aria-hidden>
            <span
              className={fraction === null ? "dl-fill dl-unknown" : "dl-fill"}
              style={fraction === null ? undefined : { width: `${Math.round(fraction * 100)}%` }}
            />
          </div>
          {sync.failures > 0 && <p className="dl-sub dl-error">{tx("{failures} failed", { failures: sync.failures })}</p>}
        </>
      )}

      {cancelled && <p className="dl-sub">{t("Stopped. The matches downloaded so far are kept; the next sync carries on from there.")}</p>}

      {done && (
        <p className="dl-sub">
          {sync.result.kind === "reprocess"
            ? t("Every match rebuilt from stored data.")
            : sync.result.fetched === 0
              ? t("Up to date — no new matches.")
              : t("{fetched} new match{1}, rated and in the list.", { "1": sync.result.fetched === 1 ? "" : "es", fetched: sync.result.fetched })}
          {sync.result.failed > 0 && (
            <span className="dl-error"> {tx("{failed} failed; next sync retries them.", { failed: sync.result.failed })}</span>
          )}
        </p>
      )}

      {done && sync.result.report && <SyncReportList r={sync.result.report} />}

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
          {d.state === "queued" && t("Waiting to download")}
          {d.state === "running" && (d.stage ? t("Working on the demo") : t("Downloading demo"))}
          {d.state === "done" && t("Demo ready")}
          {d.state === "failed" && t("Download failed")}
        </span>
        <button className="dl-close" onClick={() => dismissDownload(d.logId)} title={t("Dismiss")}>
          ×
        </button>
      </div>
      <p className="dl-label">{t(d.label)}</p>

      {d.state === "queued" && (
        <p className="dl-sub">
          {d.position === 1 ? t("Next, once the one before it finishes.") : t("{0} ahead of it in the queue.", { "0": d.position ?? 1 })}
        </p>
      )}

      {d.state === "running" && <DemoSteps d={d} pct={pct} />}

      {d.state === "done" && (
        <>
          <p className="dl-sub">{t("Read and linked: every player's movement is on the match now.")}</p>
          <button
            className="dl-go"
            onClick={() => {
              onOpenMatch(d.logId);
              dismissDownload(d.logId);
            }}
          >{t("Open the match")}</button>
        </>
      )}

      {d.state === "failed" && <p className="dl-sub dl-error">{d.error ?? t("Something went wrong.")}</p>}
    </div>
  );
}

/**
 * Everything between the click and "ready", as a list of steps: the download,
 * then linking, reading and saving. The last three used to happen behind a
 * bar stuck at 100%, which read as the app having hung.
 */
function DemoSteps({ d, pct }: { d: Download; pct: number | null }) {
  const s = d.stage;
  // Which step is under way: the download until the backend says otherwise.
  const at = !s ? 0 : s.step === "linking" ? 1 : s.step === "saving" ? 3 : 2;
  const whose = (kind: string | null) => (kind === "pov" ? t("your recording") : t("the server's recording"));
  const steps: Array<{ label: string; detail?: string; bar?: number | null }> = [
    {
      label: at === 0 ? t("Downloading") : t("Downloaded"),
      detail: at === 0
        ? mb(d.bytes) + (d.total ? t(" of {0} · {1}%", { "0": mb(d.total), "1": pct!.toFixed(0) }) : t(" so far"))
        : mb(d.total ?? d.bytes),
      // Without a total the server never said how big it is, so the bar
      // slides instead of filling.
      bar: at === 0 ? pct : undefined,
    },
    { label: at > 1 ? t("Linked to its match") : t("Linking it to its match") },
    {
      label: at > 2 ? t("Read") : s?.step === "keeping" ? t("Keeping it") : t("Reading the demo"),
      detail:
        at !== 2 || !s
          ? undefined
          : s.step === "keeping"
            ? t("Compressing {0} into its timeline", { "0": whose(s.kind) })
            : (s.of ?? 1) > 1
              ? t("{0}, {1} of {2} · {3}%", { "0": whose(s.kind), "1": s.demo ?? 1, "2": s.of ?? 1, "3": s.pct ?? 0 })
              : t("{0} · {1}%", { "0": whose(s.kind), "1": s.pct ?? 0 }),
      bar: at === 2 && s?.step === "reading" ? s.pct ?? 0 : undefined,
    },
    { label: t("Saving aim, deaths and movement") },
  ];
  return (
    <ol className="dl-steps" aria-live="polite">
      {steps.map((step, i) => (
        <li key={i} className={i < at ? "dl-step done" : i === at ? "dl-step now" : "dl-step todo"}>
          <span className="dl-mark" aria-hidden>
            {i === at && <span className="spin" />}
          </span>
          <span className="dl-step-body">
            <span>{step.label}</span>
            {i === at && step.detail && <span className="dl-sub">{step.detail}</span>}
            {i === at && step.bar !== undefined && (
              <span className="dl-bar" aria-hidden>
                <span className={step.bar === null ? "dl-fill dl-unknown" : "dl-fill"} style={step.bar === null ? undefined : { width: `${step.bar}%` }} />
              </span>
            )}
          </span>
        </li>
      ))}
    </ol>
  );
}

function mb(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(0)} MB`;
}
