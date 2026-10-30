import { useEffect, useRef, useState } from "react";
import { friendlyText } from "../lib/apierror";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import { type MapRow, type MapView, type Placement } from "../api/types";
import { capitalize, splitMap } from "../lib/format";
import { t } from "../lib/i18n";
import { themeColour } from "./analysis/common";
import { Loading } from "./Spinner";

/**
 * Lining a top-down image up over the kills (Q31).
 *
 * The image fills the frame; every place a kill or death was ever stored on
 * this map is drawn on top as a dot, where the placement says it lands.
 * Drag the dots across the image, scroll (or the buttons) to spread them
 * out or pull them in. When they sit on floors, balconies and doorways, it
 * is lined up. The placement
 * is saved beside the image and wins over the built-in one.
 */
export function MapAligner({ row, onClose, onSaved }: { row: MapRow; onClose: () => void; onSaved: () => void }) {
  const img = useQuery({ queryKey: ["overviewImage", row.base], queryFn: () => api.overviewImage(row.name), staleTime: 0 });
  const view = useQuery({ queryKey: ["mapview", row.name], queryFn: () => api.getMapView(row.name), staleTime: 5 * 60_000 });
  const [p, setP] = useState<Placement | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Start from the saved placement, else the square around the kills.
  useEffect(() => {
    if (p || !img.data) return;
    if (img.data.placement) setP(img.data.placement);
    else if (view.data) setP(guess(view.data, img.data.aspect));
  }, [img.data, view.data, p]);

  const title = capitalize(splitMap(row.name).name ?? row.base);
  const save = async () => {
    if (!p) return;
    setBusy(true);
    setError(null);
    try {
      await api.saveOverviewPlacement(row.base, p);
      onSaved();
    } catch (e) {
      setError(friendlyText(e));
      setBusy(false);
    }
  };

  let body;
  if (img.isPending || view.isPending) body = <Loading>{t("Loading…")}</Loading>;
  else if (!img.data) body = <p className="hint">{t("This map has no image to line up. Import one first.")}</p>;
  else if (!view.data) body = <p className="hint">{t("No kills are stored on this map yet, so there is nothing to line the image up against.")}</p>;
  else if (p)
    body = (
      <>
        <p className="hint">{t("Drag the dots -- every place a kill or death happened on this map -- until they sit on floors, balconies and doorways. Scroll to spread them out or pull them together.")}</p>
        <AlignCanvas image={img.data.image} aspect={img.data.aspect} view={view.data} p={p} onChange={setP} />
        <div className="row" style={{ gap: 8, marginTop: 8, flexWrap: "wrap" }}>
          <button onClick={() => setP(scaleAbout(p, 1 / 1.05))}>{t("Spread out")}</button>
          <button onClick={() => setP(scaleAbout(p, 1.05))}>{t("Pull together")}</button>
          <button onClick={() => setP(guess(view.data!, img.data!.aspect))}>{t("Start again")}</button>
        </div>
      </>
    );

  return (
    <div className="modal-scrim" role="dialog" aria-modal="true" aria-label={t("Line up {map}", { map: title })}>
      <div className="modal map-aligner">
        <h3>{t("Line up {map}", { map: title })}</h3>
        {body}
        {error && <p className="error">{error}</p>}
        <div className="modal-actions">
          <button className="primary" onClick={() => void save()} disabled={busy || !p}>
            {busy ? t("Saving…") : t("Save")}
          </button>
          <button onClick={onClose} disabled={busy}>{t("Cancel")}</button>
        </div>
      </div>
    </div>
  );
}

/** The square (or the image's shape) around every stored position. */
function guess(v: MapView, aspect: number): Placement {
  const w = v.width * v.cell;
  const h = v.height * v.cell;
  const size = Math.max(w, h / aspect) * 1.05;
  const cx = v.minX + w / 2;
  const cy = v.maxY - h / 2;
  return { minX: cx - size / 2, maxY: cy + (size * aspect) / 2, size };
}

/** Bigger or smaller about the image's middle. */
function scaleAbout(p: Placement, f: number): Placement {
  const cx = p.minX + p.size / 2;
  const cy = p.maxY - p.size / 2;
  const size = p.size * f;
  return { minX: cx - size / 2, maxY: cy + size / 2, size };
}

function AlignCanvas({ image, aspect, view, p, onChange }: { image: string; aspect: number; view: MapView; p: Placement; onChange: (p: Placement) => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [img, setImg] = useState<HTMLImageElement | null>(null);
  const drag = useRef<{ x: number; y: number; p: Placement } | null>(null);
  // As wide as the dialog allows, and no taller than the window.
  const W = Math.round(Math.min(640, (window.innerHeight - 260) / aspect, window.innerWidth - 96));
  const H = Math.round(W * aspect);
  const s = W / p.size; // pixels per game unit

  useEffect(() => {
    const i = new Image();
    i.onload = () => setImg(i);
    i.src = image;
  }, [image]);

  useEffect(() => {
    const c = canvas.current;
    const g = c?.getContext("2d");
    if (!c || !g) return;
    g.clearRect(0, 0, W, H);
    if (img) g.drawImage(img, 0, 0, W, H);
    const max = Math.max(1, ...view.occupancy);
    g.fillStyle = themeColour("--accent", "#e8a33d");
    const dot = Math.max(1.5, view.cell * s);
    for (let i = 0; i < view.occupancy.length; i++) {
      const n = view.occupancy[i];
      if (n === 0) continue;
      const x = view.minX + ((i % view.width) + 0.5) * view.cell;
      const y = view.maxY - (Math.floor(i / view.width) + 0.5) * view.cell;
      g.globalAlpha = 0.25 + 0.6 * (Math.log1p(n) / Math.log1p(max));
      g.fillRect((x - p.minX) * s - dot / 2, (p.maxY - y) * s - dot / 2, dot, dot);
    }
    g.globalAlpha = 1;
  }, [img, view, p, W, H, s]);

  // Scroll to scale about the pointer: the spot under it stays put.
  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = c.getBoundingClientRect();
      const gx = p.minX + (e.clientX - r.left) / s;
      const gy = p.maxY - (e.clientY - r.top) / s;
      const f = e.deltaY < 0 ? 1 / 1.03 : 1.03;
      const size = p.size * f;
      const s2 = W / size;
      onChange({ minX: gx - (e.clientX - r.left) / s2, maxY: gy + (e.clientY - r.top) / s2, size });
    };
    c.addEventListener("wheel", onWheel, { passive: false });
    return () => c.removeEventListener("wheel", onWheel);
  }, [p, s, W, onChange]);

  return (
    <canvas
      ref={canvas}
      className="align-canvas"
      width={W}
      height={H}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { x: e.clientX, y: e.clientY, p };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        // The dots follow the pointer across the image.
        onChange({ ...d.p, minX: d.p.minX - (e.clientX - d.x) / s, maxY: d.p.maxY + (e.clientY - d.y) / s });
      }}
      onPointerUp={() => (drag.current = null)}
    />
  );
}
