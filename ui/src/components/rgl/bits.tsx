import type { RglMatch } from "../../api/types";
import { t } from "../../lib/i18n";
import { openOnKey } from "../../lib/rows";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { RglMark } from "../RglMark";
import { Countdown, clockTime, fullDate } from "../etf2l/Countdown";
import { secs, useNameSearch, day, LADDER } from "./data";
import { useShowResults } from "../../lib/spoilers";
import { decided, weekOf } from "../../lib/rglstandings";
import type { Mark } from "../../lib/rglmatches";

/** A team's form before a time: see formIndex. */
export type FormOf = (team: number, before: number | null) => Mark[];

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

/** A team's last results as small W / L marks, the newest on the right. */
export function FormMarks({ marks, side }: { marks: Mark[]; side: "l" | "r" }) {
  return (
    <span className={`rgl-form ${side}`} title={t("Last five results this season, newest on the right")} aria-label={marks.join(" ")}>
      {marks.map((x, i) => (
        <span key={i} className={x === "W" ? "w" : "l"} aria-hidden>
          {x === "W" ? t("W") : t("L")}
        </span>
      ))}
    </span>
  );
}

/** A match's week, its division with the ETF2L tier it stands for, and its maps, as small chips. */
export function MatchChips({ m }: { m: RglMatch }) {
  const tier = m.divisionName ? LADDER[m.divisionName] : undefined;
  const maps = m.maps.map((x) => x.mapName).join(", ");
  return (
    <span className="rgl-chips">
      {m.matchName && <span className="rgl-chip">{weekOf(m)}</span>}
      {m.divisionName && (
        <span className="rgl-chip div" title={tier ? t("{0} is about ETF2L {1}", { "0": m.divisionName, "1": t(tier) }) : undefined}>
          {m.divisionName}
          {tier && <span className="muted"> â‰ˆ {t(tier)}</span>}
        </span>
      )}
      {maps && <span className="rgl-chip map">{maps}</span>}
    </span>
  );
}

/**
 * One match: home and away, the score between, form marks beside each team
 * (when given), and the week, division and map as chips under them; the
 * whole line opens the match. With "Show results" off the score is a dash,
 * nobody is bold and the form marks are gone.
 */
export function WeekLine({ m, onTeam, onOpen, countdown, form }: { m: RglMatch; onTeam: (id: number) => void; onOpen: () => void; countdown?: boolean; form?: FormOf }) {
  const [shown] = useShowResults();
  const home = m.teams.find((x) => x.isHome) ?? m.teams[0];
  const away = m.teams.find((x) => x !== home);
  const hs = m.maps.reduce((n, x) => n + (x.homeScore ?? 0), 0);
  const as = m.maps.reduce((n, x) => n + (x.awayScore ?? 0), 0);
  const at = secs(m.matchDate);
  const marks = shown && form;
  const side = (x: RglMatch["teams"][number] | undefined, cls: "l" | "r") =>
    x ? (
      <span className={`rgl-wl-side ${cls}${shown && m.winner === x.teamId ? " won" : ""}`}>
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
      <span className={`rgl-wl-side ${cls}`}>â€“</span>
    );
  // The marks are the team's form going into this match, so an old result
  // does not show its own outcome in them.
  const marksOf = (x: RglMatch["teams"][number] | undefined, cls: "l" | "r") =>
    marks && x ? <FormMarks marks={form(x.teamId, m.matchDate ? at : null)} side={cls} /> : null;
  const score = !decided(m) ? t("vs") : !shown ? "â€“" : m.isForfeit ? t("forfeit") : `${hs}:${as}`;
  return (
    <li className={`rgl-wl rgl-open${marks ? " withform" : ""}`} role="link" tabIndex={0} onClick={onOpen} onKeyDown={openOnKey(onOpen)} title={t("Open the match")}>
      <span className="muted rgl-wl-day" title={m.matchDate ? fullDate(at) : undefined}>
        {countdown ? clockTime(at) : day(m.matchDate)}
      </span>
      {side(home, "l")}
      {marksOf(home, "l")}
      <span className="rgl-wl-score">{score}</span>
      {marksOf(away, "r")}
      {side(away, "r")}
      <span className="rgl-wl-map">
        <MatchChips m={m} />
        {!decided(m) && m.matchDate && (
          <span className="muted">
            <Awaiting time={at} countdown={!!countdown} />
          </span>
        )}
      </span>
    </li>
  );
}

// ---- Matches: the live season's week, what is to come, and the results ---------
