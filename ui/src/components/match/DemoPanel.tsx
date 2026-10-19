import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { api, inTauri } from "../../api/client";
import { errorMessage, type DemosTfFound, type DemoLinked, type DemoView, type MatchDetail, type ReRead } from "../../api/types";
import { copy } from "../../lib/toast";
import { beginDownload, failDownload, useDownload } from "../../lib/downloads";
import { beforeDemosTf } from "../../lib/stv";
import { locale, t, tx } from "../../lib/i18n";
import { Spinner } from "../Spinner";

/**
 * Demo linking: the demos behind this match, and two ways to add one --
 * the SourceTV demo from demos.tf, or a demo of your own dropped on the page
 * (Flashy), checked against the log before it is linked.
 *
 * TF2 cannot open a demo and seek in one console line (`demo_gototick` runs
 * before the demo has loaded), so the flow is two steps: open the demo once
 * with `playdemo`, then paste a `demo_gototick` for each moment. Every
 * timeline marker below copies its own tick.
 */
export function DemoPanel({ d }: { d: MatchDetail }) {
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);

  // The download is followed app-wide: it keeps going, and keeps reporting,
  // while you read another match.
  const download = useDownload(d.logId);
  useEffect(() => {
    if (download?.state !== "done") return;
    void qc.invalidateQueries({ queryKey: ["match", d.logId] });
    void qc.invalidateQueries({ queryKey: ["matches"] });
    void qc.invalidateQueries({ queryKey: ["spychecks", d.logId] });
    void qc.invalidateQueries({ queryKey: ["demostats", d.logId] });
    void qc.invalidateQueries({ queryKey: ["cart", d.logId] });
    // The demo names its map, which can settle rounds the log left unknown.
    void qc.invalidateQueries({ queryKey: ["analysis", d.logId] });
  }, [download?.state, d.logId, qc]);

  async function fetchStv() {
    setError(null);
    beginDownload(d.logId, `${d.map ?? "this match"}, log ${d.logId}`);
    try {
      await api.fetchStv(d.logId);
    } catch (e) {
      setError(errorMessage(e));
      failDownload(d.logId, errorMessage(e));
    }
  }

  // A deleted STV is not one this machine has: offering the download
  // again is how it comes back, with the same progress bar as the first
  // time, rather than a second button that does the same thing.
  const hasStv = d.demos.some((x) => x.kind === "stv" && !x.deleted);
  // demos.tf first; ETF2L's own upload for a match demos.tf has none of (Q48).
  const fromEtf2l = d.demosTfId === null && (d.etf2lDemos ?? 0) > 0;
  // Q62: demos.tf no longer has any file from before 14 October 2019.
  const tooOld = beforeDemosTf(d.playedAt);
  const canFetch = ((d.demosTfId !== null && !tooOld) || fromEtf2l) && !hasStv;

  return (
    <section className="panel demo-panel">
      <header className="demo-head">
        <div>
          <h2>{t("Demo linking")}</h2>
          {d.demos.length > 0 && (
            <p className="hint">{tx("Open the demo once, then click any marker on the round timeline to copy its{0}{1}. Jumps land 5 seconds early, so you see the lead-up.", { "0": " ", "1": <code>{t("demo_gototick")}</code> })}</p>
          )}
        </div>
        <ReadAgain d={d} />
      </header>

      {d.demos.map((demo) => (
        <DemoRow key={demo.demoId} demo={demo} />
      ))}

      <div className="demo-sources">
        <div className="demo-source" id="demo-linking-download">
          <strong>{fromEtf2l ? t("SourceTV demo on ETF2L") : t("SourceTV demo on demos.tf")}</strong>
          {canFetch ? (
            <>
              <p className="hint">
                {fromEtf2l
                  ? t("demos.tf has none, but a player uploaded the SourceTV demo to the match's ETF2L page. Every demo there is downloaded and checked against this match's logs (map, players, kills), so each map's demo goes to its own log.")
                  : t("All 18 players, not just your view. A combined log brings every part's demo; a half demos.tf did not link can be added by its link below.")}
              </p>
              {download ? (
                <div className="dl-progress">
                  <div className="progress-track">
                    <div
                      className={download.total ? "progress-fill" : "progress-fill indeterminate"}
                      style={download.total ? { width: `${Math.round((download.bytes / download.total) * 100)}%` } : undefined}
                    />
                  </div>
                  <span className="hint">
                    {mb(download.bytes)}
                    {download.total ? t(" of {0}", { "0": mb(download.total) }) : ""}
                  </span>
                </div>
              ) : (
                <button className="primary" onClick={() => void fetchStv()}>{t("Download to tf/demos/stv")}</button>
              )}
            </>
          ) : (
            <p className="hint">
              {hasStv
                ? t("This match's SourceTV demo is on this machine.")
                : tooOld
                  ? t("demos.tf's demos go back to 14 October 2019; this match is older.")
                  : t("Neither demos.tf nor ETF2L has a SourceTV demo for this match.")}
            </p>
          )}
          {error && <p className="error">{error}</p>}
          {!hasStv && !tooOld && <LookAgain d={d} />}
          <ByLink d={d} />
        </div>
        <DropZone d={d} />
      </div>
    </section>
  );
}

