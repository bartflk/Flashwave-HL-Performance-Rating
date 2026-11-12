import { api } from "../api/client";
import type { Diagnostics, JobKind } from "../api/types";
import { t } from "./i18n";

/** What Settings > Diagnostics and the Problems report put into words. */

export function jobName(kind: JobKind): string {
  switch (kind) {
    case "reprocess":
      return t("Rebuild");
    case "rerate":
      return t("Re-rate");
    case "demos":
      return t("Demos read again");
    case "startup":
      return t("Rating at startup");
    default:
      return t("Sync");
  }
}

/** `sync_state` names the two sources it records by their own keys. */
export function sourceName(source: string): string {
  switch (source) {
    case "trends":
      return "trends.tf";
    case "logstf_search":
      return t("logs.tf, whole history");
    default:
      return source;
  }
}

/** SQLite's UTC "YYYY-MM-DD HH:MM:SS", in local time. */
export function sqliteTime(s: string | null): string {
  if (!s) return "—";
  const d = new Date(`${s.replace(" ", "T")}Z`);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString();
}

/** Everything Diagnostics shows, as text for Discord. */
export function diagnosticsText(d: Diagnostics): string {
  const lines = [
    `Flashwave.tf ${d.version} · model ${d.model}`,
    `Data folder: ${d.dataDir}`,
    `Matches: ${d.stats.normalized} stored, ${d.stats.pending} waiting, ${d.stats.failed} failed, ${d.stats.givenUp ?? 0} given up`,
    `Rated games by model: ${d.rated.map(([v, n]) => `${v} ${n}`).join(", ") || "none"}`,
    "",
    "Sources:",
    ...d.sources.map((s) => `  ${sourceName(s.source)}: ${sqliteTime(s.lastRunAt)}${s.lastError ? ` — ${s.lastError}` : ""}`),
    "",
    "Recent runs:",
    ...d.runs.map(
      (r) =>
        `  ${new Date(r.at * 1000).toLocaleString()} ${jobName(r.kind)} ${r.seconds}s` +
        (r.kind === "sync" ? ` +${r.fetched} new, ${r.failed} failed` : "") +
        (r.error ? ` — FAILED: ${r.error}` : "") +
        r.problems.map((p) => `\n    ${p}`).join(""),
    ),
  ];
  return lines.join("\n");
}

/** The last lines of the log file, for a report; empty when it cannot be read. */
export async function logTail(lines = 200): Promise<string> {
  try {
    return await api.recentLog(lines);
  } catch {
    return "";
  }
}

