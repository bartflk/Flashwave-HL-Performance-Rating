import { useEffect, useRef, useState } from "react";
import { useFormatsPlayed } from "../lib/gameformat";
import { openSettingsSection } from "../lib/goto";
import { copyReport } from "../lib/report";
import { openRatingGuide } from "../lib/sixesguide";
import { t } from "../lib/i18n";

/**
 * The top bar's "?" (Flashy: "can this page be put somewhere thats easier to
 * see ... like in a dropdown in the main menu"): How ratings work for each
 * format, the activity log, and a problem report to paste. The bookmarks
 * menu's look, so the two read as one row.
 */
export function HelpMenu() {
  const { sixesEnabled } = useFormatsPlayed();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);

  // A click anywhere else, or Escape, closes it.
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  const item = (label: string, sub: string, run: () => void) => (
    <li key={label}>
      <button
        className="bm-go"
        onClick={() => {
          setOpen(false);
          run();
        }}
      >
        <span className="bm-label">{label}</span>
        <span className="hint bm-sub">{sub}</span>
      </button>
    </li>
  );

  return (
    <div className="bm-menu" ref={box}>
      <button className={open ? "bm-open help-open on" : "bm-open help-open"} onClick={() => setOpen((o) => !o)} aria-expanded={open} title={t("Help")} aria-label={t("Help")}>
        ?
      </button>
      {open && (
        <div className="bm-pop help-pop" role="dialog" aria-label={t("Help")}>
          <h3>{t("Help")}</h3>
          <section>
            <h4>{t("How ratings work")}</h4>
            <ul>
              {item(sixesEnabled ? t("Highlander") : t("How ratings work"), t("What each class is rated on, and how a game becomes a number"), () => openRatingGuide("hl"))}
              {sixesEnabled && item(t("6v6"), t("What each role is rated on, and what is not measured yet"), () => openRatingGuide("6s"))}
            </ul>
          </section>
          <section>
            <h4>{t("Something wrong?")}</h4>
            <ul>
              {item(t("Copy a problem report"), t("The app's details and the log's last lines, ready to paste on Discord"), () => void copyReport(null))}
              {item(t("Activity log"), t("Every request and error, live, with the log file"), () => openSettingsSection("activity"))}
            </ul>
          </section>
        </div>
      )}
    </div>
  );
}
