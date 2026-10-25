import { useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { ClassStat, LogFlags, MatchDetail, PlayerPing, PlayerRow } from "../../api/types";
import { clock, rating, teamLabel } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";
import { t as tr, tx, k } from "../../lib/i18n";
import { classLabel } from "../analysis/common";
import { DivTag, useMatchDivisions } from "./Divisions";
import { PlayerPopCard, nameProps, usePlayerCard } from "../players/PlayerPopCard";
import { StatTip } from "../StatTip";
import { tip } from "../../lib/glossary";

const CLASS_ORDER = ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"];

type Key = "team" | "name" | "k" | "a" | "d" | "da" | "dapm" | "kad" | "kd" | "dt" | "dtpm" | "hp" | "bs" | "hs" | "as" | "cap" | "spawn" | "ping" | "rating" | "impact";

/** The log's flags, and whether anyone's caps cost their team a respawn. */
type Flags = LogFlags & { spawn: boolean; ping: Map<number, PlayerPing> | null; impact: Map<number, number> | null };

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
  { key: "hs", label: "HS", title: k("Headshot kills"), value: (p, f) => (f.hs ? p.headshots : null) },
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
  {
    key: "impact",
    label: k("Impact"),
    title: k("Round-win chance their kills added and their deaths gave away, against the class: 1.00 is an average game. Shown beside the rating, never part of it."),
    value: (p, f) => f.impact?.get(p.accountId) ?? null,
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
  // Impact from the server log, where there is one (the MVP research).
  const impact = useQuery({ queryKey: ["match_impact", d.logId], queryFn: () => api.getMatchImpact(d.logId) });
  const flags: Flags = useMemo(
    () => ({
      ...d.flags,
      spawn: d.players.some((p) => (p.spawnDelayS ?? 0) > 0),
      ping: pings && pings.length > 0 ? new Map(pings.map((x) => [x.accountId, x])) : null,
      impact: impact.data && Object.keys(impact.data).length > 0 ? new Map(Object.entries(impact.data).map(([a, v]) => [Number(a), v])) : null,
    }),
    [d.flags, d.players, pings, impact.data],
  );
  const cols = COLS.filter((c) => (c.key !== "ping" || flags.ping) && (c.key !== "impact" || flags.impact));
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
        <h2>{tr("Scoreboard")}</h2>
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
                  {c.key === "rating" || c.key === "impact" ? (
                    <StatTip k={c.key} inline>{tr(c.label)}</StatTip>
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
                  <ClassCard p={p} flags={flags} />
                </td>
                {cols.map((c) => {
                  const v = c.value(p, flags);
                  const pg = c.key === "ping" ? flags.ping?.get(p.accountId) : undefined;
                  return (
                    <td
                      key={c.key}
                      className={c.key === "rating" ? "num sb-rating" : c.key === "impact" ? "num sb-impact" : "num"}
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

/**
 * A player's classes, and on hover what each was (Flashy: "hide airshots
 * behind a hover over the soldier icon ... same for backstabs. Accuracy per
 * class also on hover like logs.tf"): time, K / D / A, damage and accuracy
 * per class, airshots beside Soldier and Demoman, backstabs beside Spy. The
 * scoreboard keeps one column fewer for each.
 */
function ClassCard({ p, flags }: { p: PlayerRow; flags: Flags }) {
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  const box = useRef<HTMLSpanElement>(null);
  // An older backend sends only each class's time: the rest shows as "–".
  const known = p.classLines !== undefined;
  const lines: ClassStat[] = p.classLines ?? p.classes.map(([c, t]) => ({ class: c, timeS: t, kills: 0, deaths: 0, assists: 0, dmg: 0, shots: 0, hits: 0 }));
  const show = () => {
    const r = box.current?.getBoundingClientRect();
    if (!r) return;
    const width = 340;
    setPos({ top: r.bottom + 6, left: Math.max(8, Math.min(r.left, window.innerWidth - width - 8)) });
  };
  const acc = (c: ClassStat) => (c.shots > 0 ? `${Math.round((c.hits / c.shots) * 100)}%` : "–");
  // Airshots and backstabs are the log's totals, not per class: they go on
  // the class that makes them.
  const extra = (c: ClassStat): string | null => {
    if (c.class === "spy" && flags.bs && p.backstabs > 0) return tr("{0} backstabs", { "0": p.backstabs });
    if ((c.class === "soldier" || c.class === "demoman") && flags.airshots && p.airshots > 0) return tr("{0} airshots", { "0": p.airshots });
    if (c.class === "sniper" && flags.hs && p.headshots > 0) return tr("{0} headshot kills", { "0": p.headshots });
    return null;
  };
  return (
    <span className="sb-class-icons" ref={box} tabIndex={0} onMouseEnter={show} onMouseLeave={() => setPos(null)} onFocus={show} onBlur={() => setPos(null)}>
      {p.classes.map(([c], i) => (
        <ClassIcon key={c} cls={c} size={i === 0 ? 22 : 16} faded={i > 0} />
      ))}
      {pos &&
        createPortal(
          <span className="sb-class-card" role="tooltip" style={{ top: pos.top, left: pos.left }}>
            <span className="sb-cc-head">{p.name}</span>
            <span className="sb-cc-row sb-cc-labels">
              <span>{tr("Class")}</span>
              <span>{tr("Time")}</span>
              <span>{tr("K / D / A")}</span>
              <span>{tr("DA")}</span>
              <span>{tr("Acc.")}</span>
            </span>
            {lines.map((c) => (
              <span key={c.class} className="sb-cc-class">
                <span className="sb-cc-row">
                  <span className="sb-cc-name">
                    <ClassIcon cls={c.class} size={16} /> {classLabel(c.class)}
                  </span>
                  <span>{clock(c.timeS)}</span>
                  <span>{known ? `${c.kills} / ${c.deaths} / ${c.assists}` : "–"}</span>
                  <span>{known ? c.dmg.toLocaleString() : "–"}</span>
                  <span>{acc(c)}</span>
                </span>
                {extra(c) && <span className="sb-cc-extra">{extra(c)}</span>}
              </span>
            ))}
            <span className="sb-cc-foot">{tr("Accuracy is hits over shots on the class's weapons, as logs.tf counts them.")}</span>
          </span>,
          document.body,
        )}
    </span>
  );
}
