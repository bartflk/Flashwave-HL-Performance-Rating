import { useEffect, useState } from "react";
import { Help } from "./StatTip";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage } from "../api/types";
import { locale, t } from "../lib/i18n";
import { Spinner } from "./Spinner";
import { SortTh, useSort } from "../lib/sort";

/**
 * The league sample (Flashy): officials from every ETF2L division, fetched
 * slowly in the background so ratings can be read against the league and a
 * player can be placed in the division they play in. Off until switched on:
 * it is hours of requests against community servers.
 */
export function LeagueSamplePanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["league_sample"], queryFn: api.getLeagueSample, refetchInterval: 10_000 });
  const s = q.data;
  const sort = useSort("league-sample", s?.tiers ?? [], {
    division: (x) => -x.tier,
    matches: (x) => x.matches,
    logs: (x) => (x.logs > 0 ? (x.jsonLogstf + x.jsonDropstf + x.jsonMoretf) / x.logs : null),
    raw: (x) => (x.logs > 0 ? (x.raw + x.rawMissing) / x.logs : null),
    rosters: (x) => (x.matches > 0 ? x.rosters / x.matches : null),
    maps: (x) => x.maps,
    from: (x) => x.oldest,
  });

  const toggle = async (on: boolean) => {
    await api.setLeagueSample(on);
    void qc.invalidateQueries({ queryKey: ["league_sample"] });
  };

  const totals = s?.tiers.reduce(
    (a, x) => ({ logs: a.logs + x.logs, json: a.json + x.jsonLogstf + x.jsonDropstf + x.jsonMoretf, raw: a.raw + x.raw + x.rawMissing, matches: a.matches + x.matches, rosters: a.rosters + x.rosters }),
    { logs: 0, json: 0, raw: 0, matches: 0, rosters: 0 },
  );
  const phase = !s
    ? ""
    : !s.enabled
      ? t("Paused.")
      : s.discoveredAt === null
        ? t("Listing ETF2L's officials and choosing the matches (about ten minutes)…")
        : totals && totals.json < totals.logs
          ? s.logstfResting
            ? t("logs.tf asked us to slow down: resting for a few minutes, with more.tf filling in.")
            : t("Downloading the logs…")
          : totals && totals.raw < totals.logs
            ? t("Downloading the server logs…")
            : t("Everything chosen is here. New officials are looked for once a day.");
  const year = (at: number | null) => (at ? new Date(at * 1000).toLocaleDateString(locale(), { year: "numeric", month: "short" }) : "–");

  return (
    <div className="panel league-panel">
      <h2>{t("League sample")}<Help>{t("Every ETF2L Highlander official of the last six years, playoffs and preseason cups included, downloaded slowly in the background while the app is open. Kept apart from your own matches: it lets ratings be read against the whole league, and tells you which division the players you face come from.")}</Help></h2>
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {s && (
        <>
          <div className="row" style={{ marginTop: 12, gap: 10, alignItems: "center", flexWrap: "wrap" }}>
            <button className={s.enabled ? undefined : "primary"} onClick={() => void toggle(!s.enabled)}>
              {s.enabled ? t("Pause") : s.discoveredAt === null ? t("Start") : t("Resume")}
            </button>
            <span className="hint">{phase}</span>
          </div>
          <ActivityBar />
          {s.tiers.length > 0 && (
            <>
              <table className="league-table">
                <thead>
                  <tr>
                    <SortTh k="division" s={sort}>{t("Division")}</SortTh>
                    <SortTh k="matches" s={sort}>{t("Matches")}</SortTh>
                    <SortTh k="logs" s={sort}>{t("Logs")}</SortTh>
                    <SortTh k="raw" s={sort}>{t("Server logs")}</SortTh>
                    <SortTh k="rosters" s={sort}>{t("Rosters")}</SortTh>
                    <SortTh k="maps" s={sort}>{t("Maps")}</SortTh>
                    <SortTh k="from" s={sort}>{t("From")}</SortTh>
                  </tr>
                </thead>
                <tbody>
                  {sort.sorted.map((x) => (
                    <tr key={x.tier}>
                      <th scope="row">{x.division}</th>
                      <td>{x.matches}</td>
                      <td>
                        <Bar done={x.jsonLogstf + x.jsonDropstf + x.jsonMoretf} of={x.logs} />
                        {x.jsonDropstf > 0 && <span className="muted" title={t("logs.tf's own logs, from drops.tf by Icewind")}> ({x.jsonDropstf} drops.tf)</span>}
                        {x.jsonMoretf > 0 && <span className="muted" title={t("From more.tf while logs.tf was resting; asked of logs.tf again later")}> ({x.jsonMoretf} more.tf)</span>}
                      </td>
                      <td>
                        <Bar done={x.raw + x.rawMissing} of={x.logs} />
                      </td>
                      <td>
                        <Bar done={x.rosters} of={x.matches} />
                      </td>
                      <td>{x.maps}</td>
                      <td className="muted">
                        {year(x.oldest)} – {year(x.newest)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <p className="hint" style={{ marginTop: 8 }}>
                {t("Player catalogue: {0} players from {1} of {2} officials' rosters.", { "0": s.players.toLocaleString(), "1": s.rostersRead.toLocaleString(), "2": s.officials.toLocaleString() })}{" "}
                {t("{0} ETF2L logs listed; {1} MB held so far.", { "0": s.logsListed.toLocaleString(), "1": (s.bytes / 1e6).toFixed(0) })}{" "}

              </p>
            </>
          )}
        </>
      )}
    </div>
  );
}

function Bar({ done, of }: { done: number; of: number }) {
  const pct = of > 0 ? Math.min(100, (done / of) * 100) : 0;
  return (
    <span className="league-bar" title={`${done} / ${of}`}>
      <span className="progress-track">
        <span className="progress-fill" style={{ width: `${pct}%` }} />
      </span>
      <span className="league-bar-n">
        {done}/{of}
      </span>
    </span>
  );
}

function stateLabel(state: string): string {
  switch (state) {
    case "starting":
      return t("Starting");
    case "working":
      return t("Working");
    case "waiting":
      return t("Waiting");
    case "resting":
      return t("Throttled");
    case "sync":
      return t("Your sync first");
    case "paused":
      return t("Paused");
    case "done":
      return t("Up to date");
    default:
      return state;
  }
}

/**
 * What the job is doing this second (Flashy): a job that spends most of its
 * time waiting between requests looks stuck without it. Polled every two
 * seconds; the countdown ticks every second in between.
 */
function ActivityBar() {
  const q = useQuery({ queryKey: ["league_activity"], queryFn: api.getLeagueActivity, refetchInterval: 2000 });
  const [nowS, setNowS] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const id = window.setInterval(() => setNowS(Math.floor(Date.now() / 1000)), 1000);
    return () => window.clearInterval(id);
  }, []);
  const a = q.data;
  if (!a) return null;
  const left = a.nextAt !== null ? Math.max(0, a.nextAt - nowS) : null;
  const line =
    a.doing ??
    (a.state === "resting"
      ? t("logs.tf asked us to slow down. Back to it in {0}.", { "0": clock(a.logstfRestLeft ?? left ?? 0) })
      : a.state === "sync"
        ? t("Waiting for your own sync to finish.")
        : a.state === "paused"
          ? t("Paused.")
          : a.state === "done"
            ? t("Nothing left to fetch. Looking for new officials again in {0}.", { "0": clock(left ?? 0) })
            : left !== null
              ? t("Next request in {0}.", { "0": clock(left) })
              : t("Starting…"));
  const ago = (at: number) => clock(Math.max(0, nowS - at));
  const fill = a.state === "working" || left === null ? undefined : { width: `${Math.max(0, 100 - (left / Math.max(1, gapOf(a.state))) * 100)}%` };
  return (
    <div className={`league-activity league-${a.state}`} role="status" aria-live="polite">
      <div className="league-activity-top">
        <span className="league-state">
          {a.state === "working" && <Spinner />}
          {stateLabel(a.state)}
        </span>
        <span className="league-doing">{line}</span>
        <span className="league-rate" title={t("Requests to logs.tf, more.tf, trends.tf and ETF2L in the last hour")}>
          {t("{0} requests in the last hour", { "0": a.lastHour.toLocaleString() })}
        </span>
      </div>
      <div className="league-activity-track" aria-hidden>
        {/* Between requests: how much of the wait has passed. */}
        <span className={a.state === "working" ? "league-activity-fill indeterminate" : "league-activity-fill"} style={fill} />
      </div>
      {a.recent.length > 0 && (
        <ol className="league-recent">
          {a.recent.map((e, i) => (
            <li key={`${e.at}-${i}`} className={e.ok ? undefined : "bad"}>
              <span className="league-recent-ago">{t("{0} ago", { "0": ago(e.at) })}</span>
              <span>{e.text}</span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

/** The wait the job takes after a step in this state, in seconds. */
function gapOf(state: string): number {
  return state === "resting" ? 60 : state === "done" ? 1800 : state === "paused" || state === "sync" ? 20 : 4;
}

function clock(s: number): string {
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m}m ${s % 60}s` : `${Math.floor(m / 60)}h ${m % 60}m`;
}
