import { api } from "../api/client";
import type { ActivityLine, ActivitySite } from "../api/types";
import { lineText } from "./activity";
import { diagnosticsText } from "./diagnostics";
import { t } from "./i18n";
import { copy } from "./toast";

/**
 * A problem report to paste into a chat (Flashy: "when you get errors like
 * this, you can make a copy-able rapport so that its easier for me to copy
 * paste into here and you can determine the problem easier"): what failed
 * and its message, the app and its data, every warning and error this run,
 * the lines around the failure, and how each site has been answering.
 *
 * Each part is read on its own; one that cannot be read is said to be
 * missing, so the report is never lost to the thing that is broken.
 */

/** The warnings and errors kept, newest last. */
const PROBLEMS = 40;
/** Lines of every level kept, newest last. */
const RECENT = 120;

export interface Failure {
  /** What failed, in a few words: "Sync failed". */
  what: string;
  /** The error as the app showed it. */
  message?: string;
  /** Anything else the screen showed: the steps that failed, notes. */
  details?: string[];
}

export async function problemReport(f: Failure | null): Promise<string> {
  const out: string[] = ["=== Flashwave.tf problem report ===", `Made: ${new Date().toString()}`];
  if (f) {
    out.push("", `What failed: ${f.what}`);
    if (f.message) out.push(`Error: ${f.message}`);
    for (const d of f.details ?? []) out.push(`  ${d}`);
  }

  out.push("", "--- App ---");
  try {
    const d = await api.getDiagnostics();
    out.push(diagnosticsText(d));
    if (d.logFile) out.push(`Log file: ${d.logFile}`);
  } catch (e) {
    out.push(`(could not be read: ${String(e)})`);
  }

  let lines: ActivityLine[] = [];
  let sites: ActivitySite[] = [];
  try {
    const v = await api.getActivity(0);
    lines = v.lines;
    sites = v.sites;
  } catch (e) {
    out.push("", `(the activity log could not be read: ${String(e)})`);
  }

  const problems = lines.filter((l) => l.level === "error" || l.level === "warn").slice(-PROBLEMS);
  out.push("", `--- Warnings and errors this run (${problems.length}) ---`);
  out.push(...(problems.length ? problems.map(lineText) : ["none"]));

  out.push("", `--- The last ${Math.min(RECENT, lines.length)} lines ---`);
  out.push(...lines.slice(-RECENT).map(lineText));

  if (sites.length) {
    out.push("", "--- Sites ---");
    for (const s of sites) {
      const resting = s.restingUntil !== null && s.restingUntil > Date.now() ? `, resting until ${new Date(s.restingUntil).toLocaleTimeString()}` : "";
      out.push(
        `  ${s.host}: ${s.requests} requests, ${s.ok} ok, ${s.notModified} unchanged, ${s.notFound} not found, ${s.failed} failed, ` +
          `avg ${Math.round(s.avgMs)} ms, last ${s.lastStatus ?? "-"}${s.lastError ? ` (${s.lastError})` : ""}${resting}`,
      );
    }
  }
  out.push("", "=== end of report ===");
  return out.join("\n");
}

/** Build the report and put it on the clipboard. */
export async function copyReport(f: Failure | null): Promise<void> {
  await copy(await problemReport(f), t("the problem report"));
}
