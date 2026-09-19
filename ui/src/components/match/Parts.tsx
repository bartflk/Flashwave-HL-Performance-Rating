import type { MatchDetail } from "../../api/types";
import { capitalize, formatDate, minutes, splitMap } from "../../lib/format";

/**
 * The per-round logs a combined log was built from.
 *
 * Only the combined log is a match here: counting a part as well would
 * double every kill in it. The parts are still worth seeing, so each one
 * links to logs.tf.
 */
export function Parts({ d }: { d: MatchDetail }) {
  if (d.parts.length === 0) return null;

  return (
    <section className="panel parts">
      <header className="parts-head">
        <h2>Combined from {d.parts.length} logs</h2>
        <p className="hint">
          Someone uploaded these rounds as one log. The parts are not counted again here, so your totals stay
          right; open one on logs.tf to see it on its own.
        </p>
      </header>
      <div className="table-wrap">
        <table className="match-table">
          <thead>
            <tr>
              <th>Log</th>
              <th>When</th>
              <th>Map</th>
              <th className="num">Length</th>
              <th className="num">Players</th>
              <th>Title</th>
            </tr>
          </thead>
          <tbody>
            {d.parts.map((p) => {
              const { mode, name } = splitMap(p.map);
              return (
                <tr key={p.logId}>
                  <td>
                    <a className="part-link" href={`https://logs.tf/${p.logId}`} target="_blank" rel="noreferrer">
                      #{p.logId}
                    </a>
                  </td>
                  <td className="muted nowrap">{formatDate(p.playedAt, true)}</td>
                  <td className="nowrap">
                    {mode && <span className={`mode mode-${mode}`}>{mode}</span>}
                    <span className={name ? "" : "muted"}>{name ? capitalize(name) : "unknown"}</span>
                  </td>
                  <td className="num">{p.durationS ? minutes(p.durationS) : <span className="muted">-</span>}</td>
                  <td className="num">{p.playerCount ?? <span className="muted">-</span>}</td>
                  <td className="muted part-title">{p.title ?? ""}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
