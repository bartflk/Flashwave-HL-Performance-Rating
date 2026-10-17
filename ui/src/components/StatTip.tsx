import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { GLOSSARY, type GlossaryKey } from "../lib/glossary";
import { goTo } from "../lib/goto";
import { t } from "../lib/i18n";

/**
 * A statistic that explains itself (Q80): hover or focus it for what it
 * means, how it is worked out and where it comes from, with a way to the
 * How ratings work page for the long answer.
 *
 * The card floats above the page (drawn at the end of the document), so a
 * panel that clips its overflow -- a profile's header, a scrolling table --
 * never cuts it off; it is kept inside the window's edges.
 */
export function StatTip({ k: key, children, inline }: { k: GlossaryKey; children?: ReactNode; inline?: boolean }) {
  const e = GLOSSARY[key];
  const more = "more" in e ? e.more : undefined;
  const anchor = useRef<HTMLSpanElement>(null);
  const card = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const hide = useRef<number | null>(null);

  const show = () => {
    if (hide.current) window.clearTimeout(hide.current);
    setOpen(true);
  };
  // A moment's grace, so the pointer can cross from the label to the card.
  const leave = () => {
    hide.current = window.setTimeout(() => setOpen(false), 120);
  };

  useLayoutEffect(() => {
    if (!open || !anchor.current || !card.current) return;
    const a = anchor.current.getBoundingClientRect();
    const c = card.current.getBoundingClientRect();
    const left = Math.max(8, Math.min(a.left - 8, window.innerWidth - c.width - 8));
    const below = a.bottom + 6;
    const top = below + c.height > window.innerHeight - 8 ? Math.max(8, a.top - c.height - 6) : below;
    setPos({ left, top });
  }, [open]);

  return (
    <span
      ref={anchor}
      className={inline ? "stip inline" : "stip"}
      tabIndex={0}
      aria-describedby={open ? `stip-${key}` : undefined}
      onMouseEnter={show}
      onMouseLeave={leave}
      onFocus={show}
      onBlur={leave}
      onKeyDown={(ev) => ev.key === "Escape" && setOpen(false)}
    >
      {children ?? t(e.label)}
      <span className="stip-mark" aria-hidden>
        ?
      </span>
      {open &&
        createPortal(
          <span
            ref={card}
            className="stip-card"
            role="tooltip"
            id={`stip-${key}`}
            style={pos ? { left: pos.left, top: pos.top } : { left: -9999, top: 0 }}
            onMouseEnter={show}
            onMouseLeave={leave}
          >
            <strong>{t(e.label)}</strong>
            <span className="stip-what">{t(e.what)}</span>
            <span className="stip-how">{t(e.how)}</span>
            <span className="stip-foot">
              {e.source.map((s) => (
                <span key={s} className="stip-src">
                  {s}
                </span>
              ))}
              {more && (
                <button
                  className="linkish stip-more"
                  onMouseDown={(ev) => ev.preventDefault()}
                  onClick={(ev) => {
                    ev.stopPropagation();
                    setOpen(false);
                    goTo("rating", more);
                  }}
                >
                  {t("How it's decided")} ›
                </button>
              )}
            </span>
          </span>,
          document.body,
        )}
    </span>
  );
}
