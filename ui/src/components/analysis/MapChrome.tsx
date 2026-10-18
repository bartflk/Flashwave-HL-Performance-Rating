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
  // Lives on a map's controls (Flashy: "add some icons to differentiate
  // what buttons do what").
  auto: "M8 2.5l1.3 3.2 3.2 1.3-3.2 1.3L8 11.5 6.7 8.3 3.5 7l3.2-1.3zM12.5 11v2.5M11.25 12.25h2.5",
  routes: "M2.5 13C5.5 13 4.5 3 8 3s2.5 10 5.5 10M2.5 8.5c1.5 0 2-2 3.2-2",
  grid: "M3 3h4v4H3zM9 3h4v4H9zM3 9h4v4H3zM9 9h4v4H9z",
  skull: "M8 2.5a4.5 4.5 0 0 0-4.5 4.5c0 1.6.8 2.6 1.8 3.2V12h5.4v-1.8c1-.6 1.8-1.6 1.8-3.2A4.5 4.5 0 0 0 8 2.5zM6.2 7.4h.1M9.7 7.4h.1M7.1 12v1.5M8.9 12v1.5",
  flag: "M4 13.5V2.5M4 3h7.5l-1.8 2.5L11.5 8H4",
  any: "M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11z",
  cap: "M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11zM8 5.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5z",
  afterCap: "M12.5 8A4.5 4.5 0 1 1 11 4.6M12.6 2.4v2.8H9.8",
  swords: "M3 3l7.5 7.5M9.5 13l3.5-3.5M11.25 11.25 13.5 13.5M13 3 5.5 10.5M6.5 13 3 9.5M4.75 11.25 2.5 13.5",
  history: "M8 3a5 5 0 1 1-4.6 3M3 2.8v3.2h3.2M8 5.6V8l1.7 1.1",
  both: "M8 2.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11zM8 2.5v11",
  check: "M3.5 8.5 6.5 11.5 12.5 4.5",
  close: "M4.5 4.5l7 7M11.5 4.5l-7 7",
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
