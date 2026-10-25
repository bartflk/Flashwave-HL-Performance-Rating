import { useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import { GLOSSARY, type Entry, type GlossaryKey } from "../lib/glossary";
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
export function StatTip({ k: key, entry, children, inline }: { k?: GlossaryKey; entry?: Entry; children?: ReactNode; inline?: boolean }) {
  // A glossary entry, or one written where the number is (a table's own
  // columns: Fights, Aim, the scoreboard).
  const e: Entry = entry ?? GLOSSARY[key ?? "rating"];
  const more = "more" in e ? e.more : undefined;
  return (
    <Tip
      className={inline ? "stip inline" : "stip"}
      label={children ?? t(e.label)}
      card={(close) => (
        <>
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
                  close();
                  goTo("rating", more);
                }}
              >
                {t("How it's decided")} ›
              </button>
            )}
          </span>
        </>
      )}
    />
  );
}

/**
 * A section's explanation behind its help mark (Flashy: "most of the
 * descriptions can be removed, and if it's really needed, put them into the
 * questionmark icon instead"): the mark alone beside a heading, the text on
 * hover or focus. A click on it never folds the section it sits in.
 */
export function Help({ children, title }: { children: ReactNode; title?: string }) {
  return <Tip className="stip help" label={null} ariaLabel={title ?? t("What this shows")} card={() => <span className="stip-what">{children}</span>} />;
}

/** The help mark: a ring with a question mark, drawn so it sits centred at
 *  any size (the text "?" in a bordered circle never did). */
export function HelpIcon() {
  return (
    <svg className="stip-mark" viewBox="0 0 16 16" width="14" height="14" aria-hidden focusable="false">
      <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M6.1 6.2a1.95 1.95 0 0 1 3.8.55c0 1.3-1.9 1.6-1.9 2.75" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
      <circle cx="8" cy="11.6" r="0.85" fill="currentColor" />
    </svg>
  );
}

function Tip({ className, label, ariaLabel, card: body }: { className: string; label: ReactNode; ariaLabel?: string; card: (close: () => void) => ReactNode }) {
  const id = useId();
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
      className={className}
      tabIndex={0}
      aria-label={ariaLabel}
      aria-describedby={open ? id : undefined}
      onClick={label === null ? (ev) => ev.stopPropagation() : undefined}
      onMouseEnter={show}
      onMouseLeave={leave}
      onFocus={show}
      onBlur={leave}
      onKeyDown={(ev) => ev.key === "Escape" && setOpen(false)}
    >
      {label}
      <HelpIcon />
      {open &&
        createPortal(
          <span
            ref={card}
            className="stip-card"
            role="tooltip"
            id={id}
            style={pos ? { left: pos.left, top: pos.top } : { left: -9999, top: 0 }}
            onMouseEnter={show}
            onMouseLeave={leave}
          >
            {body(() => setOpen(false))}
          </span>,
          document.body,
        )}
    </span>
  );
}

/**
 * A rating component (kills in context, untraded deaths, ...) explained from
 * the model's own description of it -- the same text the How ratings work
 * page shows, written next to the component in Rust.
 */
export function ComponentTip({ component, label, weight, children }: { component: string; label: string; weight?: number; children?: ReactNode }) {
  const q = useQuery({ queryKey: ["rating_guide"], queryFn: api.getRatingGuide, staleTime: Infinity });
  const info = q.data?.classes.flatMap((c) => c.components).find((c) => c.key === component);
  const entry: Entry = {
    label,
    what: info?.description ?? label,
    how:
      weight !== undefined
        ? t("{0}% of this class's rating. Each game's number becomes a percentile against every game on the class in the pool; where fewer is better it is flipped.", { "0": Math.round(weight * 100) })
        : t("Each game's number becomes a percentile against every game on the class in the pool; where fewer is better it is flipped."),
    source: info?.source === "serverLog" ? ["server log"] : ["logs.tf"],
    more: "rating",
  };
  return <StatTip entry={entry}>{children ?? label}</StatTip>;
}
