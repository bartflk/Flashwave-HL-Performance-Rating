import { useMemo } from "react";
import type { Analysis, FightStats } from "../../api/types";
import { capitalize, clock, teamLabel } from "../../lib/format";
import { playerMap } from "./common";

/**
 * Every player's kills in context (PLAN §11 B and D): who opened fights,
 * whose kills were traded straight back, who only cleaned up, and who died
 * around their own team's uber. A table, one team at a time, the owner's
 * team first; click a row to pick that player for every view.
 */
export function Fights({ a, player, onPick }: { a: Analysis; player: number; onPick: (id: number) => void }) {
  const players = useMemo(() => playerMap(a), [a]);
  const byId = useMemo(() => new Map(a.fights.map((f) => [f.accountId, f])), [a.fights]);
  const me = a.players.find((p) => p.isMe);
  const teams = me?.team === "Red" ? (["Red", "Blue"] as const) : (["Blue", "Red"] as const);
  const name = (id: number) => players.get(id)?.name ?? "?";

  const pct = (n: number, d: number) => (d > 0 ? `${Math.round((n / d) * 100)}%` : "–");

  return (
    <div className="fights">
      <p className="hint">
        A fight starts when nobody has died for 10 s. A kill is <strong>traded</strong> when the killer&apos;s team loses
        someone within 3 s, and a <strong>clean-up</strong> when the killer&apos;s team was already up a player. Both
        windows were measured on this account&apos;s 195,000 kills.
      </p>
      <div className="table-wrap">
        <table className="match-table fights-table">
          <thead>
            <tr>
              <th>Player</th>
              <th className="num" title="First kills of fights won and lost">Opening duels</th>
              <th className="num" title="First kills of rounds: got one / was one">First picks</th>
              <th className="num" title="Share of this player's kills where their team lost someone within 3 s">Traded</th>
              <th className="num" title="Died within 3 s of their own kill">Died after kill</th>
              <th className="num" title="Avenged a teammate killed within 3 s before">Trades</th>
              <th className="num" title="Kills while their team was already up a player">Clean-ups</th>
              <th className="num" title="Combo players killed while their team held a ready charge; Medics among them dropped">Into charge</th>
              <th className="num" title="Enemy ubers popped right after this player's damage on the Medic">Forces</th>
              <th className="num" title="Deaths in the 10 s before their team popped, during it, and in the 10 s after">Around own uber</th>
              <th className="num" title="Deaths your team killed back within 3 s">Deaths traded</th>
              <th className="num" title="Died to the enemy Sniper / a flanker (Scout, Spy, Soldier) / the combo (Medic, Demoman, Heavy, Pyro)">Died to Sniper / flank / combo</th>
            </tr>
          </thead>
          {teams.map((team) => (
            <tbody key={team}>
              <tr className="fights-team">
                <th colSpan={12} className={`team-${team.toLowerCase()}`}>
                  {teamLabel(team)}
                </th>
              </tr>
              {a.players
                .filter((p) => p.team === team)
                .map((p) => ({ p, f: byId.get(p.accountId) }))
                .filter((r): r is { p: typeof r.p; f: FightStats } => r.f !== undefined)
                .sort((x, y) => y.f.openingKills - x.f.openingKills)
                .map(({ p, f }) => (
                  <tr key={p.accountId} className={p.accountId === player ? "clickable sel" : "clickable"} onClick={() => onPick(p.accountId)}>
                    <td>
                      {p.name}
                      {p.mainClass && <span className="muted"> · {capitalize(p.mainClass)}</span>}
                    </td>
                    <td className="num">
                      {f.openingKills}–{f.openingDeaths}
                    </td>
                    <td className="num">
                      {f.firstPicks} / {f.firstDeaths}
                    </td>
                    <td className="num">{pct(f.tradedKills, f.kills)}</td>
                    <td className="num">{f.diedAfterKill}</td>
                    <td className="num">{f.tradeKills}</td>
                    <td className="num">{f.cleanupKills}</td>
                    <td className="num">
                      {f.chargedPicks}
                      {f.drops > 0 && <span className="muted"> ({f.drops} drop{f.drops > 1 ? "s" : ""})</span>}
                    </td>
                    <td className="num">{f.forces || "–"}</td>
                    <td className="num">
                      {f.deathsBeforeUber} / {f.deathsDuringUber} / {f.deathsAfterUber}
                    </td>
                    <td className="num">
                      {f.tradedDeaths} <span className="muted">of {f.deaths}</span>
                    </td>
                    <td className="num">
                      {f.deathsToSniper} / {f.deathsToFlank} / {f.deathsToCombo}
                    </td>
                  </tr>
                ))}
            </tbody>
          ))}
        </table>
      </div>

      {a.firstPicks.length > 0 && (
        <>
          <h3 className="fights-sub">First pick of each round</h3>
          <ol className="fights-firsts">
            {a.firstPicks.map((f, i) => (
              <li key={i}>
                <span className="muted">R{f.roundNum}</span> <strong>{clock(f.afterS)}</strong>{" "}
                <span className="muted">after the round went live:</span>{" "}
                <span className={`team-${(players.get(f.killer)?.team ?? "none").toLowerCase()}`}>{name(f.killer)}</span> →{" "}
                <span className={`team-${(players.get(f.victim)?.team ?? "none").toLowerCase()}`}>{name(f.victim)}</span>
              </li>
            ))}
          </ol>
        </>
      )}
    </div>
  );
}
