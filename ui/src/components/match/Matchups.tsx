import { useState } from "react";
import type { MatchDetail, Matchup, Part, Side, Team } from "../../api/types";
import { capitalize, teamLabel } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";

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
          <ClassIcon cls={m.class} size={22} />
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

/** Every component for both players, mirrored: the raw number, and a bar for
 *  where it sits among the players you face. Weights sum to one, so each
 *  row's swing (weight x percentile gap) adds up to the rating gap. */
function Breakdown({ m, left, right }: { m: Matchup; left: Team; right: Team }) {
  const lr = m.left?.rating ?? null;
  const rr = m.right?.rating ?? null;
  const find = (parts: Part[] | undefined, key: string) => parts?.find((p) => p.component === key);

  if (!lr && !rr) {
    return (
      <div className="mu-breakdown">
        <p className="hint">Neither side is rated here.</p>
      </div>
    );
  }

  const rows = [...((lr ?? rr)?.parts ?? [])]
    .sort((x, y) => y.weight - x.weight)
    .map((p) => {
      const a = find(lr?.parts, p.component) ?? null;
      const b = find(rr?.parts, p.component) ?? null;
      const swing = a && b ? p.weight * (a.percentile - b.percentile) : null;
      return { p, a, b, swing };
    });

  // The rows that moved the gap most, in the leader's favour.
  const leader = lr && rr ? (lr.score >= rr.score ? "left" : "right") : null;
  const drivers = rows
    .filter((r) => r.swing !== null && (leader === "left" ? r.swing > 0.5 : leader === "right" && r.swing < -0.5))
    .sort((x, y) => Math.abs(y.swing!) - Math.abs(x.swing!))
    .slice(0, 3);
  const lc = left.toLowerCase();
  const rc = right.toLowerCase();

  return (
    <div className="mu-breakdown">
      <div className="bd">
        <div className="bd-head">
          <ScoreCard side={m.left} team={lc} align="left" lead={leader === "left"} />
          <div className="bd-vs">
            <span className="bd-vs-label">Rating</span>
            {lr && rr && (
              <span className="bd-gap">
                {Math.abs(lr.score - rr.score).toFixed(1)} <small>pts apart</small>
              </span>
            )}
          </div>
          <ScoreCard side={m.right} team={rc} align="right" lead={leader === "right"} />
        </div>

        {drivers.length > 0 && (
          <p className="bd-drivers">
            <span className="muted">Decided by </span>
            {drivers.map((r, i) => (
              <span key={r.p.component}>
                {i > 0 && <span className="muted">, </span>}
                <b>{r.p.label}</b>{" "}
                <span className={`team-${leader === "left" ? lc : rc}`}>+{Math.abs(r.swing!).toFixed(1)}</span>
              </span>
            ))}
          </p>
        )}

        <div className="bd-rows">
          {rows.map(({ p, a, b, swing }) => (
            <div className="bd-row" key={p.component} title={`${p.label}, ${p.unit}. Weight ${Math.round(p.weight * 100)}%.`}>
              <span className={a && b && a.percentile > b.percentile ? "bd-val better" : "bd-val"}>{a ? fmtRaw(a) : "—"}</span>
              <Bar part={a} team={lc} align="left" better={!!a && (!b || a.percentile >= b.percentile)} />
              <span className="bd-label">
                <span className="bd-name">{p.label}</span>
                <span className="bd-meta">
                  {p.unit} · {Math.round(p.weight * 100)}%
                  {swing !== null && Math.abs(swing) >= 0.1 && (
                    <span className={`bd-swing team-${swing > 0 ? lc : rc}`}>
                      {" "}
                      · {swing > 0 ? "◂" : ""}+{Math.abs(swing).toFixed(1)}
                      {swing < 0 ? "▸" : ""}
                    </span>
                  )}
                </span>
              </span>
              <Bar part={b} team={rc} align="right" better={!!b && (!a || b.percentile >= a.percentile)} />
              <span className={a && b && b.percentile > a.percentile ? "bd-val right better" : "bd-val right"}>
                {b ? fmtRaw(b) : "—"}
              </span>
            </div>
          ))}
        </div>
      </div>
      <p className="hint bd-hint">
        Bars are the percentile against every player on the class in your matches, flipped for deaths so longer is
        always better. The coloured number is how many rating points that row swung, and to whom.
      </p>
    </div>
  );
}

function ScoreCard(props: { side: Side | null; team: string; align: "left" | "right"; lead: boolean }) {
  const { side, team, align, lead } = props;
  const r = side?.rating ?? null;
  return (
    <div className={`bd-card ${align}${lead ? " lead" : ""}`}>
      <span className={`bd-card-name team-${team}`}>{side?.name ?? "nobody"}</span>
      <span className="bd-card-score">{r ? r.score.toFixed(1) : "—"}</span>
      {side && (
        <span className="bd-card-kda">
          <b>{side.kills}</b> K · <b>{side.deaths}</b> D · <b>{side.assists}</b> A
        </span>
      )}
    </div>
  );
}

function Bar(props: { part: Part | null; team: string; align: "left" | "right"; better: boolean }) {
  const { part, team, align, better } = props;
  const pct = part ? Math.max(0, Math.min(100, part.percentile)) : 0;
  return (
    <span className={`bd-bar ${align}`}>
      {part && (
        <>
          <span className={`bd-fill fill-${team}${better ? "" : " dim"}`} style={{ width: `${Math.max(pct, 2)}%` }} />
          <span className="bd-pct">p{Math.round(pct)}</span>
        </>
      )}
    </span>
  );
}

function fmtRaw(p: Part): string {
  if (p.unit.startsWith("%")) return `${p.raw.toFixed(0)}%`;
  if (p.component === "heal" || p.component === "dpm") return p.raw.toFixed(0);
  return p.raw.toFixed(2);
}
