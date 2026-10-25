import { useState } from "react";
import type { MatchDetail, MatchDivisions, Matchup, Part, Side, Team } from "../../api/types";
import { rating, RATING_GAP_FULL, teamLabel } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";
import { t, tx } from "../../lib/i18n";
import { classLabel } from "../analysis/common";
import { goTo } from "../../lib/goto";
import { DivTag, SideDivisions, useMatchDivisions } from "./Divisions";
import { PlayerPopCard, nameProps, usePlayerCard } from "../players/PlayerPopCard";
import { Help, ComponentTip, StatTip } from "../StatTip";

/**
 * The nine class matchups: the headline of the match page.
 *
 * Two different kinds of number live here and the UI keeps them apart:
 * the head-to-head is read straight from the log and is objective; the rating
 * is a model, and every component of it is one click away.
 */
export function Matchups({ d }: { d: MatchDetail }) {
  const [open, setOpen] = useState<string | null>(null);
  const divisions = useMatchDivisions(d.logId);
  // Click a name: their card (Q40).
  const card = usePlayerCard();
  const isMe = (accountId: number) => d.players.some((p) => p.accountId === accountId && p.isMe);

  const left = d.leftTeam;
  const right: Team = left === "Red" ? "Blue" : "Red";
  const us = d.myTeam !== null;

  // Scale bars to the biggest gap in this match, with a floor so a match of
  // small gaps does not render them as dramatic.
  // The widest gap in this match, but never so small that an even match
  // draws a full bar: RATING_GAP_FULL is two standard deviations.
  const maxGap = Math.max(RATING_GAP_FULL, ...d.matchups.map((m) => Math.abs(m.diff ?? 0)));

  const wonByLeft = d.matchups.filter((m) => m.winner === "left").length;
  const wonByRight = d.matchups.filter((m) => m.winner === "right").length;
  const even = d.matchups.filter((m) => m.winner === "even").length;

  return (
    <section className="panel mu">
      <header className="mu-head">
        <div>
          <h2>
            {t("Class matchups")}
            <Help>{t("Ratings measure individual play, not who won the match. Click a row for the working.")}</Help>
          </h2>
          <p className="hint">
            {d.rated ? (
              <>
                {tx("{0} won {wonByLeft}, {2} won {wonByRight}{4}.", { "0": us ? t("You") : teamLabel(left), "2": us ? t("they") : teamLabel(right), "4": even > 0 && t(", {even} even", { even: even }), wonByLeft: wonByLeft, wonByRight: wonByRight })}</>
            ) : (
              t("Not rated yet — ratings appear after the next Sync or Rebuild.")
            )}
          </p>
          <SideDivisions d={d} divisions={divisions} />
        </div>
        <div className="mu-head-right">
          <span className="model-tag">
            <StatTip k="rating">{tx("model {modelVersion} · one model per class", { modelVersion: d.modelVersion })}</StatTip>
          </span>
          <button className="km-chip mu-guide-link" onClick={() => goTo("rating")}>{t("How ratings work →")}</button>
        </div>
      </header>

      <div className="mu-cols" aria-hidden>
        <span />
        <span className={`team-${left.toLowerCase()} mu-col-l`}>
          {us ? t("Us") : ""} {teamLabel(left)}
        </span>
        <span className="mu-col-c">{t("head-to-head")}</span>
        <span className={`team-${right.toLowerCase()} mu-col-r`}>
          {teamLabel(right)} {us ? t("Them") : ""}
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
          perPct={d.ratingPerPercentile}
          open={open === m.class}
          onToggle={() => setOpen(open === m.class ? null : m.class)}
          divisions={divisions}
          onName={(side, cls) => card.open({ accountId: side.accountId, name: side.name, cls, isMe: isMe(side.accountId) })}
        />
      ))}
      {card.target && <PlayerPopCard target={card.target} divisions={divisions} onClose={card.close} />}

      {d.weightsWarning && <p className="error mu-warn">{d.weightsWarning}</p>}
    </section>
  );
}

function MatchupRow(props: {
  m: Matchup;
  left: Team;
  right: Team;
  maxGap: number;
  perPct: number;
  open: boolean;
  onToggle: () => void;
  divisions: MatchDivisions | undefined;
  onName: (side: Side, cls: string) => (e: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>) => void;
}) {
  const { m, left, right, maxGap, perPct, open, onToggle, divisions, onName } = props;
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
          {classLabel(m.class)}
          {m.involvesMe && <span className="you-tag">{t("you")}</span>}
        </span>

        <SideCell side={m.left} align="left" winning={leftWins} divisions={divisions} onName={m.left ? onName(m.left, m.class) : undefined} />

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
          <span className="h2h" title={t("Kills on each other, read straight from the log")}>
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

        <SideCell side={m.right} align="right" winning={rightWins} divisions={divisions} onName={m.right ? onName(m.right, m.class) : undefined} />

        <span className="mu-tag">
          {m.decisive ? (
            <span className="decisive-tag">{t("decisive")}</span>
          ) : m.winner === "even" ? (
            <span className="muted">{t("even")}</span>
          ) : null}
        </span>
      </button>

      {open && <Breakdown m={m} left={left} right={right} perPct={perPct} />}
    </>
  );
}

