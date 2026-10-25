import { useEffect, useRef, useState } from "react";
import { t } from "../lib/i18n";

/** A column the viewer can turn on or off. `label` is the table's heading,
 *  `name` what it is in words; `on` whether it shows before any choice. */
export type PickerCol = { key: string; label: string; name: string; group?: string; on?: boolean };

/**
 * Which of a table's columns this viewer shows (Flashy: "add the columns
 * button to the other tables too ... i want all the stats possible"),
 * remembered per table. Only the columns someone ticked or unticked are
 * stored, so a column added later starts as its default.
 */
export function useColumns(id: string, cols: PickerCol[]) {
  const storeKey = `hl.cols.${id}`;
  const [picked, setPicked] = useState<Record<string, boolean>>(() => {
    try {
      const s = localStorage.getItem(storeKey);
      if (s) return JSON.parse(s) as Record<string, boolean>;
    } catch {
      // Blocked or broken storage: the defaults.
    }
    return {};
  });
  const save = (next: Record<string, boolean>) => {
    setPicked(next);
    try {
      localStorage.setItem(storeKey, JSON.stringify(next));
    } catch {
      // Blocked storage: the choice holds for this session.
    }
  };
  const shown = (c: PickerCol) => picked[c.key] ?? c.on !== false;
  return {
    shown,
    toggle: (c: PickerCol) => save({ ...picked, [c.key]: !shown(c) }),
    reset: () => save({}),
    showAll: () => save(Object.fromEntries(cols.map((c) => [c.key, true]))),
  };
}

/** The Columns button and its checklist, grouped, for a table's header. */
export function ColumnPicker({ cols, state }: { cols: PickerCol[]; state: ReturnType<typeof useColumns> }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  const groups: Array<[string, PickerCol[]]> = [];
  for (const c of cols) {
    const g = c.group ?? "";
    const at = groups.find(([name]) => name === g);
    if (at) at[1].push(c);
    else groups.push([g, [c]]);
  }
  const count = cols.filter(state.shown).length;

  return (
    <div className="cp" ref={box}>
      <button className={open ? "secondary cp-open on" : "secondary cp-open"} aria-expanded={open} onClick={() => setOpen((o) => !o)} title={t("Pick the table's columns")}>
        <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden focusable="false">
          <path d="M2 4h12M2 8h12M2 12h12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          <circle cx="5" cy="4" r="1.8" fill="var(--raised)" stroke="currentColor" strokeWidth="1.4" />
          <circle cx="11" cy="8" r="1.8" fill="var(--raised)" stroke="currentColor" strokeWidth="1.4" />
          <circle cx="7" cy="12" r="1.8" fill="var(--raised)" stroke="currentColor" strokeWidth="1.4" />
        </svg>
        {t("Columns")}
      </button>
      {open && (
        <div className="cp-pop" role="dialog" aria-label={t("Columns")}>
          <header className="cp-head">
            <strong>{t("Columns")}</strong>
            <span className="cp-count">{t("{0} of {1} shown", { "0": count, "1": cols.length })}</span>
          </header>
          <div className="cp-groups">
            {groups.map(([g, list]) => (
              <section key={g} className="cp-group">
                {g && <h4>{t(g)}</h4>}
                <div className="cp-items">
                  {list.map((c) => {
                    const on = state.shown(c);
                    return (
                      <button key={c.key} className={on ? "cp-item on" : "cp-item"} role="menuitemcheckbox" aria-checked={on} onClick={() => state.toggle(c)}>
                        <span className="cp-box" aria-hidden>
                          <svg viewBox="0 0 12 12" width="10" height="10">
                            <path d="M2.2 6.3l2.4 2.4 5-5.2" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
                          </svg>
                        </span>
                        <span className="cp-label">{t(c.label)}</span>
                        <span className="cp-name">{t(c.name)}</span>
                      </button>
                    );
                  })}
                </div>
              </section>
            ))}
          </div>
          <footer className="cp-foot">
            <button className="secondary cp-small" onClick={state.showAll}>
              {t("Show all")}
            </button>
            <button className="secondary cp-small" onClick={state.reset}>
              {t("Back to the defaults")}
            </button>
          </footer>
        </div>
      )}
    </div>
  );
}
