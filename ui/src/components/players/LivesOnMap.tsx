import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ApiError } from "../ApiError";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { type LifeOnMap, type MapPlayer } from "../../api/types";
import { capitalize, formatDate } from "../../lib/format";
import { k, t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";
import { MassImportButton } from "./MassImport";
import { Loading } from "../Spinner";
import { CtlGroup, Icon, Seg } from "../analysis/MapChrome";
import { ReadAgain } from "../match/DemoPanel";
import { StvPartsNotice } from "../match/StvParts";
import { useCallouts } from "../analysis/Callouts";

/**
 * A player's lives on one map (Q57, Emiel): pick a map and a class, and
 * every route the demos on this machine followed them through is drawn on
 * it -- every Spy cross on Product, split by playing RED and BLU. Only the
 * matches with an STV, or with their own recording, have routes; the
 * counts say how many that is.
 */

type Side = "both" | "Red" | "Blue";
type Ending = "any" | "died" | "survived";
type View = "auto" | "routes" | "density" | "opening";
type Situation = "capped" | "afterCap" | "midFight";

/** The situation filters (Q67): `[key, label, what it means]`. */
const SITUATIONS: Array<[Situation, string, string]> = [
  ["capped", k("With a cap"), k("Their team took a point during the life")],
  ["afterCap", k("After the other team capped"), k("The life began within 20 s of the other team taking a point in the same round")],
  ["midFight", k("Spawned mid-fight"), k("Others were killing each other both in the 5 s before the life began and the 10 s after")],
];

/** Each situation's icon. */
const SITUATION_ICON: Record<Situation, "cap" | "afterCap" | "swords"> = { capped: "cap", afterCap: "afterCap", midFight: "swords" };

/** "Auto" shows where they usually go from this many lives up (Q65):
 *  past it, separate lines are a solid smear. */
const DENSITY_FROM = 300;

/** The density grid's cells across the frame. */
const CELLS = 140;

/** The round-openings view's hexes across the frame. */
const HEXES = 70;

/** The main position leaves out each round's first seconds: the spawn
 *  room and the way out of it, which every rollout shares. */
const OUT_OF_SPAWN_S = 8;

/** Nor anywhere this close (map units) to where a route began: the spawn
 *  room and its doors. */
const SPAWN_RADIUS = 900;

/** A callout zone counts as a spawn room by its name. */
const SPAWN_NAME = /spawn/i;

/** A map's spawn rooms from its callouts, in game units. */
function useSpawnZones(map: string | null): Array<Array<[number, number]>> {
  const q = useCallouts(map);
  return useMemo(() => (q.data?.zones ?? []).filter((z) => SPAWN_NAME.test(z.name) && !/exit/i.test(z.name)).map((z) => z.points), [q.data]);
}

/** Whether a point lies inside a polygon (even-odd). */
function inPolygon([x, y]: [number, number], poly: Array<[number, number]>): boolean {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [x1, y1] = poly[i];
    const [x2, y2] = poly[j];
    if (y1 > y !== y2 > y && x < ((x2 - x1) * (y - y1)) / (y2 - y1) + x1) c = !c;
  }
  return c;
}

/** How far (map units) around where each route begins is left undrawn. */
const SPAWN_HIDE = 650;

/** Where a route begins: after any teleport in its first seconds. A life
 *  alive across a round restart carries its old place for a moment before
 *  the respawn puts it in the spawn room. */
function beginOf(l: LifeOnMap): [number, number] | undefined {
  let at = 0;
  for (let j = 1; j < l.points.length; j++) {
    if ((l.openingT[j] ?? Infinity) > OUT_OF_SPAWN_S) break;
    const [a, b] = [l.points[j - 1], l.points[j]];
    if (Math.hypot(b[0] - a[0], b[1] - a[1]) > JUMP) at = j;
  }
  return l.points[at];
}

/** A hex both use at least this much (of each one's busiest) is outlined. */
const OVERLAP = 0.25;

/** How much of each round's start the openings view takes (Flashy: "the
 *  rollout heatmap first 20 seconds or something"). */
const WINDOWS = [10, 20, 30, 40] as const;
type Window = (typeof WINDOWS)[number];

/** One player's (or one side's) openings in the view, and how to draw them. */
type Group = { key: string; label: string; cls: string | null; colour: string; lives: LifeOnMap[]; of?: number };

/** A life cut to the round's first `window` seconds, and at the round's
 *  first kill when `toKill`: `null` when nothing is left to draw. */
function cut(l: LifeOnMap, window: number, toKill: boolean): LifeOnMap | null {
  const end = toKill && l.firstKill !== null ? Math.min(window, l.firstKill) : window;
  let n = 0;
  while (n < l.openingT.length && l.openingT[n] <= end) n++;
  if (n < 2) return null;
  // From the respawn on: a life alive across the round restart carries its
  // old place for a moment first.
  const w = { ...l, points: l.points.slice(l.openingSkip, l.openingSkip + n), openingT: l.openingT.slice(0, n) };
  const from = w.points.indexOf(beginOf(w) ?? w.points[0]);
  return from > 0 ? { ...w, points: w.points.slice(from), openingT: w.openingT.slice(from) } : w;
}

/** A step longer than this went through a teleporter or a respawn: the line
 *  breaks rather than crossing the map. Points are about four a second, so
 *  this is well past running speed. */
const JUMP = 900;

