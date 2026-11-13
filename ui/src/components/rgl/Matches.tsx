import { useEffect, useMemo, useState } from "react";
import type { RglMatch } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { decided, weekOf } from "../../lib/rglstandings";
import { NO_FILTER, filterMatches, formIndex, icsOf, parseFilter } from "../../lib/rglmatches";
import type { MatchFilter } from "../../lib/rglmatches";
import { toast, toastError } from "../../lib/toast";
import { api } from "../../api/client";
import { LiveStrip } from "./LiveStrip";
import { ApiError } from "../ApiError";
import { Seg } from "../analysis/MapChrome";
import { useSeason, useLive, LADDER, byDay } from "./data";
import type { Loc } from "./data";
import { Reading, WeekLine } from "./bits";

/** Home's two panels: the next matches and the latest results. */
export function LiveMatches({ onTeam, onMatch, go }: { onTeam: (id: number) => void; onMatch: (id: number) => void; go: (l: Loc) => void }) {
  const live = useLive();
  const all = live.matches;
  const next = (all ?? []).filter((m) => !decided(m)).slice(0, 6);
  const last = (all ?? []).filter(decided).slice(-6).reverse();
  const form = useMemo(() => formIndex(all ?? []), [all]);
  const panel = (title: string, rows: RglMatch[], empty: string, countdown: boolean) => (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{title}</h2>
        <button className="e2-more" onClick={() => go({ at: "matches" })}>
          {t("All matches")} ›
        </button>
      </div>
      {live.error ? (
        <>
          <ApiError error={live.error} />
          <button onClick={live.retry}>{t("Try again")}</button>
        </>
      ) : live.none ? (
        <p className="hint">{t("RGL lists no seasons.")}</p>
      ) : all === null ? (
        <Reading />
      ) : rows.length === 0 ? (
        <p className="hint">{empty}</p>
      ) : (
        <ol className="rgl-results">
          {rows.map((m) => (
            <WeekLine key={m.matchId} m={m} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} countdown={countdown} form={form} />
          ))}
        </ol>
      )}
    </section>
  );
  return (
    <>
      <LiveStrip onMatch={onMatch} go={go} />
      <div className="rgl-live">
        {panel(t("Next matches"), next, t("No match is scheduled."), true)}
        {panel(t("Latest results"), last, t("No results yet."), false)}
      </div>
    </>
  );
}

export type MatchesMode = "week" | "upcoming" | "results";

const KEPT = "rgl-matches-view";

/** The viewer's last tab and filters, if their storage kept them. */
function kept(): { mode: MatchesMode; filter: MatchFilter } {
  try {
    const raw = localStorage.getItem(KEPT);
    const o = JSON.parse(raw ?? "null") as { mode?: string } | null;
    const mode = o?.mode === "week" || o?.mode === "results" ? o.mode : "upcoming";
    return { mode, filter: parseFilter(raw) };
  } catch {
    return { mode: "upcoming", filter: NO_FILTER };
  }
}

