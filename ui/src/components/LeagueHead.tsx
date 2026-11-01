import type { ReactNode } from "react";
import { useOneLine } from "../lib/oneline";

/**
 * A league tab's header (ETF2L, RGL): the title, the menu and the search on
 * one line where they fit; otherwise the title on a line of its own with
 * the menu and the search under it; and where even those two do not fit,
 * the search beside the title and the menu under both (useOneLine). The
 * parts are the children marked `data-fit`.
 */
export function LeagueHead({ children }: { children: ReactNode }) {
  const [ref, lines] = useOneLine(24);
  return (
    <header ref={ref} className={lines === "one" ? "e2-head" : lines === "two" ? "e2-head stacked" : "e2-head stacked split"}>
      {children}
    </header>
  );
}
