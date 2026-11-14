import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { SixesMatch } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { formatDate } from "../../lib/format";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { DivisionBadge } from "../players/PlayerProfile";
import { tierOf } from "../etf2l/Brackets";
import { mapMode } from "../../lib/gameformat";

/**
 * The current 6v6 season's officials by week (the 6v6 plan, P2; question
 * 12: "Matches by week" first). Read from ETF2L when this section opens and
 * kept for half an hour. Opens on the week still being played; every
 * division at once, or one.
 */
export function SixesMatches() {
  const q = useQuery({ queryKey: ["sixes_matches"], queryFn: api.getSixesMatches, staleTime: 30 * 60_000, retry: false });
  const all = useMemo(() => q.data?.matches ?? [], [q.data]);
  const weeks = useMemo(() => [...new Set(all.map((m) => m.week).filter((w): w is number => w !== null))].sort((a, b) => a - b), [all]);
  // The week still being played: the first with a match to come, else the last.
  const current = useMemo(() => weeks.find((w) => all.some((m) => m.week === w && !m.played)) ?? weeks[weeks.length - 1] ?? null, [weeks, all]);
  const [picked, setPicked] = useState<number | null>(null);
  const week = picked ?? current;
  const [division, setDivision] = useState<string>("");
  const divisions = useMemo(
    () => [...new Set(all.map((m) => m.division).filter((d): d is string => !!d))].sort((a, b) => tierOf(a) - tierOf(b) || a.localeCompare(b)),
    [all],
  );
  const groups = useMemo(() => {
    const rows = all.filter((m) => m.week === week && (division === "" || m.division === division));
    const by = new Map<string, SixesMatch[]>();
    for (const m of rows) {
      const k = m.division ?? t("No division");
      by.set(k, [...(by.get(k) ?? []), m]);
    }
    return [...by.entries()].sort(([a], [b]) => tierOf(a) - tierOf(b) || a.localeCompare(b));
  }, [all, week, division]);

  if (q.error) return <ApiError error={q.error} />;
  if (q.isPending) return <Loading>{t("Reading ETF2L's 6v6 matches…")}</Loading>;
  if (!q.data || all.length === 0) return <p className="hint">{t("ETF2L lists no 6v6 matches for this season yet.")}</p>;

  const inWeek = all.filter((m) => m.week === week);
  const played = inWeek.filter((m) => m.played).length;
  return (
    <div className="sx-matches">
      <div className="sx-week-bar">
        <div className="segmented" role="tablist" aria-label={t("Week")}>
          {weeks.map((w) => (
            <button key={w} role="tab" aria-selected={w === week} className={w === week ? "seg active" : "seg"} onClick={() => setPicked(w)}>
              {tx("Week {0}", { "0": w })}
            </button>
          ))}
        </div>
        <label className="an-field sx-div-pick">
          <span>{t("Division")}</span>
          <select value={division} onChange={(e) => setDivision(e.target.value)}>
            <option value="">{t("Every division")}</option>
            {divisions.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </label>
        <span className="muted sx-week-count">
          {tx("{0} of {1} played", { "0": played, "1": inWeek.length })}
        </span>
      </div>
      <p className="hint sx-explain">{t("The score is ETF2L's: league points over the two maps, 2 for a map won and 1 for a map drawn.")}</p>
      {groups.length === 0 ? (
        <p className="hint">{t("No matches this week in this division.")}</p>
      ) : (
        <div className="sx-week-groups">
          {groups.map(([d, rows]) => (
            <section key={d} className="panel e2-panel">
              <div className="e2-panel-head">
                <h2 className="sx-div-name">
                  <DivisionBadge d={{ name: d, tier: Math.floor(tierOf(d)) }} />
                </h2>
                <span className="muted">{rows.length === 1 ? t("1 match") : tx("{0} matches", { "0": rows.length })}</span>
              </div>
              <ul className="sx-fixtures">
                {rows.map((m) => (
                  <Fixture key={m.id} m={m} />
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}

function Fixture({ m }: { m: SixesMatch }) {
  const won = m.played && m.r1 !== null && m.r2 !== null && m.r1 !== m.r2 ? (m.r1 > m.r2 ? 1 : 2) : 0;
  const team = (side: 1 | 2) => {
    const c = side === 1 ? m.clan1 : m.clan2;
    const cls = won === 0 ? "sx-fx-team" : won === side ? "sx-fx-team sx-fx-won" : "sx-fx-team sx-fx-lost";
    return (
      <a className={cls} href={`https://etf2l.org/teams/${c.id}/`} target="_blank" rel="noreferrer" title={t("Open on ETF2L")}>
        {c.avatar ? <img src={c.avatar} alt="" /> : <span className="sx-fx-noav" aria-hidden="true" />}
        <span className="sx-fx-name">{c.name}</span>
      </a>
    );
  };
  return (
    <li className="sx-fx">
      <span className="sx-fx-when muted">{m.time ? formatDate(m.time) : t("Unscheduled")}</span>
      {team(1)}
      <a className="sx-fx-score" href={`https://etf2l.org/matches/${m.id}/`} target="_blank" rel="noreferrer" title={t("Open on ETF2L")}>
        {m.played && m.r1 !== null && m.r2 !== null ? (
          <>
            {m.r1}–{m.r2}
            {m.defaultWin && <span className="sx-fx-default">{t("default")}</span>}
          </>
        ) : (
          <span className="muted">{t("vs")}</span>
        )}
      </a>
      {team(2)}
      <span className="sx-fx-maps">
        {m.maps.map((x) => (
          <span key={x} className={`sx-map sx-${mapMode(x)}`}>
            {x.replace(/^(cp|koth)_/, "").replace(/_.*$/, "")}
          </span>
        ))}
      </span>
    </li>
  );
}
