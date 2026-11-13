import type { RglMatch } from "../api/types";
import { decided, weekOf } from "./rglstandings";

/**
 * The pure parts of the RGL Matches page: who is playing now, a team's form,
 * the filters, and the calendar file. No React, no network: everything comes
 * from the season's matches the tab has read already.
 */

const secs = (iso: string | null) => (iso ? Date.parse(iso) / 1000 : 0);

/** A match counts as on for this long after its start (the strip's own rule). */
export const LIVE_FOR = 2.5 * 3600;

/** A pairing with RGL's placeholder for a team that has the week off. */
const isBye = (m: RglMatch) => m.teams.some((x) => x.teamName.startsWith("Bye Week"));

/** Started within the last 2.5 h and not decided; and the rest of today, soonest first. */
export function liveStrip(matches: RglMatch[], now: number): { live: RglMatch[]; tonight: RglMatch[] } {
  const d = new Date(now * 1000);
  const endOfDay = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1).getTime() / 1000;
  const live: RglMatch[] = [];
  const tonight: RglMatch[] = [];
  for (const m of matches) {
    if (!m.matchDate || decided(m) || isBye(m) || m.teams.length < 2) continue;
    const at = secs(m.matchDate);
    if (at <= now && at > now - LIVE_FOR) live.push(m);
    else if (at > now && at < endOfDay) tonight.push(m);
  }
  const by = (a: RglMatch, b: RglMatch) => secs(a.matchDate) - secs(b.matchDate);
  return { live: live.sort(by), tonight: tonight.sort(by) };
}

export type Mark = "W" | "L";

/**
 * Each team's decided results in date order, for form marks. `form(team,
 * before)` is its last `n` results (oldest first, so the newest is on the
 * right) from matches before `before` (seconds), or all of them when null.
 * A forfeit with no winner named is a loss for both, as in the standings.
 */
export function formIndex(matches: RglMatch[]) {
  const by = new Map<number, Array<{ at: number; mark: Mark }>>();
  for (const m of matches) {
    if (!decided(m) || isBye(m) || !m.matchDate) continue;
    for (const x of m.teams) {
      const list = by.get(x.teamId) ?? [];
      list.push({ at: secs(m.matchDate), mark: m.winner === x.teamId ? "W" : "L" });
      by.set(x.teamId, list);
    }
  }
  for (const list of by.values()) list.sort((a, b) => a.at - b.at);
  return (team: number, before: number | null = null, n = 5): Mark[] => {
    const list = by.get(team) ?? [];
    const upto = before === null ? list : list.filter((r) => r.at < before);
    return upto.slice(-n).map((r) => r.mark);
  };
}

export type MatchFilter = { division: string; week: string; team: string; status: "" | "toPlay" | "played" | "forfeit" };
export const NO_FILTER: MatchFilter = { division: "", week: "", team: "", status: "" };

/** A match's teams contain what was typed, in a name or a tag, any case. */
export const hasTeam = (m: RglMatch, q: string) => {
  const s = q.trim().toLowerCase();
  return s === "" || m.teams.some((x) => x.teamName.toLowerCase().includes(s) || (x.teamTag ?? "").toLowerCase().includes(s));
};

export function filterMatches(matches: RglMatch[], f: MatchFilter): RglMatch[] {
  return matches.filter(
    (m) =>
      (f.division === "" || m.divisionName === f.division) &&
      (f.week === "" || weekOf(m) === f.week) &&
      hasTeam(m, f.team) &&
      (f.status === "" ||
        (f.status === "toPlay" && !decided(m)) ||
        (f.status === "played" && decided(m) && !m.isForfeit) ||
        (f.status === "forfeit" && m.isForfeit)),
  );
}

/** What the viewer's storage held, kept only where it is a filter. */
export function parseFilter(raw: string | null): MatchFilter {
  try {
    const o = JSON.parse(raw ?? "null") as Partial<MatchFilter> | null;
    if (!o || typeof o !== "object") return NO_FILTER;
    const str = (v: unknown) => (typeof v === "string" ? v : "");
    const status = o.status === "toPlay" || o.status === "played" || o.status === "forfeit" ? o.status : "";
    return { division: str(o.division), week: str(o.week), team: str(o.team), status };
  } catch {
    return NO_FILTER;
  }
}

// ---- The calendar file -----------------------------------------------------

/** iCalendar text escaping: backslash, semicolon, comma, newline. */
export const icsEscape = (s: string) => s.replace(/\\/g, "\\\\").replace(/;/g, "\\;").replace(/,/g, "\\,").replace(/\r?\n/g, "\\n");

/** 20261112T193000Z */
export const icsTime = (sec: number) => new Date(sec * 1000).toISOString().replace(/[-:]/g, "").replace(/\.\d{3}/, "");

/** Lines over 75 octets are folded: CRLF and a space. Cut by characters, never inside one. */
function fold(line: string): string {
  const enc = new TextEncoder();
  if (enc.encode(line).length <= 75) return line;
  const out: string[] = [];
  let cur = "";
  let size = 0;
  for (const ch of line) {
    const w = enc.encode(ch).length;
    if (size + w > (out.length === 0 ? 75 : 74)) {
      out.push(cur);
      cur = "";
      size = 0;
    }
    cur += ch;
    size += w;
  }
  out.push(cur);
  return out.join("\r\n ");
}

/**
 * One VEVENT per match with a start time, in UTC, a stable UID per match id
 * (so importing again updates rather than duplicates). A match lasts two
 * hours in the calendar; RGL gives no end.
 */
export function icsOf(matches: RglMatch[], opts: { name: string; stamp: number }): string {
  const lines = ["BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Flashwave.tf//RGL Highlander//EN", "CALSCALE:GREGORIAN", "METHOD:PUBLISH", `X-WR-CALNAME:${icsEscape(opts.name)}`];
  for (const m of matches) {
    if (!m.matchDate) continue;
    const at = secs(m.matchDate);
    lines.push(
      "BEGIN:VEVENT",
      `UID:rgl-match-${m.matchId}@flashwave.tf`,
      `DTSTAMP:${icsTime(opts.stamp)}`,
      `DTSTART:${icsTime(at)}`,
      `DTEND:${icsTime(at + 2 * 3600)}`,
      `SUMMARY:${icsEscape(m.teams.map((x) => x.teamName).join(" vs "))}`,
      `DESCRIPTION:${icsEscape([m.matchName, m.divisionName, m.maps.map((x) => x.mapName).join(", ")].filter(Boolean).join(" · "))}`,
      "END:VEVENT",
    );
  }
  lines.push("END:VCALENDAR");
  return lines.map(fold).join("\r\n") + "\r\n";
}
