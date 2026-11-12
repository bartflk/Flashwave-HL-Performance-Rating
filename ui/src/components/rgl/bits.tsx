import type { RglMatch } from "../../api/types";
import { t } from "../../lib/i18n";
import { openOnKey } from "../../lib/rows";
import { decided } from "../../lib/rglstandings";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { RglMark } from "../RglMark";
import { Countdown, clockTime, fullDate } from "../etf2l/Countdown";
import { secs, useNameSearch, day } from "./data";

/**
 * The wait for a season's matches, which are read one by one. The tabs stay
 * usable meanwhile: leaving the page does not cancel the read, it carries on
 * and is kept for the next visit.
 */
export function Reading() {
  return <Loading>{t("Reading the season from RGL… about 40 seconds the first time. You can look at another page meanwhile: the read carries on in the background.")}</Loading>;
}

/** An undecided match: its countdown, or that a past one has no result yet. */
export function Awaiting({ time, countdown }: { time: number; countdown: boolean }) {
  // A match is "on" for three hours after its start; later with no result
  // entered it is waiting for one, not played.
  if (time < Date.now() / 1000 - 3 * 3600) return <> · {t("awaiting result")}</>;
  return countdown ? (
    <>
      {" · "}
      <Countdown time={time} />
    </>
  ) : null;
}

/** What RGL's name search found: players, then NA Highlander teams. */
export function SearchHits({ found, onPlayer, onTeam }: { found: ReturnType<typeof useNameSearch>; onPlayer: (s: string) => void; onTeam: (id: number) => void }) {
  // Each list says if its own question failed; the other's answer still shows.
  const none = found.players.length === 0 && found.teams.length === 0;
  if (none && found.waiting) return <Loading>{t("Asking RGL…")}</Loading>;
  if (none && !found.playersError && !found.teamsError) return <p className="hint">{t("No player or NA Highlander team by that name on RGL.")}</p>;
  return (
    <>
      {(found.players.length > 0 || found.playersError) && <h4>{t("Players")}</h4>}
      {found.playersError && <ApiError error={found.playersError} />}
      {found.players.map((x) => (
        <button key={x.steamId} className="e2-hit" onClick={() => onPlayer(x.steamId)}>
          <span className="e2-hit-name">{x.name}</span>
          <span className="e2-hit-meta">{t("Player")}</span>
        </button>
      ))}
      {(found.teams.length > 0 || found.teamsError) && <h4>{t("Teams")}</h4>}
      {found.teamsError && <ApiError error={found.teamsError} />}
      {found.teams.map((x) => (
        <button key={x.id} className="e2-hit" onClick={() => onTeam(x.id)}>
          <span className="e2-hit-name">
            {x.name}
            {x.tag && <span className="rgl-tag">{x.tag}</span>}
          </span>
          <span className="e2-hit-meta">
            {x.seasonName}
            {x.division && ` · ${x.division}`}
          </span>
        </button>
      ))}
    </>
  );
}

// ---- Home ------------------------------------------------------------------

/** The page on rgl.gg, in a header's links column as on a profile. */
export function OnRgl({ href }: { href: string }) {
  return (
    <nav className="pp-links" aria-label={t("Elsewhere")}>
      <a href={href} target="_blank" rel="noreferrer">
        <RglMark size={12} /> {t("Open on RGL")} <span aria-hidden>↗</span>
      </a>
    </nav>
  );
}

/** One match of a week: home and away, the score between, the map; the
 *  whole line opens the match. */
export function WeekLine({ m, onTeam, onOpen, countdown }: { m: RglMatch; onTeam: (id: number) => void; onOpen: () => void; countdown?: boolean }) {
  const home = m.teams.find((x) => x.isHome) ?? m.teams[0];
  const away = m.teams.find((x) => x !== home);
  const hs = m.maps.reduce((n, x) => n + (x.homeScore ?? 0), 0);
  const as = m.maps.reduce((n, x) => n + (x.awayScore ?? 0), 0);
  const side = (x: RglMatch["teams"][number] | undefined, cls: string) =>
    x ? (
      <span className={`${cls}${m.winner === x.teamId ? " won" : ""}`}>
        <button
          className="linkish"
          title={x.teamName}
          onClick={(e) => {
            e.stopPropagation();
            onTeam(x.teamId);
          }}
        >
          {x.teamName}
        </button>
      </span>
    ) : (
      <span className={cls}>–</span>
    );
  return (
    <li className="rgl-wl rgl-open" role="link" tabIndex={0} onClick={onOpen} onKeyDown={openOnKey(onOpen)} title={t("Open the match")}>
      <span className="muted rgl-wl-day" title={m.matchDate ? fullDate(secs(m.matchDate)) : undefined}>
        {countdown ? clockTime(secs(m.matchDate)) : day(m.matchDate)}
      </span>
      {side(home, "rgl-wl-side l")}
      <span className="rgl-wl-score">{m.isForfeit ? t("forfeit") : m.winner === null ? t("vs") : `${hs}:${as}`}</span>
      {side(away, "rgl-wl-side r")}
      <span className="muted rgl-wl-map">
        {m.maps.map((x) => x.mapName).join(", ")}
        {!decided(m) && m.matchDate && <Awaiting time={secs(m.matchDate)} countdown={!!countdown} />}
      </span>
    </li>
  );
}

// ---- Matches: the live season's week, what is to come, and the results ---------
