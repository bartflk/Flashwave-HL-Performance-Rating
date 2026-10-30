import { useState } from "react";
import { friendlyText } from "../../lib/apierror";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { type CalloutFile, type CalloutZone } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { CalloutPresetButtons } from "../CalloutPresets";

/**
 * Q28 (Flashy): callouts as zones on the map -- the jigsaw -- and an editor
 * to draw and fix them, because seeded callouts are drafts until someone
 * who plays the map has checked them. Zones are in game units.
 *
 * Where zones overlap, the smaller one wins: Shack inside Flank is Shack.
 * The Rust side decides the same way, so counts, positions and the drawing
 * agree, and nobody has to keep a list in the right order.
 */

type Pt = [number, number];

export function useCallouts(map: string | null) {
  return useQuery({
    queryKey: ["callouts", map],
    queryFn: () => api.getCallouts(map ?? ""),
    enabled: map !== null,
    staleTime: Infinity,
  });
}

function inside([x, y]: Pt, poly: Pt[]): boolean {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [x1, y1] = poly[i];
    const [x2, y2] = poly[j];
    if (y1 > y !== y2 > y && x < ((x2 - x1) * (y - y1)) / (y2 - y1) + x1) c = !c;
  }
  return c;
}

/** A polygon's area, by the shoelace formula. */
export function area(poly: Pt[]): number {
  let a = 0;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) a += (poly[j][0] + poly[i][0]) * (poly[j][1] - poly[i][1]);
  return Math.abs(a / 2);
}

/** The smallest zone holding a point, as the Rust side decides it. */
export function zoneAt(zones: CalloutZone[], p: Pt): number | null {
  let best: number | null = null;
  let bestA = Infinity;
  zones.forEach((z, i) => {
    if (z.points.length < 3 || !inside(p, z.points)) return;
    const a = area(z.points);
    if (a < bestA) {
      bestA = a;
      best = i;
    }
  });
  return best;
}

/** Twelve hues, cycled: neighbours differ, and the names carry identity. */
const HUES = [205, 25, 140, 285, 50, 330, 170, 0, 95, 245, 310, 70];

/** What a drag on the editing handles is doing. */
export type HandleDrag = { kind: "corner" | "mid"; zone: number; index: number };

