import { useEffect, useMemo, useRef, useState } from "react";
import { RatingValue } from "../RatingValue";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type SeasonRow, type StatTotals } from "../../api/types";
import { rating } from "../../lib/format";
import { openPlayer } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { SortTh, useSort } from "../../lib/sort";
import { tip, type Entry, type GlossaryKey } from "../../lib/glossary";
import { ClassIcon } from "../ClassIcon";
import { CLASS_ORDER, classLabel } from "../analysis/common";
import { Loading, Refreshing } from "../Spinner";
import { SeasonBreakdown } from "./SeasonBreakdown";
import { StatTip } from "../StatTip";
import { ColumnPicker, useColumns, type PickerCol } from "../ColumnPicker";

/**
 * Season rankings (the MVP research, October 2026): every player of one
 * class in one division and season, with every number logs.tf records,
 * sortable by any column -- laid out like trends.tf's competition Players
 * page, with the app's season rating, impact and awards added. Worked out
 * from the logs on this machine; nothing is fetched.
 */

type Stage = "all" | "regular" | "playoffs";
type View = "per30" | "totals";

/** A column: its header, explanation, value per row, and how it reads. */
interface Col {
  key: string;
  label: string;
  tip: Entry | GlossaryKey;
  value: (r: SeasonRow) => number | null;
  digits: number;
  /** Shown only for these classes. */
  only?: string[];
  /** Needs the server log (blank where a player has none). */
  raw?: boolean;
}

const per30 = (t: StatTotals, v: number) => (t.timeS > 0 ? (v * 1800) / t.timeS : null);
const per30raw = (t: StatTotals, v: number) => (t.rawTimeS > 0 ? (v * 1800) / t.rawTimeS : null);
const perMin = (t: StatTotals, v: number) => (t.timeS > 0 ? (v * 60) / t.timeS : null);
const ratio = (a: number, b: number) => (b > 0 ? a / b : null);

const LOGS = "logs.tf" as const;
const RAW = "server log" as const;

