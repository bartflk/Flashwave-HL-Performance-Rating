import type { MatchDetail } from "../../api/types";
import { Help } from "../StatTip";
import { capitalize, formatDate, minutes, splitMap } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { SortTh, useSort } from "../../lib/sort";

/**
 * The per-round logs a combined log was built from.
 *
 * Only the combined log is a match here: counting a part as well would
 * double every kill in it. The parts are still worth seeing, so each one
 * links to logs.tf.
 */
export function Parts({ d }: { d: MatchDetail }) {
  const sort = useSort("log-parts", d.parts, {
    log: (p) => p.logId,
    when: (p) => p.playedAt,
    map: (p) => splitMap(p.map).name ?? null,
    length: (p) => p.durationS ?? null,
    players: (p) => p.playerCount ?? null,
    title: (p) => p.title ?? null,
  });
  if (d.parts.length === 0) return null;

  return (
    <section className="panel parts">
      <header className="parts-head">
        <h2>
          {tx("Combined from {parts} logs", { parts: d.parts.length })}
          <Help>{t("Someone uploaded these rounds as one log. The parts are not counted again here, so your totals stay right; open one on logs.tf to see it on its own.")}</Help>
        </h2>
      </header>
      <div className="table-wrap">
        <table className="match-table">
          <thead>
            <tr>
              <SortTh k="log" s={sort}>{t("Log")}</SortTh>
              <SortTh k="when" s={sort}>{t("When")}</SortTh>
              <SortTh k="map" s={sort}>{t("Map")}</SortTh>
              <SortTh k="length" s={sort} className="num">{t("Length")}</SortTh>
              <SortTh k="players" s={sort} className="num">{t("Players")}</SortTh>
              <SortTh k="title" s={sort}>{t("Title")}</SortTh>
            </tr>
          </thead>
          <tbody>
            {sort.sorted.map((p) => {
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
                    <span className={name ? "" : "muted"}>{name ? capitalize(name) : t("unknown")}</span>
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