/**
 * Ask demos.tf again for this match (Flashy: "a refresh button so i can
 * refresh to see if demos.tf updated"): an STV is often uploaded after the
 * log was read, and the sync only looks again on its own schedule.
 */
function LookAgain({ d }: { d: MatchDetail }) {
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [got, setGot] = useState<DemosTfFound | null>(null);
  const [error, setError] = useState<string | null>(null);
  async function look() {
    setBusy(true);
    setError(null);
    setGot(null);
    try {
      const r = await api.refreshDemostf(d.logId);
      setGot(r);
      if (r.matched > 0) void qc.invalidateQueries({ queryKey: ["match", d.logId] });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="demo-look-again">
      <button className="km-chip" onClick={() => void look()} disabled={busy} title={t("Ask demos.tf again, for a demo uploaded since the log was read")}>
        {busy ? <Spinner /> : <span aria-hidden>⟳</span>} {d.demosTfId === null ? t("Check demos.tf again") : t("Check demos.tf for more parts")}
      </button>
      {got && (
        <span className="hint">
          {got.matched > 0
            ? t("Found on demos.tf: the download is ready above.")
            : t("Nothing new on demos.tf yet ({0} of your demos around this match checked). Uploads can take a few hours.", { "0": got.listed })}
        </span>
      )}
      {error && <span className="error">{error}</span>}
    </div>
  );
}

/**
 * Q53 (Emiel): a demos.tf link pasted in, downloaded and checked like a
 * dropped demo. For the STV the lookup missed, and for a match's other
 * halves when demos.tf linked only one.
 */
function ByLink({ d }: { d: MatchDetail }) {
  const qc = useQueryClient();
  const [link, setLink] = useState("");
  const [busy, setBusy] = useState(false);
  const [got, setGot] = useState<DemoLinked | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function add() {
    if (!link.trim()) return;
    setBusy(true);
    setError(null);
    setGot(null);
    try {
      const linked = await api.linkDemostf(d.logId, link.trim());
      setGot(linked);
      setLink("");
      for (const key of [["match", d.logId], ["matches"], ["spychecks", d.logId], ["demostats", d.logId], ["cart", d.logId], ["positions", d.logId], ["aim"], ["paths", d.logId], ["analysis", d.logId]]) {
        void qc.invalidateQueries({ queryKey: key });
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="demo-by-link">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <input
          type="text"
          value={link}
          onChange={(e) => setLink(e.target.value)}
          placeholder="https://demos.tf/990239"
          aria-label={t("demos.tf link")}
          disabled={busy}
        />
        <button type="submit" disabled={busy || !link.trim()}>
          {busy ? t("Downloading…") : t("Add by link")}
        </button>
      </form>
      <span className="hint">{t("Have the demos.tf page of this match's STV? Paste its link: it is downloaded and checked against this match first.")}</span>
      {got && (
        <p className="demo-drop-ok">
          {tx("Linked {0}: {1} of the log's {2} kills line up, {3} players in both.", {
            "0": <code>{got.fileName}</code>,
            "1": got.killsMatched,
            "2": got.logKills,
            "3": got.playersShared,
          })}
        </p>
      )}
      {error && <p className="error">{error}</p>}
    </div>
  );
}

/**
 * Read this match again with this version of the app: its demos from their
 * files, then aim and the server log's fights. After an update that reads
 * demos better, the match catches up without a whole rebuild.
 */
function ReadAgain({ d }: { d: MatchDetail }) {
  const qc = useQueryClient();
  const [step, setStep] = useState<string | null>(null);
  const [done, setDone] = useState<ReRead | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let off: (() => void) | undefined;
    void api.onRereadStep((s) => setStep(s.step)).then((f) => (off = f));
    return () => off?.();
  }, []);

  async function run() {
    setError(null);
    setDone(null);
    setStep(t("Starting"));
    try {
      const r = await api.rereadMatch(d.logId);
      setDone(r);
      // Everything on the page that reads the demos or the fights.
      for (const key of ["match", "spychecks", "demostats", "cart", "analysis", "positions", "parts"]) {
        void qc.invalidateQueries({ queryKey: [key, d.logId] });
      }
      // Keyed by player or map as well, and read across matches too.
      for (const key of ["aim", "paths", "mapview", "profile"]) {
        void qc.invalidateQueries({ queryKey: [key] });
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setStep(null);
    }
  }

  return (
    <div className="read-again">
      <button onClick={() => void run()} disabled={step !== null} title={t("Read this match's demos and server log again with this version of the app, after an update")}>
        {step !== null ? <span className="read-again-spin" aria-hidden /> : <span aria-hidden>↻</span>} {step !== null ? t(step) : t("Read again")}
      </button>
      {done && (
        <span className="hint">
          {done.demos > 0
            ? tx("{0} demo{1} read again", { "0": done.demos, "1": done.demos === 1 ? "" : "s" })
            : t("No demo file to read")}
          {done.missing > 0 && t(" · {0} file{1} gone, kept as read", { "0": done.missing, "1": done.missing === 1 ? "" : "s" })}
          {done.fights && t(" · fights redone")}
        </span>
      )}
      {error && <span className="error">{error}</span>}
    </div>
  );
}

/**
 * Drop a .dem here, or click to pick one: the demo is checked against this
 * match (map, players, kills lining up) and linked if it is this match's.
 * Tauri hands dropped files over as paths through its own drag-and-drop
 * event, not the page's, so the zone listens to the window and checks the
 * drop landed on it.
 */
function DropZone({ d }: { d: MatchDetail }) {
  const qc = useQueryClient();
  const zone = useRef<HTMLDivElement>(null);
  const [over, setOver] = useState(false);
  const [busy, setBusy] = useState(false);
  const [got, setGot] = useState<DemoLinked | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function link(path: string) {
    if (!path.toLowerCase().endsWith(".dem")) {
      setError(t("That is not a .dem file."));
      return;
    }
    setBusy(true);
    setError(null);
    setGot(null);
    try {
      const linked = await api.linkDemo(d.logId, path);
      setGot(linked);
      for (const key of [["match", d.logId], ["spychecks", d.logId], ["demostats", d.logId], ["cart", d.logId], ["positions", d.logId], ["aim"], ["paths", d.logId], ["analysis", d.logId]]) {
        void qc.invalidateQueries({ queryKey: key });
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const linkRef = useRef(link);
  linkRef.current = link;

  useEffect(() => {
    if (!inTauri) return;
    let off: (() => void) | undefined;
    let dead = false;
    const within = (x: number, y: number) => {
      const r = zone.current?.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      const [cx, cy] = [x / dpr, y / dpr];
      return !!r && cx >= r.left && cx <= r.right && cy >= r.top && cy <= r.bottom;
    };
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((e) => {
          const p = e.payload;
          if (p.type === "over") setOver(within(p.position.x, p.position.y));
          else if (p.type === "drop") {
            const hit = within(p.position.x, p.position.y);
            setOver(false);
            // A callout file dropped here is the window's to import (Q32).
            const file = p.paths.find((x) => !x.toLowerCase().endsWith(".callouts.json"));
            if (hit && file) void linkRef.current(file);
          } else setOver(false);
        })
        .then((u) => {
          if (dead) u();
          else off = u;
        }),
    );
    return () => {
      dead = true;
      off?.();
    };
  }, []);

  async function pick() {
    if (!inTauri) {
      // The browser build has no paths; a made-up one shows the flow.
      void link("D:\\demos\\match-20260925-2211-koth_proot_b5b.dem");
      return;
    }
    const chosen = await open({ multiple: false, directory: false, filters: [{ name: t("TF2 demo"), extensions: ["dem"] }] });
    if (typeof chosen === "string") void link(chosen);
  }

  return (
    <div
      ref={zone}
      className={over ? "demo-drop over" : busy ? "demo-drop busy" : "demo-drop"}
      role="button"
      tabIndex={0}
      onClick={() => !busy && void pick()}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && !busy && void pick()}
      // The browser build: the page's own drop, which has names, not paths.
      onDragOver={(e) => {
        if (inTauri) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => !inTauri && setOver(false)}
      onDrop={(e) => {
        if (inTauri) return;
        e.preventDefault();
        setOver(false);
        const f = e.dataTransfer.files[0];
        if (f) void link(`D:\\demos\\${f.name}`);
      }}
    >
      {busy ? (
        <>
          <Spinner />
          <strong>{t("Reading the demo…")}</strong>
          <span className="hint">{t("Checking it is this match, then linking it.")}</span>
        </>
      ) : (
        <>
          <span className="demo-drop-icon" aria-hidden>⤓</span>
          <strong>{over ? t("Drop it to link it") : t("Drop a demo here")}</strong>
          <span className="hint">{t("Your own recording or an STV from elsewhere, or click to pick a file. It is checked against this match first.")}</span>
        </>
      )}
      {got && !busy && (
        <p className="demo-drop-ok">
          {tx("Linked {0}: {1} of the log's {2} kills line up, {3} players in both.", {
            "0": <code>{got.fileName}</code>,
            "1": got.killsMatched,
            "2": got.logKills,
            "3": got.playersShared,
          })}
        </p>
      )}
      {error && !busy && <p className="error">{error}</p>}
    </div>
  );
}

function DemoRow({ demo }: { demo: DemoView }) {
  const cmd = `playdemo ${demo.playdemoArg}`;
  return (
    <div className="demo-row">
      <div className="demo-info">
        <div className="demo-name">
          <span className={demo.kind === "stv" ? "badge badge-demo" : "badge badge-pov"}>
            {demo.kind === "stv" ? t("STV") : t("POV")}
          </span>
          <code>{demo.fileName}</code>
        </div>
        <span className="hint">
          {tx("{0} min{1}{2}{3}{4}", { "0": Math.round(demo.durationS / 60), "1": demo.recordedAt !== null && t(" · recorded {0}", { "0": new Date(demo.recordedAt * 1000).toLocaleString(locale()) }), "2": t(" · {0}% of this match inside", { "0": Math.round(demo.logShare * 100) }), "3": demo.markers > 0 && t(" · {markers} killstreak marker{1}", { "1": demo.markers === 1 ? "" : "s", markers: demo.markers }), "4": demo.approximate && t(" · jump positions estimated") })}
        </span>
      </div>
      {demo.deleted ? (
        // Handing over a playdemo command for a file that is not there
        // sends someone into TF2 to be told so by the console.
        <span className="hint" title={t("Deleted to save space. Everything read from it is still here.")}>{t("deleted · download again below")}</span>
      ) : (
        <button className="primary" onClick={() => void copy(cmd, "playdemo command")} title={cmd}>{t("Copy playdemo")}</button>
      )}
    </div>
  );
}

function mb(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}
