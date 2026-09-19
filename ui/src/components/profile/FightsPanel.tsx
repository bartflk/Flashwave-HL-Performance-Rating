import type { FightLine, FightsCard } from "../../api/types";
import { capitalize } from "../../lib/format";

/**
 * Your kills in context against the players you face on the class: who opens
 * fights, whose kills get traded straight back, who dies right after their
 * own kill. Same filters as the rest of the profile.
 */
export function FightsPanel({ card, cls }: { card: FightsCard; cls: string }) {
  return (
    <section className="panel fights-panel">
      <header>
        <h2>Fights</h2>
        <p className="hint">
          Your {card.games} rated {capitalize(cls)} games against {card.poolGames.toLocaleString()} games by the{" "}
          {capitalize(cls)}s you have faced. A fight starts after 10 s without a kill; a trade is a kill back within 3 s.
        </p>
      </header>
      <div className="table-wrap">
        <table className="match-table fights-card">
          <thead>
            <tr>
              <th />
              <th className="num">You</th>
              <th className="num">Players you face</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {card.lines.map((l) => (
              <tr key={l.label} title={l.hint}>
                <td>
                  {l.label} <span className="muted">· {l.unit}</span>
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
  if (Math.abs(rel) < 0.05) return <span className="muted">about the same</span>;
  const good = rel * l.better > 0;
  return (
    <span className={good ? "verdict good" : "verdict bad"}>
      {good ? "▲ better" : "▼ worse"} by {Math.round(Math.abs(rel) * 100)}%
    </span>
  );
}
