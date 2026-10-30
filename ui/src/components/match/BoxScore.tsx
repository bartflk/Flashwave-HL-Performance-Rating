import { useMemo, useRef, useState, type ReactNode } from "react";
import { RatingValue } from "../RatingValue";
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
import { tierOf, useStatScales } from "../../lib/tiers";
import { TierPill } from "../TierPill";
import { ColumnPicker, useColumns } from "../ColumnPicker";
import { RankBySwitch, useRankBy } from "../RankBy";

const CLASS_ORDER = ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"];

type Key =
  | "team" | "name" | "k" | "a" | "d" | "da" | "dt" | "hr" | "heal" | "ubers" | "drops" | "hp" | "hs" | "hshit" | "bs" | "as" | "cap" | "time"
  | "dapm" | "dtpm" | "hrpm" | "healpm" | "k30" | "a30" | "d30" | "ka30" | "kad" | "kd" | "dmgk" | "acc" | "spawn" | "ping" | "rating" | "impact"
  | "sg" | "bld";

/** The log's flags, and whether anyone's caps cost their team a respawn. */
type Flags = LogFlags & { spawn: boolean; ping: Map<number, PlayerPing> | null; impact: Map<number, number> | null };

type Col = {
  key: Key;
  label: string;
  /** What it is in words, for the Columns list. */
  name: string;
  group: string;
  /** Shown before the viewer picks; true unless said. */
  on?: boolean;
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

const per30 = (n: number, p: PlayerRow) => (p.timeS > 0 ? n / (p.timeS / 1800) : 0);
const f1 = (v: number) => v.toFixed(1);
const f0 = (v: number) => v.toFixed(0);
const thousands = (v: number) => v.toLocaleString();

/** Every stat the log has (Flashy: "i want all the stats possible"): the
 *  common ones on, the rest a tick away in Columns. Backstabs and
 *  airshots start off, the class card has them; accuracy shows (Flashy:
 *  "the accuracy can be on by default"). */
const COLS: Col[] = [
  { key: "k", label: "K", name: k("Kills"), group: k("Totals"), title: k("Kills"), value: (p) => p.kills },
  { key: "a", label: "A", name: k("Assists"), group: k("Totals"), title: k("Assists"), value: (p) => p.assists },
  { key: "d", label: "D", name: k("Deaths"), group: k("Totals"), title: k("Deaths"), value: (p) => p.deaths },
  { key: "da", label: "DA", name: k("Damage dealt"), group: k("Totals"), title: k("Damage dealt"), value: (p) => p.dmg, fmt: thousands },
  { key: "dt", label: "DT", name: k("Damage taken"), group: k("Totals"), title: k("Damage taken"), value: (p, f) => (f.dt ? p.dt : null), fmt: thousands },
  { key: "hr", label: "HR", name: k("Healing received"), group: k("Totals"), title: k("Healing received"), value: (p) => p.hr, fmt: thousands, on: false },
  { key: "heal", label: k("Heals"), name: k("Healing given"), group: k("Totals"), title: k("Healing given, mostly the Medic's"), value: (p) => p.heal, fmt: thousands, on: false },
  { key: "ubers", label: k("Ubers"), name: k("Ubers used"), group: k("Totals"), title: k("Charges used"), value: (p) => p.ubers, on: false },
  { key: "drops", label: k("Drops"), name: k("Ubers dropped"), group: k("Totals"), title: k("Died with a full charge"), value: (p) => p.drops, on: false },
  { key: "hp", label: "HP", name: k("Health packs"), group: k("Totals"), title: k("Health packs picked up"), value: (p) => p.medkits },
  { key: "hs", label: "HS", name: k("Headshot kills"), group: k("Totals"), title: k("Headshot kills"), value: (p, f) => (f.hs ? p.headshots : null) },
  { key: "hshit", label: "HSH", name: k("Headshots landed"), group: k("Totals"), title: k("Headshots landed, kills or not"), value: (p, f) => (f.hs ? p.headshotsHit : null), on: false },
  { key: "bs", label: "BS", name: k("Backstabs"), group: k("Totals"), title: k("Backstabs"), value: (p, f) => (f.bs ? p.backstabs : null), on: false },
  { key: "as", label: "AS", name: k("Airshots"), group: k("Totals"), title: k("Airshots"), value: (p, f) => (f.airshots ? p.airshots : null), on: false },
  { key: "cap", label: k("CAP"), name: k("Points captured"), group: k("Totals"), title: k("Points captured"), value: (p, f) => (f.cp ? p.cpc : null) },
  { key: "sg", label: k("SG"), name: k("Sentries destroyed"), group: k("Totals"), title: k("Sentries destroyed, from the server log"), value: (p) => p.sentryKills ?? null, on: false },
  { key: "bld", label: k("BLD"), name: k("Buildings destroyed"), group: k("Totals"), title: k("Sentries, dispensers and teleporters destroyed, from the server log"), value: (p) => p.buildingKills ?? null, on: false },
  { key: "time", label: k("Time"), name: k("Time played"), group: k("Totals"), title: k("Time played"), value: (p) => p.timeS, fmt: (v) => clock(v), on: false },

  { key: "dapm", label: "DA/M", name: k("Damage per minute"), group: k("Per minute"), title: k("Damage per minute, coloured by where it stands among the class's games: muted slate for an ordinary game, through amber and red to violet, black for the top 1%."), value: (p) => p.dpm },
  { key: "dtpm", label: "DT/M", name: k("Damage taken per minute"), group: k("Per minute"), title: k("Damage taken per minute"), value: (p, f) => (f.dt ? per(p.dt, p) : null), fmt: f0 },
  { key: "hrpm", label: "HR/M", name: k("Healing received per minute"), group: k("Per minute"), title: k("Healing received per minute"), value: (p) => per(p.hr, p), fmt: f0, on: false },
  { key: "healpm", label: k("Heals/M"), name: k("Healing given per minute"), group: k("Per minute"), title: k("Healing given per minute"), value: (p) => (p.heal > 0 ? per(p.heal, p) : null), fmt: f0, on: false },

  { key: "k30", label: "K/30", name: k("Kills per 30 minutes"), group: k("Per 30 minutes"), title: k("Kills per 30 minutes played"), value: (p) => per30(p.kills, p), fmt: f1, on: false },
  { key: "a30", label: "A/30", name: k("Assists per 30 minutes"), group: k("Per 30 minutes"), title: k("Assists per 30 minutes played"), value: (p) => per30(p.assists, p), fmt: f1, on: false },
  { key: "d30", label: "D/30", name: k("Deaths per 30 minutes"), group: k("Per 30 minutes"), title: k("Deaths per 30 minutes played"), value: (p) => per30(p.deaths, p), fmt: f1, on: false },
  { key: "ka30", label: "KA/30", name: k("Kills and assists per 30 minutes"), group: k("Per 30 minutes"), title: k("Kills and assists per 30 minutes played"), value: (p) => per30(p.kills + p.assists, p), fmt: f1, on: false },

  { key: "kad", label: "KA/D", name: k("Kills and assists per death"), group: k("Ratios"), title: k("Kills and assists per death"), value: (p) => (p.kills + p.assists) / Math.max(1, p.deaths), fmt: f1 },
  { key: "kd", label: "K/D", name: k("Kills per death"), group: k("Ratios"), title: k("Kills per death"), value: (p) => p.kills / Math.max(1, p.deaths), fmt: f1 },
  { key: "dmgk", label: "DA/K", name: k("Damage per kill"), group: k("Ratios"), title: k("Damage dealt per kill"), value: (p) => (p.kills > 0 ? p.dmg / p.kills : null), fmt: f0, on: false },
  {
    key: "acc",
    label: k("Acc"),
    name: k("Accuracy"),
    group: k("Ratios"),
    title: k("Accuracy: hits over shots on the main class, coloured by where it stands among the class's games. Only logs with logs.tf's accuracy stats have it."),
    value: (p) => p.accuracy ?? null,
    fmt: (v) => `${Math.round(v * 100)}%`,
  },

  {
    key: "spawn",
    label: k("RSP"),
    name: k("Respawn cost of caps"),
    group: k("Match"),
    title: k("Seconds of respawn your caps cost your own dead teammates: capping resets the wave, so whoever was waiting waits longer. KOTH and 5CP only. Shown, not rated."),
    value: (p, f) => (f.spawn ? (p.spawnDelayS ?? null) : null),
    fmt: (v) => `${v}s`,
  },
  {
    key: "ping",
    label: k("Ping"),
    name: k("Average ping"),
    group: k("Match"),
    title: k("Their average ping over the match, in milliseconds; hover a cell for the lowest, the highest and any spikes."),
    value: (p, f) => f.ping?.get(p.accountId)?.avg ?? null,
  },
  {
    key: "rating",
    label: k("Rating"),
    name: k("Rating"),
    group: k("Match"),
    title: k("Rating on the main class against the players you face: 1.00 is an average game. Not rated under 5 minutes."),
    value: (p) => p.rating?.score ?? null,
    fmt: (v) => rating(v),
  },
  {
    key: "impact",
    label: k("Impact"),
    name: k("Round-win impact"),
    group: k("Match"),
    title: k("Round-win chance their kills added and their deaths gave away, against the class: 1.00 is an average game. Shown beside the rating, never part of it."),
    value: (p, f) => f.impact?.get(p.accountId) ?? null,
    fmt: (v) => rating(v),
  },
];

/** The table's order: logs.tf's for the stats it has (Flashy: "re-order
 *  the table a bit to be more like logs.tf"), then the extras, then ping,
 *  rating and impact last. The Columns list groups them as above. */
const ORDER: Key[] = [
  "k", "a", "d", "da", "dapm", "kad", "kd", "dt", "dtpm", "hp", "bs", "hs", "as", "cap", "sg", "bld", "acc",
  "hr", "hrpm", "heal", "healpm", "ubers", "drops", "hshit", "k30", "a30", "d30", "ka30", "dmgk", "time",
  "spawn", "ping", "rating", "impact",
];
const ORDERED = [...COLS].sort((x, y) => ORDER.indexOf(x.key) - ORDER.indexOf(y.key));

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
  const hasAcc = d.players.some((p) => p.accuracy != null);
  // What this log has, then what the viewer picked of it.
  const available = ORDERED.filter(
    (c) =>
      (c.key !== "ping" || flags.ping) &&
      (c.key !== "impact" || flags.impact) &&
      (c.key !== "acc" || hasAcc) &&
      (c.key !== "dt" && c.key !== "dtpm" || flags.dt) &&
      (c.key !== "hs" && c.key !== "hshit" || flags.hs) &&
      (c.key !== "cap" || flags.cp) &&
      (c.key !== "spawn" || flags.spawn) &&
      ((c.key !== "heal" && c.key !== "healpm" && c.key !== "ubers" && c.key !== "drops") || d.players.some((p) => p.heal > 0)) &&
      (c.key !== "bs" || flags.bs) &&
      (c.key !== "as" || flags.airshots),
  );
  const picker = useColumns("scoreboard", available);
  const cols = available.filter(picker.shown);
  const scales = useStatScales().data;
  const [sort, setSort] = useState<{ key: Key; desc: boolean }>({ key: "team", desc: false });
  const [rankBy, pickRank] = useRankBy("scoreboard");
  const hasPct = cols.some((c) => c.key === "dapm" || c.key === "acc");
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
      const raw = (p: PlayerRow) => col.value(p, flags);
      const stat = col.key === "dapm" ? "dpm" : col.key === "acc" ? "accuracy" : null;
      // By percentile: a class without a scale (too few games) sinks with
      // the unrecorded.
      const v = stat && rankBy === "pct" ? (p: PlayerRow) => tierOf(scales, p.mainClass, stat, raw(p), d.mapKey)?.pct ?? null : raw;
      out.sort((a, b) => {
        const x = v(a);
        const y = v(b);
        if (x === null || y === null) return x === null ? (y === null ? 0 : 1) : -1;
        return y - x || byTeamClass(a, b);
      });
    }
    if (sort.desc) out.reverse();
    return out;
  }, [d, sort, flags, cols, rankBy, scales]);

  const click = (key: Key) =>
    setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: false }));
  const arrow = (key: Key) => (sort.key === key ? (sort.desc ? " ▴" : " ▾") : "");

  return (
    <section className="panel box">
      <header className="box-head">
        <h2>{tr("Scoreboard")}</h2>
        {/* Which log the page is reading. It had a panel of its own for one
            select; it belongs with the numbers it scopes. */}
        <div className="sb-head-tools">
          {reading}
          {hasPct && <RankBySwitch value={rankBy} onPick={pickRank} stat="DA/M" />}
          <ColumnPicker cols={available} state={picker} />
        </div>
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
                      {v === null ? (
                        <span className="muted">–</span>
                      ) : c.key === "rating" || c.key === "impact" ? (
                        <RatingValue v={v} />
                      ) : c.key === "dapm" || c.key === "acc" ? (
                        <TierPill text={c.fmt ? c.fmt(v) : String(Math.round(v))} tier={tierOf(scales, p.mainClass, c.key === "dapm" ? "dpm" : "accuracy", v, d.mapKey)} cls={p.mainClass} fmt={c.fmt} />
                      ) : c.fmt ? (
                        c.fmt(v)
                      ) : (
                        String(Math.round(v))
                      )}
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
  const [pos, setPos] = useState<{ top?: number; bottom?: number; left: number } | null>(null);
  const box = useRef<HTMLSpanElement>(null);
  // An older backend sends only each class's time: the rest shows as "–".
  const known = p.classLines !== undefined;
  const lines: ClassStat[] = p.classLines ?? p.classes.map(([c, t]) => ({ class: c, timeS: t, kills: 0, deaths: 0, assists: 0, dmg: 0, shots: 0, hits: 0 }));
  const show = () => {
    const r = box.current?.getBoundingClientRect();
    if (!r) return;
    const width = 460;
    const left = Math.max(8, Math.min(r.left, window.innerWidth - width - 8));
    // Above the icons when the card would run off the window's foot.
    const tall = 120 + 46 * Math.max(1, p.classes.length);
    setPos(r.bottom + 6 + tall > window.innerHeight ? { bottom: window.innerHeight - r.top + 6, left } : { top: r.bottom + 6, left });
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
        <ClassIcon key={c} cls={c} size={i === 0 ? 22 : 16} faded={i > 0} untitled />
      ))}
      {pos &&
        createPortal(
          <span className="sb-class-card" role="tooltip" style={pos}>
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
