import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type Analysis, type KillView, type MapView, type Overview, type BuildingRow, type PathRow, type Vec3 } from "../../api/types";
import { capitalize, splitMap, teamLabel } from "../../lib/format";
import { DEATH, KILL, inSlice, jumpTo, playerMap, roundClock, themeColour, type Slice, classLabel } from "./common";
import { LifeList } from "./LifeList";
import { beginDownload, failDownload, useDownload } from "../../lib/downloads";
import { useMeasuredWidth } from "../../lib/measure";
import type { StvInfo } from "./AnalysisPanel";
import { t as tr, tx } from "../../lib/i18n";
import { MapPicker } from "./MapPicker";
import { drawIcon, useMapIcons, type IconName, type MapIcons } from "./mapIcons";
import { CtlGroup, Icon, LegendChip, MapMenu, Seg, Switch, ZoomControls } from "./MapChrome";
import { CalloutEditor, useCallouts, zoneAt, ZoneShapes, type HandleDrag } from "./Callouts";
import type { CalloutFile, CalloutZone } from "../../api/types";
import { SortTh, useSort } from "../../lib/sort";
import { tip } from "../../lib/glossary";

/**
 * Where the player's kills and deaths happened, top-down.
 *
 * The map is an overview image where one is saved locally (see
 * `overview.rs`), and otherwise drawn from data: every kill stored on this
 * map records where both players stood, and the density of those positions
 * traces the playable space (see `mapview.rs`). A kill is a blue dot where the victim
 * fell, a death an orange skull where the player fell; a hollow ring marks
 * where the shooter stood, joined by a line. Click any of them to copy the
 * demo tick.
 */

type Layer = "dots" | "paths" | "heat";
type HeatOf = "kills" | "deaths";
type Scope = "match" | "career";

/** The grid everything is drawn on, in game units. */
interface Frame {
  minX: number;
  maxY: number;
  cell: number;
  width: number;
  height: number;
}

interface Mark {
  k: KillView;
  kind: "kill" | "death";
  /** Where it happened (the victim) and where the shot came from. */
  at: [number, number];
  from: [number, number] | null;
}

const MAX_H = 640;
/** Room left for the controls and the note when the map fills the window. */
const FULL_CHROME = 150;

