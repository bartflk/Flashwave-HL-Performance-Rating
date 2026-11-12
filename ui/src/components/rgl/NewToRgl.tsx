import { useState } from "react";
import { t } from "../../lib/i18n";
import { OnRgl } from "./bits";
import { LADDER } from "./data";

const KEY = "rgl-new-card-dismissed";

/** Said already in this run, for when storage is blocked and cannot remember. */
let hiddenThisRun = false;

function hidden(): boolean {
  if (hiddenThisRun) return true;
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

/**
 * A short primer for someone who has not played RGL's Highlander
 * (tf2esports has a guide for newcomers; Flashy: "keep working and iterating
 * on the rgl page ... i want a lot of the site features copied"). Written by
 * hand: it asks nothing of RGL. The divisions come from the same ladder the
 * rest of the tab uses, so what it says about ETF2L is what the pages show.
 * Dismissed once, it stays gone for this viewer.
 */
export function NewToRgl() {
  const [open, setOpen] = useState(() => !hidden());
  if (!open) return null;
  const close = () => {
    hiddenThisRun = true;
    try {
      localStorage.setItem(KEY, "1");
    } catch {
      // It comes back next time, which is harmless.
    }
    setOpen(false);
  };
  // The ladder lists the best division first; a newcomer reads from the bottom.
  const divisions = Object.entries(LADDER).reverse();
  return (
    <section className="panel e2-panel rgl-new">
      <div className="e2-panel-head">
        <h2>{t("New to RGL Highlander?")}</h2>
        <button className="e2-more" onClick={close} aria-label={t("Hide this card")}>
          {t("Hide")}
        </button>
      </div>
      <p>{t("RGL is a long-running league for competitive Team Fortress 2. Its Highlander format is nine against nine, one player on each class, played in teams that sign up for a season.")}</p>
      <p>{t("Teams are placed in a division by their level, from Newcomer for a first season up to Invite for the best teams in North America. A team can move up or down a division from one season to the next. This is how they line up with ETF2L's divisions:")}</p>
      <ul className="rgl-new-divs">
        {divisions.map(([name, tier]) => (
          <li key={name}>
            <strong>{name}</strong>
            <span className="muted">≈ {t(tier)}</span>
          </li>
        ))}
      </ul>
      <p>{t("A season starts with a regular season of one match a week against the other teams in your division. The best teams of each division then go on to the playoffs, and the winner of the final is the division's champion.")}</p>
      <p>{t("To play, sign up on rgl.gg: make a player profile, then join a team or start your own when a season is open for signups.")}</p>
      <OnRgl href="https://rgl.gg" />
    </section>
  );
}
