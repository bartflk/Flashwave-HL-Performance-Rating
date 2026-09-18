import { useState } from "react";
import type { MatchDetail, Matchup, Part, Side, Team } from "../../api/types";
import { capitalize, teamLabel } from "../../lib/format";

/**
 * The nine class matchups: the headline of the match page.
 *
 * Two different kinds of number live here and the UI keeps them apart:
 * the head-to-head is read straight from the log and is objective; the rating
 * is a model, and every component of it is one click away.
 */
export function Matchups({ d }: { d: MatchDetail }) {
  const [open, setOpen] = useState<string | null>(null);

  const left = d.leftTeam;
  const right: Team = left === "Red" ? "Blue" : "Red";
  const us = d.myTeam !== null;

  // Scale bars to the biggest gap in this match, with a floor so a match of
  // small gaps does not render them as dramatic.
  const maxGap = Math.max(15, ...d.matchups.map((m) => Math.abs(m.diff ?? 0)));

  const wonByLeft = d.matchups.filter((m) => m.winner === "left").length;
  const wonByRight = d.matchups.filter((m) => m.winner === "right").length;
  const even = d.matchups.filter((m) => m.winner === "even").length;

  return (
    <section className="panel mu">
      <header className="mu-head">
        <div>
          <h2>Class matchups</h2>
          <p className="hint">
            {d.rated ? (
              <>
                {us ? "You" : teamLabel(left)} won {wonByLeft}, {us ? "they" : teamLabel(right)} won {wonByRight}
                {even > 0 && `, ${even} even`}. Ratings measure individual play, not who won the match.
                Click a row for the working.
              </>
            ) : (
              "Not rated yet — ratings appear after the next Sync or Rebuild."
            )}
          </p>
        </div>
        <span
          className="model-tag"
          title="Each component is a percentile against every other player's games on that class in your stored matches (yours excluded). The rating is their weighted average, 0-100; 50 is the typical player you face."
        >
          model {d.modelVersion} · 0–100 vs players you face
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
          {m.decisive ? (
            <span className="decisive-tag">decisive</span>
          ) : m.winner === "even" ? (
            <span className="muted">even</span>
          ) : null}
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
  const score = (
    <span
      className={winning ? "mu-score win" : "mu-score"}
      title={side.rating ? undefined : "Not rated: nobody here had this as their main class for 5+ minutes"}
    >
      {side.rating ? side.rating.score.toFixed(0) : "—"}
    </span>
  );
  return <span className={`mu-side ${align}`}>{align === "left" ? <>{name}{score}</> : <>{score}{name}</>}</span>;
}

/** Every component for both players: the raw number, and where it sits
 *  among the players you face. Enough to check any winner call by hand. */
function Breakdown({ m, left, right }: { m: Matchup; left: Team; right: Team }) {
  const lr = m.left?.rating ?? null;
  const rr = m.right?.rating ?? null;
  const rows = (lr ?? rr)?.parts ?? [];
  const find = (parts: Part[] | undefined, key: string) => parts?.find((p) => p.component === key);

  if (!lr && !rr) {
    return (
      <div className="mu-breakdown">
        <p className="hint">Neither side is rated here.</p>
      </div>
    );
  }

  return (
    <div className="mu-breakdown">
      <table>
        <thead>
          <tr>
            <th>Component</th>
            <th className="num">weight</th>
            <th className={`num team-${left.toLowerCase()}`}>{m.left?.name ?? teamLabel(left)}</th>
            <th className="num" />
            <th className={`num team-${right.toLowerCase()}`}>{m.right?.name ?? teamLabel(right)}</th>
            <th className="num" />
          </tr>
        </thead>
        <tbody>
          {rows.map((p) => {
            const a = find(lr?.parts, p.component);
            const b = find(rr?.parts, p.component);
            return (
              <tr key={p.component}>
                <td title={p.unit}>
                  {p.label} <span className="unit">{p.unit}</span>
                </td>
                <td className="num muted">{Math.round(p.weight * 100)}%</td>
                <td className="num">{a ? fmtRaw(a) : "—"}</td>
                <td className="num pct">{a ? pctLabel(a.percentile) : ""}</td>
                <td className="num">{b ? fmtRaw(b) : "—"}</td>
                <td className="num pct">{b ? pctLabel(b.percentile) : ""}</td>
              </tr>
            );
          })}
          <tr className="total">
            <td>Rating</td>
            <td />
            <td className="num">{lr ? lr.score.toFixed(1) : "—"}</td>
            <td />
            <td className="num">{rr ? rr.score.toFixed(1) : "—"}</td>
            <td />
          </tr>
          <tr>
            <td>K / D / A</td>
            <td />
            <td className="num">{m.left ? `${m.left.kills} / ${m.left.deaths} / ${m.left.assists}` : "—"}</td>
            <td />
            <td className="num">{m.right ? `${m.right.kills} / ${m.right.deaths} / ${m.right.assists}` : "—"}</td>
            <td />
          </tr>
        </tbody>
      </table>
      <p className="hint">
        The small number is the percentile: how this compares with every other player on the class
        in your matches. It is flipped for deaths, so higher is always better.
      </p>
    </div>
  );
}

function fmtRaw(p: Part): string {
  if (p.component === "headshot_share") return `${p.raw.toFixed(0)}%`;
  if (p.component === "heal" || p.component === "dpm") return p.raw.toFixed(0);
  return p.raw.toFixed(2);
}

function pctLabel(pct: number): string {
  return `p${Math.round(pct)}`;
}
