import { useState } from "react";
import type { MatchDetail, Matchup, Side, Team, Value } from "../../api/types";
import { capitalize, signed, teamLabel } from "../../lib/format";

/**
 * The nine class matchups: the headline of the match page.
 *
 * Two different kinds of number live here and the UI keeps them apart:
 * the head-to-head is read straight from the log and is objective; the value
 * score is a model (v0, provisional) and every term of it is one click away.
 */
export function Matchups({ d }: { d: MatchDetail }) {
  const [open, setOpen] = useState<string | null>(null);

  const left = d.leftTeam;
  const right: Team = left === "Red" ? "Blue" : "Red";
  const us = d.myTeam !== null;

  // Scale bars to the biggest gap in this match, with a floor so a match of
  // tiny gaps does not render them as dramatic.
  const maxGap = Math.max(5, ...d.matchups.map((m) => Math.abs(m.diff ?? 0)));

  const wonByLeft = d.matchups.filter((m) => m.winner === "left").length;
  const wonByRight = d.matchups.filter((m) => m.winner === "right").length;

  return (
    <section className="panel mu">
      <header className="mu-head">
        <div>
          <h2>Class matchups</h2>
          <p className="hint">
            {us ? "You" : teamLabel(left)} won {wonByLeft}, {us ? "they" : teamLabel(right)} won{" "}
            {wonByRight}. Click a row for the working.
          </p>
        </div>
        <span className="model-tag" title="Generic formula: impact-weighted kills and assists minus the cost of dying, per 10 minutes. Class-specific models replace it in M3.">
          model {d.modelVersion} · provisional
        </span>
      </header>

      <div className="mu-cols" aria-hidden>
        <span />
        <span className={`team-${left.toLowerCase()} mu-col-l`}>
          {us ? "Us" : ""} {teamLabel(left)}
        </span>
        <span className="mu-col-c">head-to-head</span>
        <span className={`team-${right.toLowerCase()} mu-col-r`}>
          {teamLabel(right)} {us ? "Them" : ""}
        </span>
        <span />
      </div>

      {d.matchups.map((m) => (
        <MatchupRow
          key={m.class}
          m={m}
          left={left}
          right={right}
          maxGap={maxGap}
          open={open === m.class}
          onToggle={() => setOpen(open === m.class ? null : m.class)}
        />
      ))}

      {d.weightsWarning && <p className="error mu-warn">{d.weightsWarning}</p>}
    </section>
  );
}

function MatchupRow(props: {
  m: Matchup;
  left: Team;
  right: Team;
  maxGap: number;
  open: boolean;
  onToggle: () => void;
}) {
  const { m, left, right, maxGap, open, onToggle } = props;
  const pct = m.diff === null ? 0 : Math.min(50, (Math.abs(m.diff) / maxGap) * 50);
  const leftWins = m.winner === "left";
  const rightWins = m.winner === "right";

  const classes = ["mu-row"];
  if (m.involvesMe) classes.push("mine");
  if (m.decisive) classes.push("decisive");
  if (open) classes.push("open");

  return (
    <>
      <button className={classes.join(" ")} onClick={onToggle} aria-expanded={open}>
        <span className="mu-class">
          {capitalize(m.class)}
          {m.involvesMe && <span className="you-tag">you</span>}
        </span>

        <SideCell side={m.left} align="left" winning={leftWins} />

        <span className="mu-center">
          <span className="bar">
            <span className="bar-mid" />
            {m.diff !== null && m.winner !== "even" && (
              <span
                className={`bar-fill team-bg-${(leftWins ? left : right).toLowerCase()}`}
                style={leftWins ? { right: "50%", width: `${pct}%` } : { left: "50%", width: `${pct}%` }}
              />
            )}
          </span>
          <span className="h2h" title="Kills on each other, read straight from the log">
            {m.headToHead ? (
              <>
                <b className={m.headToHead[0] > m.headToHead[1] ? "h2h-win" : ""}>{m.headToHead[0]}</b>
                <span className="dash">–</span>
                <b className={m.headToHead[1] > m.headToHead[0] ? "h2h-win" : ""}>{m.headToHead[1]}</b>
              </>
            ) : (
              <span className="muted">{m.left && m.right ? "n/a" : ""}</span>
            )}
          </span>
        </span>

        <SideCell side={m.right} align="right" winning={rightWins} />

        <span className="mu-tag">
          {m.decisive ? <span className="decisive-tag">decisive</span> : m.winner === "even" ? <span className="muted">even</span> : null}
        </span>
      </button>

      {open && <Breakdown m={m} left={left} right={right} />}
    </>
  );
}

