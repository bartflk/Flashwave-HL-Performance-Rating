import type { LogFlags, MatchDetail, PlayerRow, Team } from "../../api/types";
import { capitalize, clock, teamLabel } from "../../lib/format";

/** The full scoreboard, one table per team, your team first. */
export function BoxScore({ d }: { d: MatchDetail }) {
  const left = d.leftTeam;
  const right: Team = left === "Red" ? "Blue" : "Red";
  return (
    <section className="panel box">
      <h2>Scoreboard</h2>
      <TeamTable team={left} label={d.myTeam ? "Us" : null} players={d.players} flags={d.flags} />
      <TeamTable team={right} label={d.myTeam ? "Them" : null} players={d.players} flags={d.flags} />
    </section>
  );
}

function TeamTable(props: { team: Team; label: string | null; players: PlayerRow[]; flags: LogFlags }) {
  const { team, label, players, flags } = props;
  const rows = players.filter((p) => p.team === team);
  return (
    <div className="table-wrap box-team">
      <table className="match-table">
        <thead>
          <tr>
            <th className={`team-${team.toLowerCase()}`}>
              {label ? `${label} · ` : ""}
              {teamLabel(team)}
            </th>
            <th>Player</th>
            <th className="num">K</th>
            <th className="num">A</th>
            <th className="num">D</th>
            <th className="num">Dmg</th>
            <th className="num">DPM</th>
            <th className="num">DT</th>
            <th>Class detail</th>
            <th className="num" title="Provisional value score (v0), per 10 minutes on the main class">
              Value
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((p) => (
            <tr key={p.accountId} className={p.isMe ? "me-row" : undefined}>
              <td className="nowrap" title={p.classes.map(([c, t]) => `${capitalize(c)} ${clock(t)}`).join(", ")}>
                {p.mainClass ? capitalize(p.mainClass) : "—"}
                {p.classes.length > 1 && <span className="sub-tag">+{p.classes.length - 1}</span>}
              </td>
              <td className="nowrap player-name">
                {p.name}
                {p.isMe && <span className="you-tag">you</span>}
              </td>
              <td className="num">{p.kills}</td>
              <td className="num">{p.assists}</td>
              <td className="num">{p.deaths}</td>
              <td className="num">{p.dmg.toLocaleString()}</td>
              <td className="num">{p.dpm}</td>
              <td className="num">{flags.dt ? p.dt.toLocaleString() : "—"}</td>
              <td className="muted nowrap">{classDetail(p, flags)}</td>
              <td className="num">{p.value ? p.value.score.toFixed(1) : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** The one or two numbers that matter for this player's class. A stat the
 *  log did not record shows as absent rather than as zero. */
function classDetail(p: PlayerRow, f: LogFlags): string {
  switch (p.mainClass) {
    case "sniper":
      return f.hs ? `${p.headshots} headshot kills` + (f.hsHit ? ` · ${p.headshotsHit} landed` : "") : "headshots not recorded";
    case "medic":
      return `${(p.heal / 1000).toFixed(1)}k heal · ${p.ubers} uber${p.ubers === 1 ? "" : "s"} · ${p.drops} drop${p.drops === 1 ? "" : "s"}`;
    case "spy":
      return f.bs ? `${p.backstabs} backstabs` : "backstabs not recorded";
    case "soldier":
    case "demoman":
      return f.airshots ? `${p.airshots} airshots` : "airshots not recorded";
    default:
      return p.cpc > 0 ? `${p.cpc} caps` : "";
  }
}