export function LivesOnMapTab({ accountId, name }: { accountId: number; name: string }) {
  const mapsQ = useQuery({ queryKey: ["lives_maps", accountId], queryFn: () => api.livesMaps(accountId), staleTime: 5 * 60_000 });
  const maps = mapsQ.data ?? [];
  const [picked, setPicked] = useState<string | null>(null);
  const map = picked ?? maps[0]?.map ?? null;
  const fullName = maps.find((m) => m.map === map)?.name ?? map;

  const livesQ = useQuery({
    queryKey: ["lives_on_map", accountId, map],
    queryFn: () => api.livesOnMap(accountId, map!),
    enabled: map !== null,
    staleTime: 5 * 60_000,
  });
  const spawnZones = useSpawnZones(fullName);
  const overviewQ = useQuery({
    queryKey: ["overview", fullName],
    queryFn: () => api.getMapOverview(fullName ?? ""),
    enabled: fullName !== null,
    staleTime: Infinity,
  });

  const [cls, setCls] = useState<string | null>(null);
  const [side, setSide] = useState<Side>("both");
  const [ending, setEnding] = useState<Ending>("any");
  // Q67: situations, each narrowing the lives further.
  const [situations, setSituations] = useState<Set<Situation>>(new Set());
  // Q66: only the most recent N matches (of this class and side); null is all.
  const [lastN, setLastN] = useState<number | null>(null);
  // Q65: every route, or where they usually go; "auto" picks by how many.
  const [view, setView] = useState<View>("auto");
  const [hover, setHover] = useState<LifeOnMap | null>(null);
  // The openings view's rollout window, and whether it stops at the round's
  // first kill.
  const [rollout, setRollout] = useState<Window>(20);
  const [toKill, setToKill] = useState(true);
  // Someone to compare with, and their class (Flashy: "where both snipers
  // rollout").
  // "opp": whoever played their class on the other team in the same rounds.
  const [vs, setVs] = useState<number | "opp" | null>(null);
  const [vsCls, setVsCls] = useState<string | null>(null);

  const all = useMemo(() => livesQ.data?.lives ?? [], [livesQ.data]);
  // The classes they played here, most lives first; the side filter applies.
  const classes = useMemo(() => {
    const n = new Map<string, number>();
    for (const l of all) if (l.class && (side === "both" || l.team === side)) n.set(l.class, (n.get(l.class) ?? 0) + 1);
    return [...n.entries()].sort((a, b) => b[1] - a[1]);
  }, [all, side]);
  // The class and side asked for, then the most recent N of their matches.
  const recent = useMemo(() => {
    const mine = all.filter((l) => (cls === null || l.class === cls) && (side === "both" || l.team === side));
    if (lastN === null) return { lives: mine, matches: new Set(mine.map((l) => l.logId)).size };
    const byDate = [...new Map(mine.map((l) => [l.logId, l.playedAt ?? 0])).entries()].sort((a, b) => b[1] - a[1]);
    const keep = new Set(byDate.slice(0, lastN).map(([id]) => id));
    return { lives: mine.filter((l) => keep.has(l.logId)), matches: keep.size };
  }, [all, cls, side, lastN]);
  // The same array until a filter changes (Q64): hovering re-renders this
  // component, and a new array would redraw every route on each move.
  const shown = useMemo(
    () =>
      recent.lives.filter(
        (l) =>
          (ending === "any" || (ending === "died") === l.died) &&
          (!situations.has("capped") || l.capped) &&
          (!situations.has("afterCap") || l.afterEnemyCap) &&
          (!situations.has("midFight") || l.midFight),
      ),
    [recent, ending, situations],
  );
  // The round openings (PGL's heatmaps): only the lives that began with a
  // round, cut to its first seconds. The end and situation filters are about
  // the rest of the life, so they do not apply.
  const openings = useMemo(() => recent.lives.flatMap((l) => cut(l, rollout, toKill) ?? []), [recent, rollout, toKill]);
  const opening = view === "opening";
  const playersQ = useQuery({
    queryKey: ["lives_players", accountId, map],
    queryFn: () => api.livesPlayers(accountId, map!),
    enabled: opening && map !== null,
    staleTime: 5 * 60_000,
  });
  const vsQ = useQuery({
    queryKey: vs === "opp" ? ["lives_opposite", accountId, map, cls] : ["lives_on_map", vs, map],
    queryFn: () => (vs === "opp" ? api.livesOpposite(accountId, map!, cls) : api.livesOnMap(vs!, map!)),
    enabled: opening && vs !== null && map !== null,
    staleTime: 5 * 60_000,
  });
  const opp = vs === "opp";
  const vsPlayer = (playersQ.data ?? []).find((p) => p.accountId === vs) ?? null;
  const vsName = opp ? (cls ? t("Opposite {0}", { "0": capitalize(cls) }) : t("Their opposite")) : (vsPlayer?.name ?? "?");
  const vsAll = useMemo(() => (vs === null ? [] : (vsQ.data?.lives ?? [])), [vs, vsQ.data]);
  // Their classes here among the lives that began a round, the side filter applied.
  const vsClasses = useMemo(() => {
    const n = new Map<string, number>();
    for (const l of vsAll) if (l.class && l.openingT.length > 0 && (side === "both" || l.team === side)) n.set(l.class, (n.get(l.class) ?? 0) + 1);
    return [...n.entries()].sort((a, b) => b[1] - a[1]);
  }, [vsAll, side]);
  // The opposite number plays the other side, in the same matches as the
  // ones shown (the Last N filter too).
  const vsOpenings = useMemo(() => {
    const logs = new Set(recent.lives.map((l) => l.logId));
    const keep = opp
      ? (l: LifeOnMap) => logs.has(l.logId) && (side === "both" || (l.team !== null && l.team !== side))
      : (l: LifeOnMap) => (vsCls === null || l.class === vsCls) && (side === "both" || l.team === side);
    return vsAll.filter(keep).flatMap((l) => cut(l, rollout, toKill) ?? []);
  }, [vsAll, vsCls, side, rollout, toKill, opp, recent]);
  // Side by side: theirs and the other player's, each in its own colour;
  // alone, RED's and BLU's.
  const groups: Group[] =
    vs !== null
      ? [
          { key: "a", label: name, cls, colour: "--cmp-a", lives: openings },
          { key: "b", label: vsName, cls: opp ? cls : vsCls, colour: "--cmp-b", lives: vsOpenings },
        ]
      : [
          { key: "Red", label: t("RED side"), cls, colour: "--red", lives: openings.filter((l) => l.team === "Red") },
          { key: "Blue", label: t("BLU side"), cls, colour: "--blu", lives: openings.filter((l) => l.team === "Blue") },
        ];
  const pickVs = (id: number | "opp" | null) => {
    setVs(id);
    // Their class: the one picked for this player when they played it here,
    // else their usual one.
    const p = (playersQ.data ?? []).find((x) => x.accountId === id);
    setVsCls(p ? (cls !== null && p.classes.some(([c]) => c === cls) ? cls : (p.classes[0]?.[0] ?? null)) : null);
  };
  const sides = { Red: all.filter((l) => (cls === null || l.class === cls) && l.team === "Red").length, Blue: all.filter((l) => (cls === null || l.class === cls) && l.team === "Blue").length };
  const mode: Mode = view === "auto" ? (shown.length > DENSITY_FROM ? "density" : "routes") : view;
  const toggle = (s: Situation) =>
    setSituations((was) => {
      const next = new Set(was);
      if (next.has(s)) next.delete(s);
      else next.add(s);
      return next;
    });

  if (mapsQ.isPending) return <Loading>{t("Loading…")}</Loading>;
  if (mapsQ.isError) return <ApiError error={mapsQ.error} />;
  if (maps.length === 0) {
    return (
      <div className="lm-empty">
        <p className="hint">
          {tx("No demo on this machine followed {0}. Routes come from SourceTV demos (download one from a match's Demo linking panel) and from their own recordings.", { "0": <strong>{name}</strong> })}
        </p>
        <MassImportButton accountId={accountId} name={name} map={null} cls={null} />
      </div>
    );
  }

  const v = livesQ.data;
  return (
    <div className="lm">
      {/* Every control in a labelled group, each option with its icon
          (Flashy: "add some icons to differentiate what buttons do what"). */}
      <div className="ctl-bar">
        <CtlGroup label={t("Map")}>
          <select
            className="lm-select"
            value={map ?? ""}
            onChange={(e) => {
              setPicked(e.target.value);
              setCls(null);
              setHover(null);
              setVs(null);
              setVsCls(null);
            }}
          >
            {maps.map((m) => (
              <option key={m.map} value={m.map}>
                {capitalize(m.map)} ({tx("{0} match{1}", { "0": m.matches, "1": m.matches === 1 ? "" : "es" })})
              </option>
            ))}
          </select>
        </CtlGroup>
        <CtlGroup label={t("Side")}>
          <Seg
            label={t("Side")}
            value={side}
            onChange={setSide}
            options={[
              { v: "both", label: <><Icon name="both" size={14} /> {t("Both")}</> },
              { v: "Red", label: <><i className="lm-dot red" /> RED <span className="lm-n">{sides.Red}</span></> },
              { v: "Blue", label: <><i className="lm-dot blu" /> BLU <span className="lm-n">{sides.Blue}</span></> },
            ]}
          />
        </CtlGroup>
        <CtlGroup label={t("Matches")}>
          <label className="lm-lastn" title={t("Only their most recent matches with this class and side; empty for all")}>
            <Icon name="history" size={14} />
            <span>{t("Last")}</span>
            <input
              type="number"
              min={1}
              step={1}
              inputMode="numeric"
              placeholder={t("all")}
              value={lastN ?? ""}
              onChange={(e) => {
                const n = Math.floor(Number(e.target.value));
                setLastN(e.target.value === "" || !Number.isFinite(n) || n < 1 ? null : n);
              }}
            />
            {lastN !== null ? (
              <button className="lm-clear" onClick={() => setLastN(null)} title={t("Every match")} aria-label={t("Every match")}>
                <Icon name="close" size={12} />
              </button>
            ) : (
              <span className="muted">{t("matches")}</span>
            )}
          </label>
        </CtlGroup>
        <CtlGroup label={t("View")}>
          <Seg
            label={t("View")}
            value={view}
            onChange={setView}
            options={[
              { v: "auto", label: <><Icon name="auto" size={14} /> {t("Auto")}</>, title: t("Most used from {0} lives, every route below that", { "0": DENSITY_FROM }) },
              { v: "routes", label: <><Icon name="routes" size={14} /> {t("Every route")}</>, title: t("One line per life") },
              { v: "density", label: <><Icon name="grid" size={14} /> {t("Most used")}</>, title: t("How many lives passed each spot: the busier, the stronger") },
              { v: "opening", label: <><Icon name="hex" size={14} /> {t("Round openings")}</>, title: t("Where they go at the start of each round, and where someone else does") },
            ]}
          />
        </CtlGroup>
        {/* Q69: far more than this machine recorded, from demos.tf. */}
        <span className="lm-head-actions">
          <MassImportButton accountId={accountId} name={name} map={map} cls={cls} />
        </span>
      </div>

      <div className="ctl-bar">
        {mode !== "opening" && (
          <>
            <CtlGroup label={t("How the life ended")}>
              <Seg
                label={t("How the life ended")}
                value={ending}
                onChange={setEnding}
                options={[
                  { v: "any", label: <><Icon name="any" size={14} /> {t("Any end")}</> },
                  { v: "died", label: <><Icon name="skull" size={14} /> {t("Ended in a death")}</> },
                  { v: "survived", label: <><Icon name="flag" size={14} /> {t("Lived to the round's end")}</> },
                ]}
              />
            </CtlGroup>
            <CtlGroup label={t("Situation")}>
              <div className="lm-toggles" role="group" aria-label={t("Situation")}>
                {SITUATIONS.map(([s, label, hint]) => (
                  <button key={s} className={situations.has(s) ? "lm-toggle on" : "lm-toggle"} aria-pressed={situations.has(s)} onClick={() => toggle(s)} title={t(hint)}>
                    <Icon name={SITUATION_ICON[s]} size={14} />
                    {t(label)}
                    <span className="lm-check" aria-hidden>
                      {situations.has(s) && <Icon name="check" size={12} />}
                    </span>
                  </button>
                ))}
              </div>
            </CtlGroup>
          </>
        )}
        {mode === "opening" && (
          <CtlGroup label={t("Rollout")}>
            <Seg
              label={t("Rollout")}
              value={String(rollout)}
              onChange={(v) => setRollout(Number(v) as Window)}
              options={WINDOWS.map((w) => ({ v: String(w), label: tx("First {0} s", { "0": w }) }))}
            />
            <button className={toKill ? "lm-toggle on" : "lm-toggle"} aria-pressed={toKill} onClick={() => setToKill(!toKill)} title={t("Stop each round at its first kill, whoever got it")}>
              <Icon name="skull" size={14} />
              {t("Until the first kill")}
              <span className="lm-check" aria-hidden>
                {toKill && <Icon name="check" size={12} />}
              </span>
            </button>
          </CtlGroup>
        )}
        {classes.length > 0 && (
          <CtlGroup label={vs !== null ? name : t("Class")}>
            <div className="class-filter" role="tablist" aria-label={t("Class")}>
              <button role="tab" aria-selected={cls === null} className={cls === null ? "cf active" : "cf"} onClick={() => setCls(null)}>
                {t("All")}
              </button>
              {classes.map(([c, n]) => (
                <button key={c} role="tab" aria-selected={cls === c} className={cls === c ? "cf active" : "cf"} onClick={() => setCls(cls === c ? null : c)} title={`${capitalize(c)}: ${n}`}>
                  <ClassIcon cls={c} size={20} />
                  <span className="cf-n">{n}</span>
                </button>
              ))}
            </div>
          </CtlGroup>
        )}
        {mode === "opening" && (
          <CtlGroup label={t("Compare with")}>
            <select
              className="lm-select"
              value={vs ?? ""}
              onChange={(e) => pickVs(e.target.value === "" ? null : e.target.value === "opp" ? "opp" : Number(e.target.value))}
              title={t("Someone else a demo on this machine followed on this map")}
            >
              <option value="">{t("Nobody: RED and BLU")}</option>
              <option value="opp">{cls ? t("Their opposite {0}: the other team's, same rounds", { "0": capitalize(cls) }) : t("Their opposite number: the other team's same class, same rounds")}</option>
              {(playersQ.data ?? []).map((p) => (
                <option key={p.accountId} value={p.accountId}>
                  {playerOption(p)}
                </option>
              ))}
            </select>
          </CtlGroup>
        )}
        {mode === "opening" && vs !== null && !opp && vsClasses.length > 0 && (
          <CtlGroup label={vsPlayer?.name ?? t("Class")}>
            <div className="class-filter" role="tablist" aria-label={t("Class")}>
              <button role="tab" aria-selected={vsCls === null} className={vsCls === null ? "cf active" : "cf"} onClick={() => setVsCls(null)}>
                {t("All")}
              </button>
              {vsClasses.map(([c, n]) => (
                <button key={c} role="tab" aria-selected={vsCls === c} className={vsCls === c ? "cf active" : "cf"} onClick={() => setVsCls(vsCls === c ? null : c)} title={`${capitalize(c)}: ${n}`}>
                  <ClassIcon cls={c} size={20} />
                  <span className="cf-n">{n}</span>
                </button>
              ))}
            </div>
          </CtlGroup>
        )}
      </div>
      {mode === "opening" && vs !== null && !opp && side === "both" && (
        <p className="hint lm-count">{t("Pick a side to compare like for like: RED's rollouts and BLU's go different ways.")}</p>
      )}
      {mode === "opening" && vs !== null && vsQ.isPending && <Loading>{t("Reading the routes…")}</Loading>}
      {mode === "opening" && opp && vsQ.data && vsOpenings.length === 0 && (
        <p className="hint lm-count">{t("No opposite number in these rounds: the demos here did not follow anyone on that class on the other team.")}</p>
      )}

      {v && (
        <p className="hint lm-count">
          {tx("{0} lives drawn, from {1} match{2} on {3}: {4} with a SourceTV demo, {5} from their own recordings. Matches without either have no routes.", {
            "0": shown.length,
            "1": v.matches,
            "2": v.matches === 1 ? "" : "es",
            "3": capitalize(v.map),
            "4": v.stvMatches,
            "5": v.matches - v.stvMatches,
          })}
          {lastN !== null && <> {tx("Only the last {0} of them with this class and side.", { "0": recent.matches })}</>}
        </p>
      )}
      {livesQ.isPending && <Loading>{t("Reading the routes…")}</Loading>}
      {livesQ.isError && <ApiError error={livesQ.error} />}

      {v && (
        <Drawing
          lives={mode === "opening" ? groups.flatMap((g) => g.lives) : shown}
          groups={groups}
          spawnZones={spawnZones}
          window={rollout}
          toKill={toKill}
          overview={overviewQ.data ?? null}
          hover={hover}
          onHover={setHover}
          mode={mode}
        />
      )}

      {hover && mode === "routes" && (
        <p className="lm-hover">
          <span className={`team-${(hover.team ?? "").toLowerCase()}`}>{hover.team === "Red" ? "RED" : hover.team === "Blue" ? "BLU" : "?"}</span>{" "}
          {hover.class ? capitalize(hover.class) : t("unknown class")} · {formatDate(hover.playedAt)}
          {hover.roundNum !== null && <> · {tx("round {0}", { "0": hover.roundNum })}</>} · {tx("{0} s", { "0": Math.round(hover.seconds) })}
          {hover.died ? <> · {t("died")}</> : null} · {hover.stv ? t("STV") : t("POV")}
        </p>
      )}
    </div>
  );
}

