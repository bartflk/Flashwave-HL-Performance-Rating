import { errorMessage } from "../api/types";
import { t } from "./i18n";

/** The sites the app reads, as an error names them. */
const SITES = ["trends.tf", "logs.tf", "drops.tf", "more.tf", "demos.tf", "ETF2L", "RGL", "Steam"];

/**
 * An error fit to show (Flashy: "if we have api errors, make a more generic
 * error"): a site that did not answer, refused or failed says so in a line,
 * without the request, URL and OS error. Anything else, already written for
 * people, comes through as it was. The full text stays in `detail`, for a
 * tooltip.
 */
export function friendlyError(e: unknown): { text: string; detail: string } {
  const detail = typeof e === "string" ? e : errorMessage(e);
  const lower = detail.toLowerCase();
  const site = SITES.find((s) => lower.includes(s.toLowerCase())) ?? t("The site");
  let text: string | null = null;
  if (/\b429\b|too many requests|rate.?limit/.test(lower)) {
    text = t("{0} is busy right now; try again in a few minutes.", { "0": site });
  } else if (/error sending request|connect|timed? ?out|dns|os error|tls|certificate|unreachable|network/.test(lower)) {
    text = t("{0} isn't answering right now; try again later.", { "0": site });
  } else if (/\b5\d\d\b|server error|bad gateway|unavailable/.test(lower)) {
    text = t("{0} is having trouble right now; try again later.", { "0": site });
  } else if (/https?:\/\/|\bhttp\b|status \d{3}|decoding|expected .* at line/.test(lower)) {
    text = t("{0} could not be read right now.", { "0": site });
  }
  return { text: text ?? detail, detail };
}
