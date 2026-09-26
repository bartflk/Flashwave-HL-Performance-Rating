import { useSyncExternalStore } from "react";

/**
 * Everything that went wrong, kept where a person can read it.
 *
 * Until now a failure had three possible fates: a `tracing::warn!` in the
 * backend that no user will ever see, a red line in one panel that vanishes
 * on the next render, or a black window. None of them can be sent to anyone.
 * So: the last few hundred problems live here, the header says how many are
 * new, Settings lists them, and one button puts the lot on the clipboard.
 */

export interface Problem {
  id: number;
  at: number;
  /** What was being attempted: "fetching log 4042136", "the scoreboard". */
  what: string;
  message: string;
  /** A stack, a component trace, a server error chain — anything long. */
  detail?: string;
  /** Repeats of the same thing are counted rather than listed again. */
  count: number;
}

/** Enough to cover a long sync without growing without bound. */
const KEEP = 200;

let problems: Problem[] = [];
let seen = 0;
let nextId = 1;
let listeners: Array<() => void> = [];

function emit() {
  problems = [...problems];
  listeners.forEach((l) => l());
}

/**
 * Write a problem down. Identical messages collapse into a count, so a sync
 * that fails on forty logs for the same reason reads as one line, not forty.
 */
export function noteError(p: { what: string; message: string; detail?: string }) {
  const last = problems[0];
  if (last && last.what === p.what && last.message === p.message) {
    last.count += 1;
    last.at = Date.now();
    emit();
    return;
  }
  problems.unshift({ id: nextId++, at: Date.now(), count: 1, ...p });
  if (problems.length > KEEP) problems.length = KEEP;
  emit();
}

export function clearProblems() {
  problems = [];
  seen = 0;
  emit();
}

/** Called when the list is on screen: the badge is about unread ones. */
export function markProblemsSeen() {
  seen = problems.length === 0 ? 0 : problems[0].id;
  emit();
}

export function useProblems(): Problem[] {
  return useSyncExternalStore(subscribe, () => problems);
}

export function useUnreadProblems(): number {
  return useSyncExternalStore(subscribe, () => problems.filter((p) => p.id > seen).length);
}

function subscribe(l: () => void) {
  listeners.push(l);
  return () => {
    listeners = listeners.filter((x) => x !== l);
  };
}

/**
 * The whole lot as markdown, for pasting into Discord.
 *
 * The version and the counts are in it because they are the first two
 * questions anyone asks about a report, and nobody should have to take a
 * second screenshot to answer them.
 */
export function report(about: { version: string; model?: string; matches?: number }): string {
  const lines = [
    "**Flashwave.tf problem report**",
    `version: ${about.version}`,
    about.model ? `model: ${about.model}` : null,
    about.matches !== undefined ? `matches: ${about.matches}` : null,
    `problems: ${problems.length}`,
    "",
  ].filter(Boolean) as string[];

  if (problems.length === 0) lines.push("_nothing recorded_");
  for (const p of problems.slice(0, 40)) {
    const when = new Date(p.at).toISOString().replace("T", " ").slice(0, 19);
    lines.push(`- \`${when}\` **${p.what}**${p.count > 1 ? ` (x${p.count})` : ""}: ${p.message}`);
    if (p.detail) lines.push("  ```", ...p.detail.split("\n").slice(0, 12).map((l) => `  ${l}`), "  ```");
  }
  return lines.join("\n");
}
