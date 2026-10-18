import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { LogFlags, MatchDetail, PlayerPing, PlayerRow } from "../../api/types";
import { clock, rating, teamLabel } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";
import { t as tr, tx, k } from "../../lib/i18n";
import { classLabel } from "../analysis/common";
import { DivTag, useMatchDivisions } from "./Divisions";
import { PlayerPopCard, nameProps, usePlayerCard } from "../players/PlayerPopCard";
import { StatTip } from "../StatTip";
import { tip } from "../../lib/glossary";

const CLASS_ORDER = ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"];

type Key = "team" | "name" | "k" | "a" | "d" | "da" | "dapm" | "kad" | "kd" | "dt" | "dtpm" | "hp" | "bs" | "hs" | "as" | "cap" | "spawn" | "ping" | "rating";

/** The log's flags, and whether anyone's caps cost their team a respawn. */
type Flags = LogFlags & { spawn: boolean; ping: Map<number, PlayerPing> | null };

type Col = {
  key: Key;
  label: string;
  title: string;
  /** The value sorted on and shown; `null` when the log did not record it. */
  value: (p: PlayerRow, f: Flags) => number | null;
  fmt?: (v: number) => string;
};

const per = (n: number, p: PlayerRow) => (p.timeS > 0 ? n / (p.timeS / 60) : 0);

/** How the scoreboard's columns are counted. */
const BOX_HOW = k("logs.tf's own count for the whole match, or the part picked; blank where the log did not track it (the oldest logs have no damage taken, headshots or airshots).");
const PING_HOW = k("Read from a demo of this match (the STV, or your own recording): what the server measured, about once a second. Blank without one.");
const SPAWN_HOW = k("Worked out from the server log: each cap's reset of the respawn wave, against the teammates who were dead and waiting.");

const COLS: Col[] = [
  { key: "k", label: "K", title: k("Kills"), value: (p) => p.kills },
  { key: "a", label: "A", title: k("Assists"), value: (p) => p.assists },
  { key: "d", label: "D", title: k("Deaths"), value: (p) => p.deaths },
  { key: "da", label: "DA", title: k("Damage dealt"), value: (p) => p.dmg, fmt: (v) => v.toLocaleString() },
  { key: "dapm", label: "DA/M", title: k("Damage per minute"), value: (p) => p.dpm },
  { key: "kad", label: "KA/D", title: k("Kills and assists per death"), value: (p) => (p.kills + p.assists) / Math.max(1, p.deaths), fmt: (v) => v.toFixed(1) },
  { key: "kd", label: "K/D", title: k("Kills per death"), value: (p) => p.kills / Math.max(1, p.deaths), fmt: (v) => v.toFixed(1) },
  { key: "dt", label: "DT", title: k("Damage taken"), value: (p, f) => (f.dt ? p.dt : null), fmt: (v) => v.toLocaleString() },
  { key: "dtpm", label: "DT/M", title: k("Damage taken per minute"), value: (p, f) => (f.dt ? per(p.dt, p) : null), fmt: (v) => v.toFixed(0) },
  { key: "hp", label: "HP", title: k("Health packs picked up"), value: (p) => p.medkits },
  { key: "bs", label: "BS", title: k("Backstabs"), value: (p, f) => (f.bs ? p.backstabs : null) },
  { key: "hs", label: "HS", title: k("Headshot kills"), value: (p, f) => (f.hs ? p.headshots : null) },
  { key: "as", label: "AS", title: k("Airshots"), value: (p, f) => (f.airshots ? p.airshots : null) },
  { key: "cap", label: k("CAP"), title: k("Points captured"), value: (p, f) => (f.cp ? p.cpc : null) },
  {
    key: "spawn",
    label: k("RSP"),
    title: k("Seconds of respawn your caps cost your own dead teammates: capping resets the wave, so whoever was waiting waits longer. KOTH and 5CP only. Shown, not rated."),
    value: (p, f) => (f.spawn ? (p.spawnDelayS ?? null) : null),
    fmt: (v) => `${v}s`,
  },
  {
    key: "ping",
    label: k("Ping"),
    title: k("Their average ping over the match, in milliseconds; hover a cell for the lowest, the highest and any spikes."),
    value: (p, f) => f.ping?.get(p.accountId)?.avg ?? null,
  },
  {
    key: "rating",
    label: k("Rating"),
    title: k("Rating on the main class against the players you face: 1.00 is an average game. Not rated under 5 minutes."),
    value: (p) => p.rating?.score ?? null,
    fmt: (v) => rating(v),
  },
];

/**
 * The scoreboard, laid out like logs.tf: one table, both teams, a column per
 * stat, every column sortable. It opens sorted by team then class, your team
 * first.
 */
