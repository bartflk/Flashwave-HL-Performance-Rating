import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglMatch, RglMatchMore, RglMember } from "../../api/types";
import { t } from "../../lib/i18n";
import { openOnKey } from "../../lib/rows";
import { decided } from "../../lib/rglstandings";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { fullDate } from "../etf2l/Countdown";
import { LogButton } from "../etf2l/Matches";
import { KEEP, useMatch, secs, LADDER, day } from "./data";
import { Awaiting, OnRgl } from "./bits";

export function MatchPage({ id, onTeam, onMatch, onPlayer, onOpenLog }: { id: number; onTeam: (id: number) => void; onMatch: (id: number) => void; onPlayer: (s: string) => void; onOpenLog: (logId: number) => void }) {
  const q = useMatch(id);
  const more = useQuery({ queryKey: ["rgl_match_more", id], queryFn: () => api.getRglMatchMore(id), staleTime: KEEP, retry: false });
  const m = q.data;
  if (q.error) return <ApiError error={q.error} />;
  if (q.isPending) return <Loading>{t("Loading the match…")}</Loading>;
  if (!m) return <p className="hint">{t("RGL has no such match.")}</p>;
  const home = m.teams.find((x) => x.isHome) ?? m.teams[0];
  const away = m.teams.find((x) => x !== home);
  const hs = m.maps.reduce((n, x) => n + (x.homeScore ?? 0), 0);
  const as = m.maps.reduce((n, x) => n + (x.awayScore ?? 0), 0);
  const at = secs(m.matchDate);
  const tier = m.divisionName ? LADDER[m.divisionName] : undefined;
  const side = (x: RglMatch["teams"][number] | undefined, cls: string) =>
    x ? (
      <button className={`rgl-mp-team ${cls}${m.winner === x.teamId ? " won" : ""}`} onClick={() => onTeam(x.teamId)}>
        <span className="rgl-mp-name" title={x.teamName}>{x.teamName}</span>
        {x.teamTag && <span className="rgl-tag">{x.teamTag}</span>}
      </button>
    ) : (
      <span className={cls}>–</span>
    );
  const x = more.data;
  const lineup = (teamId: number | undefined) => x?.lineups.find((l) => l.teamId === teamId)?.players ?? [];
  return (
    <>
      <div className="panel pp-head rgl-mp-head">
        <div className="rgl-mp-meta">
          <span className="ts-eyebrow">{[m.seasonName, m.divisionName && (tier ? `${m.divisionName} ≈ ${t(tier)}` : m.divisionName), m.matchName].filter(Boolean).join(" · ")}</span>
          <OnRgl href={`https://rgl.gg/Public/Match?m=${m.matchId}`} />
        </div>
        <div className="rgl-mp-vs">
          {side(home, "l")}
          <span className="rgl-mp-score">{m.isForfeit ? t("forfeit") : decided(m) ? `${hs} : ${as}` : t("vs")}</span>
          {side(away, "r")}
        </div>
        <div className="rgl-mp-when muted">
          {at > 0 && fullDate(at)}
          {!decided(m) && at > 0 && <Awaiting time={at} countdown />}
        </div>
      </div>
      <div className="e2-match-grid">
        <div className="e2-main">
          <section className="panel e2-panel">
            <div className="e2-panel-head">
              <h2>{m.maps.length === 1 ? t("Map") : t("Maps")}</h2>
            </div>
            {m.maps.length === 0 ? (
              <p className="hint">{t("No map yet.")}</p>
            ) : (
              <table className="match-table">
                <thead>
                  <tr>
                    <th>{t("Map")}</th>
                    <th className="num">{home?.teamTag ?? home?.teamName ?? t("Home")}</th>
                    <th className="num">{away?.teamTag ?? away?.teamName ?? t("Away")}</th>
                  </tr>
                </thead>
                <tbody>
                  {m.maps.map((mp, i) => (
                    <tr key={`${mp.mapName}-${i}`}>
                      <td>{mp.mapName}</td>
                      <td className={`num${(mp.homeScore ?? 0) > (mp.awayScore ?? 0) ? " team-won" : ""}`}>{mp.homeScore ?? "–"}</td>
                      <td className={`num${(mp.awayScore ?? 0) > (mp.homeScore ?? 0) ? " team-won" : ""}`}>{mp.awayScore ?? "–"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>
          <section className="panel e2-panel">
            <div className="e2-panel-head">
              <h2>{t("Lineups")}</h2>
              <span className="muted">{decided(m) ? t("each team's roster on the day") : t("each team's roster now")}</span>
            </div>
            {more.error ? (
              <ApiError error={more.error} />
            ) : !x ? (
              <Loading>{t("Reading the teams…")}</Loading>
            ) : (
              <div className="e2-lineups">
                {[home, away].map((team) =>
                  team ? (
                    <div key={team.teamId}>
                      <h3 className="e2-lu-team">{team.teamName}</h3>
                      <RglLineup players={lineup(team.teamId)} onPlayer={onPlayer} />
                    </div>
                  ) : null,
                )}
              </div>
            )}
            <p className="hint">{t("RGL lists rosters, not who played: a stand-in is not on them.")}</p>
          </section>
        </div>
        <aside className="e2-rail">
          {home && away && <RglMeetings them={x?.lineups.find((l) => l.teamId === away.teamId)?.ids ?? [away.teamId]} list={x?.meetings ?? null} onMatch={onMatch} />}
          <section className="panel e2-panel">
            <div className="e2-panel-head">
              <h2>{t("Logs")}</h2>
            </div>
            {!x ? (
              more.error ? <ApiError error={more.error} /> : <Loading>{t("Looking for the logs…")}</Loading>
            ) : x.logs.length > 0 ? (
              <div className="e2-logs">
                {x.logs.map((l) => (
                  <LogButton key={l.logId} logId={l.logId} label={l.map || `#${l.logId}`} stored={l.stored} matchId={id} onOpenLog={onOpenLog} onFetched={() => void more.refetch()} />
                ))}
              </div>
            ) : (
              <p className="hint">{logsWhy(x.logsWhy)}</p>
            )}
          </section>
        </aside>
      </div>
    </>
  );
}

/** Why a match page lists no logs. */
function logsWhy(why: RglMatchMore["logsWhy"]): string {
  switch (why) {
    case "notPlayed":
      return t("Not played yet.");
    case "forfeit":
      return t("A forfeit has no logs.");
    case "tooSoon":
      return t("Logs are looked for an hour after the match, once they are uploaded.");
    case "resting":
      return t("logs.tf is refusing requests for now; the logs are looked for next time.");
    case "unavailable":
      return t("logs.tf did not answer; the logs are looked for next time.");
    case "noRoster":
      return t("No roster to look for the logs by.");
    default:
      return t("No log on logs.tf has a player of each team at the match's time.");
  }
}

/** A team's roster on the day, as a lineup: leaders first, each opening their RGL page. */
export function RglLineup({ players, onPlayer }: { players: RglMember[]; onPlayer: (s: string) => void }) {
  if (players.length === 0) return <p className="hint">{t("No one on the roster then.")}</p>;
  return (
    <ol className="e2-lineup rgl-lineup">
      {players.map((p) => (
        <li key={p.steamId}>
          <button onClick={() => onPlayer(p.steamId)} title={t("Open their RGL page")}>
            <span className="rgl-lu-mark" aria-hidden>
              {p.isLeader ? "★" : ""}
            </span>
            <span className="e2-lu-name">{p.name}</span>
            {p.isLeader ? <span className="badge">{t("Leader")}</span> : <span />}
          </button>
        </li>
      ))}
    </ol>
  );
}

/** Earlier matches between the two teams, from the first team's side.
 *  `them`: the other team's ids across its seasons (a new id each season). */
export function RglMeetings({ them, list, onMatch }: { them: number[]; list: RglMatch[] | null; onMatch: (id: number) => void }) {
  const theirs = new Set(them);
  const ours = (m: RglMatch) => {
    const home = (m.teams.find((x) => x.isHome) ?? m.teams[0])?.teamId;
    const h = m.maps.reduce((n, x) => n + (x.homeScore ?? 0), 0);
    const a = m.maps.reduce((n, x) => n + (x.awayScore ?? 0), 0);
    return home !== undefined && theirs.has(home) ? [a, h] : [h, a];
  };
  const played = (list ?? []).filter((m) => decided(m) && m.winner !== null);
  const won = played.filter((m) => !theirs.has(m.winner!)).length;
  const lost = played.length - won;
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Head-to-head")}</h2>
        {played.length > 0 && <span className="e2-h2h-rec">{`${won}–${lost}`}</span>}
      </div>
      {list === null ? (
        <Loading>{t("Reading both teams' seasons…")}</Loading>
      ) : list.length === 0 ? (
        <p className="hint">{t("Their first meeting on RGL.")}</p>
      ) : (
        <ol className="e2-meetings rgl-meetings">
          {list.slice(0, 8).map((m) => {
            const [a, b] = ours(m);
            return (
              <li key={m.matchId} tabIndex={0} role="link" onClick={() => onMatch(m.matchId)} onKeyDown={openOnKey(() => onMatch(m.matchId))}>
                <span className="muted">{m.matchDate ? day(m.matchDate) : (m.seasonName ?? "")}</span>
                <span className="e2-map-score">
                  {m.isForfeit ? (
                    <span>{t("forfeit")}</span>
                  ) : (
                    <>
                      <span className={a > b ? "won" : undefined}>{decided(m) ? a : "–"}</span>
                      <span className="e2-rl-dash">:</span>
                      <span className={b > a ? "won" : undefined}>{decided(m) ? b : "–"}</span>
                    </>
                  )}
                </span>
                <span className="muted">{m.seasonName ?? m.divisionName ?? ""}</span>
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
