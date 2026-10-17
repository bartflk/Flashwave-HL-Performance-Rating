import { useState } from "react";
import type { Analysis } from "../../api/types";
import { CLASS_ORDER, DEATH, KILL, classLabel, playerMap, sliceLabel, type Slice } from "./common";
import { ClassIcon } from "../ClassIcon";
import { t as tr, tx } from "../../lib/i18n";
import { SortTh, useSort } from "../../lib/sort";

/**
 * Who the player hurt and who hurt them, class by class: back-to-back bars
 * with what was done to them on the left (orange) and what they did on the
 * right (blue). Side and colour carry the same meaning, so neither is needed
 * alone. Scoped to whatever the filter row selects — the whole match, one
 * map of a combined log, or one round.
 */
export function Spread({ a, player, slice }: { a: Analysis; player: number; slice: Slice }) {
  const [asTable, setAsTable] = useState(false);
  const p = playerMap(a).get(player);
  const inSlice = (round: number) => slice.rounds === null || slice.rounds.has(round);

  const dmg = CLASS_ORDER.map((c) => {
    const rows = a.damage.filter((d) => d.accountId === player && d.otherClass === c && inSlice(d.roundNum));
    return {
      cls: c,
      left: rows.reduce((n, r) => n + r.taken, 0),
      right: rows.reduce((n, r) => n + r.dealt, 0),
    };
  });
  const mine = a.kills.filter((k) => inSlice(k.roundNum));
  const kills = CLASS_ORDER.map((c) => ({
    cls: c,
    left: mine.filter((k) => k.victim === player && k.killer !== player && k.killerClass === c).length,
    right: mine.filter((k) => k.killer === player && k.victim !== player && k.victimClass === c).length,
  }));

  return (
    <div className="spread">
      <div className="spread-head">
        <p className="hint">
          {tx("{0} against each enemy class, {1}.{2}", { "0": p?.name ?? tr("The player"), "1": sliceLabel(slice), "2": !a.damageCapped && tr(" This log predates logs.tf's 450-per-hit cap, so backstabs count in full.") })}
        </p>
        <button className="linkish" onClick={() => setAsTable((t) => !t)}>
          {asTable ? tr("Show bars") : tr("Show as table")}
        </button>
      </div>
      <div className="spread-grid">
        <Butterfly
          title={tr("Damage spread")}
          leftLabel={tr("Damage taken")}
          rightLabel={tr("Damage dealt")}
          rows={dmg}
          asTable={asTable}
          format={(n) => n.toLocaleString()}
        />
        <Butterfly
          title={tr("Kill spread")}
          leftLabel={tr("Deaths")}
          rightLabel={tr("Kills")}
          rows={kills}
          asTable={asTable}
          format={(n) => String(n)}
        />
      </div>
      <p className="hint spread-foot">{tr("On combined logs, damage taken can differ from logs.tf's figure.")}</p>
    </div>
  );
}

function Butterfly(props: {
  title: string;
  leftLabel: string;
  rightLabel: string;
  rows: Array<{ cls: string; left: number; right: number }>;
  asTable: boolean;
  format: (n: number) => string;
}) {
  const { title, leftLabel, rightLabel, rows, asTable, format } = props;
  const sort = useSort(`spread-${title}`, rows, { cls: (r) => classLabel(r.cls, true), left: (r) => r.left, right: (r) => r.right });
  // One scale for both sides, so a long bar on either side means the same.
  const max = Math.max(1, ...rows.flatMap((r) => [r.left, r.right]));
  const totalL = rows.reduce((n, r) => n + r.left, 0);
  const totalR = rows.reduce((n, r) => n + r.right, 0);

  return (
    <section className="butterfly">
      <header className="bf-head">
        <h3>{title}</h3>
      </header>
      <div className="bf-cols" aria-hidden>
        <span className="bf-col bf-col-l" style={{ color: DEATH }}>
          {leftLabel} <b>{format(totalL)}</b>
        </span>
        <span className="bf-col bf-col-r" style={{ color: KILL }}>
          {rightLabel} <b>{format(totalR)}</b>
        </span>
      </div>
      {asTable ? (
        <table className="match-table bf-table">
          <thead>
            <tr>
              <SortTh k="cls" s={sort}>{tr("Class")}</SortTh>
              <SortTh k="left" s={sort} className="num">{leftLabel}</SortTh>
              <SortTh k="right" s={sort} className="num">{rightLabel}</SortTh>
            </tr>
          </thead>
          <tbody>
            {sort.sorted.map((r) => (
              <tr key={r.cls}>
                <td>{classLabel(r.cls, true)}</td>
                <td className="num">{format(r.left)}</td>
                <td className="num">{format(r.right)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div className="bf-rows" role="img" aria-label={tr("{title} {leftLabel} and {rightLabel} by class", { title: title, leftLabel: leftLabel, rightLabel: rightLabel })}>
          {rows.map((r) => (
            <div className="bf-row" key={r.cls} title={`${classLabel(r.cls, true)}: ${leftLabel} ${format(r.left)}, ${rightLabel} ${format(r.right)}`}>
              <span className="bf-val bf-val-l">{r.left > 0 ? format(r.left) : ""}</span>
              <span className="bf-side bf-l">
                <span className="bf-bar" style={{ width: `${(r.left / max) * 100}%`, background: DEATH }} />
              </span>
              <span className="bf-cls">
                <ClassIcon cls={r.cls} size={20} faded={r.left === 0 && r.right === 0} />
              </span>
              <span className="bf-side bf-r">
                <span className="bf-bar" style={{ width: `${(r.right / max) * 100}%`, background: KILL }} />
              </span>
              <span className="bf-val bf-val-r">{r.right > 0 ? format(r.right) : ""}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
