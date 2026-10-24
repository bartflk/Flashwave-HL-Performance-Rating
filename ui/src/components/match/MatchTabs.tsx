import { useEffect, useState } from "react";
import { t, k } from "../../lib/i18n";
import { openFold } from "../Fold";

/** The match page's sections, in page order, by their Fold ids. */
const SECTIONS: Array<[string, string]> = [
  ["scoreboard", k("Scoreboard")],
  ["matchups", k("Matchups")],
  ["h2h", k("Head to head")],
  ["demos", k("Demos")],
  ["spychecks", k("Spychecks")],
  ["reflects", k("Reflects")],
  ["cart", k("Cart")],
  ["positions", k("Positions")],
  ["rounds", k("Rounds")],
  ["analysis", k("Kill by kill")],
];

/**
 * Q82 (Flashy): a sticky row of link tabs under the match's header, one per
 * section the page actually drew, the one in view marked. A click opens the
 * section if it was folded and scrolls to it.
 */
export function MatchTabs() {
  const [present, setPresent] = useState<string[]>([]);
  const [active, setActive] = useState<string | null>(null);

  // Which sections drew a panel: some draw nothing without a demo or a
  // payload map, and they load at their own pace.
  useEffect(() => {
    const page = document.querySelector(".match-page");
    if (!page) return;
    const scan = () => {
      const ids = SECTIONS.map(([id]) => id).filter((id) => document.getElementById(`sec-${id}`)?.querySelector(".panel"));
      setPresent((old) => (old.join() === ids.join() ? old : ids));
    };
    scan();
    const mo = new MutationObserver(scan);
    mo.observe(page, { childList: true, subtree: true });
    return () => mo.disconnect();
  }, []);

  // The section in view: the last one whose top has passed under the tabs.
  useEffect(() => {
    const onScroll = () => {
      let cur: string | null = present[0] ?? null;
      for (const id of present) {
        const el = document.getElementById(`sec-${id}`);
        if (el && el.getBoundingClientRect().top < 140) cur = id;
      }
      setActive(cur);
    };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true, capture: true });
    return () => window.removeEventListener("scroll", onScroll, { capture: true });
  }, [present]);

  if (present.length < 2) return null;
  return (
    <nav className="pf-tabs mx-tabs" aria-label={t("Match sections")}>
      {SECTIONS.filter(([id]) => present.includes(id)).map(([id, label]) => (
        <button
          key={id}
          className={active === id ? "pf-tab on" : "pf-tab"}
          aria-current={active === id ? "true" : undefined}
          onClick={() => {
            openFold(id);
            document.getElementById(`sec-${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
          }}
        >
          {t(label)}
        </button>
      ))}
    </nav>
  );
}
