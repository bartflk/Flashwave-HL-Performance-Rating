import { useEffect, useState } from "react";
import type { RglMatch } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { openOnKey } from "../../lib/rows";
import { liveStrip } from "../../lib/rglmatches";
import { weekOf } from "../../lib/rglstandings";
import { clockTime, fullDate } from "../etf2l/Countdown";
import { useLive, secs } from "./data";
import type { Loc } from "./data";

/** Cards shown at most; a busy night says how many more there are. */
const MOST = 8;

/**
 * "Live now" and tonight, as tf2esports' ticker has it: a row of small cards,
 * each with both teams, a LIVE dot or the start time, the week and the
 * division. From the season's matches the tab has read already, so it asks
 * for nothing; matches with no result yet that started within 2.5 hours are
 * live (RGL gives no live scores). Gone when there is nothing to show.
 */
export function LiveStrip({ onMatch, go }: { onMatch: (id: number) => void; go?: (l: Loc) => void }) {
  const live = useLive();
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now() / 1000), 30_000);
    return () => window.clearInterval(id);
  }, []);
  const { live: on, tonight } = liveStrip(live.matches ?? [], now);
  const all = [...on, ...tonight];
  if (all.length === 0) return null;
  const card = (m: RglMatch) => {
    const isLive = on.includes(m);
    const [a, b] = [m.teams.find((x) => x.isHome) ?? m.teams[0], m.teams.find((x) => x.isHome === false) ?? m.teams[1]];
    return (
      <li key={m.matchId} className={`rgl-tick${isLive ? " live" : ""}`} role="link" tabIndex={0} onClick={() => onMatch(m.matchId)} onKeyDown={openOnKey(() => onMatch(m.matchId))} title={t("Open the match")}>
        <span className="rgl-tick-when" title={fullDate(secs(m.matchDate))}>
          {isLive ? (
            <span className="rgl-tick-live">
              <span className="cd-dot" aria-hidden /> {t("LIVE")}
            </span>
          ) : (
            clockTime(secs(m.matchDate))
          )}
        </span>
        <span className="rgl-tick-teams">
          <span title={a.teamName}>{a.teamTag ?? a.teamName}</span>
          <span className="muted">{t("vs")}</span>
          <span title={b.teamName}>{b.teamTag ?? b.teamName}</span>
        </span>
        <span className="rgl-tick-meta muted">{[weekOf(m), m.divisionName].filter(Boolean).join(" · ")}</span>
      </li>
    );
  };
  return (
    <section className="panel e2-panel rgl-strip">
      <div className="e2-panel-head">
        <h2>{on.length > 0 ? t("Live now") : t("Tonight")}</h2>
        {go ? (
          <button className="e2-more" onClick={() => go({ at: "matches" })}>
            {t("All matches")} ›
          </button>
        ) : (
          <span className="muted">{all.length === 1 ? t("1 match") : tx("{0} matches", { "0": all.length })}</span>
        )}
      </div>
      <ul className="rgl-ticks">{all.slice(0, MOST).map(card)}</ul>
      {all.length > MOST && <p className="hint">{tx("and {0} more tonight", { "0": all.length - MOST })}</p>}
    </section>
  );
}
