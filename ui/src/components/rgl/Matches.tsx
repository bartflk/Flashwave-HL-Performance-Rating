import { useMemo, useState } from "react";
import type { RglMatch } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { decided, weekOf } from "../../lib/rglstandings";
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
            <WeekLine key={m.matchId} m={m} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} countdown={countdown} />
          ))}
        </ol>
      )}
    </section>
  );
  return (
    <div className="rgl-live">
      {panel(t("Next matches"), next, t("No match is scheduled."), true)}
      {panel(t("Latest results"), last, t("No results yet."), false)}
    </div>
  );
}

export type MatchesMode = "week" | "upcoming" | "results";

export function MatchesPage({ onTeam, onMatch }: { onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const live = useLive();
  const season = useSeason(live.season?.id ?? null).data;
  const [mode, setMode] = useState<MatchesMode>("week");
  const [division, setDivision] = useState("");
  const all = useMemo(() => live.matches ?? [], [live.matches]);
  const order = (season?.divisions ?? []).map((d) => d.name);
  const divisions = [...new Set([...order, ...all.map((m) => m.divisionName ?? "")])].filter((d) => d && all.some((m) => m.divisionName === d));
  const shown = all.filter((m) => division === "" || m.divisionName === division);
  const weeks = [...new Set(all.map(weekOf))];
  const week = weeks.find((w) => all.some((m) => weekOf(m) === w && !decided(m))) ?? weeks[weeks.length - 1];
  const list = (rows: RglMatch[], countdown: boolean) =>
    byDay(rows).map(([label, ms]) => (
      <section key={label} className="panel e2-panel">
        <div className="e2-panel-head">
          <h2>{label}</h2>
          <span className="muted">{ms.length === 1 ? t("1 match") : tx("{0} matches", { "0": ms.length })}</span>
        </div>
        <ol className="rgl-results">
          {ms.map((m) => (
            <WeekLine key={m.matchId} m={m} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} countdown={countdown} />
          ))}
        </ol>
      </section>
    ));
  const upcoming = shown.filter((m) => !decided(m));
  const played = shown.filter(decided).reverse();
  const inWeek = shown.filter((m) => weekOf(m) === week);
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">
          {t("RGL Highlander")}
          {live.season ? ` · ${live.season.name}` : ""}
        </span>
        <h1>{t("Matches")}</h1>
      </header>
      <div className="e2-filters">
        <Seg
          label={t("Matches")}
          value={mode}
          onChange={setMode}
          options={[
            { v: "week", label: week ?? t("Week") },
            { v: "upcoming", label: tx("Upcoming ({0})", { "0": all.filter((m) => !decided(m)).length }) },
            { v: "results", label: t("Results") },
          ]}
        />
        <label className="km-field">
          <span>{t("Division")}</span>
          <select value={division} onChange={(e) => setDivision(e.target.value)}>
            <option value="">{t("All divisions")}</option>
            {divisions.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </label>
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
          <p className="hint">{t("No matches in this division this week.")}</p>
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
                        <WeekLine key={m.matchId} m={m} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} countdown={!decided(m)} />
                      ))}
                  </ol>
                </section>
              ))}
          </div>
        )
      ) : mode === "upcoming" ? (
        upcoming.length === 0 ? <p className="hint">{t("No match is scheduled.")}</p> : list(upcoming, true)
      ) : played.length === 0 ? (
        <p className="hint">{t("No results yet.")}</p>
      ) : (
        list(played, false)
      )}
    </>
  );
}

// ---- One match --------------------------------------------------------------
