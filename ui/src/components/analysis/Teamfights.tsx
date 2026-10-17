import { useMemo } from "react";
import type { Analysis, TeamfightView } from "../../api/types";
import { clock } from "../../lib/format";
import { playerMap, type Slice } from "./common";
import { t, tx } from "../../lib/i18n";
import { SortTh, useSort } from "../../lib/sort";
import { tip } from "../../lib/glossary";

/** Arrivals this close together count as one collapse. Mirrors `TOGETHER_S`. */
const TOGETHER_S = 3;
/** A side smaller than this is a skirmish, not a teamfight. Mirrors `MIN_SIDE`. */
const MIN_SIDE = 3;

/**
 * Who turned up to each fight, and how together (Q7b, Taiga's ask).
 *
 * The question Highlander players argue about after a lost round, which no
 * scoreboard answers: did we go in as one, or trickle in to die one at a
 * time? A player is in a fight from the first moment they deal or take
 * damage, kill, or die — so a Soldier who lands two rockets and lives was
 * there, where a kill feed says he never was.
 *
 * **How much of a side went in together**: the share of it that joined
 * within three seconds of that side's own first arrival. This replaced the
 * obvious measure -- the gap between a side's first and last arrival --
 * after it was tried on a real match and called 0 of 51 teamfights
 * together. With eight or nine players a side there is nearly always one
 * who arrives late for a good reason, a Sniper at the back or a respawn,
 * and the gap measures them rather than the collapse. "6 of 8 in together"
 * does not care about the one.
 */
export function Teamfights({ a, player, slice }: { a: Analysis; player: number; slice: Slice }) {
  const names = useMemo(() => playerMap(a), [a]);
  const me = a.players.find((p) => p.isMe) ?? null;
  const ours = me?.team ?? a.players.find((p) => p.accountId === player)?.team ?? "Red";
  const team = (id: number) => names.get(id)?.team;

  const fights = (a.teamfights ?? []).filter(
    (f) => slice.rounds === null || slice.rounds.has(f.roundNum),
  );

  const side = (f: TeamfightView, us: boolean) => {
    const arr = f.arrivals.filter((x) => (team(x.accountId) === ours) === us);
    const first = arr.length > 0 ? Math.min(...arr.map((x) => x.joinedS)) : 0;
    const inTogether = arr.filter((x) => x.joinedS <= first + TOGETHER_S).length;
    return {
      n: arr.length,
      inTogether,
      // Only a side big enough to be a teamfight gets a share at all.
      share: arr.length >= MIN_SIDE ? inTogether / arr.length : null,
      lost: arr.filter((x) => x.diedS !== null).length,
    };
  };

  const rows = fights.map((f) => ({ f, us: side(f, true), them: side(f, false) }));
  const measured = rows.filter((r) => r.us.share !== null);
  const avgShare = measured.length > 0 ? measured.reduce((n, r) => n + (r.us.share ?? 0), 0) / measured.length : null;
  const won = (r: (typeof rows)[number]) => r.us.lost < r.them.lost;
  const fightsWon = measured.filter(won).length;

  // The selected player's own habit: joined how many, how early.
  const joins = fights
    .map((f) => f.arrivals.find((x) => x.accountId === player)?.joinedS)
    .filter((x): x is number => x !== undefined)
    .sort((x, y) => x - y);
  const median = joins.length > 0 ? joins[Math.floor(joins.length / 2)] : null;
  const who = names.get(player)?.name ?? "this player";
  const sort = useSort("teamfights", rows, {
    round: (r) => r.f.roundNum * 100000 + r.f.t,
    time: (r) => r.f.t,
    us: (r) => r.us.share,
    them: (r) => r.them.share,
    lost: (r) => r.us.lost - r.them.lost,
    me: (r) => r.f.arrivals.find((x) => x.accountId === player)?.joinedS ?? null,
  });

  if (fights.length === 0) {
    return <p className="hint an-empty">{t("No teamfights here: every fight was too small to say who collapsed on it.")}</p>;
  }

  const pct = (n: number, d: number) => (d > 0 ? `${Math.round((n / d) * 100)}%` : "–");
  const togetherCell = (s: { n: number; inTogether: number; share: number | null }) =>
    s.share === null ? (
      <span className="muted">{s.n > 0 ? t("{n} in", { n: s.n }) : "–"}</span>
    ) : (
      <span className={s.share >= 0.75 ? "tf-together" : s.share < 0.5 ? "tf-apart" : undefined}>
        {tx("{inTogether} of {n}", { inTogether: s.inTogether, n: s.n })}
      </span>
    );

  return (
    <div className="teamfights">
      <dl className="kv">
        <dt>{t("Your side in together")}</dt>
        <dd>
          {tx("{0}, over {measured} teamfights", { "0": avgShare === null ? "–" : t("{0}% on average", { "0": Math.round(avgShare * 100) }), measured: measured.length })}</dd>
        <dt>{t("Fights won")}</dt>
        <dd>
          {tx("{fightsWon} of {measured} ({2}), losing fewer than the other side", { "2": pct(fightsWon, measured.length), fightsWon: fightsWon, measured: measured.length })}</dd>
        <dt>{who}</dt>
        <dd>{tx("in {joins} of {fights}{2}", { "2": median !== null && t(", usually {0} the first kill", { "0": median > 0 ? `${median}s after` : median < 0 ? `${-median}s before` : "at" }), joins: joins.length, fights: fights.length })}
        </dd>
      </dl>

      <div className="table-wrap">
        <table className="match-table">
          <thead>
            <tr>
              <SortTh k="round" s={sort}>{t("Round")}</SortTh>
              <SortTh k="time" s={sort} className="num">{t("Time")}</SortTh>
              <SortTh k="us" s={sort} className="num" tip={tip(t("You, in together"), t("How many of your side were in within {TOGETHER_S}s of your first arrival", { TOGETHER_S: TOGETHER_S }), t("A player is in a fight from their first damage, kill or death, read from the server log."), "server log")}>{t("You, in together")}</SortTh>
              <SortTh k="them" s={sort} className="num">{t("Them")}</SortTh>
              <SortTh k="lost" s={sort} className="num">{t("Lost")}</SortTh>
              <SortTh k="me" s={sort} className="num">{who.length > 14 ? `${who.slice(0, 13)}…` : who}</SortTh>
            </tr>
          </thead>
          <tbody>
            {sort.sorted.map(({ f, us, them }) => {
              const mine = f.arrivals.find((x) => x.accountId === player);
              return (
                <tr key={`${f.roundNum}-${f.t}`} className={won({ f, us, them }) ? undefined : "muted-row"}>
                  <td>R{f.roundNum}</td>
                  <td className="num">{clock(f.t)}</td>
                  <td className="num">{togetherCell(us)}</td>
                  <td className="num">{togetherCell(them)}</td>
                  <td className="num">
                    {us.lost}–{them.lost}
                  </td>
                  <td className="num">
                    {mine === undefined ? (
                      <span className="muted">{t("not in it")}</span>
                    ) : (
                      <>
                        {mine.joinedS > 0 ? `+${mine.joinedS}s` : `${mine.joinedS}s`}
                        {mine.diedS !== null && <span className="muted">{" "}{t("· died")}</span>}
                      </>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="hint">{tx("A player is in a fight from their first damage, kill or death. In together means within {TOGETHER_S}s of their side's first.", { TOGETHER_S: TOGETHER_S })}</p>
    </div>
  );
}
