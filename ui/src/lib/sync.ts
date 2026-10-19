import { useSyncExternalStore } from "react";
import type { QueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { noteError } from "./problems";
import { errorMessage, type Progress, type SyncDone } from "../api/types";
import { t } from "./i18n";
import { formatDate } from "./format";
import { loadEtf2lNames } from "./etf2lNames";
import { checkEtf2lDaily } from "./etf2lcheck";

/**
 * A sync, tracked app-wide rather than by the strip that started it.
 *
 * A sync runs for minutes in the backend whatever the window is showing, so
 * its progress belongs beside the demo downloads in the corner rather than in
 * a bar across the top of one page. The backend's events are listened to once,
 * here; anything that wants to show them reads this store.
 */

export type SyncState =
  | { state: "idle" }
  | { state: "running"; progress: Progress | null; failures: number; notes: string[] }
  | { state: "done"; result: SyncDone; notes: string[] }
  | { state: "error"; message: string }
  /** Stopped with Cancel: what landed before is kept. */
  | { state: "cancelled" };

/** How often the match list may refresh while logs are arriving. Newest are
 *  fetched first, so last night's game shows up within seconds — but a
 *  refresh per log would re-render the list for an hour straight. */
const REFRESH_EVERY_MS = 1000;

/** Seconds per log until this sync has timed a few of its own: drops.tf's
 *  pace (four requests a second) plus storing and rating. */
const SECONDS_PER_LOG = 0.75;

/** When this sync's downloads started, and how many were done then: the
 *  estimate follows the pace actually seen, whichever site is answering. */
let pace: { at: number; done: number } | null = null;

/** Seconds a log is taking in this sync, once there is enough to tell. */
function secondsPerLog(done: number): number {
  if (!pace || done - pace.done < 3) return SECONDS_PER_LOG;
  return (Date.now() - pace.at) / 1000 / (done - pace.done);
}

let status: SyncState = { state: "idle" };
let listeners: Array<() => void> = [];
let started = false;
let refreshTimer: number | null = null;

function set(next: SyncState) {
  status = next;
  listeners.forEach((l) => l());
}

/** Everything a finished sync could have changed. */
function invalidateAll(qc: QueryClient) {
  for (const key of [
    "matches",
    "index_stats",
    "profile",
    "match",
    "teammates",
    "context_counts",
    "rawlog_stats",
    "played_filters",
    "all_history",
  ]) {
    void qc.invalidateQueries({ queryKey: [key] });
  }
}

/** Start listening to the backend's sync events. Safe to call twice. */
export function watchSync(qc: QueryClient) {
  if (started) return;
  started = true;

  // A sync may already be running: one started before the window reloaded.
  void api.syncBusy().then((busy) => {
    if (busy && status.state === "idle") {
      set({ state: "running", progress: null, failures: 0, notes: [] });
    }
  });

  void api.onSync({
    onProgress: (p) => {
      const was = status.state === "running" ? status : null;
      // What a source could not give us, kept for the card at the end: a
      // sync that carried on without logs.tf succeeded, but not completely.
      const note =
        p.kind === "sourceFailed"
          ? t("{source} could not be reached; the sync carried on without it.", { source: p.source })
          : p.kind === "gaveUp"
            ? t("{source} stopped answering after {1} of {2} {what}; the rest waits for the next sync.", { "1": p.done.toLocaleString(), "2": p.total.toLocaleString(), source: p.source, what: gaveUpWhat(p.what) })
            : null;
      // Every failure is written down with its reason. The counter alone
      // ("2 failed") was all anyone ever saw, and it cannot be acted on.
      if (p.kind === "fetchFailed") {
        noteError({ what: `fetching log ${p.logId}`, message: explain(p.error), detail: p.error });
      } else if (p.kind === "sourceFailed") {
        noteError({ what: p.source, message: explain(p.error), detail: p.error });
      } else if (p.kind === "gaveUp") {
        noteError({
          what: p.source,
          message: `stopped answering after ${p.done} of ${p.total} ${p.what}; the rest waits for the next sync`,
        });
      }
      set({
        state: "running",
        progress: p,
        failures: (was?.failures ?? 0) + (p.kind === "fetchFailed" ? 1 : 0),
        notes: note && !was?.notes.includes(note) ? [...(was?.notes ?? []), note] : (was?.notes ?? []),
      });
      // Each log is rated as it lands, so a refresh shows finished rows.
      if (p.kind === "fetching" && p.done > 0 && refreshTimer === null) {
        refreshTimer = window.setTimeout(() => {
          refreshTimer = null;
          void qc.invalidateQueries({ queryKey: ["matches"] });
        }, REFRESH_EVERY_MS);
      }
    },
    onDone: (result) => {
      set({
        state: "done",
        result,
        notes: status.state === "running" ? status.notes : [],
      });
      invalidateAll(qc);
      // A sync can bring new official rosters, and with them new names.
      void loadEtf2lNames();
      // The daily ETF2L check, if the sync kept it from running at startup.
      void checkEtf2lDaily(qc);
    },
    onError: (e) => {
      if (e.kind === "cancelled") {
        set({ state: "cancelled" });
        invalidateAll(qc);
        return;
      }
      noteError({ what: "the sync", message: explain(e.message), detail: e.message });
      set({ state: "error", message: e.message });
    },
  });
}

/**
 * A server's error in words a player can act on.
 *
 * The raw chain is kept as the detail; this is the line shown first. Three
 * of these cover almost everything a sync hits, and the difference between
 * them matters: one is worth retrying, one never will be, and one is not
 * about this app at all.
 */
export function explain(raw: string): string {
  const m = raw.toLowerCase();
  if (m.includes("10060") || m.includes("timed out") || m.includes("error sending request")) {
    return t("could not reach the server — it may be down, or the connection dropped");
  }
  if (m.includes("404") || m.includes("not found")) {
    return t("the server does not have this log");
  }
  // logs.tf turns a connection away for a while once it has had too many
  // requests from it; nothing in the app fixes that but waiting.
  if (m.includes("403") || m.includes("forbidden")) {
    return t("the server is refusing requests from this connection for now, usually after too many; try again later");
  }
  if (m.includes("429") || m.includes("too many")) {
    return t("asked for too much too quickly; it will be retried");
  }
  if (m.includes("500") || m.includes("502") || m.includes("503")) {
    return t("the server answered with an error of its own");
  }
  if (m.includes("json") || m.includes("expected")) {
    return t("the answer was not in the shape we expect");
  }
  // Long chains read badly in a list; the whole thing is in the detail.
  return raw.length > 160 ? `${raw.slice(0, 157)}…` : raw;
}

/** Ask for a sync, and show it as running from the click rather than from the
 *  first event — indexing takes a few seconds before anything is reported. */
/** Stop the running sync (Flashy). The card says so when the backend has. */
export async function cancelSync() {
  try {
    await api.syncCancel();
  } catch (e) {
    noteError({ what: "cancelling the sync", message: errorMessage(e) });
  }
}

export async function startSync(full = false) {
  set({ state: "running", progress: null, failures: 0, notes: [] });
  try {
    await api.syncStart(full);
  } catch (e) {
    set({ state: "error", message: errorMessage(e) });
  }
}

/** The same, for rebuilding every match from stored data. */
export async function startRebuild() {
  set({ state: "running", progress: null, failures: 0, notes: [] });
  try {
    await api.reprocessStart();
  } catch (e) {
    set({ state: "error", message: errorMessage(e) });
  }
}

export function dismissSync() {
  if (status.state === "done" || status.state === "error" || status.state === "cancelled") set({ state: "idle" });
}

export function useSyncStatus(): SyncState {
  return useSyncExternalStore(
    (l) => {
      listeners.push(l);
      return () => {
        listeners = listeners.filter((x) => x !== l);
      };
    },
    () => status,
  );
}

/** How far along, 0 to 1, or null when a phase cannot say. */
export function fractionOf(p: Progress | null): number | null {
  if (!p) return null;
  switch (p.kind) {
    case "fetching":
    case "reprocessing":
    case "rating":
    case "rawLogs":
    case "parts":
    case "fights":
    case "readingDemos":
    case "standIns":
      return p.total > 0 ? p.done / p.total : 1;
    case "etf2l":
      return p.total > 0 ? p.done / p.total : null;
    default:
      return null;
  }
}

/** The steps of a sync, in the order it runs them (sync_start). */
const STEPS = 9;

/** The short stages that share step 6, each with what it is for. */
/** What a source stopped giving, in words (the sync names the step: the
 *  logs themselves, their raw server logs, or a combined log's parts). */
function gaveUpWhat(what: string): string {
  switch (what) {
    case "raw server logs":
      return t("raw server logs");
    case "combined logs' parts":
      return t("per-map logs of combined logs");
    default:
      return t("new logs");
  }
}

function stageAbout(what: string): string {
  switch (what) {
    case "Finding combined logs' parts":
      return t("Looking on logs.tf for the maps and halves a combined log was built from.");
    case "Refreshing your profile":
      return t("Your name and picture, from ETF2L or Steam.");
    case "Matching demos.tf":
      return t("Finding the demos.tf demo of each log trends.tf never linked one to.");
    case "Reading ETF2L seasons":
      return t("Divisions, tables and results of ETF2L's seasons, for the Teams page.");
    case "Scanning your demos folder":
      return t("Linking the demos you recorded to their matches.");
    case "Resolving each round's map":
      return t("Working out which map each round was played on.");
    default:
      return "";
  }
}

/**
 * Where a sync is and what that step is for (Flashy): "Step 3 of 9",
 * a name for it, and one sentence on what it does -- so a sync that takes
 * minutes says why. `null` for a rebuild, which names itself.
 */
export function phaseOf(p: Progress | null): { step: number; of: number; title: string; about: string } | null {
  const at = (step: number, title: string, about: string) => ({ step, of: STEPS, title, about });
  if (!p) return at(1, t("Finding your logs"), t("Asking trends.tf and logs.tf which logs you are in, to spot the ones not stored yet."));
  switch (p.kind) {
    case "indexing":
    case "indexed":
      return at(1, t("Finding your logs"), t("Asking trends.tf and logs.tf which logs you are in, to spot the ones not stored yet."));
    case "etf2l":
      return at(2, t("Checking ETF2L"), t("Reading your ETF2L officials, so each match can be marked official, scrim or pug."));
    case "gaveUp":
      // The step that stopped, not always the download (the audit: raw logs
      // and parts sent the card back to step 3).
      if (p.what === "raw server logs") return at(5, t("Server logs"), t("The raw server logs: every kill with its time, classes and positions."));
      if (p.what === "combined logs' parts") return at(4, t("Per-map logs"), t("The per-map logs a combined log was built from, for each map's score."));
      return at(3, t("Downloading new matches"), t("Each new log from drops.tf, or logs.tf for the last hour's, rated as it lands."));
    case "fetching":
    case "fetchFailed":
      return at(3, t("Downloading new matches"), t("Each new log from drops.tf, or logs.tf for the last hour's, rated as it lands."));
    case "standIns":
      return at(3, t("Downloading new matches"), t("logs.tf is refusing requests, so new logs come from more.tf's copy of them."));
    case "parts":
      return at(4, t("Per-map logs"), t("The per-map logs a combined log was built from, for each map's score."));
    case "rawLogs":
      return at(5, t("Server logs"), t("The raw server logs: every kill with its time, classes and positions."));
    case "stage":
      return at(6, t("Linking demos, seasons and maps"), stageAbout(p.what));
    case "fights":
      return at(7, t("Reading fights"), t("Grouping every kill into the fight it was part of, for trades and fight stats."));
    case "readingDemos":
      return at(8, t("Reading demos"), t("Aim, movement and deaths from newly linked demos. About 16 seconds a demo."));
    case "rating":
      return at(9, t("Rating"), t("Every match rated again: new matches shift the baselines the ratings are measured against."));
    case "reprocessing":
      return null;
    default:
      return null;
  }
}

/** The match a step is reading: "Official vs Kebab · pl_upward · 14 Sept". */
export function matchOf(p: Progress | null): string | null {
  if (!p || p.kind !== "readingDemos" || !p.what) return null;
  const w = p.what;
  const kind = w.kind === "official" ? t("Official") : w.kind === "scrim" ? t("Scrim") : w.kind === "pug" ? t("Pug") : null;
  const who = w.opponent ? (kind ? t("{0} vs {1}", { "0": kind, "1": w.opponent }) : t("vs {0}", { "0": w.opponent })) : kind;
  const parts = [who, w.map, w.playedAt !== null ? formatDate(w.playedAt) : null].filter((x): x is string => !!x);
  return parts.length > 0 ? parts.join(" · ") : null;
}

/** One line saying what the sync is doing now. */
export function labelOf(p: Progress | null): string {
  if (!p) return t("Starting…");
  const n = (x: number) => x.toLocaleString();
  switch (p.kind) {
    case "indexing":
      return p.rows > 0 ? t("Indexing {source} — {rows} rows", { source: p.source, rows: n(p.rows) }) : t("Indexing {source}…", { source: p.source });
    case "indexed":
      return t("Indexed. {superseded} per-round logs folded into their match.", { superseded: p.superseded });
    case "fetching":
      if (p.total === 0) return t("Nothing new to fetch.");
      if (p.done === 0 || !pace) pace = { at: Date.now(), done: p.done };
      return (
        t("Matches {done} of {total}", { done: n(p.done), total: n(p.total) }) +
        (p.done < p.total ? t(" — about {0} left", { "0": eta(p.total - p.done, p.done) }) : "")
      );
    case "fetchFailed":
      return t("Log {logId} failed; continuing.", { logId: p.logId });
    case "reprocessing":
      // The card's title already says what this is.
      return t("{done} of {total} matches", { done: n(p.done), total: n(p.total) });
    case "rating":
      return t("Rating {done} of {total} matches", { done: n(p.done), total: n(p.total) });
    case "rawLogs":
      if (p.total === 0) return t("Server logs up to date.");
      return (
        t("Server logs {done} of {total}", { done: n(p.done), total: n(p.total) }) +
        (p.done < p.total ? t(" — about {0} left", { "0": eta(p.total - p.done) }) : "")
      );
    case "parts":
      return t("Per-map logs {done} of {total}", { done: n(p.done), total: n(p.total) });
    case "stage":
      return `${t(p.what)}…`;
    case "fights":
      return p.total === 0 ? t("Fights up to date.") : t("Reading fights {done} of {total}", { done: n(p.done), total: n(p.total) });
    case "readingDemos":
      // Around 16 s each, so the count moves slowly and the match it is on
      // is the part worth saying.
      if (p.total === 0) return t("Demos already read.");
      return t("Reading demos {done} of {total}", { done: n(p.done), total: n(p.total) }) + (p.logId !== null && !p.what ? t(" — log {logId}", { logId: p.logId }) : "");
    case "etf2l":
      return p.total === 0 ? t("Checking ETF2L…") : t("ETF2L officials {done} of {total}", { done: p.done, total: p.total });
    case "sourceFailed":
      return t("{source} could not be reached; carrying on without it.", { source: p.source });
    case "gaveUp":
      return t("{source} stopped answering after {1} of {2} {what}; the rest waits for the next sync.", { "1": n(p.done), "2": n(p.total), source: p.source, what: gaveUpWhat(p.what) });
    case "standIns":
      return p.total === 0 ? t("Nothing new on more.tf.") : t("logs.tf is refusing us: new matches from more.tf, {done} of {total}", { done: n(p.done), total: n(p.total) });
  }
}

export function eta(logs: number, done?: number): string {
  const mins = Math.ceil((logs * (done === undefined ? SECONDS_PER_LOG : secondsPerLog(done))) / 60);
  return mins <= 1 ? t("1 min") : t("{mins} min", { mins: mins });
}
