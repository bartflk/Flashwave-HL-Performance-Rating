import { useMemo, useState } from "react";
import type { LogFlags, MatchDetail, PlayerRow } from "../../api/types";
import { capitalize, clock, teamLabel } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";

const CLASS_ORDER = ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"];

type Key = "team" | "name" | "k" | "a" | "d" | "da" | "dapm" | "kad" | "kd" | "dt" | "dtpm" | "hp" | "bs" | "hs" | "as" | "cap" | "rating";

type Col = {
  key: Key;
  label: string;
  title: string;
  /** The value sorted on and shown; `null` when the log did not record it. */
  value: (p: PlayerRow, f: LogFlags) => number | null;
  fmt?: (v: number) => string;
};

const per = (n: number, p: PlayerRow) => (p.timeS > 0 ? n / (p.timeS / 60) : 0);

const COLS: Col[] = [
  { key: "k", label: "K", title: "Kills", value: (p) => p.kills },
  { key: "a", label: "A", title: "Assists", value: (p) => p.assists },
  { key: "d", label: "D", title: "Deaths", value: (p) => p.deaths },
  { key: "da", label: "DA", title: "Damage dealt", value: (p) => p.dmg, fmt: (v) => v.toLocaleString() },
  { key: "dapm", label: "DA/M", title: "Damage per minute", value: (p) => p.dpm },
  { key: "kad", label: "KA/D", title: "Kills and assists per death", value: (p) => (p.kills + p.assists) / Math.max(1, p.deaths), fmt: (v) => v.toFixed(1) },
  { key: "kd", label: "K/D", title: "Kills per death", value: (p) => p.kills / Math.max(1, p.deaths), fmt: (v) => v.toFixed(1) },
  { key: "dt", label: "DT", title: "Damage taken", value: (p, f) => (f.dt ? p.dt : null), fmt: (v) => v.toLocaleString() },
  { key: "dtpm", label: "DT/M", title: "Damage taken per minute", value: (p, f) => (f.dt ? per(p.dt, p) : null), fmt: (v) => v.toFixed(0) },
  { key: "hp", label: "HP", title: "Health packs picked up", value: (p) => p.medkits },
  { key: "bs", label: "BS", title: "Backstabs", value: (p, f) => (f.bs ? p.backstabs : null) },
  { key: "hs", label: "HS", title: "Headshot kills", value: (p, f) => (f.hs ? p.headshots : null) },
  { key: "as", label: "AS", title: "Airshots", value: (p, f) => (f.airshots ? p.airshots : null) },
  { key: "cap", label: "CAP", title: "Points captured", value: (p, f) => (f.cp ? p.cpc : null) },
  {
    key: "rating",
    label: "Rating",
    title: "Rating on the main class, 0-100 against the players you face. Not rated under 5 minutes.",
    value: (p) => p.rating?.score ?? null,
    fmt: (v) => v.toFixed(0),
  },
];

/**
 * The scoreboard, laid out like logs.tf: one table, both teams, a column per
 * stat, every column sortable. It opens sorted by team then class, your team
 * first.
 */
export function BoxScore({ d }: { d: MatchDetail }) {
  const [sort, setSort] = useState<{ key: Key; desc: boolean }>({ key: "team", desc: false });

  const rows = useMemo(() => {
    const byTeamClass = (a: PlayerRow, b: PlayerRow) =>
      (a.team === d.leftTeam ? 0 : 1) - (b.team === d.leftTeam ? 0 : 1) ||
      CLASS_ORDER.indexOf(a.mainClass ?? "") - CLASS_ORDER.indexOf(b.mainClass ?? "");
    const col = COLS.find((c) => c.key === sort.key);
    const out = [...d.players];
    if (sort.key === "team") out.sort(byTeamClass);
    else if (sort.key === "name") out.sort((a, b) => a.name.localeCompare(b.name));
    else if (col) {
      // Unrecorded values sink to the bottom whichever way the column is sorted.
      const v = (p: PlayerRow) => col.value(p, d.flags);
      out.sort((a, b) => {
        const x = v(a);
        const y = v(b);
        if (x === null || y === null) return x === null ? (y === null ? 0 : 1) : -1;
        return y - x || byTeamClass(a, b);
      });
    }
    if (sort.desc) out.reverse();
    return out;
  }, [d, sort]);

  const click = (key: Key) =>
    setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: false }));
  const arrow = (key: Key) => (sort.key === key ? (sort.desc ? " ▴" : " ▾") : "");

  return (
    <section className="panel box">
      <header className="box-head">
        <h2>Scoreboard</h2>
        <p className="hint">Click a column to sort. Hover a class for time played; a dash means the log did not record it.</p>
      </header>
      <div className="table-wrap">
        <table className="match-table scoreboard">
          <thead>
            <tr>
              <th className="sortable" onClick={() => click("team")} aria-sort={sort.key === "team" ? "ascending" : "none"}>
                Team{arrow("team")}
              </th>
              <th className="sortable" onClick={() => click("name")}>
                Name{arrow("name")}
              </th>
              <th>C</th>
              {COLS.map((c) => (
                <th key={c.key} className="num sortable" title={c.title} onClick={() => click(c.key)}>
                  {c.label}
                  {arrow(c.key)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((p) => (
              <tr key={p.accountId} className={p.isMe ? "me-row" : undefined}>
                <td className={`sb-team sb-team-${p.team.toLowerCase()}`}>{teamLabel(p.team)}</td>
                <td className="nowrap player-name">
                  {p.name}
                  {p.isMe && <span className="you-tag">you</span>}
                </td>
                <td className="sb-classes">
                  {p.classes.map(([c, t], i) => (
                    <span key={c} title={`${capitalize(c)} ${clock(t)}`}>
                      <ClassIcon cls={c} size={i === 0 ? 22 : 16} faded={i > 0} />
                    </span>
                  ))}
                </td>
                {COLS.map((c) => {
                  const v = c.value(p, d.flags);
                  return (
                    <td key={c.key} className={c.key === "rating" ? "num sb-rating" : "num"}>
                      {v === null ? <span className="muted">–</span> : c.fmt ? c.fmt(v) : String(Math.round(v))}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