/**
 * One match's round openings (Flashy: "i want to see this per match on the
 * match page too"): RED's and BLU's players of one class, their first
 * seconds of each round, from the match's demos. The class follows the
 * player picked above, and the rounds the round picker.
 */
export function MatchOpenings({
  logId,
  mapName,
  rounds,
  roundCount,
  playerClass,
  names,
  teams,
}: {
  logId: number;
  /** The full map name, for its picture; null across several maps. */
  mapName: string | null;
  rounds: Set<number> | null;
  /** How many rounds are shown, for "found in 4 of 9". */
  roundCount: number;
  playerClass: string | null;
  names: Map<number, string>;
  /** Each player's team in the match: a stopwatch map swaps sides at half
   *  time, and a team's player is still theirs on the other side. */
  teams: Map<number, "Red" | "Blue">;
}) {
  const q = useQuery({ queryKey: ["match_openings", logId], queryFn: () => api.matchOpenings(logId), staleTime: 5 * 60_000 });
  const spawnZones = useSpawnZones(mapName);
  const overviewQ = useQuery({
    queryKey: ["overview", mapName],
    queryFn: () => api.getMapOverview(mapName ?? ""),
    enabled: mapName !== null,
    staleTime: Infinity,
  });
  const [picked, setPicked] = useState<string | null>(null);
  const [rollout, setRollout] = useState<Window>(20);
  const [toKill, setToKill] = useState(true);
  const all = useMemo(() => (q.data ?? []).filter((l) => rounds === null || (l.openingRound !== null && rounds.has(l.openingRound))), [q.data, rounds]);
  // Each class with how many played it (Flashy: "the game has 2 snipers,
  // not 4"): players, not lives.
  const classes = useMemo(() => {
    const n = new Map<string, Set<number>>();
    for (const l of all) if (l.class) n.set(l.class, (n.get(l.class) ?? new Set()).add(l.accountId));
    return [...n.entries()].map(([c, who]) => [c, who.size] as [string, number]).sort((a, b) => CLASS_ORDER.indexOf(a[0]) - CLASS_ORDER.indexOf(b[0]));
  }, [all]);
  const cls = picked ?? (playerClass && classes.some(([c]) => c === playerClass) ? playerClass : (classes[0]?.[0] ?? null));
  // One card a team, by the team each player is on in the match, whichever
  // side they had that round.
  const groups: Group[] = useMemo(() => {
    const side = (team: "Red" | "Blue"): Group => {
      const lives = all.filter((l) => (teams.get(l.accountId) ?? l.team) === team && l.class === cls).flatMap((l) => cut(l, rollout, toKill) ?? []);
      const who = [...new Set(lives.map((l) => names.get(l.accountId) ?? "?"))];
      return { key: team, label: `${team === "Red" ? "RED" : "BLU"}${who.length > 0 ? ` · ${who.join(", ")}` : ""}`, cls, colour: team === "Red" ? "--red" : "--blu", lives, of: roundCount };
    };
    return [side("Red"), side("Blue")];
  }, [all, cls, rollout, toKill, names, teams, roundCount]);
  const [hover, setHover] = useState<LifeOnMap | null>(null);

  if (q.isPending) return <Loading>{t("Reading the routes…")}</Loading>;
  if (q.isError) return <ApiError error={q.error} />;
  if ((q.data ?? []).length === 0) {
    return (
      <div className="lm-empty">
        <p className="hint">{t("No demo of this match has routes yet. Round openings come from its SourceTV demo, or from a player's own recording.")}</p>
        <ReadAgain logId={logId} />
      </div>
    );
  }
  if (mapName === null) return <p className="hint">{t("Pick one map above: the openings of different maps do not share a picture.")}</p>;
  return (
    <div className="lm">
      <div className="ctl-bar">
        <CtlGroup label={t("Class")}>
          <div className="class-filter" role="tablist" aria-label={t("Class")}>
            {classes.map(([c, n]) => (
              <button key={c} role="tab" aria-selected={cls === c} className={cls === c ? "cf active" : "cf"} onClick={() => setPicked(c)} title={t("{0}: {1} played it", { "0": capitalize(c), "1": n })}>
                <ClassIcon cls={c} size={20} />
                <span className="cf-n">{n}</span>
              </button>
            ))}
          </div>
        </CtlGroup>
        <CtlGroup label={t("Rollout")}>
          <Seg
            label={t("Rollout")}
            value={String(rollout)}
            onChange={(v) => setRollout(Number(v) as Window)}
            options={WINDOWS.map((w) => ({ v: String(w), label: tx("First {0} s", { "0": w }) }))}
          />
          <button className={toKill ? "lm-toggle on" : "lm-toggle"} aria-pressed={toKill} onClick={() => setToKill(!toKill)} title={t("Stop each round at its first kill, whoever got it")}>
            <Icon name="skull" size={14} />
            {t("Until the first kill")}
            <span className="lm-check" aria-hidden>
              {toKill && <Icon name="check" size={12} />}
            </span>
          </button>
        </CtlGroup>
        {/* The routes come from reading the demos: after an update that reads
            them better, this match catches up here. */}
        <span className="lm-head-actions">
          <ReadAgain logId={logId} />
        </span>
      </div>
      <StvPartsNotice logId={logId} label={`${mapName ?? "this match"}, log ${logId}`} />
      <Drawing
        lives={groups.flatMap((g) => g.lives)}
        groups={groups}
        spawnZones={spawnZones}
        window={rollout}
        toKill={toKill}
        overview={overviewQ.data ?? null}
        hover={hover}
        onHover={setHover}
        mode="opening"
      />
    </div>
  );
}