function columns(view: View): Col[] {
  const p = view === "per30";
  const c = (key: string, label: string, what: string, how: string, value: (t: StatTotals) => number | null, digits: number, extra: Partial<Col> = {}): Col => ({
    key,
    label,
    tip: tip(label, what, how, extra.raw ? RAW : LOGS),
    value: (r) => value(r.totals),
    digits,
    ...extra,
  });
  const count = (key: string, label: string, what: string, field: keyof StatTotals, extra: Partial<Col> = {}) =>
    p
      ? c(`${key}30`, `${label}/30`, what, t("Per 30 minutes on the class."), (x) => (extra.raw ? per30raw : per30)(x, x[field] as number), 1, extra)
      : c(key, label, what, t("Summed over the season's games on the class."), (x) => x[field] as number, 0, extra);
  return [
    count("k", t("K"), t("Kills."), "kills"),
    count("d", t("D"), t("Deaths."), "deaths"),
    count("a", t("A"), t("Assists."), "assists"),
    c("kd", t("K/D"), t("Kills per death."), t("Kills over deaths, summed first."), (x) => ratio(x.kills, x.deaths), 2),
    c("kad", t("KA/D"), t("Kills and assists per death."), t("Kills plus assists, over deaths."), (x) => ratio(x.kills + x.assists, x.deaths), 2),
    p
      ? c("dpm", t("DA/M"), t("Damage dealt per minute."), t("Damage on the class over minutes on it."), (x) => perMin(x, x.dmg), 0)
      : c("dmg", t("DA"), t("Damage dealt."), t("Summed over the season's games."), (x) => x.dmg, 0),
    p
      ? c("dtm", t("DT/M"), t("Damage taken per minute."), t("Logged per player, on the class they played most of each log."), (x) => perMin(x, x.dt), 0)
      : c("dt", t("DT"), t("Damage taken."), t("Logged per player, on the class they played most of each log."), (x) => x.dt, 0),
    c("ddm", t("ΔD/M"), t("Damage dealt minus taken, per minute."), t("DA/M minus DT/M."), (x) => (x.timeS > 0 ? ((x.dmg - x.dt) * 60) / x.timeS : null), 0),
    c("dr", t("DR"), t("Damage dealt per damage taken."), t("Dealt over taken."), (x) => ratio(x.dmg, x.dt), 2),
    p
      ? c("heal", t("H/M"), t("Healing per minute."), t("Healing over minutes on the class."), (x) => perMin(x, x.heal), 0, { only: ["medic"] })
      : c("heal", t("Heals"), t("Healing."), t("Summed over the season's games."), (x) => x.heal, 0, { only: ["medic"] }),
    count("ubers", t("Ü"), t("Charges used."), "ubers", { only: ["medic"] }),
    count("drops", t("Drops"), t("Deaths with a full charge."), "drops", { only: ["medic"] }),
    c("advlost", t("Adv. lost"), t("Times the uber advantage was lost."), t("logs.tf's Medic statistics."), (x) => x.advLost, 0, { only: ["medic"] }),
    c("build", t("Build"), t("Average seconds to build a charge."), t("logs.tf's Medic statistics, averaged over the games that have them."), (x) => x.buildS, 1, { only: ["medic"] }),
    count("hs", t("HS"), t("Headshot kills."), "headshots", { only: ["sniper"] }),
    count("bs", t("BS"), t("Backstabs."), "backstabs", { only: ["spy"] }),
    count("air", t("Air"), t("Airshots."), "airshots", { only: ["soldier", "demoman"] }),
    count("caps", t("Caps"), t("Points captured."), "caps"),
    c("kast", t("KAST"), t("Share of fights with a kill, assist, survival or trade."), t("From the server log's fights."), (x) => ratio(x.fightsKast, x.fightsPresent) && ratio(x.fightsKast, x.fightsPresent)! * 100, 0, { raw: true }),
    count("open", t("Open"), t("Opening kills: the first kill of a fight."), "openingKills", { raw: true }),
    count("trade", t("Trades"), t("Kills avenging a teammate within 3 s."), "tradeKills", { raw: true }),
    count("down", t("Down"), t("Kills with the own side a player or more down."), "downKills", { raw: true }),
    count("multi", t("Multi"), t("Bursts of three or more kills, each within 5 s of the last."), "multi3", { raw: true }),
    c("streak", t("Streak"), t("Longest killstreak in one game."), t("logs.tf's killstreaks, the season's longest."), (x) => x.longestStreak, 0),
  ];
}

const AWARD: Record<string, [string, string]> = {
  season: ["★", "Season MVP"],
  evp: ["☆", "EVP"],
  class: ["✦", "Best of the class"],
};

