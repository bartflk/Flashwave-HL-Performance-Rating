import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { ActivityLine, ActivitySite } from "../../api/types";
import { activityPaused, clearActivity, lineText, setActivityPaused, setVerbose, useActivity } from "../../lib/activity";
import { logTail } from "../../lib/diagnostics";
import { copy } from "../../lib/toast";
import { copyReport } from "../../lib/report";
import { t, tx } from "../../lib/i18n";

type Level = ActivityLine["level"];
const LEVELS: Array<[Level | "all", () => string]> = [
  ["all", () => t("Everything")],
  ["error", () => t("Errors")],
  ["warn", () => t("Warnings")],
  ["info", () => t("Info")],
  ["http", () => t("Requests")],
  ["debug", () => t("Debug")],
];
/** Lines drawn at once; the copy takes every line that matches. */
const SHOWN = 400;

/**
 * Settings › Activity log (Flashy: "logging all these things + api statuses
 * and other debug options ... easy to copy from the settings, file location
 * and everything"): the log file and how to get it out, how each site has
 * answered this run, and every line the app wrote, live.
 */
export function ActivityPanel() {
  const { lines, sites, verbose } = useActivity();
  const diag = useQuery({ queryKey: ["diagnostics"], queryFn: api.getDiagnostics }).data;
  const [level, setLevel] = useState<Level | "all">("all");
  const [source, setSource] = useState("");
  const [search, setSearch] = useState("");
  const [paused, setPaused] = useState(activityPaused());

  const sources = useMemo(() => [...new Set(lines.map((l) => l.source))].sort(), [lines]);
  const q = search.trim().toLowerCase();
  const matching = lines.filter(
    (l) => (level === "all" || l.level === level) && (!source || l.source === source) && (!q || l.text.toLowerCase().includes(q) || l.source.includes(q)),
  );
  const shown = matching.slice(-SHOWN).reverse();
  const file = diag?.logFile ?? diag?.logDir ?? null;

  return (
    <div className="panel act-panel">
      <h2>{t("Activity log")}</h2>

      <h3 className="act-h">{t("The log file")}</h3>
      <p className="hint">{t("Every line below is also written to this file, with the two runs before this one. Send it when asking for help.")}</p>
      <div className="act-file">
        {file ? <code className="act-path">{file}</code> : <span className="muted">{t("Could not be opened; the folder may be read-only.")}</span>}
        <div className="act-buttons">
          {file && <button onClick={() => void copy(file, t("the log file's location"))}>{t("Copy path")}</button>}
          {diag && <button onClick={() => void api.revealPath(diag.logFile ?? diag.logDir)}>{t("Open the log folder")}</button>}
          <button onClick={() => void logTail(100_000).then((text) => copy(text, t("the log file")))} title={t("The last 256 KB of this run's file")}>
            {t("Copy the log file")}
          </button>
        </div>
      </div>

      <h3 className="act-h">{t("Sites")}</h3>
      <p className="hint">{t("How each site has answered since the app opened. Unchanged means it said nothing new, so nothing was downloaded again.")}</p>
      {sites.length === 0 ? (
        <p className="hint">{t("Nothing asked yet this run.")}</p>
      ) : (
        <div className="table-wrap" style={{ marginTop: 10 }}>
          <table className="match-table act-sites">
            <thead>
              <tr>
                <th>{t("Site")}</th>
                <th className="num">{t("Requests")}</th>
                <th className="num">{t("Answered")}</th>
                <th className="num">{t("Unchanged")}</th>
                <th className="num">{t("Not found")}</th>
                <th className="num">{t("Failed")}</th>
                <th className="num">{t("Average")}</th>
                <th className="num" title={t("What the site last said is left of its rate limit")}>{t("Limit left")}</th>
                <th>{t("Status")}</th>
              </tr>
            </thead>
            <tbody>
              {sites.map((s) => (
                <SiteRow key={s.host} s={s} />
              ))}
            </tbody>
          </table>
        </div>
      )}

      <h3 className="act-h">{t("Live")}</h3>
      <div className="act-tools">
        <div className="segmented" role="tablist" aria-label={t("Level")}>
          {LEVELS.map(([id, name]) => (
            <button key={id} role="tab" aria-selected={level === id} className={level === id ? "seg active" : "seg"} onClick={() => setLevel(id)}>
              {name()}
            </button>
          ))}
        </div>
        <select value={source} onChange={(e) => setSource(e.target.value)} aria-label={t("Source")}>
          <option value="">{t("Every source")}</option>
          {sources.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <input type="search" value={search} onChange={(e) => setSearch(e.target.value)} placeholder={t("Search the log")} aria-label={t("Search the log")} />
      </div>
      <div className="act-tools">
        <button
          onClick={() => {
            setActivityPaused(!paused);
            setPaused(!paused);
          }}
        >
          {paused ? t("Resume") : t("Pause")}
        </button>
        <button onClick={() => void copy(matching.map(lineText).join("\n"), t("{0} lines", { "0": matching.length }))} disabled={matching.length === 0}>
          {t("Copy log")}
        </button>
        <button onClick={() => void copyReport(null)} title={t("The app's details, every warning and error this run and the log's last lines, ready to paste")}>
          {t("Copy report")}
        </button>
        <button className="linkish" onClick={() => clearActivity()}>
          {t("Clear the view")}
        </button>
        <label className="act-verbose" title={t("Debug lines say far more, for tracking a problem down; for this run only.")}>
          <input type="checkbox" checked={verbose} onChange={(e) => void setVerbose(e.target.checked)} /> {t("Verbose (debug lines)")}
        </label>
        <span className="muted act-count">{tx("{0} of {1} lines", { "0": matching.length.toLocaleString(), "1": lines.length.toLocaleString() })}</span>
      </div>

      <div className="act-log" role="log" aria-live="off">
        {shown.length === 0 ? (
          <p className="hint">{t("Nothing yet: lines appear here as the app works.")}</p>
        ) : (
          shown.map((l) => (
            <div key={l.seq} className={`act-line lvl-${l.level}`}>
              <span className="act-time">{new Date(l.at).toLocaleTimeString(undefined, { hour12: false })}</span>
              <span className="act-lvl">{l.level}</span>
              <span className="act-src">{l.source}</span>
              <span className="act-text">{l.text}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function SiteRow({ s }: { s: ActivitySite }) {
  const resting = s.restingUntil !== null && s.restingUntil > Date.now();
  const status = resting
    ? tx("Asked us to wait until {0}", { "0": new Date(s.restingUntil!).toLocaleTimeString(undefined, { hour12: false, hour: "2-digit", minute: "2-digit" }) })
    : s.lastStatus === "error" || (s.lastStatus !== null && Number(s.lastStatus) >= 400 && s.lastStatus !== "404")
      ? tx("Last request failed: {0}", { "0": s.lastError ?? s.lastStatus })
      : t("Answering");
  const tone = resting ? "act-bad" : status === t("Answering") ? "act-good" : "act-warn";
  return (
    <tr>
      <td>
        <strong>{s.host}</strong>
      </td>
      <td className="num">{s.requests.toLocaleString()}</td>
      <td className="num">{s.ok.toLocaleString()}</td>
      <td className="num">{s.notModified.toLocaleString()}</td>
      <td className="num">{s.notFound.toLocaleString()}</td>
      <td className={s.failed > 0 ? "num team-lost" : "num"}>{s.failed.toLocaleString()}</td>
      <td className="num">{tx("{0} ms", { "0": Math.round(s.avgMs) })}</td>
      <td className="num">{s.rateRemaining ?? "–"}</td>
      <td className={tone} title={s.lastPath ?? undefined}>
        {status}
      </td>
    </tr>
  );
}