/** Classes in the scoreboard's order. */
const CLASS_ORDER = ["scout", "soldier", "pyro", "demoman", "heavyweapons", "heavy", "engineer", "medic", "sniper", "spy"];

/** The routes over the map's top-down picture, or over their own extent. */
function Drawing({
  lives,
  groups,
  spawnZones,
  window: seconds,
  toKill,
  overview,
  hover,
  onHover,
  mode,
}: {
  lives: LifeOnMap[];
  groups: Group[];
  /** The map's spawn rooms from its callouts, in game units. */
  spawnZones: Array<Array<[number, number]>>;
  window: number;
  toKill: boolean;
  overview: { minX: number; maxY: number; size: number; aspect: number; image: string } | null;
  hover: LifeOnMap | null;
  onHover: (l: LifeOnMap | null) => void;
  mode: Mode;
}) {
  // Q65: the cell under the mouse in the density view, and its counts.
  const [cellHover, setCellHover] = useState<{ red: number; blue: number; other: number } | null>(null);
  // The round-openings view's hex under the mouse.
  const [hexHover, setHexHover] = useState<number[] | null>(null);
  // The frame in game units: the picture's, else the routes' own bounds.
  const frame = useMemo(() => {
    if (overview) return { minX: overview.minX, maxY: overview.maxY, w: overview.size, h: overview.size * overview.aspect };
    let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
    for (const l of lives)
      for (const [x, y] of l.points) {
        x0 = Math.min(x0, x);
        x1 = Math.max(x1, x);
        y0 = Math.min(y0, y);
        y1 = Math.max(y1, y);
      }
    if (!Number.isFinite(x0)) return null;
    const pad = 200;
    const size = Math.max(x1 - x0, y1 - y0) + pad * 2;
    return { minX: x0 - pad, maxY: y1 + pad, w: size, h: size };
  }, [lives, overview]);

  const box = useRef<HTMLDivElement>(null);
  const base = useRef<HTMLCanvasElement>(null);
  const top = useRef<HTMLCanvasElement>(null);
  // The stage's width in CSS pixels; the height follows the frame's aspect.
  const [width, setWidth] = useState(0);
  const framed = frame !== null;
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    // Measured at once, not only when the observer first reports: a window
    // that is not painting (minimised, or behind another) delays that.
    setWidth(el.clientWidth);
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, [framed]);
  const height = frame && width > 0 ? Math.min((width * frame.h) / frame.w, window.innerHeight * 0.78) : 0;
  // Fainter the more there are, so a hundred routes still read as a pattern.
  const opacity = Math.max(0.12, Math.min(0.7, 9 / Math.sqrt(lives.length + 1)));

  // Game units to CSS pixels at the current size, the frame centred.
  const proj = useMemo(() => {
    if (!frame || width === 0 || height === 0) return null;
    const scale = Math.min(width / frame.w, height / frame.h);
    const ox = (width - frame.w * scale) / 2;
    const oy = (height - frame.h * scale) / 2;
    return { scale, ox, oy, px: (x: number) => ox + (x - frame.minX) * scale, py: (y: number) => oy + (frame.maxY - y) * scale };
  }, [frame, width, height]);

  // Every point by grid cell, for hover: a cell is 1% of the frame.
  const grid = useMemo(() => {
    if (!frame) return null;
    const cell = Math.max(frame.w, frame.h) / 100;
    const cells = new Map<string, Array<[number, number, number]>>();
    lives.forEach((l, i) => {
      for (const [x, y] of l.points) {
        const k = `${Math.floor(x / cell)},${Math.floor(y / cell)}`;
        let c = cells.get(k);
        if (!c) cells.set(k, (c = []));
        c.push([x, y, i]);
      }
    });
    return { cell, cells };
  }, [lives, frame]);

  // Q65: how many lives passed through each cell, per side. Each life counts
  // once per cell, so a camper does not outweigh a hundred passers-by.
  const density = useMemo(() => (frame && mode === "density" ? densityOf(lives, frame) : null), [lives, frame, mode]);

  // The round openings, in hexes.
  const hexes = useMemo(() => (frame && mode === "opening" ? hexesOf(groups.map((g) => g.lives), frame, spawnZones) : null), [groups, frame, mode, spawnZones]);

  // Each hex its group's colour (a side's, or a player's), stronger the more
  // rounds went through it; where both went, outlined in white. Each one's
  // main position is ringed.
  useEffect(() => {
    if (mode !== "opening") return;
    const ctx = prepare(base.current, width, height);
    if (!ctx || !proj || !hexes || !frame) return;
    const cs = getComputedStyle(document.documentElement);
    const col = groups.map((g) => cs.getPropertyValue(g.colour).trim() || "#999");
    const rgb = col.map((c) => toRgb(ctx, c));
    const r = hexes.size * proj.scale;
    const at = (q: number, rr: number): [number, number] => {
      const [x, y] = hexCentre(q, rr, hexes.size);
      return [proj.ox + x * proj.scale, proj.oy + y * proj.scale];
    };
    const hexPath = (cx: number, cy: number, radius: number) => {
      ctx.beginPath();
      for (let i = 0; i < 6; i++) {
        const a = (Math.PI / 3) * i + Math.PI / 6;
        const x = cx + radius * Math.cos(a);
        const y = cy + radius * Math.sin(a);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.closePath();
    };
    for (const [key, n] of hexes.cells) {
      if (hexes.start.has(key)) continue;
      const [q, rr] = key.split(",").map(Number);
      const [cx, cy] = at(q, rr);
      // The hex goes to whichever spent longer there, each against its own
      // longest; its colour runs from deep through the group's own to near
      // white the longer it was (Flashy: "need more gradients"), as PGL's
      // "less time, more time". Only a spot both use often is outlined, so a
      // shared lane does not turn the whole map white.
      const share = n.map((count, g) => count / hexes.max[g]);
      const heat = hexes.time.get(key) ?? groups.map(() => 0);
      const warm = heat.map((s, g) => Math.sqrt(s / hexes.maxTime[g]));
      const top = warm[0] > warm[1] || (warm[0] === warm[1] && share[0] >= share[1]) ? 0 : 1;
      const v = Math.max(0.06, warm[top]);
      ctx.globalAlpha = 0.5 + 0.5 * v;
      ctx.fillStyle = ramp(rgb[top], v);
      if (v > 0.6) {
        ctx.shadowColor = col[top];
        ctx.shadowBlur = 10 * v;
      }
      hexPath(cx, cy, r * 0.9);
      ctx.fill();
      ctx.shadowBlur = 0;
      if (Math.min(share[0], share[1]) >= OVERLAP) {
        ctx.globalAlpha = 0.85;
        ctx.strokeStyle = "#fff";
        ctx.lineWidth = 1.2;
        hexPath(cx, cy, r * 0.82);
        ctx.stroke();
      }
    }
    ctx.globalAlpha = 1;
    hexes.main.forEach((main, g) => {
      if (!main) return;
      const [cx, cy] = at(main.q, main.r);
      ctx.lineWidth = 2.5;
      ctx.strokeStyle = "#fff";
      hexPath(cx, cy, r * 1.15);
      ctx.stroke();
      ctx.strokeStyle = col[g];
      ctx.lineWidth = 1.5;
      hexPath(cx, cy, r * 1.45);
      ctx.stroke();
    });
  }, [mode, hexes, groups, proj, frame, width, height]);

  // The density view: each cell its side's colour, stronger the more lives
  // passed through it. Square root, so the quieter lanes still show.
  useEffect(() => {
    if (mode !== "density") return;
    const ctx = prepare(base.current, width, height);
    if (!ctx || !proj || !density || !frame) return;
    const col = colours();
    const size = density.cell * proj.scale;
    for (const [team, counts] of [["Red", density.red], ["Blue", density.blue], ["none", density.other]] as const) {
      const max = counts.reduce((m, n) => Math.max(m, n), 0);
      if (max === 0) continue;
      ctx.fillStyle = col[team];
      for (let i = 0; i < counts.length; i++) {
        const n = counts[i];
        if (n === 0) continue;
        const cx = i % density.cols;
        const cy = Math.floor(i / density.cols);
        ctx.globalAlpha = 0.12 + 0.83 * Math.sqrt(n / max);
        ctx.fillRect(proj.ox + cx * size, proj.oy + cy * size, size + 0.5, size + 0.5);
      }
    }
  }, [mode, density, proj, frame, width, height]);

  // All the routes. Each life is stroked on its own, so where many overlap
  // the line builds up and the usual lanes stand out (one path per side
  // would paint every overlap once, flat).
  useEffect(() => {
    if (mode !== "routes") return;
    const ctx = prepare(base.current, width, height);
    if (!ctx || !proj) return;
    const col = colours();
    ctx.lineWidth = 1.6;
    for (const team of ["Red", "Blue", "none"] as const) {
      const mine = lives.filter((l) => (l.team ?? "none") === team);
      if (mine.length === 0) continue;
      ctx.globalAlpha = opacity;
      ctx.strokeStyle = col[team];
      for (const l of mine) {
        ctx.beginPath();
        trace(ctx, l, proj);
        ctx.stroke();
      }
      // Where a life ended in a death.
      ctx.globalAlpha = Math.min(1, opacity * 1.6);
      ctx.fillStyle = col[team];
      ctx.beginPath();
      for (const l of mine) {
        const last = l.points[l.points.length - 1];
        if (!l.died || !last) continue;
        ctx.moveTo(proj.px(last[0]) + 3, proj.py(last[1]));
        ctx.arc(proj.px(last[0]), proj.py(last[1]), 3, 0, Math.PI * 2);
      }
      ctx.fill();
    }
  }, [mode, lives, proj, opacity, width, height]);

  // The hovered route on the canvas above, at full strength.
  useEffect(() => {
    const ctx = prepare(top.current, width, height);
    if (!ctx || !proj || !hover || mode !== "routes") return;
    const col = colours()[hover.team ?? "none"];
    ctx.strokeStyle = col;
    ctx.fillStyle = col;
    ctx.lineWidth = 3.5;
    ctx.beginPath();
    trace(ctx, hover, proj);
    ctx.stroke();
    const last = hover.points[hover.points.length - 1];
    if (hover.died && last) {
      ctx.beginPath();
      ctx.arc(proj.px(last[0]), proj.py(last[1]), 6, 0, Math.PI * 2);
      ctx.fill();
    }
  }, [hover, proj, width, height, mode]);

  if (!frame) return <p className="hint">{t("Nothing to draw with these filters.")}</p>;

  // The nearest point under the mouse, within HOVER_PX, picks the route; in
  // the density view, the cell under it says how many lives went through.
  function onMove(e: React.MouseEvent<HTMLCanvasElement>) {
    if (!proj || !grid || !frame) return;
    const r = e.currentTarget.getBoundingClientRect();
    const gx = frame.minX + (e.clientX - r.left - proj.ox) / proj.scale;
    const gy = frame.maxY - (e.clientY - r.top - proj.oy) / proj.scale;
    if (mode === "opening") {
      if (!hexes) return;
      const [q, r] = hexAt(gx - frame.minX, frame.maxY - gy, hexes.size);
      setHexHover(hexes.cells.get(`${q},${r}`) ?? null);
      return;
    }
    if (mode === "density") {
      if (!density) return;
      const cx = Math.floor((gx - frame.minX) / density.cell);
      const cy = Math.floor((frame.maxY - gy) / density.cell);
      const i = cy * density.cols + cx;
      const inside = cx >= 0 && cy >= 0 && cx < density.cols && cy < density.rows;
      const at = inside ? { red: density.red[i], blue: density.blue[i], other: density.other[i] } : null;
      setCellHover(at && at.red + at.blue + at.other > 0 ? at : null);
      return;
    }
    const cx = Math.floor(gx / grid.cell);
    const cy = Math.floor(gy / grid.cell);
    const reach = HOVER_PX / proj.scale;
    let best: [number, number] | null = null; // [distance, life]
    for (let dx = -1; dx <= 1; dx++)
      for (let dy = -1; dy <= 1; dy++)
        for (const [x, y, i] of grid.cells.get(`${cx + dx},${cy + dy}`) ?? []) {
          const d = Math.hypot(x - gx, y - gy);
          if (d <= reach && (best === null || d < best[0])) best = [d, i];
        }
    const l = best ? lives[best[1]] : null;
    if (l !== hover) onHover(l);
  }

  return (
    <>
      <div className="lm-stage" ref={box} style={{ height: height || undefined }}>
        {overview && proj && (
          <img
            className="lm-picture"
            src={overview.image}
            alt=""
            style={{ left: proj.ox, top: proj.oy, width: frame.w * proj.scale, height: frame.h * proj.scale }}
          />
        )}
        <canvas ref={base} className="lm-canvas" style={{ width, height }} aria-hidden />
        <canvas
          ref={top}
          className="lm-canvas lm-canvas-top"
          style={{ width, height }}
          role="img"
          aria-label={t("Routes on the map")}
          onMouseMove={onMove}
          onMouseLeave={() => {
            onHover(null);
            setCellHover(null);
            setHexHover(null);
          }}
        />
        {mode === "density" && cellHover && density && (
          <p className="lm-cell">
            {tx("{0} of the {1} lives passed here ({2}%)", {
              "0": cellHover.red + cellHover.blue + cellHover.other,
              "1": lives.length,
              "2": Math.round(((cellHover.red + cellHover.blue + cellHover.other) / Math.max(1, lives.length)) * 100),
            })}
            {cellHover.red > 0 && cellHover.blue > 0 && <> · <span className="team-red">RED {cellHover.red}</span> · <span className="team-blue">BLU {cellHover.blue}</span></>}
          </p>
        )}
        {mode === "opening" && hexHover && hexes && (
          <p className="lm-cell">
            {groups.map((g, i) =>
              hexHover[i] > 0 ? (
                <span key={g.key} className="lm-cell-group" style={{ color: `var(${g.colour})` }}>
                  {tx("{0}: {1} of {2} rounds", { "0": g.label, "1": hexHover[i], "2": hexes.rounds[i] })}
                </span>
              ) : null,
            )}
          </p>
        )}
      </div>
      {mode === "opening" && hexes && <OpeningCards hexes={hexes} groups={groups} window={seconds} toKill={toKill} />}
    </>
  );
}