export function BoxScore({ d, reading }: { d: MatchDetail; reading?: ReactNode }) {
  const divisions = useMatchDivisions(d.logId);
  // Click a name: their card (Q40).
  const card = usePlayerCard();
  // The respawn column only where someone's caps cost one: KOTH and 5CP.
  // Ping from the match's demo, when there is one (Q83: it had a panel of
  // its own; Flashy wanted it beside the rest of the numbers).
  const demo = useQuery({ queryKey: ["demostats", d.logId], queryFn: () => api.getDemoStats(d.logId) });
  const pings = demo.data?.pings;
  const flags: Flags = useMemo(
    () => ({
      ...d.flags,
      spawn: d.players.some((p) => (p.spawnDelayS ?? 0) > 0),
      ping: pings && pings.length > 0 ? new Map(pings.map((x) => [x.accountId, x])) : null,
    }),
    [d.flags, d.players, pings],
  );
  const cols = COLS.filter((c) => c.key !== "ping" || flags.ping);
  const [sort, setSort] = useState<{ key: Key; desc: boolean }>({ key: "team", desc: false });
  const rows = useMemo(() => {
    const byTeamClass = (a: PlayerRow, b: PlayerRow) =>
      (a.team === d.leftTeam ? 0 : 1) - (b.team === d.leftTeam ? 0 : 1) ||
      CLASS_ORDER.indexOf(a.mainClass ?? "") - CLASS_ORDER.indexOf(b.mainClass ?? "");
    const col = cols.find((c) => c.key === sort.key);
    const out = [...d.players];
    if (sort.key === "team") out.sort(byTeamClass);
    else if (sort.key === "name") out.sort((a, b) => a.name.localeCompare(b.name));
    else if (col) {
      // Unrecorded values sink to the bottom whichever way the column is sorted.
      const v = (p: PlayerRow) => col.value(p, flags);
      out.sort((a, b) => {
        const x = v(a);
        const y = v(b);
        if (x === null || y === null) return x === null ? (y === null ? 0 : 1) : -1;
        return y - x || byTeamClass(a, b);
      });
    }
    if (sort.desc) out.reverse();
    return out;
  }, [d, sort, flags, cols]);

  const click = (key: Key) =>
    setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: false }));
  const arrow = (key: Key) => (sort.key === key ? (sort.desc ? " ▴" : " ▾") : "");

  return (
    <section className="panel box">
      <header className="box-head">
        <div>
          <h2>{tr("Scoreboard")}</h2>
          <p className="hint">{tr("Click a column to sort.")}</p>
        </div>
        {/* Which log the page is reading. It had a panel of its own for one
            select; it belongs with the numbers it scopes. */}
        {reading}
      </header>
      <div className="table-wrap">
        <table className="match-table scoreboard">
          <thead>
            <tr>
              <th className="sortable" onClick={() => click("team")} aria-sort={sort.key === "team" ? "ascending" : "none"}>{tx("Team{0}", { "0": arrow("team") })}
              </th>
              <th className="sortable" onClick={() => click("name")}>{tx("Name{0}", { "0": arrow("name") })}
              </th>
              <th>C</th>
              {cols.map((c) => (
                <th key={c.key} className="num sortable" onClick={() => click(c.key)}>
                  {/* What the column is and where it comes from (Q80). */}
                  {c.key === "rating" ? (
                    <StatTip k="rating" inline>{tr(c.label)}</StatTip>
                  ) : (
                    <StatTip entry={tip(tr(c.label), tr(c.title), c.key === "spawn" ? tr(SPAWN_HOW) : c.key === "ping" ? tr(PING_HOW) : tr(BOX_HOW), c.key === "spawn" ? "server log" : c.key === "ping" ? "STV demo" : "logs.tf")} inline>
                      {tr(c.label)}
                    </StatTip>
                  )}
                  {arrow(c.key)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((p) => (
              <tr key={p.accountId} className={`row-${p.team.toLowerCase()}${p.isMe ? " me-row" : ""}`}>
                <td className={`sb-team sb-team-${p.team.toLowerCase()}`}>
                  <span className={`mx-colour mx-colour-${p.team.toLowerCase()}`}>{teamLabel(p.team)}</span>
                </td>
                <td className="nowrap player-name">
                  <span {...nameProps(card.open({ accountId: p.accountId, name: p.name, cls: p.classes[0]?.[0] ?? null, isMe: p.isMe }))} title={tr("Their card")}>
                    {p.name}
                  </span>
                  <DivTag divisions={divisions} accountId={p.accountId} />
                  {p.isMe && <span className="you-tag">{tr("you")}</span>}
                </td>
                <td className="sb-classes">
                  {p.classes.map(([c, t], i) => (
                    <span key={c} title={`${classLabel(c)} ${clock(t)}`}>
                      <ClassIcon cls={c} size={i === 0 ? 22 : 16} faded={i > 0} />
                    </span>
                  ))}
                </td>
                {cols.map((c) => {
                  const v = c.value(p, flags);
                  const pg = c.key === "ping" ? flags.ping?.get(p.accountId) : undefined;
                  return (
                    <td
                      key={c.key}
                      className={c.key === "rating" ? "num sb-rating" : "num"}
                      title={
                        pg
                          ? [
                              tr("{0}–{1} ms", { "0": pg.min, "1": pg.max }),
                              ...pg.spikes.map(([a, b, peak]) => tr("spike {0}-{1}, up to {2} ms", { "0": clock(a), "1": clock(b), "2": peak })),
                            ].join("\n")
                          : undefined
                      }
                    >
                      {v === null ? <span className="muted">–</span> : c.fmt ? c.fmt(v) : String(Math.round(v))}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {card.target && <PlayerPopCard target={card.target} divisions={divisions} onClose={card.close} />}
    </section>
  );
}