export function SeasonRankings({ season, onTeam }: { season: number; onTeam: (id: number) => void }) {
  const [tier, setTier] = useState<number | null>(null);
  const [cls, setCls] = useState("scout");
  const [stage, setStage] = useState<Stage>("all");
  const [view, setView] = useState<View>("per30");
  const [minOff, setMinOff] = useState(3);
  const [open, setOpen] = useState<number | null>(null);
  // The breakdown under a row is as wide as the table's window, not the
  // table, so it stays in view however far the columns scroll.
  const scroller = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => el.style.setProperty("--sr-view", `${el.clientWidth}px`));
    ro.observe(el);
    return () => ro.disconnect();
  });
  const q = useQuery({
    queryKey: ["season_table", season, tier, cls, stage],
    queryFn: () => api.getSeasonTable(season, tier, cls, stage),
    placeholderData: keepPreviousData,
    staleTime: 5 * 60_000,
  });
  const v = q.data;
  const offered = useMemo(() => columns(view).filter((c) => !c.only || c.only.includes(cls)), [view, cls]);
  // The viewer's pick of them (Flashy: "add the columns button to the other
  // tables too"); a stat and its per-30 form are picked apart.
  const pickable: PickerCol[] = useMemo(
    () => offered.map((c) => ({ key: c.key, label: c.label, name: typeof c.tip === "string" ? c.label : c.tip.what.replace(/\.$/, ""), group: c.raw ? t("From the server log") : t("From logs.tf") })),
    [offered],
  );
  const picker = useColumns("season-table", pickable);
  const cols = offered.filter((c) => picker.shown(pickable.find((x) => x.key === c.key)!));
  const rows = useMemo(() => (v?.rows ?? []).filter((r) => r.officials >= minOff || minOff <= 1), [v, minOff]);
  const sortCols: Record<string, (r: SeasonRow) => number | string | null> = {
    rank: (r) => r.rank ?? 9999,
    name: (r) => r.name,
    team: (r) => r.team?.name ?? "",
    off: (r) => r.officials,
    maps: (r) => r.maps,
    time: (r) => r.totals.timeS,
    rating: (r) => r.rating,
    average: (r) => r.average,
    opp: (r) => r.opponents ?? null,
    impact: (r) => r.impact,
  };
  for (const c of cols) sortCols[c.key] = c.value;
  const s = useSort(`season-table-${cls}`, rows, sortCols);
  const fmt = (x: number | null, digits: number) => (x === null || Number.isNaN(x) ? "–" : x.toFixed(digits));

  return (
    <section className="panel sr">
      <div className="sr-controls">
        <div className="segmented" role="tablist" aria-label={t("Division")}>
          {(v?.divisions ?? []).map(([id, name]) => (
            <button key={id} role="tab" aria-selected={v?.tier === id} className={v?.tier === id ? "seg active" : "seg"} onClick={() => setTier(id)}>
              {name}
            </button>
          ))}
        </div>
        <div className="sr-classes" role="tablist" aria-label={t("Class")}>
          {CLASS_ORDER.map((c) => (
            <button key={c} role="tab" aria-selected={cls === c} className={cls === c ? "sr-class on" : "sr-class"} onClick={() => setCls(c)} title={classLabel(c)}>
              <ClassIcon cls={c} size={20} />
            </button>
          ))}
        </div>
        <div className="segmented" role="tablist" aria-label={t("Stage")}>
          {(
            [
              ["all", t("Whole season")],
              ["regular", t("Regular season")],
              ["playoffs", t("Playoffs")],
            ] as [Stage, string][]
          ).map(([id, label]) => (
            <button key={id} role="tab" aria-selected={stage === id} className={stage === id ? "seg active" : "seg"} onClick={() => setStage(id)}>
              {label}
            </button>
          ))}
        </div>
        <div className="segmented" role="tablist" aria-label={t("Numbers")}>
          <button role="tab" aria-selected={view === "per30"} className={view === "per30" ? "seg active" : "seg"} onClick={() => setView("per30")}>
            {t("Per 30 minutes")}
          </button>
          <button role="tab" aria-selected={view === "totals"} className={view === "totals" ? "seg active" : "seg"} onClick={() => setView("totals")}>
            {t("Totals")}
          </button>
        </div>
        <label className="sr-min">
          {t("At least")}{" "}
          <select value={minOff} onChange={(e) => setMinOff(Number(e.target.value))} aria-label={t("Minimum officials")}>
            {[1, 2, 3, 4, 5, 6].map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>{" "}
          {t("officials")}
        </label>
        <ColumnPicker cols={pickable} state={picker} />
      </div>
      {q.isPending && <Loading>{t("Reading the season…")}</Loading>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {v && (
        <>
          <p className="hint sr-caption">
            {tx("{0}, {1}: {2} on {3}. Ranked by season rating from 3 officials; ★ Season MVP, ☆ EVP, ✦ best of the class.", {
              "0": v.seasonName,
              "1": v.division,
              "2": tx("{0} players", { "0": rows.length }),
              "3": classLabel(cls),
            })}{" "}
            {v.playoffsRequired
              ? t("The best of the class comes from the three medal teams, with {0} officials on the class for their own team; EVPs need the {0} officials, any team. Merc games are shown but do not count for awards.", { "0": v.awardOfficials })
              : t("Awards need {0} officials on the class.", { "0": v.awardOfficials })}
          </p>
          <div className="sr-scroll has-refresh" ref={scroller}>
            <Refreshing on={q.isPlaceholderData} />
            <table className="match-table sr-table">
              <thead>
                <tr>
                  <SortTh k="rank" s={s} tip="rank">
                    #
                  </SortTh>
                  <SortTh k="name" s={s}>
                    {t("Player")}
                  </SortTh>
                  <SortTh k="team" s={s}>
                    {t("Team")}
                  </SortTh>
                  <SortTh k="off" s={s} tip={tip(t("Officials"), t("Officials played on the class this season."), t("Counted once per match, however many maps it had."), "ETF2L")}>
                    {t("Off.")}
                  </SortTh>
                  <SortTh k="maps" s={s}>
                    {t("Maps")}
                  </SortTh>
                  <SortTh k="rating" s={s} tip="seasonRating">
                    {t("Rating")}
                  </SortTh>
                  <SortTh k="average" s={s} tip={tip(t("Average"), t("The plain average rating over the season's officials."), t("Each official once, not adjusted for the opponents and not pulled towards 1.00."), "logs.tf")}>
                    {t("Avg")}
                  </SortTh>
                  <SortTh k="opp" s={s} tip={tip(t("Opponents"), t("How strong the teams faced were, from what they won that season: rounds taken, and against whom. 1.00 an average team."), t("The season rating adds 0.075 for every 0.10 above 1.00, and takes it off below."), "logs.tf", "ETF2L")}>
                    {t("Opp.")}
                  </SortTh>
                  <SortTh k="impact" s={s} tip="impact">
                    {t("Impact")}
                  </SortTh>
                  {cols.map((c) => (
                    <SortTh key={c.key} k={c.key} s={s} tip={c.tip}>
                      {c.label}
                    </SortTh>
                  ))}
                  <SortTh k="time" s={s}>
                    {t("Time")}
                  </SortTh>
                </tr>
              </thead>
              <tbody>
                {s.sorted.map((r) => [
                  <tr
                    key={r.accountId}
                    className={`sr-row${r.rank === null ? " sr-unranked" : ""}${open === r.accountId ? " open" : ""}`}
                    onClick={(e) => {
                      if ((e.target as HTMLElement).closest("button")) return;
                      setOpen(open === r.accountId ? null : r.accountId);
                    }}
                    title={t("Click for how the season rating was worked out")}
                  >
                    <td className="num">{r.rank ?? "–"}</td>
                    <td className="sr-name">
                      <button className="linkish" onClick={() => openPlayer(r.accountId)}>
                        {r.name}
                      </button>
                      {!r.eligible && r.rank !== null && (
                        <span className="sr-noaward" title={t("Not eligible for the best of the class: fewer than {0} officials for their own team, or not on a medal team", { "0": v.awardOfficials })}>
                          ·
                        </span>
                      )}
                      {r.awards.map((a) => (
                        <span key={a} className={`sr-award sr-award-${a}`} title={t(AWARD[a][1])}>
                          {AWARD[a][0]}
                        </span>
                      ))}
                    </td>
                    <td className="sr-team">
                      {r.team && (
                        <button className="linkish" onClick={() => onTeam(r.team!.id)}>
                          {r.team.name}
                        </button>
                      )}
                    </td>
                    <td className="num">{r.officials}</td>
                    <td className="num">{r.maps}</td>
                    <td className="num strong"><RatingValue v={r.rating} /></td>
                    <td className="num"><RatingValue v={r.average} /></td>
                    <td className="num muted">{r.opponents == null ? "–" : rating(r.opponents)}</td>
                    <td className="num">{r.impact === null ? "–" : <RatingValue v={r.impact} />}</td>
                    {cols.map((c) => (
                      <td key={c.key} className="num">
                        {fmt(c.value(r), c.digits)}
                      </td>
                    ))}
                    <td className="num muted">{hm(r.totals.timeS)}</td>
                  </tr>,
                  open === r.accountId && (
                    <tr key={`${r.accountId}-sb`} className="sr-sb-row">
                      <td colSpan={10 + cols.length}>
                        <SeasonBreakdown season={season} tier={v.tier} cls={cls} accountId={r.accountId} />
                      </td>
                    </tr>
                  ),
                ])}
              </tbody>
            </table>
          </div>
          {rows.length === 0 && <p className="hint">{t("Nobody played enough officials on this class here.")}</p>}
          <p className="hint">
            <StatTip k="mvp" inline>
              {t("How the awards are decided")}
            </StatTip>
          </p>
        </>
      )}
    </section>
  );
}

function hm(s: number): string {
  const h = Math.floor(s / 3600);
  const m = Math.round((s % 3600) / 60);
  return `${h}:${String(m).padStart(2, "0")}`;
}