/** Under the round-openings view: each side's (or player's) rounds and main
 *  position, the scale, and what the view counts (as PGL's heatmaps put it). */
function OpeningCards({ hexes, groups, window, toKill }: { hexes: Hexes; groups: Group[]; window: number; toKill: boolean }) {
  const card = (g: number) => {
    const group = groups[g];
    const rounds = hexes.rounds[g];
    // Rounds, not lives: two of a class in one round are one round.
    const played = new Set(group.lives.map((l) => `${l.logId}:${l.openingRound ?? l.roundNum}`)).size;
    const main = hexes.main[g];
    const pct = main && rounds > 0 ? Math.round((main.rounds / rounds) * 100) : null;
    return (
      <div className="lm-op-card" style={{ "--op": `var(${group.colour})` } as React.CSSProperties}>
        <div className="lm-op-head">
          <span className="lm-op-side">
            {group.cls && <ClassIcon cls={group.cls} size={16} />}
            {group.label}
          </span>
          <span className="lm-op-rounds">
            {group.of ? (
              tx("{0} of {1} rounds", { "0": <strong>{played}</strong>, "1": group.of })
            ) : (
              <>
                <strong>{rounds}</strong> {t("rounds")}
              </>
            )}
          </span>
        </div>
        <div className="lm-op-main">
          <span>{t("Main position")}</span>
          <span className="lm-op-pct">
            <span>{pct === null ? "–" : <><strong>{pct}</strong>%</>}</span>
            <small>{t("of rounds")}</small>
          </span>
        </div>
      </div>
    );
  };
  return (
    <div className="lm-op">
      {card(0)}
      <div className="lm-op-mid">
        <div className="lm-op-scale">
          <span>{t("Less time")}</span>
          {[0.15, 0.35, 0.55, 0.75, 1].map((v) => (
            <i key={v} style={{ background: `color-mix(in srgb, var(${groups[0].colour}) ${Math.round(100 - Math.max(0, v - 0.5) * 120)}%, ${v < 0.5 ? "#000" : "#fff"})`, opacity: 0.5 + 0.5 * v }} />
          ))}
          <span>{t("More time")}</span>
        </div>
        <div className="lm-op-scale">
          <i className="both" style={{ background: `linear-gradient(90deg, var(${groups[0].colour}) 50%, var(${groups[1].colour}) 50%)` }} />
          <span>{groups[0].key === "Red" ? t("Both sides") : t("Both players")}</span>
        </div>
        <p className="lm-op-note">
          {toKill
            ? tx("First {0} seconds of each round, or until the round's first kill", { "0": window })
            : tx("First {0} seconds of each round", { "0": window })}
        </p>
      </div>
      {card(1)}
    </div>
  );
}

