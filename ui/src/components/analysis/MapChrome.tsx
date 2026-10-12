import { useEffect, useRef, useState, type ReactNode } from "react";
import { t } from "../../lib/i18n";

/**
 * The kill map's controls (Flashy's UX pass): a pill toggle for the map's own
 * modes, the zoom and full screen buttons floating on the map, the callouts
 * menu, and a legend whose entries switch their layer on and off.
 */

/** A choice of one, pill-shaped: kept apart from the page's underlined tabs,
 *  which choose what the panel shows rather than how the map draws it. */
export function Seg<T extends string>(props: {
  label: string;
  value: T;
  onChange: (v: T) => void;
  options: Array<{ v: T; label: ReactNode; title?: string; disabled?: boolean }>;
}) {
  const { label, value, onChange, options } = props;
  return (
    <div className="km-seg" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.v}
          role="radio"
          aria-checked={value === o.v}
          className={value === o.v ? "on" : undefined}
          title={o.title}
          disabled={o.disabled}
          onClick={() => onChange(o.v)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

const PATHS = {
  plus: "M8 3.5v9M3.5 8h9",
  minus: "M3.5 8h9",
  fit: "M3 6.5V3h3.5M9.5 3H13v3.5M13 9.5V13H9.5M6.5 13H3V9.5",
  full: "M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10",
  exitFull: "M6 2.5V6H2.5M13.5 6H10V2.5M10 13.5V10h3.5M2.5 10H6v3.5",
  layers: "M8 2.5 13.5 5.25 8 8 2.5 5.25zM2.5 8 8 10.75 13.5 8M2.5 10.75 8 13.5l5.5-2.75",
  caret: "M5 6.5 8 9.5l3-3",
  up: "M4 10 8 6l4 4",
  down: "M4 6l4 4 4-4",
};

export function Icon({ name, size = 16 }: { name: keyof typeof PATHS; size?: number }) {
  return (
    <svg viewBox="0 0 16 16" width={size} height={size} fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={PATHS[name]} />
    </svg>
  );
}

/** Zoom in, out and back to the whole map, and full screen: a stack in the
 *  map's top right corner, where map apps keep them. */
export function ZoomControls(props: {
  z: number;
  max: number;
  onIn: () => void;
  onOut: () => void;
  onFit: () => void;
  full: boolean;
  onFull: () => void;
}) {
  const { z, max, onIn, onOut, onFit, full, onFull } = props;
  return (
    <div className="km-ov km-ov-tr">
      <div className="km-ovstack" role="group" aria-label={t("Zoom")} title={t("Scroll on the map to zoom in; drag to move around.")}>
        <button onClick={onIn} disabled={z >= max} aria-label={t("Zoom in")} title={t("Zoom in")}>
          <Icon name="plus" />
        </button>
        <button onClick={onOut} disabled={z <= 1} aria-label={t("Zoom out")} title={t("Zoom out")}>
          <Icon name="minus" />
        </button>
        <button onClick={onFit} disabled={z === 1} aria-label={t("Whole map")} title={t("Whole map")}>
          <Icon name="fit" />
        </button>
      </div>
      <div className="km-ovstack">
        <button
          onClick={onFull}
          aria-pressed={full}
          aria-label={full ? t("Exit full screen") : t("Full screen")}
          title={full ? t("Leave full screen (Escape)") : t("Fill the window with the map")}
        >
          <Icon name={full ? "exitFull" : "full"} />
        </button>
      </div>
      {z > 1 && <span className="km-ov-pill">{Math.round(z * 100)}%</span>}
    </div>
  );
}

/** A button on the map that opens a small panel under it; a click elsewhere
 *  or Escape closes it. */
export function MapMenu({ label, on, children }: { label: string; on: boolean; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    // Captured first, so Escape closes the menu without also dropping a
    // picked life or leaving full screen.
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopImmediatePropagation();
      e.stopPropagation();
      setOpen(false);
    };
    document.addEventListener("mousedown", down);
    document.addEventListener("keydown", key, true);
    return () => {
      document.removeEventListener("mousedown", down);
      document.removeEventListener("keydown", key, true);
    };
  }, [open]);
  return (
    <div className="km-menu" ref={ref}>
      <button className={open ? "km-ovbtn open" : "km-ovbtn"} aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <Icon name="layers" />
        {label}
        <span className={on ? "km-state on" : "km-state"} aria-hidden />
        <Icon name="caret" size={14} />
      </button>
      {open && (
        <div className="km-menu-panel" role="dialog" aria-label={label}>
          {children}
        </div>
      )}
    </div>
  );
}

/** An on/off switch with its label, for the menus. */
export function Switch({ on, onChange, disabled, children }: { on: boolean; onChange: () => void; disabled?: boolean; children: ReactNode }) {
  return (
    <button role="switch" aria-checked={on} className="km-switch" onClick={onChange} disabled={disabled}>
      <span>{children}</span>
      <span className="km-switch-track" aria-hidden>
        <span className="km-switch-knob" />
      </span>
    </button>
  );
}

/** One legend entry, which is also its layer's switch: click to hide what it
 *  stands for, again to bring it back. */
export function LegendChip(props: { on: boolean; onToggle: () => void; swatch: ReactNode; label: ReactNode; count?: number }) {
  const { on, onToggle, swatch, label, count } = props;
  return (
    <button className={on ? "km-lg" : "km-lg off"} aria-pressed={on} onClick={onToggle} title={on ? t("Click to hide") : t("Click to show")}>
      <span className="km-lg-sw">{swatch}</span>
      <span>{label}</span>
      {count !== undefined && <span className="km-lg-n">{count}</span>}
    </button>
  );
}