function SideCell({ side, align, winning }: { side: Side | null; align: "left" | "right"; winning: boolean }) {
  if (!side) {
    return <span className={`mu-side ${align} muted`}>nobody</span>;
  }
  const name = (
    <span className="mu-name" title={side.subs.length ? `Also played: ${side.subs.join(", ")}` : undefined}>
      {side.name}
      {side.subs.length > 0 && <span className="sub-tag">+{side.subs.length}</span>}
    </span>
  );
  const score = <span className={winning ? "mu-score win" : "mu-score"}>{side.value.score.toFixed(1)}</span>;
  return <span className={`mu-side ${align}`}>{align === "left" ? <>{name}{score}</> : <>{score}{name}</>}</span>;
}

/** The value terms for both sides, so any winner call can be checked by hand. */
function Breakdown({ m, left, right }: { m: Matchup; left: Team; right: Team }) {
  const rows: Array<[string, (v: Value) => number, string]> = [
    ["Impact kills", (v) => v.impactKills, "Kills weighted by victim class: a Medic counts 3.0, a Scout 1.0"],
    ["Impact assists", (v) => v.impactAssists, "Assists, weighted the same way, at half value"],
    ["Death cost", (v) => -v.deathCost, "Deaths weighted by your own class: dying as Medic costs most"],
    ["Medic term", (v) => v.medicTerm, "Healing, ubers and drops (Medic only)"],
  ];
  const lv = m.left?.value;
  const rv = m.right?.value;

  return (
    <div className="mu-breakdown">
      <table>
        <thead>
          <tr>
            <th>per 10 min</th>
            <th className={`num team-${left.toLowerCase()}`}>{m.left?.name ?? teamLabel(left)}</th>
            <th className={`num team-${right.toLowerCase()}`}>{m.right?.name ?? teamLabel(right)}</th>
          </tr>
        </thead>
        <tbody>
          {rows
            .filter(([, f]) => (lv && f(lv) !== 0) || (rv && f(rv) !== 0))
            .map(([label, f, help]) => (
              <tr key={label}>
                <td title={help}>{label}</td>
                <td className="num">{lv ? signed(f(lv), 2) : "—"}</td>
                <td className="num">{rv ? signed(f(rv), 2) : "—"}</td>
              </tr>
            ))}
          <tr className="total">
            <td>Score</td>
            <td className="num">{lv ? lv.score.toFixed(2) : "—"}</td>
            <td className="num">{rv ? rv.score.toFixed(2) : "—"}</td>
          </tr>
          <tr>
            <td>K / D / A</td>
            <td className="num">{m.left ? `${m.left.kills} / ${m.left.deaths} / ${m.left.assists}` : "—"}</td>
            <td className="num">{m.right ? `${m.right.kills} / ${m.right.deaths} / ${m.right.assists}` : "—"}</td>
          </tr>
          <tr>
            <td>Damage</td>
            <td className="num">{m.left ? m.left.dmg.toLocaleString() : "—"}</td>
            <td className="num">{m.right ? m.right.dmg.toLocaleString() : "—"}</td>
          </tr>
          <tr>
            <td>Minutes</td>
            <td className="num">{lv ? lv.minutes.toFixed(1) : "—"}</td>
            <td className="num">{rv ? rv.minutes.toFixed(1) : "—"}</td>
          </tr>
        </tbody>
      </table>
      {(lv?.approximate || rv?.approximate) && (
        <p className="hint">
          Approximate: someone here mainly played another class, and logs.tf only records who you
          killed per player, not per class — so their kills were weighted as average.
        </p>
      )}
    </div>
  );
}