type Mode = "routes" | "density" | "opening";

/** The hex a point falls in (pointy-top, `size` the corner radius), from
 *  the frame's top-left in game units, y down. */
function hexAt(x: number, y: number, size: number): [number, number] {
  const fq = ((Math.sqrt(3) / 3) * x - y / 3) / size;
  const fr = ((2 / 3) * y) / size;
  const fs = -fq - fr;
  let q = Math.round(fq);
  let r = Math.round(fr);
  const s = Math.round(fs);
  const [dq, dr, ds] = [Math.abs(q - fq), Math.abs(r - fr), Math.abs(s - fs)];
  if (dq > dr && dq > ds) q = -r - s;
  else if (dr > ds) r = -q - s;
  return [q, r];
}

/** A hex's centre, from the frame's top-left in game units, y down. */
function hexCentre(q: number, r: number, size: number): [number, number] {
  return [size * Math.sqrt(3) * (q + r / 2), size * 1.5 * r];
}

/** A colour as `[r, g, b]`, whatever form the theme gives it in. */
function toRgb(ctx: CanvasRenderingContext2D, colour: string): [number, number, number] {
  ctx.fillStyle = "#000";
  ctx.fillStyle = colour;
  const c = String(ctx.fillStyle);
  if (c.startsWith("#")) return [parseInt(c.slice(1, 3), 16), parseInt(c.slice(3, 5), 16), parseInt(c.slice(5, 7), 16)];
  const m = c.match(/\d+(\.\d+)?/g)?.map(Number) ?? [153, 153, 153];
  return [m[0], m[1], m[2]];
}