export function MatchesPage({ onTeam, onMatch }: { onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const live = useLive();
  const season = useSeason(live.season?.id ?? null).data;
  const [start] = useState(kept);
  const [mode, setMode] = useState<MatchesMode>(start.mode);
  const [filter, setFilter] = useState<MatchFilter>(start.filter);
  useEffect(() => {
    try {
      localStorage.setItem(KEPT, JSON.stringify({ mode, ...filter }));
    } catch {
      // Not remembered; the page works the same.
    }
  }, [mode, filter]);
  const all = useMemo(() => live.matches ?? [], [live.matches]);
  const form = useMemo(() => formIndex(all), [all]);
  const order = (season?.divisions ?? []).map((d) => d.name);
  const divisions = [...new Set([...order, ...all.map((m) => m.divisionName ?? "")])].filter((d) => d && all.some((m) => m.divisionName === d));
  const weeks = [...new Set(all.map(weekOf))];
  const teams = useMemo(
    () => [...new Set(all.flatMap((m) => m.teams.map((x) => x.teamName)))].filter((n) => !n.startsWith("Bye Week")).sort((a, b) => a.localeCompare(b)),
    [all],
  );
  // The tab already says to play or played; the status filter narrows what it cannot.
  const status = mode === "upcoming" ? "toPlay" : mode === "results" && filter.status === "toPlay" ? "" : filter.status;
  const shown = filterMatches(all, { ...filter, status });
  // "By week" is the filter's week, or the one being played now.
  const current = weeks.find((w) => all.some((m) => weekOf(m) === w && !decided(m))) ?? weeks[weeks.length - 1];
  const week = filter.week || current;
  const upcoming = shown.filter((m) => !decided(m));
  const played = shown.filter(decided).reverse();
  const inWeek = mode === "week" ? filterMatches(all, { ...filter, week: "", status }).filter((m) => weekOf(m) === week) : [];
  const set = (p: Partial<MatchFilter>) => setFilter((f) => ({ ...f, ...p }));
  const calendar = filterMatches(all, { ...filter, status: "toPlay" });
  const exportIt = async () => {
    try {
      const name = filter.team.trim() || (live.season?.name ?? "RGL");
      const text = icsOf(calendar, { name: `${t("RGL Highlander")} · ${name}`, stamp: Date.now() / 1000 });
      const file = `${name.replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-|-$/g, "") || "rgl"}.ics`;
      const path = await api.saveCalendar(text, file);
      if (path) toast(t("Calendar saved"), path);
    } catch (e) {
      toastError(e);
    }
  };
  const list = (rows: RglMatch[], countdown: boolean) =>
    byDay(rows).map(([label, ms]) => (
      <section key={label} className="panel e2-panel">
        <div className="e2-panel-head">
          <h2>{label}</h2>
          <span className="muted">{ms.length === 1 ? t("1 match") : tx("{0} matches", { "0": ms.length })}</span>
        </div>
        <ol className="rgl-results">
          {ms.map((m) => (
            <WeekLine key={m.matchId} m={m} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} countdown={countdown} form={form} />
          ))}
        </ol>
      </section>
    ));
  const none = <p className="hint">{t("No match fits these filters.")}</p>;
  const filtered = filter.division !== "" || filter.week !== "" || filter.team !== "" || filter.status !== "";
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">
          {t("RGL Highlander")}
          {live.season ? ` · ${live.season.name}` : ""}
        </span>
        <h1>{t("Matches")}</h1>
      </header>
      <LiveStrip onMatch={onMatch} />
      <div className="e2-filters rgl-filters">
        <Seg
          label={t("Matches")}
          value={mode}
          onChange={setMode}
          options={[
            { v: "upcoming", label: tx("Upcoming ({0})", { "0": all.filter((m) => !decided(m)).length }) },
            { v: "results", label: t("Results") },
            { v: "week", label: t("By week") },
          ]}
        />
        <label className="km-field">
          <span>{t("Division")}</span>
          <select value={filter.division} onChange={(e) => set({ division: e.target.value })}>
            <option value="">{t("All divisions")}</option>
            {divisions.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </label>
        <label className="km-field">
          <span>{t("Week")}</span>
          <select value={filter.week} onChange={(e) => set({ week: e.target.value })}>
            <option value="">{mode === "week" ? tx("Now ({0})", { "0": current ?? "–" }) : t("All weeks")}</option>
            {weeks.map((w) => (
              <option key={w} value={w}>
                {w}
              </option>
            ))}
          </select>
        </label>
        <label className="km-field">
          <span>{t("Team")}</span>
          <input type="search" className="rgl-team-filter" list="rgl-season-teams" value={filter.team} placeholder={t("Any team")} onChange={(e) => set({ team: e.target.value })} aria-label={t("Filter by team")} />
          <datalist id="rgl-season-teams">
            {teams.map((n) => (
              <option key={n} value={n} />
            ))}
          </datalist>
        </label>
        {mode !== "upcoming" && (
          <label className="km-field">
            <span>{t("Status")}</span>
            <select value={status} onChange={(e) => set({ status: e.target.value as MatchFilter["status"] })}>
              <option value="">{t("Any status")}</option>
              {mode === "week" && <option value="toPlay">{t("To play")}</option>}
              <option value="played">{t("Played")}</option>
              <option value="forfeit">{t("Forfeit")}</option>
            </select>
          </label>
        )}
        <span className="rgl-filters-end">
          {filtered && <button onClick={() => setFilter(NO_FILTER)}>{t("Clear filters")}</button>}
          <button
            onClick={() => void exportIt()}
            disabled={calendar.length === 0}
            title={calendar.length === 0 ? t("No upcoming match to add") : t("Save the {0} upcoming matches shown as a calendar file (.ics)", { "0": calendar.length })}
          >
            {t("Add to calendar")}
          </button>
        </span>
      </div>
      {live.error ? (
        <>
          <ApiError error={live.error} />
          <button onClick={live.retry}>{t("Try again")}</button>
        </>
      ) : live.none ? (
        <p className="hint">{t("RGL lists no seasons.")}</p>
      ) : live.matches === null ? (
        <Reading />
      ) : mode === "week" ? (
        inWeek.length === 0 ? (
          none
        ) : (
          <div className="e2-round-grid">
            {divisions
              .filter((d) => inWeek.some((m) => m.divisionName === d))
              .map((d) => (
                <section key={d} className="panel e2-panel">
                  <div className="e2-panel-head">
                    <h2>{d}</h2>
                    {LADDER[d] && <span className="muted">≈ {t(LADDER[d])}</span>}
                  </div>
                  <ol className="rgl-results">
                    {inWeek
                      .filter((m) => m.divisionName === d)
                      .map((m) => (
                        <WeekLine key={m.matchId} m={m} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} countdown={!decided(m)} form={form} />
                      ))}
                  </ol>
                </section>
              ))}
          </div>
        )
      ) : mode === "upcoming" ? (
        upcoming.length === 0 ? (
          all.some((m) => !decided(m)) ? none : <p className="hint">{t("No match is scheduled.")}</p>
        ) : (
          list(upcoming, true)
        )
      ) : played.length === 0 ? (
        all.some(decided) ? none : <p className="hint">{t("No results yet.")}</p>
      ) : (
        list(played, false)
      )}
    </>
  );
}

// ---- One match --------------------------------------------------------------