export function KillMap({ a, player, slice, stv }: { a: Analysis; player: number; slice: Slice; stv?: StvInfo }) {
  // Full screen: the window's own where the webview allows it, and otherwise
  // the map fills the app over everything else. Escape leaves either way.
  const box = useRef<HTMLDivElement>(null);
  const [full, setFull] = useState(false);
  useEffect(() => {
    const onChange = () => {
      if (!document.fullscreenElement) setFull(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // Escape drops a picked route first, and only then leaves full screen:
      // losing the whole view to un-pick one line is a surprise.
      setFocus((f) => {
        if (f) return null;
        if (!document.fullscreenElement) setFull(false);
        return null;
      });
    };
    document.addEventListener("fullscreenchange", onChange);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("fullscreenchange", onChange);
      document.removeEventListener("keydown", onKey);
    };
  }, []);
  const toggleFull = () => {
    if (full) {
      setFull(false);
      if (document.fullscreenElement) void document.exitFullscreen();
      return;
    }
    setFull(true);
    void box.current?.requestFullscreen().catch(() => {
      // The webview refused it; the in-app overlay covers the window instead.
    });
  };
  // The slice's map: in a combined log, the map of the chosen segment.
  const mapName = slice.map;
  const mapQ = useQuery({
    queryKey: ["mapview", mapName],
    queryFn: () => api.getMapView(mapName ?? ""),
    enabled: mapName !== null,
    staleTime: 5 * 60_000,
  });
  const view = mapQ.data ?? null;
  const overviewQ = useQuery({
    queryKey: ["overview", mapName],
    queryFn: () => api.getMapOverview(mapName ?? ""),
    enabled: mapName !== null,
    staleTime: Infinity,
  });
  const overview = overviewQ.data ?? null;
  // Q28: callouts, drawn as zones over the map, and the editor for them.
  const calloutQ = useCallouts(mapName);
  const qcCallouts = useQueryClient();
  const callouts = calloutQ.data ?? null;
  const [showZones, setShowZones] = useState(true);
  const [editing, setEditing] = useState(false);
  const [drawing, setDrawing] = useState<Array<[number, number]>>([]);
  const [zoneSel, setZoneSel] = useState<number | null>(null);
  const [drawMode, setDrawMode] = useState(false);
  const [dirty, setDirty] = useState(false);
  const zones = callouts && (showZones || editing) ? callouts.zones : [];
  // Every edit -- a dragged corner, a new zone, a rename -- goes to the
  // copy the map draws from, and waits there for Save.
  const liveEdit = (next: CalloutZone[], names?: string[]) => {
    qcCallouts.setQueryData<CalloutFile>(["callouts", mapName], (f) => (f ? { ...f, zones: next, names: names ?? f.names } : f));
    setDirty(true);
  };
  // How see-through the zones are, and whether they carry names: a busy
  // map reads better with faint zones, and a zoomed-in one with labels.
  const [zoneOpacity, setZoneOpacity] = useState(() => storedNumber("hl.km.zoneOpacity", 0.5));
  const [zoneLabels, setZoneLabels] = useState(true);
  // Zoom and pan, in canvas pixels. Back to the whole map on another map.
  const [zoom, setZoom] = useState<Zoom>(NO_ZOOM);
  useEffect(() => setZoom(NO_ZOOM), [mapName]);
  // The canvas's size, so the buttons zoom about the middle of the view.
  const [stage, setStage] = useState<[number, number]>([0, 0]);
  const zoomBy = (f: number) => setZoom((z) => clampZoom(zoomAt(z, z.z * f, [stage[0] / 2, stage[1] / 2]), stage[0], stage[1]));
  const players = useMemo(() => playerMap(a), [a]);
  const me = players.get(player);
  const enemies = a.players.filter((p) => me && p.team !== me.team);

  const [enemy, setEnemy] = useState<number | null>(null);
  const [showKills, setShowKills] = useState(true);
  const [showDeaths, setShowDeaths] = useState(true);
  const [layer, setLayer] = useState<Layer>("dots");
  // Routes are only fetched once the layer is asked for: most views never
  // want them, and a match is a few hundred kilobytes of points.
  const pathQ = useQuery({
    queryKey: ["paths", a.logId],
    queryFn: () => api.getPaths(a.logId),
    enabled: layer === "paths",
    staleTime: 5 * 60_000,
  });
  // The engineers' buildings, with the routes (Emiel).
  const buildingQ = useQuery({
    queryKey: ["buildings", a.logId],
    queryFn: () => api.getBuildings(a.logId),
    enabled: layer === "paths",
    staleTime: 5 * 60_000,
  });
  // Whose routes: one player's, or everyone the demo saw. It follows the
  // panel's player until it is set by hand.
  const [pathWho, setPathWho] = useState<number | "all" | null>(null);
  const [focus, setFocus] = useState<PathRow | null>(null);
  // The download itself is followed app-wide, so it survives leaving the
  // page; this panel only starts it and reads its state.
  const qc = useQueryClient();
  const download = useDownload(a.logId);
  const stvBusy = download?.state === "running";
  useEffect(() => {
    if (download?.state !== "done") return;
    void qc.invalidateQueries({ queryKey: ["paths", a.logId] });
    void qc.invalidateQueries({ queryKey: ["aim", a.logId] });
    void qc.invalidateQueries({ queryKey: ["match", a.logId] });
    void qc.invalidateQueries({ queryKey: ["analysis", a.logId] });
  }, [download?.state, a.logId, qc]);

  const fetchStv = async () => {
    beginDownload(a.logId, `${capitalize(splitMap(a.map).name ?? "this match")}, log ${a.logId}`);
    // A refused command must reach the card, or it sits there claiming to
    // be downloading something nobody ever started.
    await api.fetchStv(a.logId).catch((e) => failDownload(a.logId, errorMessage(e)));
  };

  const [heatOf, setHeatOf] = useState<HeatOf>("deaths");
  const [scope, setScope] = useState<Scope>("match");
  const [asTable, setAsTable] = useState(false);
  // The table lists kills and deaths, so it belongs to that layer alone.
  const table = asTable && layer === "dots";
  const [hover, setHover] = useState<Mark | null>(null);

  // A new player invalidates the enemy filter; career scope is the owner's only.
  useEffect(() => {
    setEnemy(null);
    setHover(null);
    if (!players.get(player)?.isMe) setScope("match");
  }, [player, players]);

  const sliceKills = useMemo(() => a.kills.filter((k) => inSlice(k.roundNum, slice)), [a.kills, slice]);
  const frame: Frame | null = useMemo(() => (view ? view : frameFromKills(sliceKills)), [view, sliceKills]);
  const heat = useMemo(
    () =>
      layer === "heat" && frame
        ? heatGrid(frame, view, sliceKills, player, heatOf, scope, enemy)
        : null,
    [layer, frame, view, sliceKills, player, heatOf, scope, enemy],
  );

  // Routes in view: this round or map, and whose the layer is set to.
  const routes = useMemo(
    () =>
      (pathQ.data ?? []).filter(
        (r) =>
          (slice.rounds === null || (r.roundNum !== null && slice.rounds.has(r.roundNum))) &&
          (pathWho === "all" || r.accountId === (pathWho ?? player)),
      ),
    [pathQ.data, slice.rounds, pathWho, player],
  );
  // The routes are stored in demo ticks; TF2 servers run at 66.67 a second,
  // which is close enough to turn a route's length into seconds.
  const tickRate = 66.67;
  const stvLinked = stv?.hasStv ?? false;
  // How many routes each player has in the rounds on screen, so the dropdown
  // can say who the demo actually followed.
  // The two sides, the owner's first where they played.
  const teamGroups = useMemo(() => {
    const mine = a.players.find((p) => p.isMe)?.team ?? null;
    const of = (t: "Red" | "Blue") => a.players.filter((p) => p.team === t);
    const label = (t: "Red" | "Blue") =>
      mine === null ? teamLabel(t) : t === mine ? `Us · ${teamLabel(t)}` : `Them · ${teamLabel(t)}`;
    const order: Array<"Red" | "Blue"> = mine === "Blue" ? ["Blue", "Red"] : ["Red", "Blue"];
    return order.map((t) => [label(t), of(t)] as [string, typeof a.players]);
  }, [a.players]);

  // The Movement legend's switches: lives by how they ended, and each kind
  // of building (Flashy: the icons were not toggleable).
  const [hidden, setHidden] = useState<ReadonlySet<string>>(() => new Set());
  const toggleHidden = (key: string) =>
    setHidden((h) => {
      const next = new Set(h);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  const shownRoutes = useMemo(() => routes.filter((r) => !hidden.has(r.died ? "died" : "lived")), [routes, hidden]);
  const placed = useMemo(() => {
    const all = buildingQ.data ?? [];
    const ends = teleEnds(all, routes);
    return all.map((b, i) => ({ b, kind: b.kind === "teleporter" ? (b.end ?? ends.get(i) ?? "teleporter") : b.kind }));
  }, [buildingQ.data, routes]);
  const nDied = routes.filter((r) => r.died).length;
  const nLived = routes.length - nDied;
  const kindCount = (kind: string) => placed.filter((p) => p.kind === kind).length;
  const buildingKinds: Array<[string, string]> = [
    ["sentry", tr("Sentries")],
    ["dispenser", tr("Dispensers")],
    ["entrance", tr("Teleporter entrances")],
    ["exit", tr("Teleporter exits")],
    ["teleporter", tr("Teleporters")],
  ];

  const routeCounts = useMemo(() => {
    const by = new Map<number, number>();
    const rows = (pathQ.data ?? []).filter((r) => slice.rounds === null || (r.roundNum !== null && slice.rounds.has(r.roundNum)));
    for (const r of rows) by.set(r.accountId, (by.get(r.accountId) ?? 0) + 1);
    return { by, total: rows.length };
  }, [pathQ.data, slice.rounds]);

  const kills = sliceKills;
  const marks: Mark[] = [];
  // Counted whether shown or not: the legend says what a hidden layer holds.
  let nKills = 0;
  let nDeaths = 0;
  for (const k of kills) {
    if (!k.victimPos) continue;
    const isKill = k.killer === player && k.victim !== player && (enemy === null || k.victim === enemy);
    const isDeath = k.victim === player && (enemy === null || k.killer === enemy);
    if (isKill) nKills++;
    if (isDeath) nDeaths++;
    if ((isKill && showKills) || (isDeath && showDeaths)) {
      marks.push({
        k,
        kind: isKill ? "kill" : "death",
        at: [k.victimPos[0], k.victimPos[1]],
        from: k.killerPos && k.killer !== k.victim ? [k.killerPos[0], k.killerPos[1]] : null,
      });
    }
  }
  const name = me?.name ?? "player";

  if (slice.map === null && slice.multiMap) {
    return (
      <p className="hint an-empty">
        {tx("{size} maps in this match — pick one above.", { size: new Set(a.segments.map((x) => x.map)).size })}</p>
    );
  }

  if (!a.hasPositions || !frame) {
    return <p className="hint an-empty">{tr("This log recorded no positions, so there is no map to draw.")}</p>;
  }

  const { name: shortMap } = splitMap(mapName);
  const careerOk = me?.isMe && view !== null && view.myGames > 0;

  return (
    <div className={full ? "killmap full" : "killmap"} ref={box}>
      {/* Labelled groups, an icon on every option (Flashy: "do the same for
          the kill map controls"). */}
      <div className="ctl-bar">
        <CtlGroup label={tr("Layer")}>
          <Seg
            label={tr("Layer")}
            value={layer}
            onChange={setLayer}
            options={[
              { v: "dots", label: <><Icon name="dots" size={14} /> {tr("Kills and deaths")}</>, title: tr("Every kill and death, where it happened") },
              { v: "paths", label: <><Icon name="routes" size={14} /> {tr("Movement")}</>, title: tr("Where you walked, one line per life, from your own demo") },
              { v: "heat", label: <><Icon name="heat" size={14} /> {tr("Heatmap")}</>, title: tr("Where the kills or deaths gather") },
            ]}
          />
        </CtlGroup>
        {layer === "paths" ? (
          <CtlGroup label={tr("Movement of")}>
          <label className="ctl-select">
            <Icon name="person" size={14} />
            <select
              value={pathWho === null ? String(player) : String(pathWho)}
              onChange={(e) => {
                setPathWho(e.target.value === "all" ? "all" : Number(e.target.value));
                setFocus(null);
              }}
              title={
                stvLinked
                  ? tr("The STV demo carries all eighteen players")
                  : tr("A POV demo only holds its recorder's movement; download the STV demo for everyone else")
              }
            >
              <option value="all" disabled={!stvLinked}>{tx("Everyone{0}", { "0": stvLinked ? ` (${routeCounts.total})` : tr(" — needs the STV demo") })}
              </option>
              {/* Grouped by side, the owner's first, so picking an opponent is
                  a deliberate act rather than a scroll through eighteen names. */}
              {teamGroups.map(([label, players]) => (
                <optgroup key={label} label={label}>
                  {players.map((p) => {
                    const n = routeCounts.by.get(p.accountId) ?? 0;
                    return (
                      <option key={p.accountId} value={p.accountId} disabled={n === 0}>
                        {p.name}
                        {p.mainClass ? ` · ${classLabel(p.mainClass)}` : ""}
                        {n > 0 ? ` (${n})` : stvLinked ? tr(" (none)") : tr(" — needs the STV demo")}
                      </option>
                    );
                  })}
                </optgroup>
              ))}
            </select>
          </label>
          </CtlGroup>
        ) : (
          <CtlGroup label={tr("Against")}>
          <label className="ctl-select">
            <Icon name="person" size={14} />
            <select value={enemy ?? ""} onChange={(e) => setEnemy(e.target.value === "" ? null : Number(e.target.value))}>
              <option value="">{tr("Everyone")}</option>
              {enemies.map((p) => (
                <option key={p.accountId} value={p.accountId}>
                  {p.name}
                  {p.mainClass ? ` · ${classLabel(p.mainClass)}` : ""}
                </option>
              ))}
            </select>
          </label>
          </CtlGroup>
        )}
        {/* What the heatmap counts, and over which matches: only for the
            heatmap, where they mean something. */}
        {layer === "heat" && (
          <CtlGroup label={tr("Heatmap of")}>
            <Seg
              label={tr("Heatmap of")}
              value={heatOf}
              onChange={setHeatOf}
              options={[
                { v: "kills", label: <><Icon name="crosshair" size={14} /> {tr("Kills")}</>, title: tr("Where {name} got kills", { name: name }) },
                { v: "deaths", label: <><Icon name="skull" size={14} /> {tr("Deaths")}</>, title: tr("Where {name} died", { name: name }) },
              ]}
            />
          </CtlGroup>
        )}
        {layer === "heat" && careerOk && (
          <CtlGroup label={tr("Over")}>
            <Seg
              label={tr("Over")}
              value={scope}
              onChange={setScope}
              options={[
                { v: "match", label: <><Icon name="one" size={14} /> {tr("This match")}</> },
                { v: "career", label: <><Icon name="history" size={14} /> {tx("All {myGames} of your {1} matches", { "1": shortMap ?? "", myGames: view!.myGames })}</> },
              ]}
            />
          </CtlGroup>
        )}
        {layer === "dots" && (
          <div className="ctl-end">
            <CtlGroup label={tr("Show as")}>
              <Seg
                label={tr("Show as")}
                value={asTable ? "table" : "map"}
                onChange={(v) => setAsTable(v === "table")}
                options={[
                  { v: "map", label: <><Icon name="map" size={14} /> {tr("Map")}</> },
                  { v: "table", label: <><Icon name="table" size={14} /> {tr("Table")}</> },
                ]}
              />
            </CtlGroup>
          </div>
        )}
      </div>

      {table ? (
        <MarkTable marks={marks} a={a} />
      ) : (
        <>
          <div className={layer === "paths" || editing ? "km-split" : undefined}>
          <Canvas
            zoom={zoom}
            onZoom={setZoom}
            onStage={setStage}
            zoneOpacity={zoneOpacity}
            zoneLabels={zoneLabels}
            zones={zones}
            zoneCounts={layer === "dots" && zones.length > 0 && !editing ? countZones(zones, marks) : undefined}
            drawing={editing ? drawing : undefined}
            selectedZone={editing ? zoneSel : null}
            onMapClick={
              editing
                ? (p) => {
                    if (drawMode) setDrawing((d) => [...d, p]);
                    else setZoneSel(zoneAt(zones, p));
                  }
                : undefined
            }
            onEditZones={editing && !drawMode ? liveEdit : undefined}
            frame={frame}
            display={overview ? overviewFrame(overview) : frame}
            image={overview?.image ?? null}
            view={view}
            marks={layer === "dots" ? marks : []}
            paths={layer === "paths" ? shownRoutes : []}
            buildings={layer === "paths" ? placed.filter((p) => !hidden.has(p.kind)) : []}
            focus={focus}
            maxHeight={full ? Math.max(360, window.innerHeight - FULL_CHROME) : MAX_H}
            heat={heat}
            heatColor={heatOf === "kills" ? KILL : DEATH}
            hover={hover}
            onHover={setHover}
            a={a}
          >
            {/* The map's own controls float on it: callouts top left, zoom
                top right, and the legend (which is also the layers' switches)
                under it. */}
            {mapName && (
              <div className="km-ov km-ov-tl">
                <MapMenu label={tr("Callouts")} on={zones.length > 0}>
                  {callouts && callouts.zones.length > 0 ? (
                    <>
                      {(callouts.author || callouts.draft) && (
                        <p className="km-menu-by">
                          {callouts.author && tr("by {0}", { "0": callouts.author })}
                          {callouts.draft && (
                            <span className="km-draft" title={tr("Draft callouts, not yet checked in game")}>{tr("draft")}</span>
                          )}
                        </p>
                      )}
                      <Switch on={showZones || editing} onChange={() => setShowZones((v) => !v)} disabled={editing}>
                        {tr("Zones")}
                      </Switch>
                      <Switch on={zoneLabels} onChange={() => setZoneLabels((v) => !v)} disabled={!(showZones || editing)}>
                        {tr("Names")}
                      </Switch>
                      <label className="km-slider" title={tr("How solid the zones are")}>
                        <span>{tr("Opacity")}</span>
                        <input
                          type="range"
                          min={0}
                          max={100}
                          step={5}
                          value={Math.round(zoneOpacity * 100)}
                          onChange={(e) => {
                            const v = Number(e.target.value) / 100;
                            setZoneOpacity(v);
                            storeNumber("hl.km.zoneOpacity", v);
                          }}
                          disabled={!(showZones || editing)}
                        />
                      </label>
                    </>
                  ) : (
                    <p className="hint km-menu-by">{tr("No callouts for this map yet.")}</p>
                  )}
                  <button className="km-menu-action" onClick={() => setEditing((e) => !e)}>
                    {editing ? tr("Stop editing") : tr("Edit callouts")}
                  </button>
                </MapMenu>
              </div>
            )}
            <ZoomControls
              z={zoom.z}
              max={MAX_ZOOM}
              onIn={() => zoomBy(1.5)}
              onOut={() => zoomBy(1 / 1.5)}
              onFit={() => setZoom(NO_ZOOM)}
              full={full}
              onFull={toggleFull}
            />
            <div className="km-legend">
              {layer === "dots" && (
                <>
                  <LegendChip on={showKills} onToggle={() => setShowKills((v) => !v)} swatch={<span className="km-key km-key-kill" />} label={tr("Kills")} count={nKills} />
                  <LegendChip on={showDeaths} onToggle={() => setShowDeaths((v) => !v)} swatch={<span className="km-key km-key-death" />} label={tr("Deaths")} count={nDeaths} />
                </>
              )}
              {layer === "paths" && (
                <>
                  {(nLived > 0 || hidden.has("lived")) && <LegendChip
                    on={!hidden.has("lived")}
                    onToggle={() => toggleHidden("lived")}
                    swatch={<span className="km-key km-key-route" />}
                    label={tr("Survived")}
                    count={nLived}
                  />}
                  {(nDied > 0 || hidden.has("died")) && <LegendChip
                    on={!hidden.has("died")}
                    onToggle={() => toggleHidden("died")}
                    swatch={<span className="km-key km-key-route died" />}
                    label={tr("Died")}
                    count={nDied}
                  />}
                  {buildingKinds.filter(([kind]) => kindCount(kind) > 0).map(([kind, label]) => (
                    <LegendChip
                      key={kind}
                      on={!hidden.has(kind)}
                      onToggle={() => toggleHidden(kind)}
                      swatch={<BuildingKey kind={kind} />}
                      label={label}
                      count={kindCount(kind)}
                    />
                  ))}
                </>
              )}
              {layer === "heat" && (
                <div className="km-scale">
                  <span>{tr("fewer")}</span>
                  <span className="km-scale-bar" style={{ background: `linear-gradient(90deg, transparent, ${heatOf === "kills" ? KILL : DEATH})` }} />
                  <span>{tx("more {0}", { "0": heatOf === "kills" ? tr("kills") : tr("deaths") })}</span>
                </div>
              )}
            </div>
          </Canvas>
          {editing && callouts && mapName && (
            <CalloutEditor
              key={mapName}
              map={mapName}
              file={callouts}
              // Save is always there to press while editing: a flag of
              // "changed" is lost when the page reloads parts of itself, and
              // the edits, kept in the query's copy, are not.
              dirty={dirty || editing}
              onChange={liveEdit}
              onSaved={() => setDirty(false)}
              drawing={drawing}
              onDrawing={setDrawing}
              drawMode={drawMode}
              onDrawMode={setDrawMode}
              selected={zoneSel}
              onSelect={setZoneSel}
              onClose={() => {
                setEditing(false);
                setZoneSel(null);
                setDrawMode(false);
              }}
            />
          )}
          {layer === "paths" && !editing && (
            <LifeList
              rows={shownRoutes}
              tickRate={tickRate}
              focus={focus}
              onFocus={setFocus}
              partial={false}
              stv={stv?.hasStv ? "linked" : stv?.demosTfId ? "available" : "none"}
              onFetchStv={fetchStv}
              fetching={stvBusy}
            />
          )}
          </div>
          <p className="hint km-note">
            {mapName === null
              ? stvLinked
                ? tr("Which map these rounds were on is not known, so only this match's positions are drawn.")
                : tr("Which map these rounds were on is not known: the log does not say. Download or drop this match's STV demo and the map is read from it.")
              : overview
              ? tr("Map image from more.tf.")
              : view
                ? tr("Map drawn from {0} positions in {games} stored {2} matches; brighter is busier.", { "0": view.points.toLocaleString(), "2": shortMap ?? "", games: view.games })
                : tr("Too few matches on this map to draw it; only this match's positions are shown.")}
            {layer === "heat" && heatOf === "kills" && tr(" The heatmap marks where the player stood when they got the kill.")}
            {layer === "paths" &&
              (pathQ.isPending
                ? tr(" Reading the demo's routes…")
                : pathQ.data && pathQ.data.length > 0
                  ? stvLinked
                    ? tr(" One line per life, four positions a second, from this match's SourceTV demo: every player, whole lives.")
                    : tr(" One line per life, four positions a second, from your own recording. A POV demo only holds its recorder's movement; the SourceTV demo has everyone.")
                  : tr(" No demo is linked to this match, so there is no movement to draw."))}
          </p>
          {!slice.multiMap || mapName !== null ? (
            <div className="km-map-pick">
              <MapPicker
                key={`${a.logId}-${mapName ?? "none"}`}
                logId={a.logId}
                rounds={a.rounds.filter((r) => inSlice(r.roundNum, slice)).map((r) => r.roundNum)}
                current={mapName}
              />
            </div>
          ) : null}
          {layer === "dots" && <TimeStrip a={a} slice={slice} marks={marks} hover={hover} onHover={setHover} />}
        </>
      )}
    </div>
  );
}

/** The map, a heat layer or the kill marks, and the hover card. */
function Canvas(props: {
  /** Zoom and pan, and how to change them (the wheel and a drag). */
  zoom: Zoom;
  onZoom: (z: Zoom | ((z: Zoom) => Zoom)) => void;
  onStage: (size: [number, number]) => void;
  zoneOpacity: number;
  zoneLabels: boolean;
  /** Q28: callout zones to draw, how many marks fell in each, a zone being drawn. */
  zones: import("../../api/types").CalloutZone[];
  zoneCounts?: Map<number, number>;
  drawing?: Array<[number, number]>;
  selectedZone: number | null;
  /** Editing callouts: a click on the map, in game units. */
  onMapClick?: (p: [number, number]) => void;
  /** Editing callouts: the selected zone's corners and body can be dragged. */
  onEditZones?: (zones: CalloutZone[]) => void;
  /** The grid the heat and outline are counted on. */
  frame: Frame;
  /** What the canvas shows: the image's square when there is one. */
  display: Frame;
  image: string | null;
  view: MapView | null;
  marks: Mark[];
  /** Routes to draw under the marks, one per life. */
  paths: PathRow[];
  /** Engineer buildings, drawn over the routes, a teleporter's end resolved. */
  buildings: Array<{ b: BuildingRow; kind: string }>;
  /** Controls that float on the map. */
  children?: ReactNode;
  /** One route to pick out, with the rest faded. */
  focus: PathRow | null;
  /** How tall the map may be; the window's height in full screen. */
  maxHeight: number;
  heat: number[] | null;
  heatColor: string;
  hover: Mark | null;
  onHover: (m: Mark | null) => void;
  a: Analysis;
}) {
  const { frame, display, image, view, marks, paths, buildings, focus, maxHeight, heat, heatColor, hover, onHover, a, zones, zoneCounts, drawing, selectedZone, onMapClick, onEditZones, zoom, onZoom, onStage, zoneOpacity, zoneLabels, children } = props;
  // The three colours the canvas draws with, resolved from the theme.
  const killColour = themeColour("--kill", "#5791c8");
  const deathColour = themeColour("--death", "#d6763a");
  const icons = useMapIcons();
  const skullFilter = `km-skull-${useId().replace(/:/g, "")}`;
  const heatResolved = heatColor.startsWith("var(")
    ? themeColour(heatColor.slice(4, -1), "#5791c8")
    : heatColor;
  const [img, setImg] = useState<HTMLImageElement | null>(null);
  useEffect(() => {
    setImg(null);
    if (!image) return;
    const el = new Image();
    el.onload = () => setImg(el);
    el.src = image;
  }, [image]);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [boxW, wrap] = useMeasuredWidth(320, 800);


  const scale = Math.min(boxW / display.width, maxHeight / display.height);
  const W = Math.floor(display.width * scale);
  const H = Math.floor(display.height * scale);
  // Game units to canvas pixels, zoom and pan included: everything drawn
  // goes through here, so the image, heat, zones and marks stay together.
  const px = ([x, y]: [number, number]): [number, number] => [
    ((x - display.minX) / display.cell) * scale * zoom.z + zoom.x,
    ((display.maxY - y) / display.cell) * scale * zoom.z + zoom.y,
  ];
  // Back from the canvas to game units, for drawing callouts.
  const game = (cx: number, cy: number): [number, number] => [
    Math.round(display.minX + ((cx - zoom.x) / zoom.z / scale) * display.cell),
    Math.round(display.maxY - ((cy - zoom.y) / zoom.z / scale) * display.cell),
  ];
  // A cell of the counting grid, in canvas pixels.
  const cellRect = (i: number): [number, number, number] => {
    const [x, y] = px([frame.minX + (i % frame.width) * frame.cell, frame.maxY - Math.floor(i / frame.width) * frame.cell]);
    return [x, y, (frame.cell / display.cell) * scale * zoom.z + 0.5];
  };

  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = W * dpr;
    c.height = H * dpr;
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, W, H);

    if (img) {
      // The image, under a dark veil: the marks' blue and orange have to read
      // on sand and stone, and the veil keeps the map recessive.
      g.drawImage(img, zoom.x, zoom.y, W * zoom.z, H * zoom.z);
      g.fillStyle = "rgba(27, 23, 20, 0.45)";
      g.fillRect(0, 0, W, H);
    } else if (view) {
      // The map: occupancy on a log scale, in the muted ink, so the marks own
      // the colour. A cell seen once is a stray (a rocket jump, an old map
      // version) and would speckle the outline, so it is left out.
      const max = Math.log1p(Math.max(2, ...view.occupancy));
      for (let i = 0; i < view.occupancy.length; i++) {
        const n = view.occupancy[i];
        if (n < 2) continue;
        const v = Math.log1p(n) / max;
        g.fillStyle = `rgba(168, 153, 139, ${(0.06 + 0.6 * v ** 1.4).toFixed(3)})`;
        const [x, y, s] = cellRect(i);
        g.fillRect(x, y, s, s);
      }
    }
    // Routes: one line per life, thin and translucent so a busy match reads
    // as traffic rather than spaghetti. A life that ended in a death is drawn
    // in the death colour, and every line ends in a dot where it stopped.
    for (const route of paths) {
      if (route.points.length < 2) continue;
      const picked = focus !== null && focus.seq === route.seq && focus.demoId === route.demoId;
      const dim = focus !== null && !picked;
      g.strokeStyle = route.died
        ? `rgba(232, 106, 98, ${dim ? 0.12 : picked ? 0.95 : 0.55})`
        : `rgba(134, 171, 201, ${dim ? 0.1 : picked ? 0.95 : 0.5})`;
      g.lineWidth = picked ? 2.5 : 1.5;
      g.lineJoin = "round";
      // A jump no one can walk -- a teleporter, mostly -- breaks the line
      // rather than drawing a straight stroke across the map (Emiel). It is
      // still the same route: the picked one gets a faint dotted link.
      const gaps: [number, number][] = [];
      g.beginPath();
      route.points.forEach(([, x, y], i) => {
        const [cx, cy] = px([x, y]);
        if (i === 0 || jumped(route.points[i - 1], route.points[i])) {
          if (i > 0) gaps.push([i - 1, i]);
          g.moveTo(cx, cy);
        } else g.lineTo(cx, cy);
      });
      g.stroke();
      if (picked && gaps.length > 0) {
        g.save();
        g.setLineDash([3, 5]);
        g.lineWidth = 1;
        g.globalAlpha = 0.45;
        g.beginPath();
        for (const [from, to] of gaps) {
          const [ax, ay] = px([route.points[from][1], route.points[from][2]]);
          const [bx, by] = px([route.points[to][1], route.points[to][2]]);
          g.moveTo(ax, ay);
          g.lineTo(bx, by);
        }
        g.stroke();
        g.restore();
      }
      if (dim) continue;
      // Where it started and where it stopped: a ring for the spawn, a solid
      // dot for the end, so a route reads in one direction.
      const first = route.points[0];
      const last = route.points[route.points.length - 1];
      const [sx, sy] = px([first[1], first[2]]);
      const [ex, ey] = px([last[1], last[2]]);
      if (picked) {
        g.strokeStyle = `rgba(${themeColour("--text-rgb", "242, 230, 217")}, 0.9)`;
        g.lineWidth = 1.5;
        g.beginPath();
        g.arc(sx, sy, 4, 0, Math.PI * 2);
        g.stroke();
      }
      // Canvas takes a colour, not a CSS variable, so these are resolved
      // here rather than at module load — the theme can change live.
      g.fillStyle = route.died ? deathColour : killColour;
      g.beginPath();
      g.arc(ex, ey, picked ? 4 : 2.5, 0, Math.PI * 2);
      g.fill();
    }

    // Buildings over the routes. With a life picked, only what stood while
    // it was walked; otherwise every one, faded.
    for (const { b, kind } of buildings) {
      const live = focus === null || (b.demoId === focus.demoId && b.fromTick <= focus.toTick && b.toTick >= focus.fromTick);
      if (focus !== null && !live) continue;
      drawBuilding(g, px([b.x, b.y]), kind, b.team, focus === null ? 0.7 : 1, icons);
    }

    // Heat: one hue, transparent to full, so more is brighter. No floor:
    // with a floor every cell anyone ever died in lights up and the hot spots
    // drown; below 6% a cell stays clear.
    if (heat) {
      const max = Math.max(...heat);
      if (max > 0) {
        for (let i = 0; i < heat.length; i++) {
          const alpha = 0.92 * (heat[i] / max) ** 0.9;
          if (alpha < 0.06) continue;
          g.globalAlpha = alpha;
          g.fillStyle = heatResolved;
          const [x, y, s] = cellRect(i);
          g.fillRect(x, y, s, s);
        }
        g.globalAlpha = 1;
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [img, view, heat, heatColor, heatResolved, killColour, deathColour, frame, display, W, H, scale, paths, buildings, focus, zoom, icons]);

  const nearest = (e: React.MouseEvent<SVGSVGElement>): Mark | null => {
    const r = e.currentTarget.getBoundingClientRect();
    const mx = e.clientX - r.left;
    const my = e.clientY - r.top;
    let best: Mark | null = null;
    let bestD = 14 * 14;
    for (const m of marks) {
      const [x, y] = px(m.at);
      const d = (x - mx) ** 2 + (y - my) ** 2;
      if (d < bestD) {
        bestD = d;
        best = m;
      }
    }
    return best;
  };

  // The wheel zooms towards the cursor. A native listener, as React's is
  // passive and cannot stop the page scrolling underneath.
  const svgRef = useRef<SVGSVGElement>(null);
  useEffect(() => onStage([W, H]), [W, H, onStage]);
  useEffect(() => {
    const el = svgRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      const at: [number, number] = [e.clientX - r.left, e.clientY - r.top];
      onZoom((z) => clampZoom(zoomAt(z, z.z * Math.pow(1.0015, -e.deltaY), at), W, H));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [onZoom, W, H]);
  // A drag pans; a press that barely moves is still a click.
  const drag = useRef<{ x: number; y: number; zx: number; zy: number; moved: boolean } | null>(null);
  const [dragging, setDragging] = useState(false);
  // Editing: a corner being moved, or a whole zone.
  // Each drag works on its own copy of the zone's corners: the zones prop
  // only catches up on the next render, and a move can come before that.
  const edit = useRef<
    | { zone: number; index: number; pts: Array<[number, number]>; moved: boolean }
    | { zone: number; start: [number, number]; orig: Array<[number, number]>; moved: boolean }
    | null
  >(null);
  const zonesRef = useRef(zones);
  zonesRef.current = zones;
  const putZone = (zone: number, pts: Array<[number, number]>) =>
    onEditZones?.(zonesRef.current.map((z, i) => (i === zone ? { ...z, points: pts } : z)));
  const onHandle = (h: HandleDrag) => {
    if (!onEditZones) return;
    if (h.kind === "mid") {
      // A new corner halfway along the edge, dragged from there.
      const pts = zones[h.zone].points;
      const a = pts[h.index];
      const b = pts[(h.index + 1) % pts.length];
      const mid: [number, number] = [Math.round((a[0] + b[0]) / 2), Math.round((a[1] + b[1]) / 2)];
      const next = [...pts.slice(0, h.index + 1), mid, ...pts.slice(h.index + 1)];
      putZone(h.zone, next);
      edit.current = { zone: h.zone, index: h.index + 1, pts: next, moved: true };
    } else {
      edit.current = { zone: h.zone, index: h.index, pts: [...zones[h.zone].points], moved: true };
    }
    setDragging(true);
  };
  const deleteCorner = (zone: number, index: number) => {
    const pts = zones[zone].points;
    if (!onEditZones || pts.length <= 3) return;
    onEditZones(zones.map((z, i) => (i === zone ? { ...z, points: pts.filter((_, j) => j !== index) } : z)));
  };

  return (
    <div className="km-canvas" ref={wrap}>
      <div className="km-stage" style={{ width: W, height: H }}>
        <canvas ref={canvas} style={{ width: W, height: H }} aria-hidden />
        <svg
          width={W}
          height={H}
          className="km-svg"
          role="img"
          aria-label={tr("{marks} kills and deaths on the map", { marks: marks.length })}
          ref={svgRef}
          onMouseDown={(e) => {
            if (e.button !== 0) return;
            // Inside the selected zone while editing: move the whole zone.
            if (onEditZones && selectedZone !== null && zones[selectedZone]) {
              const r = e.currentTarget.getBoundingClientRect();
              const at = game(e.clientX - r.left, e.clientY - r.top);
              // Only where the selected zone is what wins the spot: a smaller
              // zone inside it is still there to be clicked.
              if (zoneAt(zones, at) === selectedZone) {
                edit.current = { zone: selectedZone, start: at, orig: zones[selectedZone].points, moved: false };
                return;
              }
            }
            if (zoom.z <= 1) return;
            drag.current = { x: e.clientX, y: e.clientY, zx: zoom.x, zy: zoom.y, moved: false };
          }}
          onMouseMove={(e) => {
            const ed = edit.current;
            if (ed && (e.buttons & 1) === 1) {
              const r = e.currentTarget.getBoundingClientRect();
              const at = game(e.clientX - r.left, e.clientY - r.top);
              if ("index" in ed) {
                ed.pts[ed.index] = at;
                putZone(ed.zone, [...ed.pts]);
              } else {
                const dx = at[0] - ed.start[0];
                const dy = at[1] - ed.start[1];
                // A few units of wobble is a click, not a move.
                if (!ed.moved && Math.hypot(dx, dy) < 12) return;
                if (!ed.moved) {
                  ed.moved = true;
                  setDragging(true);
                }
                putZone(ed.zone, ed.orig.map(([x, y]) => [x + dx, y + dy] as [number, number]));
              }
              return;
            }
            const d = drag.current;
            if (d && (e.buttons & 1) === 1) {
              const dx = e.clientX - d.x;
              const dy = e.clientY - d.y;
              if (!d.moved && Math.hypot(dx, dy) > 4) {
                d.moved = true;
                setDragging(true);
              }
              if (d.moved) {
                onZoom((z) => clampZoom({ ...z, x: d.zx + dx, y: d.zy + dy }, W, H));
                return;
              }
            }
            if (!onMapClick) onHover(nearest(e));
          }}
          onMouseUp={() => {
            // Cleared after the click that follows, so a drag is not a click.
            const wasEdit = edit.current?.moved === true;
            setTimeout(() => {
              drag.current = null;
              edit.current = null;
              setDragging(false);
            }, 0);
            if (wasEdit) drag.current = { x: 0, y: 0, zx: 0, zy: 0, moved: true };
          }}
          onMouseLeave={() => {
            drag.current = null;
            edit.current = null;
            setDragging(false);
            onHover(null);
          }}
          onClick={(e) => {
            if (drag.current?.moved) return;
            if (onMapClick) {
              const r = e.currentTarget.getBoundingClientRect();
              onMapClick(game(e.clientX - r.left, e.clientY - r.top));
              return;
            }
            const m = nearest(e);
            if (m?.k.jump) jumpTo(m.k.jump, `${m.kind} at ${roundClock(m.k.t, a.rounds)}`);
          }}
          style={{ cursor: dragging ? "grabbing" : onMapClick ? "crosshair" : hover?.k.jump ? "pointer" : zoom.z > 1 ? "grab" : "default" }}
        >
          {zones.length > 0 || drawing ? (
            <ZoneShapes
              zones={zones}
              px={px}
              selected={selectedZone}
              counts={zoneCounts}
              drawing={drawing}
              opacity={zoneOpacity}
              labels={zoneLabels}
              onHandle={onEditZones ? onHandle : undefined}
              onDeleteCorner={onEditZones ? deleteCorner : undefined}
            />
          ) : null}
          <defs>
            {/* The kill feed's skull for a death (Q72): tinted the death
                colour over a dark rim, so it reads on a light map. */}
            <filter id={skullFilter} x="-30%" y="-30%" width="160%" height="160%">
              <feMorphology in="SourceAlpha" operator="dilate" radius={1.2} result="rim" />
              <feFlood floodColor="rgb(20, 16, 12)" floodOpacity={0.85} />
              <feComposite in2="rim" operator="in" result="dark" />
              <feFlood style={{ floodColor: DEATH }} />
              <feComposite in2="SourceAlpha" operator="in" result="fill" />
              <feMerge>
                <feMergeNode in="dark" />
                <feMergeNode in="fill" />
              </feMerge>
            </filter>
          </defs>
          {marks.map((m, i) => {
            const [x, y] = px(m.at);
            const from = m.from ? px(m.from) : null;
            const color = m.kind === "kill" ? KILL : DEATH;
            const dim = hover && hover !== m ? 0.35 : 1;
            return (
              <g key={i} opacity={dim}>
                {from && <line x1={from[0]} y1={from[1]} x2={x} y2={y} stroke={color} strokeWidth={1.25} opacity={0.7} />}
                {from && <circle cx={from[0]} cy={from[1]} r={3.5} className="km-shooter" />}
                {m.kind === "kill" ? (
                  <circle cx={x} cy={y} r={4.5} fill={color} className="km-mark" />
                ) : (
                  <image href="/icons/skull.png" x={x - 8} y={y - 5} width={16} height={10.3} filter={`url(#${skullFilter})`} />
                )}
              </g>
            );
          })}
        </svg>
        {hover && <HoverCard m={hover} a={a} pos={px(hover.at)} W={W} />}
      </div>
      {children}
    </div>
  );
}

function HoverCard({ m, a, pos, W }: { m: Mark; a: Analysis; pos: [number, number]; W: number }) {
  const players = playerMap(a);
  const k = m.k;
  const left = Math.min(W - 250, Math.max(0, pos[0] + 12));
  return (
    <div className="km-tip" style={{ left, top: Math.max(0, pos[1] - 10) }}>
      <div className="km-tip-head">
        <span className={m.kind === "kill" ? "km-key km-key-kill" : "km-key km-key-death"} aria-hidden />
        <strong>{m.kind === "kill" ? tr("Kill") : tr("Death")}</strong>
        <span className="muted">{roundClock(k.t, a.rounds)}</span>
      </div>
      <div>
        {players.get(k.killer)?.name ?? "?"} <span className="muted">({k.killerClass ?? "?"})</span> →{" "}
        {players.get(k.victim)?.name ?? "?"} <span className="muted">({k.victimClass ?? "?"})</span>
      </div>
      <div className="muted">
        {k.weapon}
        {k.custom && ` · ${k.custom}`}
        {k.distance !== null && tr(" · {0} units", { "0": Math.round(k.distance).toLocaleString() })}
      </div>
      {k.jump && <div className="km-tip-jump">{tx("Click to copy demo_gototick {tick}", { tick: k.jump.tick })}</div>}
    </div>
  );
}

/** Every mark along the match's game time: kills above the line, deaths below. */
function TimeStrip(props: { a: Analysis; slice: Slice; marks: Mark[]; hover: Mark | null; onHover: (m: Mark | null) => void }) {
  const { a, slice, marks, hover, onHover } = props;
  const [w, wrap] = useMeasuredWidth(320, 800);
  const H = 44;
  const mid = H / 2;
  const span = Math.max(1, slice.endS - slice.startS);
  const x = (t: number) => ((t - slice.startS) / span) * w;

  const nearest = (e: React.MouseEvent<SVGSVGElement>) => {
    const mx = e.clientX - e.currentTarget.getBoundingClientRect().left;
    let best: Mark | null = null;
    let bestD = 8;
    for (const m of marks) {
      const d = Math.abs(x(m.k.t) - mx);
      if (d < bestD) {
        bestD = d;
        best = m;
      }
    }
    return best;
  };

  return (
    <div className="km-strip" ref={wrap}>
      <svg
        width={w}
        height={H + 16}
        role="img"
        aria-label={tr("Kills and deaths over the match")}
        onMouseMove={(e) => onHover(nearest(e))}
        onMouseLeave={() => onHover(null)}
        onClick={(e) => {
          const m = nearest(e);
          if (m?.k.jump) jumpTo(m.k.jump, `${m.kind} at ${roundClock(m.k.t, a.rounds)}`);
        }}
      >
        <line x1={0} x2={w} y1={mid} y2={mid} className="km-axis" />
        {a.rounds.filter((r) => inSlice(r.roundNum, slice)).map((r) => (
          <g key={r.roundNum}>
            {r.startS > slice.startS && <line x1={x(r.startS)} x2={x(r.startS)} y1={2} y2={H - 2} className="km-round" />}
            <text x={x(r.startS) + 4} y={H + 12} className="km-round-label">
              R{r.roundNum}
            </text>
          </g>
        ))}
        {marks.map((m, i) => (
          <rect
            key={i}
            x={x(m.k.t) - 1.5}
            y={m.kind === "kill" ? mid - 16 : mid + 2}
            width={3}
            height={14}
            rx={1}
            fill={m.kind === "kill" ? KILL : DEATH}
            opacity={hover && hover !== m ? 0.35 : 1}
          />
        ))}
      </svg>
    </div>
  );
}

/** The same marks without the map: every value reachable without hovering. */
function MarkTable({ marks: all, a }: { marks: Mark[]; a: Analysis }) {
  const players = playerMap(a);
  const sort = useSort("killmap-marks", all, {
    when: (m) => m.k.t,
    kind: (m) => m.kind,
    killer: (m) => players.get(m.k.killer)?.name ?? null,
    victim: (m) => players.get(m.k.victim)?.name ?? null,
    weapon: (m) => m.k.weapon,
    distance: (m) => m.k.distance,
  });
  const marks = sort.sorted;
  if (marks.length === 0) return <p className="hint an-empty">{tr("Nothing to show for this filter.")}</p>;
  return (
    <div className="table-wrap">
      <table className="match-table">
        <thead>
          <tr>
            <SortTh k="when" s={sort}>{tr("When")}</SortTh>
            <SortTh k="kind" s={sort}>{""}</SortTh>
            <SortTh k="killer" s={sort}>{tr("Killer")}</SortTh>
            <SortTh k="victim" s={sort}>{tr("Victim")}</SortTh>
            <SortTh k="weapon" s={sort}>{tr("Weapon")}</SortTh>
            <SortTh k="distance" s={sort} className="num" tip={tip(tr("Distance"), tr("How far apart killer and victim were, in map units; a Sniper sightline is around 1,500."), tr("From the server log's positions at the kill."), "server log")}>{tr("Distance")}</SortTh>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {marks.map((m, i) => (
            <tr key={i}>
              <td className="muted nowrap">{roundClock(m.k.t, a.rounds)}</td>
              <td>
                <span className={m.kind === "kill" ? "km-key km-key-kill" : "km-key km-key-death"} aria-hidden />{" "}
                {m.kind === "kill" ? tr("kill") : tr("death")}
              </td>
              <td className="nowrap">
                {players.get(m.k.killer)?.name} <span className="muted">{m.k.killerClass}</span>
              </td>
              <td className="nowrap">
                {players.get(m.k.victim)?.name} <span className="muted">{m.k.victimClass}</span>
              </td>
              <td className="muted nowrap">
                {m.k.weapon}
                {m.k.custom && ` · ${m.k.custom}`}
              </td>
              <td className="num">{m.k.distance === null ? "—" : Math.round(m.k.distance).toLocaleString()}</td>
              <td>
                {m.k.jump && (
                  <button className="linkish" onClick={() => jumpTo(m.k.jump!, `${m.kind} at ${roundClock(m.k.t, a.rounds)}`)}>{tr("copy tick")}</button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Without a stored outline, frame the match's own positions. */
function frameFromKills(kills: KillView[]): Frame | null {
  const pts = kills.flatMap((k) => [k.killerPos, k.victimPos]).filter((p): p is Vec3 => p !== null);
  if (pts.length === 0) return null;
  let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
  for (const [x, y] of pts) {
    x0 = Math.min(x0, x);
    x1 = Math.max(x1, x);
    y0 = Math.min(y0, y);
    y1 = Math.max(y1, y);
  }
  const pad = 0.05 * Math.max(x1 - x0, y1 - y0, 1);
  const cell = Math.max(1, (Math.max(x1 - x0, y1 - y0) + 2 * pad) / 180);
  return {
    minX: x0 - pad,
    maxY: y1 + pad,
    cell,
    width: Math.ceil((x1 - x0 + 2 * pad) / cell),
    height: Math.ceil((y1 - y0 + 2 * pad) / cell),
  };
}

/**
 * Heat on the frame's grid, smoothed with two box blurs. Kills are placed
 * where the player stood when they got them; deaths where they fell.
 */
function heatGrid(
  f: Frame,
  view: MapView | null,
  kills: KillView[],
  player: number,
  of: HeatOf,
  scope: Scope,
  enemy: number | null,
): number[] {
  let grid: number[];
  if (scope === "career" && view) {
    grid = (of === "kills" ? view.myKills : view.myDeaths).slice();
  } else {
    grid = new Array(f.width * f.height).fill(0);
    for (const k of kills) {
      const mine = of === "kills" ? k.killer === player && k.victim !== player : k.victim === player;
      const vs = of === "kills" ? k.victim : k.killer;
      const pos = of === "kills" ? k.killerPos : k.victimPos;
      if (!mine || !pos || (enemy !== null && vs !== enemy)) continue;
      const cx = Math.floor((pos[0] - f.minX) / f.cell);
      const cy = Math.floor((f.maxY - pos[1]) / f.cell);
      if (cx >= 0 && cy >= 0 && cx < f.width && cy < f.height) grid[cy * f.width + cx] += 1;
    }
  }
  return blur(blur(grid, f.width, f.height), f.width, f.height);
}

function blur(g: number[], w: number, h: number): number[] {
  const out = new Array(g.length).fill(0);
  const r = 2;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      let s = 0;
      for (let dy = -r; dy <= r; dy++) {
        const yy = y + dy;
        if (yy < 0 || yy >= h) continue;
        for (let dx = -r; dx <= r; dx++) {
          const xx = x + dx;
          if (xx >= 0 && xx < w) s += g[yy * w + xx];
        }
      }
      out[y * w + x] = s;
    }
  }
  return out;
}

/** An overview image as a drawing frame 1024 units across, and as many
 *  down as its shape needs (square for the built-in renders). */
function overviewFrame(o: Overview): Frame {
  return { minX: o.minX, maxY: o.maxY, cell: o.size / 1024, width: 1024, height: Math.round(1024 * (o.aspect || 1)) };
}

/** Marks per callout zone, by where each landed (Q28). */
function countZones(zones: import("../../api/types").CalloutZone[], marks: Mark[]): Map<number, number> {
  const out = new Map<number, number>();
  for (const m of marks) {
    const z = zoneAt(zones, m.at);
    if (z !== null) out.set(z, (out.get(z) ?? 0) + 1);
  }
  return out;
}

/** Zoom and pan: canvas pixels are scaled by `z`, then moved by `x`, `y`. */
interface Zoom {
  z: number;
  x: number;
  y: number;
}

const NO_ZOOM: Zoom = { z: 1, x: 0, y: 0 };
const MAX_ZOOM = 8;

/** A new zoom level, keeping the point under `at` still. */
function zoomAt(z: Zoom, next: number, at: [number, number]): Zoom {
  const nz = Math.min(MAX_ZOOM, Math.max(1, next));
  if (nz === 1) return NO_ZOOM;
  const k = nz / z.z;
  return { z: nz, x: at[0] - (at[0] - z.x) * k, y: at[1] - (at[1] - z.y) * k };
}

/** Keep the map on the canvas: never panned so far it leaves the view. */
function clampZoom(z: Zoom, W: number, H: number): Zoom {
  if (z.z <= 1) return NO_ZOOM;
  return { z: z.z, x: Math.min(0, Math.max(W - W * z.z, z.x)), y: Math.min(0, Math.max(H - H * z.z, z.y)) };
}

function storedNumber(key: string, fallback: number): number {
  try {
    const v = Number(localStorage.getItem(key));
    return localStorage.getItem(key) !== null && Number.isFinite(v) ? v : fallback;
  } catch {
    return fallback;
  }
}

function storeNumber(key: string, v: number) {
  try {
    localStorage.setItem(key, String(v));
  } catch {
    // Blocked storage only costs remembering the setting.
  }
}

// ---- Engineer buildings on the Paths layer (Emiel's ticket) ----------------

/** Faster than anyone walks or jumps, in map units a second: a teleporter
 *  (instant), or the demo losing the player for a moment. A rocket or
 *  sticky jump peaks around 2,000. */
const JUMP_SPEED = 3_500;
const TICK_RATE = 66.67;

function jumped(a: [number, number, number, number], b: [number, number, number, number]): boolean {
  const dist = Math.hypot(b[1] - a[1], b[2] - a[2], b[3] - a[3]);
  const secs = Math.max(1, b[0] - a[0]) / TICK_RATE;
  return dist > 400 && dist / secs > JUMP_SPEED;
}

/** For teleporters read before their end was recorded: the one nearest
 *  where a route's jump starts is an entrance, nearest where it lands an
 *  exit. Keyed by the building's index. */
function teleEnds(buildings: BuildingRow[], paths: PathRow[]): Map<number, "entrance" | "exit"> {
  const out = new Map<number, "entrance" | "exit">();
  const teles = buildings.map((b, i) => [b, i] as const).filter(([b]) => b.kind === "teleporter" && b.end === null);
  if (teles.length === 0) return out;
  const nearest = (demoId: number, tick: number, x: number, y: number) => {
    let best: number | null = null;
    let bestD = 300;
    for (const [b, i] of teles) {
      if (b.demoId !== demoId || tick < b.fromTick - 66 || tick > b.toTick + 66) continue;
      const d = Math.hypot(b.x - x, b.y - y);
      if (d < bestD) [best, bestD] = [i, d];
    }
    return best;
  };
  for (const route of paths) {
    for (let i = 1; i < route.points.length; i++) {
      const [from, to] = [route.points[i - 1], route.points[i]];
      if (!jumped(from, to)) continue;
      const entrance = nearest(route.demoId, from[0], from[1], from[2]);
      const exit = nearest(route.demoId, to[0], to[1], to[2]);
      if (entrance !== null && exit !== null && entrance !== exit) {
        out.set(entrance, "entrance");
        out.set(exit, "exit");
      }
    }
  }
  return out;
}

const RED = "214, 92, 84";
const BLU = "99, 148, 196";

/** How tall each building's icon is drawn, in pixels. */
const ICON_H: Record<string, number> = { sentry: 18, dispenser: 15, entrance: 13, exit: 13, teleporter: 6 };

/** One building's marker: TF2's HUD icon in its team's colour (Q72), the
 *  teleporter's arrow saying which end it is (none when not known). Until
 *  the icons load, or if one is missing: a triangle for a sentry, a square
 *  for a dispenser, a ring for an entrance, a disc for an exit, a diamond. */
function drawBuilding(g: CanvasRenderingContext2D, [x, y]: [number, number], kind: string, team: number | null, alpha: number, icons: MapIcons | null, scale = 1) {
  const rgb = team === 2 ? RED : team === 3 ? BLU : "200, 190, 175";
  if (kind in ICON_H && drawIcon(g, icons, kind as IconName, [x, y], ICON_H[kind] * scale, `rgb(${rgb})`, alpha)) return;
  g.save();
  g.globalAlpha = alpha;
  g.lineWidth = 2;
  g.strokeStyle = "rgba(20, 16, 12, 0.85)";
  g.fillStyle = `rgb(${rgb})`;
  g.beginPath();
  if (kind === "sentry") {
    g.moveTo(x, y - 7);
    g.lineTo(x + 6.5, y + 5);
    g.lineTo(x - 6.5, y + 5);
    g.closePath();
    g.stroke();
    g.fill();
  } else if (kind === "dispenser") {
    g.rect(x - 5, y - 5, 10, 10);
    g.stroke();
    g.fill();
  } else if (kind === "entrance") {
    g.arc(x, y, 5.5, 0, Math.PI * 2);
    g.stroke();
    g.strokeStyle = `rgb(${rgb})`;
    g.lineWidth = 2.5;
    g.beginPath();
    g.arc(x, y, 5.5, 0, Math.PI * 2);
    g.stroke();
  } else if (kind === "exit") {
    g.arc(x, y, 6, 0, Math.PI * 2);
    g.stroke();
    g.fill();
    g.fillStyle = "rgba(20, 16, 12, 0.85)";
    g.beginPath();
    g.arc(x, y, 2, 0, Math.PI * 2);
    g.fill();
  } else {
    g.moveTo(x, y - 6);
    g.lineTo(x + 6, y);
    g.lineTo(x, y + 6);
    g.lineTo(x - 6, y);
    g.closePath();
    g.stroke();
    g.fill();
  }
  g.restore();
}

/** The legend's little copy of a marker. */
function BuildingKey({ kind }: { kind: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const icons = useMapIcons();
  useEffect(() => {
    const c = ref.current;
    const g = c?.getContext("2d");
    if (!c || !g) return;
    // Drawn at twice the size it shows, so the icon stays sharp.
    g.setTransform(2, 0, 0, 2, 0, 0);
    g.clearRect(0, 0, 16, 16);
    drawBuilding(g, [8, 8], kind, null, 1, icons, kind === "teleporter" ? 1 : 0.7);
  }, [kind, icons]);
  return <canvas ref={ref} width={32} height={32} className="km-building-key" />;
}
