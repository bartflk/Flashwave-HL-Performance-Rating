import type { ReactNode } from "react";
import { useOneLine } from "../lib/oneline";

/**
 * A league tab's header (ETF2L, RGL): the title, the menu and the search on
 * one line where they fit, and otherwise the title on a line of its own with
 * the menu and the search under it (useOneLine). The parts are the children
 * marked `data-fit`.
 */
export function LeagueHead({ children }: { children: ReactNode }) {
  const [ref, stacked] = useOneLine(24);
  return (
    <header ref={ref} className={stacked ? "e2-head stacked" : "e2-head"}>
      {children}
    </header>
  );
}
