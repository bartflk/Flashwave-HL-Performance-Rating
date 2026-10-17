import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { TeamEra } from "../../api/types";
import { rating } from "../../lib/format";
import { openPlayer } from "../../lib/goto";
import { locale, t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";

/**
 * Your own side of the Teams tab (once the Teammates tab's team cards):
 * every ETF2L team you played for, from your matches -- officials and
 * scrims -- with your record and rating, and who played most with you.
 */

export function useYourTeams(): TeamEra[] {
  return useQuery({ queryKey: ["teammates", false], queryFn: () => api.getTeammates(false), staleTime: 5 * 60_000 }).data?.teams ?? [];
}

const winRate = (x: { wins: number; losses: number }) => (x.wins + x.losses === 0 ? null : (x.wins / (x.wins + x.losses)) * 100);

/** "Jul 2023 – Mar 2024", or one month. */
function span(e: TeamEra): string {
  const month = (unix: number) => new Date(unix * 1000).toLocaleDateString(locale(), { month: "short", year: "numeric" });
  const a = month(e.firstPlayed);
  const b = month(e.lastPlayed);
  return a === b ? b : `${a} – ${b}`;
}

/** Your teams in one line each (Q74's Home, Flashy: smaller than the cards):
 *  badge, name, record and your rating, the newest few. */
export function YourTeamsCompact({ onTeam, max = 3 }: { onTeam: (teamId: number) => void; max?: number }) {
  const all = useYourTeams().filter((e) => e.games >= 3);
  const [open, setOpen] = useState(false);
  if (all.length === 0) return null;
  const shown = open ? all : all.slice(0, max);
  return (
    <ul className="yt-compact">
      {shown.map((e) => (
        <li key={e.teamId}>
          <button onClick={() => onTeam(e.teamId)} title={t("Open {0}", { "0": e.name })}>
            <span className="yt-c-name">{e.name || t("Unnamed team")}</span>
            <span className="yt-c-wl">
              {e.wins}–{e.losses}
            </span>
            <span className="yt-c-rating">{e.myAvg === null ? "—" : rating(e.myAvg)}</span>
          </button>
        </li>
      ))}
      {all.length > max && (
        <li>
          <button className="yt-c-more" onClick={() => setOpen((o) => !o)}>
            {open ? t("Fewer") : tx("All your teams ({0})", { "0": all.length })}
          </button>
        </li>
      )}
    </ul>
  );
}

/** On a team's page, when it was yours: how it went, and with whom. */
export function YouOnThisTeam({ teamId, named }: { teamId: number; named?: boolean }) {
  const e = useYourTeams().find((x) => x.teamId === teamId);
  if (!e) return null;
  const wr = winRate(e);
  return (
    <section className="panel yt-you">
      <div className="yt-you-head">
        <h3>{named ? e.name : t("You on this team")}</h3>
        <span className="hint">{span(e)}</span>
      </div>
      <dl className="yt-you-stats">
        <div>
          <dt>{t("Games")}</dt>
          <dd>
            {e.games}
            {e.officials > 0 && <span className="muted"> · {tx("{officials} official", { officials: e.officials })}</span>}
          </dd>
        </div>
        <div>
          <dt>{t("Record")}</dt>
          <dd>
            {e.wins}–{e.losses}
            {wr !== null && <span className="muted"> · {wr.toFixed(0)}%</span>}
          </dd>
        </div>
        <div>
          <dt>{t("Your rating")}</dt>
          <dd>{e.myAvg === null ? <span className="muted">—</span> : rating(e.myAvg)}</dd>
        </div>
      </dl>
      {e.core.length > 0 && (
        <div className="yt-core" aria-label={t("Most frequent teammates")}>
          {e.core.map((c) => (
            <button key={c.accountId} className="yt-mate" onClick={() => openPlayer(c.accountId)} title={t("{games} games together", { games: c.games })}>
              {c.mainClass && <ClassIcon cls={c.mainClass} size={16} />}
              {c.name}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
