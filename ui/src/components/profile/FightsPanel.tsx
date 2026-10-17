import type { FightLine, FightsCard } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { classLabel } from "../analysis/common";
import { SortTh, useSort } from "../../lib/sort";
import { StatTip } from "../StatTip";
import { tip } from "../../lib/glossary";

/**
 * Your kills in context against the players you face on the class: who opens
 * fights, whose kills get traded straight back, who dies right after their
 * own kill. Same filters as the rest of the profile.
 */
export function FightsPanel({ card, cls }: { card: FightsCard; cls: string }) {
  // Each row is its own statistic in its own unit: it sorts by name, by the
  // two values, or by how far better or worse than the pool (Q79).
  const sort = useSort("profile-fights", card.lines, {
    label: (l) => t(l.label),
    you: (l) => l.you,
    pool: (l) => l.pool,
    verdict: (l) => (l.you === null || l.pool === null || l.pool === 0 || l.better === 0 ? null : ((l.you - l.pool) / Math.abs(l.pool)) * l.better),
  });
  return (
    <section className="panel fights-panel">
      <header>
        <h2>{t("Fights")}</h2>
        <p className="hint">{tx("Your {games} games against {1} by the {2}s you have faced.", { "1": card.poolGames.toLocaleString(), "2": classLabel(cls), games: card.games })}</p>
      </header>
      <div className="table-wrap">
        <table className="match-table fights-card">
          <thead>
            <tr>
              <SortTh k="label" s={sort}>{t("Statistic")}</SortTh>
              <SortTh k="you" s={sort} className="num">{t("You")}</SortTh>
              <SortTh k="pool" s={sort} className="num">{t("Players you face")}</SortTh>
              <SortTh k="verdict" s={sort} tip={tip(t("Against them"), t("How far better or worse than the players you face, in percent of their number."), t("Within 5% either way reads as about the same."), "server log")}>{t("Against them")}</SortTh>
            </tr>
          </thead>
          <tbody>
            {sort.sorted.map((l) => (
              <tr key={l.label}>
                <td>
                  <StatTip entry={tip(t(l.label), t(l.hint), t("Your games on this class against the opposite numbers you faced, from the server logs of your matches."), "server log")}>{t(l.label)}</StatTip>{" "}
                  <span className="muted">· {t(l.unit)}</span>
                </td>
                <td className="num">{fmt(l, l.you)}</td>
                <td className="num muted">{fmt(l, l.pool)}</td>
                <td>
                  <Verdict l={l} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function fmt(l: FightLine, v: number | null): string {
  if (v === null) return "–";
  // Small shares keep a decimal, or 4.6% and 5.2% would both read "5%".
  if (l.unit.startsWith("%")) return `${v.toFixed(v < 10 ? 1 : 0)}%`;
  return v.toFixed(2);
}

/** Better or worse than the pool, in words and an arrow, never colour alone. */
function Verdict({ l }: { l: FightLine }) {
  if (l.you === null || l.pool === null || l.better === 0 || l.pool === 0) return null;
  const rel = (l.you - l.pool) / Math.abs(l.pool);
  if (Math.abs(rel) < 0.05) return <span className="muted">{t("about the same")}</span>;
  const good = rel * l.better > 0;
  return (
    <span className={good ? "verdict good" : "verdict bad"}>
      {tx("{0} by {1}%", { "0": good ? t("▲ better") : t("▼ worse"), "1": Math.round(Math.abs(rel) * 100) })}</span>
  );
}