/** The heat ramp from 0 to 1: deep (the colour at half its brightness),
 *  the colour itself at the middle, near white at the top. */
function ramp([r, g, b]: [number, number, number], v: number): string {
  const mix = (a: number, z: number, f: number) => Math.round(a + (z - a) * f);
  if (v < 0.5) {
    const f = 0.45 + v * 1.1;
    return `rgb(${mix(0, r, f)}, ${mix(0, g, f)}, ${mix(0, b, f)})`;
  }
  const f = (v - 0.5) * 1.3;
  return `rgb(${mix(r, 255, f)}, ${mix(g, 255, f)}, ${mix(b, 255, f)})`;
}

/** The six neighbours' offsets. */
const AROUND: Array<[number, number]> = [[1, 0], [-1, 0], [0, 1], [0, -1], [1, -1], [-1, 1]];

type Hexes = ReturnType<typeof hexesOf>;

/** Rounds through each hex, per group (a side, or a player), each counted
 *  once per hex it crossed; and each group's main position: the hex whose
 *  neighbourhood (it and the six around it) the most rounds went through, so
 *  a spot rather than one tile of it. Not the spawn room, which every round
 *  starts in. */
function hexesOf(groups: LifeOnMap[][], frame: { minX: number; maxY: number; w: number; h: number }, spawnZones: Array<Array<[number, number]>> = []) {
  const size = Math.max(frame.w, frame.h) / HEXES / Math.sqrt(3);
  const cells = new Map<string, number[]>();
  const near = groups.map(() => new Map<string, number>());
  // Where each round began, so the spawn room is not taken for a position.
  const spawn = groups.map(() => new Set<string>());
  const rounds = groups.map((g) => g.length);
  // Time spent in each hex, a point being about a quarter of a second.
  const time = new Map<string, number[]>();
  // The same past the first seconds out of spawn, for the main position
  // (Flashy: "exclude the first few seconds out of spawn"): every rollout
  // passes the spawn door, which is no one's position.
  const timeOut = new Map<string, number[]>();
  const seen = new Set<string>();
  const seenOut = new Set<string>();
  const around = new Set<string>();
  groups.forEach((lives, g) => lives.forEach((l) => {
    seen.clear();
    seenOut.clear();
    around.clear();
    let prev: [number, number] | null = null;
    const begin = beginOf(l);
    l.points.forEach((p, j) => {
      // Out of spawn: past its first seconds, and well away from where the
      // route began (a koth round starts in the pre-round freeze, so at 8 s
      // everyone still stood at the door: both rings sat there).
      const out = (l.openingT[j] ?? Infinity) >= OUT_OF_SPAWN_S && (!begin || Math.hypot(p[0] - begin[0], p[1] - begin[1]) >= SPAWN_RADIUS);
      const [pq, pr] = hexAt(p[0] - frame.minX, frame.maxY - p[1], size);
      const tk = `${pq},${pr}`;
      const tc = time.get(tk) ?? groups.map(() => 0);
      tc[g] += 1;
      time.set(tk, tc);
      if (out) {
        const to = timeOut.get(tk) ?? groups.map(() => 0);
        to[g] += 1;
        timeOut.set(tk, to);
      }
      const from: [number, number] = prev !== null && Math.hypot(p[0] - prev[0], p[1] - prev[1]) <= JUMP ? prev : p;
      const steps = Math.max(1, Math.ceil(Math.hypot(p[0] - from[0], p[1] - from[1]) / (size / 2)));
      for (let s = 0; s <= steps; s++) {
        const x = from[0] + ((p[0] - from[0]) * s) / steps;
        const y = from[1] + ((p[1] - from[1]) * s) / steps;
        const [q, r] = hexAt(x - frame.minX, frame.maxY - y, size);
        seen.add(`${q},${r}`);
        if (out) seenOut.add(`${q},${r}`);
      }
      prev = p;
    });
    if (begin) {
      const [q, r] = hexAt(begin[0] - frame.minX, frame.maxY - begin[1], size);
      for (let dq = -2; dq <= 2; dq++) for (let dr = -2; dr <= 2; dr++) spawn[g].add(`${q + dq},${r + dr}`);
    }
    for (const key of seen) {
      const c = cells.get(key) ?? groups.map(() => 0);
      c[g] += 1;
      cells.set(key, c);
    }
    for (const key of seenOut) {
      const [q, r] = key.split(",").map(Number);
      around.add(key);
      for (const [dq, dr] of AROUND) around.add(`${q + dq},${r + dr}`);
    }
    for (const key of around) near[g].set(key, (near[g].get(key) ?? 0) + 1);
  }));
  // The time spent around a hex (it and its six), to choose between spots
  // as many rounds went through: where they stood, not where they ran.
  const stay = (key: string, g: number) => {
    const [q, r] = key.split(",").map(Number);
    let s = timeOut.get(key)?.[g] ?? 0;
    for (const [dq, dr] of AROUND) s += timeOut.get(`${q + dq},${r + dr}`)?.[g] ?? 0;
    return s;
  };
  // The tiles where the routes begin (Flashy: "remove the first 3/5 tiles
  // from spawn since they dont do anything"): every round starts there, so
  // they were the hottest on the map and dulled the rest. Not drawn, and not
  // the scale either.
  // Every tile within SPAWN_HIDE of where any route began, after the
  // respawn (Flashy: "remove a bit more, it shows me being in spawn the
  // most"): a spawn room and its doors, whatever its shape.
  const begins = new Map<string, [number, number]>();
  for (const lives of groups)
    for (const l of lives) {
      const b = beginOf(l);
      if (!b) continue;
      const [q, r] = hexAt(b[0] - frame.minX, frame.maxY - b[1], size);
      begins.set(`${q},${r}`, [b[0] - frame.minX, frame.maxY - b[1]]);
    }
  const start = new Set<string>();
  // And the map's own spawn rooms, from its callouts (Flashy: "a preset
  // hammer units area ... where it doesnt track those start positions"):
  // every zone named a spawn that a route begins in. A payload map's forward
  // spawns hold no round's start, so the rollout through them still shows.
  const used = spawnZones.filter((z) => [...begins.values()].some(([bx, by]) => inPolygon([bx + frame.minX, frame.maxY - by], z)));
  if (used.length > 0)
    for (const key of new Set([...cells.keys(), ...time.keys()])) {
      const [q, r] = key.split(",").map(Number);
      const [cx, cy] = hexCentre(q, r, size);
      if (used.some((z) => inPolygon([cx + frame.minX, frame.maxY - cy], z))) start.add(key);
    }
  const reach = Math.ceil(SPAWN_HIDE / (size * 1.5)) + 1;
  for (const [key, [bx, by]] of begins) {
    const [q, r] = key.split(",").map(Number);
    for (let dq = -reach; dq <= reach; dq++)
      for (let dr = -reach; dr <= reach; dr++) {
        const [cx, cy] = hexCentre(q + dq, r + dr, size);
        if (Math.hypot(cx - bx, cy - by) <= SPAWN_HIDE) start.add(`${q + dq},${r + dr}`);
      }
  }
  const main = groups.map((_, g) => {
    let best: { q: number; r: number; rounds: number; stay: number } | null = null;
    for (const [key, n] of near[g]) {
      if (spawn[g].has(key) || start.has(key) || !cells.get(key)?.[g]) continue;
      if (best && n < best.rounds) continue;
      const s = stay(key, g);
      if (!best || n > best.rounds || s > best.stay) {
        const [q, r] = key.split(",").map(Number);
        best = { q, r, rounds: n, stay: s };
      }
    }
    return best;
  });
  const kept = (m: Map<string, number[]>) => [...m.entries()].filter(([k]) => !start.has(k)).map(([, c]) => c);
  const maxTime = groups.map((_, g) => Math.max(1, ...kept(time).map((c) => c[g])));
  const max = groups.map((_, g) => Math.max(1, ...kept(cells).map((c) => c[g])));
  return { size, cells, rounds, max, main, time, maxTime, start };
}

