import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type ContextKind, type MatchContext } from "../api/types";
import { t } from "../lib/i18n";
import { Etf2lMark } from "./Etf2lMark";

export const KIND_LABEL: Record<ContextKind, string> = {
  official: "Official",
  scrim: "Scrim",
  pug: "Pug",
};

export const KIND_PLURAL: Record<ContextKind, string> = {
  official: "Officials",
  scrim: "Scrims",
  pug: "Pugs",
};

/** Why a match got its kind, for a hover title. */
export function kindReason(c: MatchContext): string {
  if (c.linkMethod === "manual") return t("Set by hand. Click to change it, or to give it back to the app.");
  switch (c.kind) {
    case "official":
      return c.linkMethod === "roster"
        ? t("ETF2L official, found by matching both teams' rosters (trends.tf had not tagged it)")
        : t("ETF2L official, tagged by trends.tf");
    case "scrim":
      return c.regulars >= 5
        ? t("Team game: {n} of your teammates played with you regularly around then", { n: c.regulars })
        : t("Team game: most of your side was on your ETF2L roster");
    case "pug":
      return c.regulars === 1
        ? t("Pug or lobby: 1 regular teammate on your side")
        : t("Pug or lobby: {n} regular teammates on your side", { n: c.regulars });
  }
}

/** Short division label: "High", "Div 2", "Open". */
export function divisionLabel(division: string | null): string | null {
  if (!division) return null;
  return division.replace(/^Division\s+/i, "Div ");
}

export function ContextBadge({ c }: { c: MatchContext }) {
  const div = c.kind === "official" ? divisionLabel(c.official?.division ?? null) : null;
  return (
    <span className={`badge badge-${c.kind}${c.linkMethod === "manual" ? " badge-manual" : ""}`} title={kindReason(c)}>
      {c.kind === "official" && <Etf2lMark size={12} />}
      {c.kind === "official" ? t("ETF2L") : KIND_LABEL[c.kind].toUpperCase()}
      {div && <span className="badge-sub"> · {div}</span>}
      {c.linkMethod === "manual" && <span className="badge-hand" aria-label={t("set by hand")}> ✎</span>}
    </span>
  );
}

/**
 * Q63 (Flashy): the badge as a menu -- set the match's kind by hand when the
 * app got it wrong, or give it back to the app. Kept through every sync.
 */
export function KindPicker({ logId, c, children }: { logId: number; c: MatchContext; children?: React.ReactNode }) {
  const qc = useQueryClient();
  // Where the menu opens, in the window: fixed, so a table cell that clips
  // its overflow (the match list's) cannot cut it off.
  const [open, setOpen] = useState<{ top: number; left: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const box = useRef<HTMLSpanElement>(null);
  const menu = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      const t = e.target as Node;
      if (box.current && !box.current.contains(t) && !menu.current?.contains(t)) setOpen(null);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(null);
    // A fixed menu would float away from its badge on a scroll: close it.
    const scrolled = () => setOpen(null);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    window.addEventListener("scroll", scrolled, true);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
      window.removeEventListener("scroll", scrolled, true);
    };
  }, [open]);

  function toggle(e: React.MouseEvent<HTMLButtonElement>) {
    if (open) return setOpen(null);
    const r = e.currentTarget.getBoundingClientRect();
    // Kept on screen: 200 px is the menu's width, 190 its height.
    const left = Math.max(8, Math.min(r.left, window.innerWidth - 208));
    const top = r.bottom + 190 > window.innerHeight ? Math.max(8, r.top - 196) : r.bottom + 6;
    setOpen({ top, left });
  }

  async function pick(kind: ContextKind | null) {
    setError(null);
    try {
      await api.setMatchKind(logId, kind);
      setOpen(null);
      // The kind feeds the list, its filters and counts, the match page and
      // every split by kind on the profiles: refresh the lot.
      void qc.invalidateQueries();
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  const manual = c.linkMethod === "manual";
  return (
    <span className="kind-pick" ref={box} onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
      <button
        type="button"
        className="kind-pick-btn"
        onClick={toggle}
        aria-expanded={open !== null}
        aria-haspopup="menu"
        title={`${kindReason(c)}\n${t("Click to change what kind of match this is.")}`}
      >
        {children ?? <ContextBadge c={c} />}
      </button>
      {/* On the page itself: inside the match header (a layer of its own,
          for its map backdrop) the sticky section tabs drew over it. */}
      {open &&
        createPortal(
        <span className="kind-pick-menu" role="menu" ref={menu} style={{ top: open.top, left: open.left }} onClick={(e) => e.stopPropagation()}>
          <span className="kind-pick-head">{t("What kind of match is this?")}</span>
          {(["official", "scrim", "pug"] as ContextKind[]).map((k) => (
            <button key={k} role="menuitemradio" aria-checked={c.kind === k} className={c.kind === k ? "on" : ""} onClick={() => void pick(k)}>
              {c.kind === k ? "● " : "○ "}
              {t(KIND_LABEL[k])}
            </button>
          ))}
          <button role="menuitem" className="kind-pick-auto" disabled={!manual} onClick={() => void pick(null)} title={manual ? t("Let the app decide again") : t("The app is already deciding")}>
            {t("Automatic")}
          </button>
          {error && <span className="error">{error}</span>}
        </span>,
          document.body,
        )}
    </span>
  );
}
