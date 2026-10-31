import { useMemo, useState } from "react";
import { ApiError } from "../ApiError";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { type Teammate } from "../../api/types";
import { formatDate, rating, signed } from "../../lib/format";
import { openPlayer } from "../../lib/goto";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";
import { t as tr, tx, k } from "../../lib/i18n";
import { Loading, Refreshing } from "../Spinner";

/**
 * Who you play with (once its own tab): everyone with a few games on your
 * side, how you did together, and your rating with them. A name opens their
 * profile. Your teams' side of it is on each team's page under Teams.
 */

type SortKey = "games" | "officials" | "winRate" | "lastPlayed" | "delta";

const SORTS: Array<{ key: SortKey; label: string; title: string; num?: boolean }> = [
  { key: "games", label: k("Games"), title: k("Games on the same side"), num: true },
  { key: "officials", label: k("Officials"), title: k("ETF2L officials together"), num: true },
  { key: "winRate", label: k("Record"), title: k("Wins and losses together; ties left out"), num: true },
  { key: "lastPlayed", label: k("Together"), title: k("First and last game together") },
  { key: "delta", label: k("Your rating with them"), title: k("Your average rating in games with them, and the difference from your other games"), num: true },
];

/** Rows shown before "Show all". */
const FIRST = 12;

const winRate = (m: { wins: number; losses: number }) => (m.wins + m.losses === 0 ? null : (m.wins / (m.wins + m.losses)) * 100);

export function TeammatesPanel() {
  const [all, setAll] = useState(false);
  const [currentOnly, setCurrentOnly] = useState(false);
  const [sort, setSort] = useState<SortKey>("games");
  const [open, setOpen] = useState(false);
  const q = useQuery({ queryKey: ["teammates", all], queryFn: () => api.getTeammates(all), placeholderData: keepPreviousData });

  const rows = useMemo(() => {
    const list = (q.data?.teammates ?? []).filter((m) => !currentOnly || m.current);
    const key = (m: Teammate): number => {
      switch (sort) {
        case "games":
          return m.games;
        case "officials":
          return m.officials;
        case "winRate":
          return winRate(m) ?? -1;
        case "lastPlayed":
          return m.lastPlayed;
        case "delta":
          return m.myAvgDelta ?? -Infinity;
      }
    };
    return [...list].sort((a, b) => key(b) - key(a) || b.games - a.games);
  }, [q.data, currentOnly, sort]);

  if (q.isPending) return <Loading>{tr("Loading teammates…")}</Loading>;
  if (q.isError) return <ApiError error={q.error} />;
  const d = q.data;
  const shown = open ? rows : rows.slice(0, FIRST);

  return (
    <div className={q.isPlaceholderData ? "mates refetching has-refresh" : "mates has-refresh"}>
      <Refreshing on={q.isPlaceholderData} />
      <div className="mates-controls">
        <p className="hint">
          {tx("Everyone with {minGames} or more games on your side, from {0} {1}. \"Your rating with them\" compares your games together with your other games: who you played well alongside, not who made you play well.", {
            minGames: d.minGames,
            "0": d.games.toLocaleString(),
            "1": all ? tr("games, pugs included") : tr("officials and scrims"),
          })}
        </p>
        <div className="mates-toggles">
          <div className="segmented" role="tablist" aria-label={tr("Which games")}>
            <button role="tab" aria-selected={!all} className={!all ? "seg active" : "seg"} onClick={() => setAll(false)}>
              {tr("Officials and scrims")}
            </button>
            <button role="tab" aria-selected={all} className={all ? "seg active" : "seg"} onClick={() => setAll(true)}>
              {tr("Including pugs")}
            </button>
          </div>
          <label className="check">
            <input type="checkbox" checked={currentOnly} onChange={(e) => setCurrentOnly(e.target.checked)} />
            {tr("Played together in the last two months")}
          </label>
        </div>
      </div>

      {rows.length === 0 ? (
        <p className="hint">{currentOnly ? tr("Nobody recent yet.") : tr("No regular teammates yet. Sync to pull your history.")}</p>
      ) : (
        <div className="table-wrap">
          <table className="match-table mates-table">
            <thead>
              <tr>
                <th>{tr("Player")}</th>
                {SORTS.map((s) => (
                  <th key={s.key} className={s.num ? "num" : undefined} title={tr(s.title)}>
                    <button className={sort === s.key ? "sort active" : "sort"} onClick={() => setSort(s.key)} aria-pressed={sort === s.key}>
                      {tr(s.label)}
                      {sort === s.key && " ↓"}
                    </button>
                  </th>
                ))}
                <th>{tr("Teams")}</th>
              </tr>
            </thead>
            <tbody>
              {shown.map((m) => (
                <MateRow key={m.accountId} m={m} />
              ))}
            </tbody>
          </table>
        </div>
      )}
      {rows.length > FIRST && (
        <button className="linkish mates-more" onClick={() => setOpen(!open)}>
          {open ? tr("Show fewer") : tx("Show all {0} →", { "0": rows.length })}
        </button>
      )}
    </div>
  );
}

function MateRow({ m }: { m: Teammate }) {
  const wr = winRate(m);
  const go = () => openPlayer(m.accountId);
  return (
    <tr className="clickable" tabIndex={0} title={tr("Open {0}'s profile", { "0": m.name })} onClick={go} onKeyDown={(e) => e.key === "Enter" && go()}>
      <td className="nowrap mate-name">
        {m.current && <span className="dot ok" title={tr("Played together in the last two months")} />}
        {m.mainClass ? <ClassIcon cls={m.mainClass} size={16} /> : <span className="mate-noclass" />}
        <span title={m.mainClass ? classLabel(m.mainClass) : undefined}>{m.name}</span>
      </td>
      <td className="num">{m.games}</td>
      <td className="num">{m.officials || <span className="muted">–</span>}</td>
      <td className="num nowrap">
        {m.wins}–{m.losses}
        {wr !== null && <span className="muted"> {wr.toFixed(0)}%</span>}
      </td>
      <td className="muted nowrap">
        {formatDate(m.firstPlayed, true)} – {formatDate(m.lastPlayed, true)}
      </td>
      <td className="num nowrap">
        {m.myAvgWith === null ? (
          <span className="muted" title={tr("Too few rated games to compare")}>—</span>
        ) : (
          <>
            {rating(m.myAvgWith)} <span className={m.myAvgDelta! > 0 ? "delta up" : m.myAvgDelta! < 0 ? "delta down" : "delta"}>({signed(m.myAvgDelta!, 2)})</span>
          </>
        )}
      </td>
      <td className="muted mate-teams" title={m.teams.join(", ")}>
        {m.teams.join(", ")}
      </td>
    </tr>
  );
}