/** A player in the compare list: their name, usual class here, and matches. */
function playerOption(p: MapPlayer): string {
  const cls = p.classes[0]?.[0];
  const matches = t("{0} match{1}", { "0": p.matches, "1": p.matches === 1 ? "" : "es" });
  const shared = p.shared > 0 ? ` · ${t("{0} together", { "0": p.shared })}` : "";
  return `${p.name}${cls ? ` · ${capitalize(cls)}` : ""} · ${matches}${shared}`;
}

/** Lives through each cell of a `CELLS`-wide grid over the frame, per side:
 *  each life counted once per cell it crossed (Q65). Steps between points are
 *  filled in so a fast move does not skip cells; a jump (a teleporter, a
 *  respawn) is not. */
function densityOf(lives: LifeOnMap[], frame: { minX: number; maxY: number; w: number; h: number }) {
  const cell = Math.max(frame.w, frame.h) / CELLS;
  const cols = Math.ceil(frame.w / cell);
  const rows = Math.ceil(frame.h / cell);
  const red = new Uint32Array(cols * rows);
  const blue = new Uint32Array(cols * rows);
  const other = new Uint32Array(cols * rows);
  const seen = new Set<number>();
  for (const l of lives) {
    const into = l.team === "Red" ? red : l.team === "Blue" ? blue : other;
    seen.clear();
    let prev: [number, number] | null = null;
    for (const p of l.points) {
      const from: [number, number] = prev !== null && Math.hypot(p[0] - prev[0], p[1] - prev[1]) <= JUMP ? prev : p;
      const steps = Math.max(1, Math.ceil(Math.hypot(p[0] - from[0], p[1] - from[1]) / (cell / 2)));
      for (let s = 0; s <= steps; s++) {
        const x = from[0] + ((p[0] - from[0]) * s) / steps;
        const y = from[1] + ((p[1] - from[1]) * s) / steps;
        const cx = Math.floor((x - frame.minX) / cell);
        const cy = Math.floor((frame.maxY - y) / cell);
        if (cx < 0 || cy < 0 || cx >= cols || cy >= rows) continue;
        seen.add(cy * cols + cx);
      }
      prev = p;
    }
    for (const i of seen) into[i] += 1;
  }
  return { cell, cols, rows, red, blue, other };
}

/** Hover picks the nearest point within this many CSS pixels. */
const HOVER_PX = 12;

type Proj = { px: (x: number) => number; py: (y: number) => number };

/** One life's route into the current path, broken where it jumped. */
function trace(ctx: CanvasRenderingContext2D, l: LifeOnMap, p: Proj) {
  let prev: [number, number] | null = null;
  for (const pt of l.points) {
    const far = prev !== null && Math.hypot(pt[0] - prev[0], pt[1] - prev[1]) > JUMP;
    if (prev === null || far) ctx.moveTo(p.px(pt[0]), p.py(pt[1]));
    else ctx.lineTo(p.px(pt[0]), p.py(pt[1]));
    prev = pt;
  }
}

/** A canvas sized for the screen's pixel density, cleared. */
function prepare(c: HTMLCanvasElement | null, width: number, height: number): CanvasRenderingContext2D | null {
  if (!c || width === 0 || height === 0) return null;
  const dpr = window.devicePixelRatio || 1;
  c.width = Math.round(width * dpr);
  c.height = Math.round(height * dpr);
  const ctx = c.getContext("2d");
  if (!ctx) return null;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  return ctx;
}

/** The team colours from the theme, so light and dark both hold. */
function colours(): Record<"Red" | "Blue" | "none", string> {
  const cs = getComputedStyle(document.documentElement);
  const v = (n: string) => cs.getPropertyValue(n).trim() || "#999";
  return { Red: v("--red"), Blue: v("--blu"), none: v("--muted") };
}