/** The zones, drawn in SVG under the marks. `px` maps game units to the canvas. */
export function ZoneShapes(props: {
  zones: CalloutZone[];
  px: (p: Pt) => Pt;
  selected: number | null;
  counts?: Map<number, number>;
  drawing?: Pt[];
  /** 0 is outlines only, 1 as solid as the zones go. */
  opacity?: number;
  labels?: boolean;
  /** Editing: handles on the selected zone. */
  onHandle?: (h: HandleDrag) => void;
  onDeleteCorner?: (zone: number, index: number) => void;
}) {
  const { zones, px, selected, counts, drawing, opacity = 0.5, labels = true, onHandle, onDeleteCorner } = props;
  const most = counts ? Math.max(1, ...counts.values()) : 1;
  // On a mirrored map a zone takes its side's colour -- RED's ground red,
  // BLU's blue, the shared middle grey -- so a glance says whose half a
  // fight was in. One-way maps keep a hue per zone.
  const names = new Set(zones.map((z) => z.name));
  const paired = (n: string) => {
    const m = /^(RED|BLU) (.+)$/.exec(n);
    return m !== null && names.has(`${m[1] === "RED" ? "BLU" : "RED"} ${m[2]}`) ? m[1] : null;
  };
  const mirrored = zones.some((z) => paired(z.name) !== null);
  // Biggest first, so a small zone inside a big one is drawn on top of it:
  // what wins a position is also what shows.
  const order = zones.map((z, i) => ({ i, a: area(z.points) })).sort((x, y) => y.a - x.a);
  return (
    <g className="co-layer">
      {order.map(({ i }) => {
        const z = zones[i];
        if (z.points.length < 3) return null;
        const pts = z.points.map(px);
        const d = pts.map(([x, y], j) => `${j ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join("") + "Z";
        const cx = pts.reduce((a, p) => a + p[0], 0) / pts.length;
        const cy = pts.reduce((a, p) => a + p[1], 0) / pts.length;
        const side = mirrored ? paired(z.name) : null;
        const hue = side === "RED" ? 2 : side === "BLU" ? 207 : HUES[i % HUES.length];
        const sat = mirrored && side === null ? 8 : 60;
        // With counts, the fill says how busy a zone was; without, every
        // zone is the same wash and the outline does the work.
        const n = counts?.get(i) ?? 0;
        const fill = (counts ? 0.06 + 0.4 * (n / most) : 0.2) * opacity * 2;
        return (
          <g key={i} className={selected === i ? "co-zone selected" : "co-zone"}>
            <path
              d={d}
              fill={`hsla(${hue}, ${sat}%, 55%, ${Math.min(0.85, fill).toFixed(3)})`}
              stroke={selected === i ? "var(--accent)" : `hsla(${hue}, ${sat + 10}%, 70%, ${(0.25 + 0.6 * Math.min(1, opacity * 1.5)).toFixed(2)})`}
              strokeWidth={selected === i ? 2.5 : 1.2}
            />
            {(labels || selected === i) && (
              <text x={cx} y={cy} className="co-label" textAnchor="middle" dominantBaseline="middle">
                {side ? z.name.slice(4) : z.name}
                {counts && n > 0 ? ` ${n}` : ""}
              </text>
            )}
          </g>
        );
      })}
      {selected !== null && zones[selected] && onHandle && (
        <g className="co-handles">
          {zones[selected].points.map((p, j, all) => {
            const [x, y] = px(p);
            const [nx, ny] = px(all[(j + 1) % all.length]);
            return (
              <g key={j}>
                {/* Halfway along each edge: drag it to add a corner there. */}
                <circle
                  cx={(x + nx) / 2}
                  cy={(y + ny) / 2}
                  r={4}
                  className="co-mid"
                  onMouseDown={(e) => {
                    if (e.button !== 0) return;
                    e.stopPropagation();
                    onHandle({ kind: "mid", zone: selected, index: j });
                  }}
                />
                <circle
                  cx={x}
                  cy={y}
                  r={6}
                  className="co-corner"
                  onMouseDown={(e) => {
                    if (e.button !== 0) return;
                    e.stopPropagation();
                    onHandle({ kind: "corner", zone: selected, index: j });
                  }}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    onDeleteCorner?.(selected, j);
                  }}
                />
              </g>
            );
          })}
        </g>
      )}
      {drawing && drawing.length > 0 && (
        <g className="co-drawing">
          <polyline points={drawing.map(px).map(([x, y]) => `${x},${y}`).join(" ")} fill="none" strokeWidth={2} />
          {drawing.map(px).map(([x, y], i) => (
            <circle key={i} cx={x} cy={y} r={3.5} />
          ))}
        </g>
      )}
    </g>
  );
}

/**
 * The editing panel beside the map. It edits the live copy the map draws
 * from (the query's data), so a corner dragged on the map and a name typed
 * here are the same change, saved together.
 */
export function CalloutEditor(props: {
  map: string;
  file: CalloutFile;
  dirty: boolean;
  onChange: (zones: CalloutZone[], names?: string[]) => void;
  onSaved: () => void;
  drawing: Pt[];
  onDrawing: (p: Pt[]) => void;
  drawMode: boolean;
  onDrawMode: (on: boolean) => void;
  selected: number | null;
  onSelect: (i: number | null) => void;
  onClose: () => void;
}) {
  const { map, file, dirty, onChange, onSaved, drawing, onDrawing, drawMode, onDrawMode, selected, onSelect, onClose } = props;
  const qc = useQueryClient();
  const zones = file.zones;
  const names = file.names;
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function finish() {
    const n = name.trim();
    if (drawing.length < 3 || !n) return;
    onChange([...zones, { name: n, points: drawing }], names.filter((x) => x !== n));
    onDrawing([]);
    onDrawMode(false);
    setName("");
    onSelect(zones.length);
  }

  function remove(i: number) {
    const z = zones[i];
    onChange(
      zones.filter((_, j) => j !== i),
      names.includes(z.name) ? names : [...names, z.name],
    );
    onSelect(null);
  }

  function rename(i: number, n: string) {
    onChange(zones.map((z, j) => (j === i ? { ...z, name: n } : z)));
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      const saved = await api.saveCallouts(map, { ...file, zones, names, draft: false });
      qc.setQueryData(["callouts", map], saved);
      void qc.invalidateQueries({ queryKey: ["positions"] });
      onSaved();
    } catch (e) {
      setError(friendlyText(e));
    } finally {
      setBusy(false);
    }
  }

  async function reset() {
    setBusy(true);
    try {
      const back = await api.resetCallouts(map);
      qc.setQueryData(["callouts", map], back);
      onSaved();
      onSelect(null);
    } catch (e) {
      setError(friendlyText(e));
    } finally {
      setBusy(false);
    }
  }

  function discard() {
    if (dirty) void qc.invalidateQueries({ queryKey: ["callouts", map] });
    onSaved();
    onDrawing([]);
    onDrawMode(false);
    onClose();
  }

  const sel = selected !== null ? zones[selected] : null;
  // Alphabetical: the order no longer decides anything.
  const listed = zones.map((z, i) => ({ z, i })).sort((a, b) => a.z.name.localeCompare(b.z.name));

  return (
    <aside className="co-editor">
      <h3>{t("Callouts on {0}", { "0": map })}</h3>

      {drawMode ? (
        <>
          <p className="hint">
            {drawing.length === 0
              ? t("Click the map to place the new zone's corners.")
              : tx("{0} corners. Name it and finish, or keep clicking.", { "0": drawing.length })}
          </p>
          <div className="co-draw">
            <input list="co-names" autoFocus placeholder={t("Name, e.g. Cliff")} value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && finish()} />
            <datalist id="co-names">
              {names.map((n) => (
                <option key={n} value={n} />
              ))}
            </datalist>
            <button className="primary" disabled={drawing.length < 3 || !name.trim()} onClick={finish}>{t("Finish zone")}</button>
            {drawing.length > 0 && <button className="linkish" onClick={() => onDrawing(drawing.slice(0, -1))}>{t("Undo corner")}</button>}
            <button
              className="linkish"
              onClick={() => {
                onDrawing([]);
                onDrawMode(false);
              }}
            >{t("Cancel")}</button>
          </div>
          {names.length > 0 && (
            <div className="co-names">
              <span className="hint">{t("Known, not drawn yet:")}</span>
              {names.map((n) => (
                <button key={n} className="co-chip" onClick={() => setName(n)}>{n}</button>
              ))}
            </div>
          )}
        </>
      ) : (
        <>
          <p className="hint">
            {sel
              ? t("Drag a corner to move it, drag a small dot on an edge to add a corner, right-click a corner to remove it, or drag inside the zone to move all of it.")
              : t("Click a zone on the map to fix its edges.")}
          </p>
          <button
            className="km-chip"
            onClick={() => {
              onSelect(null);
              onDrawMode(true);
            }}
          >{t("Draw a new zone")}</button>
        </>
      )}

      {sel && selected !== null && !drawMode && (
        <div className="co-selected">
          <input value={sel.name} onChange={(e) => rename(selected, e.target.value)} aria-label={t("Zone name")} />
          <span className="hint">{t("{0} corners", { "0": sel.points.length })}</span>
          <button className="linkish" onClick={() => remove(selected)}>{t("Delete zone")}</button>
        </div>
      )}

      <ol className="co-list">
        {listed.map(({ z, i }) => (
          <li
            key={i}
            className={selected === i ? "selected" : undefined}
            onClick={() => {
              onDrawMode(false);
              onSelect(i);
            }}
          >
            {z.name}
          </li>
        ))}
      </ol>
      <div className="co-actions">
        <button className="primary" disabled={busy || !dirty} onClick={() => void save()}>{t("Save")}</button>
        <button disabled={busy} onClick={discard}>{dirty ? t("Discard") : t("Done")}</button>
        {file.origin === "yours" && (
          <button className="linkish" disabled={busy} onClick={() => void reset()}>{t("Back to the built-in callouts")}</button>
        )}
      </div>
      {/* Save first: an export writes what is saved, and an import replaces it. */}
      {!dirty && <CalloutPresetButtons map={map} canExport={file.origin !== "none"} canUndo={false} />}
      {error && <p className="error">{error}</p>}
      <p className="hint co-where">{t("Where zones overlap, the smaller one wins. Saved to the callouts folder in the app's data folder, one file per map, never overwritten by an update.")}</p>
    </aside>
  );
}
