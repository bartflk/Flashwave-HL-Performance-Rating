import { useMemo } from "react";
import type { Analysis, FightStats } from "../../api/types";
import { clock, teamLabel } from "../../lib/format";
import { playerMap, sliceScope, type Slice } from "./common";
import { fightsInSlice } from "./roundFights";
import { ClassIcon } from "../ClassIcon";
import { k, t, tx } from "../../lib/i18n";
import { SortTh, useSort } from "../../lib/sort";
import { tip } from "../../lib/glossary";

/** How every Fights column is counted. */
const HOW = k("From the server log's kills, each with its time: a fight starts after 10 s with no kill, a kill is traded when the killer's team loses someone within 3 s.");

/**
 * Every player's kills in context (PLAN §11 B and D): who opened fights,
 * whose kills were traded straight back, who only cleaned up, and who died
 * around their own team's uber. A table, one team at a time, the owner's
 * team first; click a row to pick that player for every view.
 */
export function Fights({ a, player, slice, onPick }: { a: Analysis; player: number; slice: Slice; onPick: (id: number) => void }) {
  const players = useMemo(() => playerMap(a), [a]);
  // A round or one map of a combined log: count its kills again, since the
  // stored counts are for the whole match.
  const partial = slice.rounds !== null;
  const byId = useMemo(
    () => (slice.rounds ? fightsInSlice(a, slice.rounds) : new Map(a.fights.map((f) => [f.accountId, f]))),
    [a, slice.rounds],
  );
  const me = a.players.find((p) => p.isMe);
  const teams = me?.team === "Red" ? (["Red", "Blue"] as const) : (["Blue", "Red"] as const);
  const name = (id: number) => players.get(id)?.name ?? "?";

  const pct = (n: number, d: number) => (d > 0 ? `${Math.round((n / d) * 100)}%` : "–");
  const share = (n: number, d: number) => (d > 0 ? n / d : null);
  // Both teams' rows in one sort, shown team by team (Q79); opening kills
  // first, as before.
  const rows = useMemo(
    () =>
      a.players
        .map((p) => ({ p, f: byId.get(p.accountId) }))
        .filter((r): r is { p: (typeof a.players)[number]; f: FightStats } => r.f !== undefined),
    [a, byId],
  );
  const sort = useSort(
    "fights",
    rows,
    {
      name: (r) => r.p.name,
      kast: (r) => (partial ? null : share(r.f.fightsKast, r.f.fightsPresent)),
      opening: (r) => r.f.openingKills - r.f.openingDeaths,
      firstPicks: (r) => r.f.firstPicks - r.f.firstDeaths,
      traded: (r) => share(r.f.tradedKills, r.f.kills),
      diedAfter: (r) => r.f.diedAfterKill,
      trades: (r) => r.f.tradeKills,
      cleanups: (r) => r.f.cleanupKills,
      charge: (r) => r.f.chargedPicks,
      forces: (r) => (partial ? null : r.f.forces),
      uber: (r) => (partial ? null : r.f.deathsBeforeUber + r.f.deathsDuringUber + r.f.deathsAfterUber),
      deathsTraded: (r) => share(r.f.tradedDeaths, r.f.deaths),
      diedTo: (r) => r.f.deathsToSniper + r.f.deathsToFlank + r.f.deathsToCombo,
    },
    { key: "opening", dir: "desc" },
  );
  /** Whole-match only: these need the fights pass, not the kills alone. */
  const whole = (v: number | string) => (partial ? <span className="muted">–</span> : v);

  return (
    <div className="fights">
      <p className="hint">{tx("A fight starts after 10 s with no kill. A kill is {0} when the killer's team loses someone within 3 s.", { "0": <strong>{t("traded")}</strong> })}</p>
      {partial && (
        <p className="hint">{tx("Counted from the kills in {0}. Fight KAST, forces and deaths around an uber need the whole match, so they are blank here.", { "0": sliceScope(slice) })}</p>
      )}
      <div className="table-wrap">
        <table className="match-table fights-table">
          <thead>
            <tr>
              <SortTh k="name" s={sort}>{t("Player")}</SortTh>
              <SortTh k="kast" s={sort} className="num" tip={tip(t("Fight KAST"), t("Fights alive for where they got a kill or assist, survived, or had their death traded"), t(HOW), "server log")}>{t("Fight KAST")}</SortTh>
              <SortTh k="opening" s={sort} className="num" tip={tip(t("Opening duels"), t("First kills of fights won and lost"), t(HOW), "server log")}>{t("Opening duels")}</SortTh>
              <SortTh k="firstPicks" s={sort} className="num" tip={tip(t("First picks"), t("First kills of rounds: got one / was one"), t(HOW), "server log")}>{t("First picks")}</SortTh>
              <SortTh k="traded" s={sort} className="num" tip={tip(t("Traded"), t("Share of this player's kills where their team lost someone within 3 s"), t(HOW), "server log")}>{t("Traded")}</SortTh>
              <SortTh k="diedAfter" s={sort} className="num" tip={tip(t("Died after kill"), t("Died within 3 s of their own kill"), t(HOW), "server log")}>{t("Died after kill")}</SortTh>
              <SortTh k="trades" s={sort} className="num" tip={tip(t("Trades"), t("Avenged a teammate killed within 3 s before"), t(HOW), "server log")}>{t("Trades")}</SortTh>
              <SortTh k="cleanups" s={sort} className="num" tip={tip(t("Clean-ups"), t("Kills while their team was already up a player"), t(HOW), "server log")}>{t("Clean-ups")}</SortTh>
              <SortTh k="charge" s={sort} className="num" tip={tip(t("Into charge"), t("Combo players killed while their team held a ready charge; Medics among them dropped"), t(HOW), "server log")}>{t("Into charge")}</SortTh>
              <SortTh k="forces" s={sort} className="num" tip={tip(t("Forces"), t("Enemy ubers popped right after this player's damage on the Medic"), t(HOW), "server log")}>{t("Forces")}</SortTh>
              <SortTh k="uber" s={sort} className="num" tip={tip(t("Around own uber"), t("Deaths in the 10 s before their team popped, during it, and in the 10 s after"), t(HOW), "server log")}>{t("Around own uber")}</SortTh>
              <SortTh k="deathsTraded" s={sort} className="num" tip={tip(t("Deaths traded"), t("Deaths your team killed back within 3 s"), t(HOW), "server log")}>{t("Deaths traded")}</SortTh>
              <SortTh k="diedTo" s={sort} className="num" tip={tip(t("Died to Sniper / flank / combo"), t("Died to the enemy Sniper / a flanker (Scout, Spy, Soldier) / the combo (Medic, Demoman, Heavy, Pyro)"), t(HOW), "server log")}>{t("Died to Sniper / flank / combo")}</SortTh>
            </tr>
          </thead>
          {teams.map((team) => (
            <tbody key={team}>
              <tr className="fights-team">
                <th colSpan={13} className={`team-${team.toLowerCase()}`}>
                  {teamLabel(team)}
                </th>
              </tr>
              {sort.sorted
                .filter((r) => r.p.team === team)
                .map(({ p, f }) => (
                  <tr key={p.accountId} className={p.accountId === player ? "clickable sel" : "clickable"} onClick={() => onPick(p.accountId)}>
                    <td className="nowrap class-cell">
                      <ClassIcon cls={p.mainClass} size={20} /> {p.name}
                    </td>
                    <td className="num" title={partial ? t("Whole-match only") : t("{fightsKast} of {fightsPresent} fights", { fightsKast: f.fightsKast, fightsPresent: f.fightsPresent })}>
                      {whole(pct(f.fightsKast, f.fightsPresent))}
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
                      {f.drops > 0 && <span className="muted">{" "}{tx("({drops} drop{1})", { "1": f.drops > 1 ? "s" : "", drops: f.drops })}</span>}
                    </td>
                    <td className="num">{whole(f.forces || "–")}</td>
                    <td className="num">
                      {whole(`${f.deathsBeforeUber} / ${f.deathsDuringUber} / ${f.deathsAfterUber}`)}
                    </td>
                    <td className="num">
                      {f.tradedDeaths} <span className="muted">{tx("of {deaths}", { deaths: f.deaths })}</span>
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

      {a.firstPicks.some((f) => !slice.rounds || slice.rounds.has(f.roundNum)) && (
        <>
          <h3 className="fights-sub">{t("First pick of each round")}</h3>
          <ol className="fights-firsts">
            {a.firstPicks
              .filter((f) => !slice.rounds || slice.rounds.has(f.roundNum))
              .map((f, i) => (
              <li key={i}>
                <span className="muted">R{f.roundNum}</span> <strong>{clock(f.afterS)}</strong>{" "}
                <span className="muted">{t("after the round went live:")}</span>{" "}
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