function SideCell({
  side,
  align,
  winning,
  divisions,
  onName,
}: {
  side: Side | null;
  align: "left" | "right";
  winning: boolean;
  divisions: MatchDivisions | undefined;
  onName?: (e: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>) => void;
}) {
  if (!side) {
    return <span className={`mu-side ${align} muted`}>{t("nobody")}</span>;
  }
  const name = (
    <span className="mu-name" title={side.subs.length ? t("Also played: {0}", { "0": side.subs.join(", ") }) : undefined}>
      {onName ? <span {...nameProps(onName)}>{side.name}</span> : side.name}
      <DivTag divisions={divisions} accountId={side.accountId} />
      {side.subs.length > 0 && <span className="sub-tag">+{side.subs.length}</span>}
    </span>
  );
  const score = (
    <span
      className={winning ? "mu-score win" : "mu-score"}
      title={side.rating ? undefined : t("Not rated: nobody here had this as their main class for 5+ minutes")}
    >
      {side.rating ? rating(side.rating.score) : "—"}
    </span>
  );
  return <span className={`mu-side ${align}`}>{align === "left" ? <>{name}{score}</> : <>{score}{name}</>}</span>;
}

/** Every component for both players, mirrored: the raw number, and a bar for
 *  where it sits among the players you face. Weights sum to one, so each
 *  row's swing — the percentile gap, weighted, in rating points — adds up to
 *  the gap between the two ratings. */
function Breakdown({ m, left, right, perPct }: { m: Matchup; left: Team; right: Team; perPct: number }) {
  const lr = m.left?.rating ?? null;
  const rr = m.right?.rating ?? null;
  const find = (parts: Part[] | undefined, key: string) => parts?.find((p) => p.component === key);

  if (!lr && !rr) {
    return (
      <div className="mu-breakdown">
        <p className="hint">{t("Neither side is rated here.")}</p>
      </div>
    );
  }

  const rows = [...((lr ?? rr)?.parts ?? [])]
    .sort((x, y) => y.weight - x.weight)
    .map((p) => {
      const a = find(lr?.parts, p.component) ?? null;
      const b = find(rr?.parts, p.component) ?? null;
      const swing = a && b ? p.weight * (a.percentile - b.percentile) * perPct : null;
      return { p, a, b, swing };
    });

  // The rows that moved the gap most, in the leader's favour.
  const leader = lr && rr ? (lr.score >= rr.score ? "left" : "right") : null;
  const drivers = rows
    // Worth calling out at about a hundredth of a rating point.
    .filter((r) => r.swing !== null && (leader === "left" ? r.swing > 0.01 : leader === "right" && r.swing < -0.01))
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
            <span className="bd-vs-label">{t("Rating")}</span>
            {lr && rr && (
              <span className="bd-gap">
                {Math.abs(lr.score - rr.score).toFixed(2)} <small>{t("apart")}</small>
              </span>
            )}
          </div>
          <ScoreCard side={m.right} team={rc} align="right" lead={leader === "right"} />
        </div>

        {drivers.length > 0 && (
          <p className="bd-drivers">
            <span className="muted">{t("Decided by")}{" "}</span>
            {drivers.map((r, i) => (
              <span key={r.p.component}>
                {i > 0 && <span className="muted">, </span>}
                <b>{t(r.p.label)}</b>{" "}
                <span className={`team-${leader === "left" ? lc : rc}`}>+{Math.abs(r.swing!).toFixed(1)}</span>
              </span>
            ))}
          </p>
        )}

        <div className="bd-rows">
          {rows.map(({ p, a, b, swing }) => (
            <div className="bd-row" key={p.component}>
              <span className={a && b && a.percentile > b.percentile ? "bd-val better" : "bd-val"}>{a ? fmtRaw(a) : "—"}</span>
              <Bar part={a} team={lc} align="left" better={!!a && (!b || a.percentile >= b.percentile)} />
              <span className="bd-label">
                <span className="bd-name">
                  <ComponentTip component={p.component} label={t(p.label)} weight={p.weight} />
                </span>
                <span className="bd-meta">
                  {t(p.unit)} · {Math.round(p.weight * 100)}%
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
      <p className="hint bd-hint">{t("Percentile against the players on this class in your matches; deaths flipped so longer is better.")}</p>
    </div>
  );
}

function ScoreCard(props: { side: Side | null; team: string; align: "left" | "right"; lead: boolean }) {
  const { side, team, align, lead } = props;
  const r = side?.rating ?? null;
  return (
    <div className={`bd-card ${align}${lead ? " lead" : ""}`}>
      <span className={`bd-card-name team-${team}`}>{side?.name ?? t("nobody")}</span>
      <span className="bd-card-score">{r ? rating(r.score) : "—"}</span>
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
