import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type H2hSide, type MatchDetail } from "../../api/types";
import { rating } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";
import { Loading } from "../Spinner";
import { Help, StatTip } from "../StatTip";

/**
 * Two players in one match, log by log (Flashy: "more granular info and a
 * breakdown on the ratings that I can inspect"): rating, impact, kills,
 * deaths, damage, who they killed and their duel on every log, and each
 * rating opened into its parts -- the raw number, where it stands among
 * everyone on that class and map, its weight, and what it added.
 */
export function HeadToHead({ d }: { d: MatchDetail }) {
  const players = d.players;
  const me = players.find((p) => p.isMe) ?? players[0];
  const rival = (a: (typeof players)[number] | undefined) =>
    players.find((p) => a && p.team !== a.team && p.mainClass === a.mainClass) ?? players.find((p) => a && p.team !== a.team);
  const [a, setA] = useState<number | null>(me?.accountId ?? null);
  const [b, setB] = useState<number | null>(rival(me)?.accountId ?? null);
  const [open, setOpen] = useState<number | null>(null);
  const q = useQuery({
    queryKey: ["head_to_head", d.logId, a, b],
    queryFn: () => api.getHeadToHead(d.logId, a!, b!),
    enabled: a !== null && b !== null && a !== b,
    staleTime: 5 * 60_000,
  });
  const name = (id: number | null) => players.find((p) => p.accountId === id)?.name ?? "?";
  const h = q.data;

  const totals = useMemo(() => {
    if (!h) return null;
    return [0, 1].map((i) => {
      const sides = h.logs.map((l) => l.sides[i]).filter((s): s is H2hSide => !!s);
      const mins = sides.reduce((n, s) => n + s.minutes, 0);
      const rated = sides.filter((s) => s.rating !== null);
      const rmins = rated.reduce((n, s) => n + s.minutes, 0);
      const victims = new Map<string, [number, number]>();
      for (const s of sides) for (const [c, n, v] of s.victims) victims.set(c, [(victims.get(c)?.[0] ?? 0) + n, v]);
      return {
        rating: rmins > 0 ? rated.reduce((n, s) => n + s.rating! * s.minutes, 0) / rmins : null,
        impact: sides.some((s) => s.impact !== null) ? sides.reduce((n, s) => n + (s.impact ?? 0), 0) : null,
        kills: sides.reduce((n, s) => n + s.kills, 0),
        deaths: sides.reduce((n, s) => n + s.deaths, 0),
        assists: sides.reduce((n, s) => n + s.assists, 0),
        dpm: mins > 0 ? (sides.reduce((n, s) => n + s.dmg, 0) / mins) : 0,
        onOther: sides.reduce((n, s) => n + s.onOther, 0),
        won: h.logs.filter((l) => l.sides[i] && l.winner === l.sides[i]!.team).length,
        victims: [...victims.entries()].sort((x, y) => y[1][0] - x[1][0]),
        value: [...victims.values()].reduce((n, [k, v]) => n + k * v, 0),
      };
    });
  }, [h]);

  const picker = (value: number | null, set: (v: number) => void, label: string) => (
    <select value={value ?? ""} onChange={(e) => set(Number(e.target.value))} aria-label={label}>
      {players.map((p) => (
        <option key={p.accountId} value={p.accountId}>
          {p.name} · {classLabel(p.mainClass)} · {p.team}
        </option>
      ))}
    </select>
  );

  return (
    <section className="panel hh">
      <header className="hh-head">
        <h2>
          {t("Head to head")}
          <Help>{t("Click a log for both ratings' working. ◀ ▶ marks the side that won the log. The duel is how often each killed the other.")}</Help>
        </h2>
        <div className="hh-pick">
          {picker(a, setA, t("First player"))}
          <span className="muted">{t("vs")}</span>
          {picker(b, setB, t("Second player"))}
        </div>
      </header>
      {a === b && <p className="hint">{t("Pick two different players.")}</p>}
      {q.isPending && a !== b && <Loading>{t("Rating every log again…")}</Loading>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {h && totals && (
        <>
          <div className="hh-totals">
            {[0, 1].map((i) => (
              <div key={i} className={`hh-card hh-${i === 0 ? "a" : "b"}`}>
                {/* One layout for both cards (Flashy: "the box design is very
                    inconsistant"): the name and what their kills were worth on
                    top, six numbers in fixed columns, then the kills by class
                    as tiles of one size. */}
                <header className="hh-card-head">
                  <strong className="hh-name">{name(h.accounts[i])}</strong>
                  <span className="hh-worth" title={t("Kills by the class that died, and what a kill of that class is worth in the rating")}>
                    <span className="hh-worth-label">{t("Kill value")}</span>
                    <strong>{totals[i].value.toFixed(1)}</strong>
                  </span>
                </header>
                <dl className="hh-stats">
                  <div>
                    <dt>
                      <StatTip k="rating" inline>
                        {t("Rating")}
                      </StatTip>
                    </dt>
                    <dd>{totals[i].rating === null ? "–" : rating(totals[i].rating)}</dd>
                  </div>
                  <div>
                    <dt>
                      <StatTip k="impact" inline>
                        {t("Impact")}
                      </StatTip>
                    </dt>
                    <dd>{totals[i].impact === null ? "–" : `${totals[i].impact! >= 0 ? "+" : ""}${totals[i].impact!.toFixed(2)}`}</dd>
                  </div>
                  <div>
                    <dt>{t("K / D / A")}</dt>
                    <dd>
                      {totals[i].kills} / {totals[i].deaths} / {totals[i].assists}
                    </dd>
                  </div>
                  <div>
                    <dt>{t("DA/M")}</dt>
                    <dd>{totals[i].dpm.toFixed(0)}</dd>
                  </div>
                  <div>
                    <dt>{t("Killed the other")}</dt>
                    <dd>{totals[i].onOther}</dd>
                  </div>
                  <div>
                    <dt>{t("Logs won")}</dt>
                    <dd>
                      {totals[i].won} / {h.logs.length}
                    </dd>
                  </div>
                </dl>
                <div className="hh-kills">
                  <span className="hh-kills-label">{t("Kills by class")}</span>
                  <div className="hh-victims" title={t("Kills by the class that died, and what a kill of that class is worth in the rating")}>
                    {totals[i].victims.map(([c, [n, v]]) => (
                      <span key={c} className="hh-victim" title={`${n} × ${v}`}>
                        <ClassIcon cls={c} size={15} />
                        <strong>{n}</strong>
                        <span className="muted">×{v}</span>
                      </span>
                    ))}
                  </div>
                </div>
              </div>
            ))}
          </div>

          <table className="match-table hh-table">
            <thead>
              <tr>
                <th>{t("Log")}</th>
                <th className="num">{name(h.accounts[0])}</th>
                <th className="num">{t("Impact")}</th>
                <th className="num">{t("K–D")}</th>
                <th className="num">{t("Duel")}</th>
                <th className="num">{t("K–D")}</th>
                <th className="num">{t("Impact")}</th>
                <th className="num">{name(h.accounts[1])}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {h.logs.map((l) => {
                const [x, y] = l.sides;
                const better = x?.rating != null && y?.rating != null ? (x.rating > y.rating ? 0 : x.rating < y.rating ? 1 : -1) : -1;
                const isOpen = open === l.logId;
                return [
                  <tr key={l.logId} className="hh-row" onClick={() => setOpen(isOpen ? null : l.logId)}>
                    <td>
                      <code>{(l.map ?? "?").replace(/_(final|rc|b)\w*$/i, "")}</code> <span className="muted">{Math.round(l.minutes)} min</span>
                      {x && l.winner && <span className={`hh-won ${l.winner === x.team ? "a" : "b"}`}>{l.winner === x.team ? "◀" : "▶"}</span>}
                    </td>
                    <td className={`num strong${better === 0 ? " hh-best" : ""}`}>{x?.rating != null ? rating(x.rating) : "–"}</td>
                    <td className="num">{x?.impact != null ? (x.impact >= 0 ? "+" : "") + x.impact.toFixed(2) : "–"}</td>
                    <td className="num">{x ? `${x.kills}–${x.deaths}` : "–"}</td>
                    <td className="num hh-duel">
                      {x?.onOther ?? 0}–{y?.onOther ?? 0}
                    </td>
                    <td className="num">{y ? `${y.kills}–${y.deaths}` : "–"}</td>
                    <td className="num">{y?.impact != null ? (y.impact >= 0 ? "+" : "") + y.impact.toFixed(2) : "–"}</td>
                    <td className={`num strong${better === 1 ? " hh-best" : ""}`}>{y?.rating != null ? rating(y.rating) : "–"}</td>
                    <td className="hh-caret">{isOpen ? "▴" : "▾"}</td>
                  </tr>,
                  isOpen && (
                    <tr key={`${l.logId}-parts`} className="hh-parts-row">
                      <td colSpan={9}>
                        <div className="hh-parts">
                          {[x, y].map((s, i) => (
                            <Breakdown key={i} s={s} who={name(h.accounts[i])} mean={h.scaleMean} sd={h.scaleSd} />
                          ))}
                        </div>
                      </td>
                    </tr>
                  ),
                ];
              })}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}

/** One rating's working: every part, its percentile, weight and share. */
function Breakdown({ s, who, mean, sd }: { s: H2hSide | null; who: string; mean: number; sd: number }) {
  if (!s) return <div className="hh-bd muted">{tx("{0} did not play this log.", { "0": who })}</div>;
  if (s.rating === null || s.score === null) return <div className="hh-bd muted">{tx("{0}: not rated here (under 5 minutes on a class).", { "0": who })}</div>;
  return (
    <div className="hh-bd">
      <strong>
        {who} <ClassIcon cls={s.class} size={14} />
      </strong>
      <table className="hh-bd-table">
        <thead>
          <tr>
            <th>{t("Part")}</th>
            <th className="num">{t("Value")}</th>
            <th>{t("Against the pool")}</th>
            <th className="num">{t("Weight")}</th>
            <th className="num">{t("Adds")}</th>
          </tr>
        </thead>
        <tbody>
          {[...s.parts]
            .sort((x, y) => y.weight - x.weight)
            .map((p) => (
              <tr key={p.component}>
                <td>{t(p.label)}</td>
                <td className="num">
                  {p.raw} <span className="muted">{t(p.unit)}</span>
                </td>
                <td>
                  <span className="hh-bar" title={t("Better than {0}% of games on this class and map", { "0": Math.round(p.percentile) })}>
                    <span style={{ width: `${Math.max(2, p.percentile)}%` }} className={p.percentile >= 60 ? "good" : p.percentile <= 40 ? "poor" : ""} />
                  </span>
                  <span className="muted"> {Math.round(p.percentile)}</span>
                </td>
                <td className="num">{Math.round(p.weight * 100)}%</td>
                <td className="num">{(p.percentile * p.weight).toFixed(1)}</td>
              </tr>
            ))}
        </tbody>
        <tfoot>
          <tr>
            <td colSpan={4}>
              {tx("Score {0} → rating 1 + ({0} − {1}) / {2} × 0.25", { "0": s.score.toFixed(1), "1": mean.toFixed(1), "2": sd.toFixed(1) })}
            </td>
            <td className="num strong">{rating(s.rating)}</td>
          </tr>
        </tfoot>
      </table>
    </div>
  );
}
