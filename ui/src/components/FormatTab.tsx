import type { ReactNode } from "react";
import { t } from "../lib/i18n";
import { setTabFormat, type FormatsPlayed, type FormatTab as Id, type GameFormat } from "../lib/gameformat";

// Each word is its own t() call, so the translation script finds it.
const name = (f: GameFormat) => (f === "6s" ? t("6v6") : t("Highlander"));
const size = (f: GameFormat) => (f === "6s" ? t("6 a side") : t("9 a side"));
const chip = (f: GameFormat) => (f === "6s" ? t("6s") : t("HL"));

/**
 * A header tab that has more than one format (the 6v6 plan, P0): Matches,
 * Profile, ETF2L. Hovering it, or moving focus into it, opens a small menu
 * of the formats you play; clicking the tab itself opens the one it showed
 * last. A chip says which format the tab is on. With one format played it
 * is a plain tab, as before 6v6.
 */
export function FormatTab(props: {
  id: Id;
  label: string;
  icon?: ReactNode;
  active: boolean;
  fmt: GameFormat;
  played: FormatsPlayed;
  onOpen: () => void;
}) {
  const { id, label, icon, active, fmt, played, onOpen } = props;
  const plain = (
    <button className={active ? "tab active" : "tab"} onClick={onOpen}>
      {icon}
      {label}
      {played.multi && <span className={fmt === "6s" ? "fmt-chip six" : "fmt-chip"}>{chip(fmt)}</span>}
    </button>
  );
  if (!played.multi) return plain;
  return (
    <div className="tab-fmt">
      {plain}
      <div className="tab-fmt-menu" role="menu" aria-label={label}>
        {played.formats.map((f) => (
          <button
            key={f}
            role="menuitemradio"
            aria-checked={f === fmt}
            className={f === fmt ? "tab-fmt-item on" : "tab-fmt-item"}
            onClick={() => {
              setTabFormat(id, f);
              onOpen();
              // The menu shows while focus is inside it; a choice closes it.
              if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
            }}
          >
            <span className="tab-fmt-name">{name(f)}</span>
            <span className="tab-fmt-size">{size(f)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
