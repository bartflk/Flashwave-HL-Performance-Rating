import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type MatchDemoStats, type MatchDetail, type ReflectOutcome } from "../../api/types";
import { clock } from "../../lib/format";
import { copy } from "../../lib/toast";
import { t, tx } from "../../lib/i18n";
import { Name } from "./Spychecks";
import { SortTh, useSort } from "../../lib/sort";
import { tip } from "../../lib/glossary";

/**
 * Q44 and Q45, read off the match's kept demo timelines: every player's
 * ping, and every Pyro's reflects (ivg). An STV has everyone and every
 * projectile; without one, the owner's own recordings stand in, which hold
 * everyone's ping but only the reflects near the person recording.
 */
function useDemoStats(d: MatchDetail) {
  return useQuery({ queryKey: ["demostats", d.logId], queryFn: () => api.getDemoStats(d.logId) });
}

function useWho(d: MatchDetail) {
  return (id: number | null, fallback?: string) => {
    const p = id === null ? undefined : d.players.find((x) => x.accountId === id);
    return { name: p?.name ?? fallback ?? (id === null ? t("unknown") : String(id)), team: p?.team ?? null, isMe: p?.isMe ?? false };
  };
}

/** A timeline from before this version: it says so, rather than nothing. */
function TooOld({ r }: { r: MatchDemoStats }) {
  if (r.tooOld === 0) return null;
  return (
    <p className="hint" style={{ marginTop: 8 }}>
      {t("This match's demo was read before ping and reflects were recorded. Reading it again adds them, while the file is still on disk.")}
    </p>
  );
}

export function PingPanel({ d }: { d: MatchDetail }) {
  const q = useDemoStats(d);
  const who = useWho(d);
  if (q.isError) {
    return (
      <section className="panel">
        <h2>{t("Ping")}</h2>
        <p className="error">{errorMessage(q.error)}</p>
      </section>
    );
  }
  const r = q.data;
  const sort = useSort("pings", r?.pings ?? [], { name: (p) => p.name, avg: (p) => p.avg, min: (p) => p.min, max: (p) => p.max, spikes: (p) => p.spikes.length });
  if (!r) return null;
  const top = Math.max(60, ...r.pings.map((p) => p.max));
  return (
    <section className="panel ping-panel">
      <h2>{t("Ping")}</h2>
      <p className="hint">
        {t("The scoreboard's ping, from the demo: what the server measured, about once a second. A spike is 40 ms or more above the player's own usual.")}
      </p>
      <TooOld r={r} />
      {r.pings.length > 0 && (
        <div className="table-wrap">
          <table className="match-table ping-table">
            <thead>
              <tr>
                <SortTh k="name" s={sort}>{t("Player")}</SortTh>
                <SortTh k="avg" s={sort} className="num" tip={tip(t("Average"), t("Their ping as the server measured it, about once a second."), t("Read from this match's STV demo."), "STV demo")}>{t("Average")}</SortTh>
                <SortTh k="min" s={sort} className="num">{t("Lowest")}</SortTh>
                <SortTh k="max" s={sort} className="num">{t("Highest")}</SortTh>
                <th>{t("Over the match")}</th>
                <SortTh k="spikes" s={sort} className="num" tip={tip(t("Spikes"), t("Stretches 40 ms or more above the player's own usual ping."), t("Read from this match's STV demo."), "STV demo")}>{t("Spikes")}</SortTh>
              </tr>
            </thead>
            <tbody>
              {sort.sorted.map((p) => (
                <tr key={p.accountId}>
                  <td><Name {...who(p.accountId, p.name)} /></td>
                  <td className="num"><strong>{Math.round(p.avg)}</strong> {t("ms")}</td>
                  <td className="num">{p.min}</td>
                  <td className="num">{p.max}</td>
                  <td><Spark points={p.points} top={top} /></td>
                  <td
                    className="num"
                    title={p.spikes.map(([a, b, peak]) => t("{0}-{1}, up to {2} ms", { "0": clock(a), "1": clock(b), "2": peak })).join("\n")}
                  >
                    {p.spikes.length || <span className="muted">–</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

/** The ping over the match, as a line; `top` is the scale's ceiling. */
function Spark({ points, top }: { points: Array<[number, number]>; top: number }) {
  if (points.length < 2) return null;
  const W = 160;
  const H = 22;
  const end = points[points.length - 1][0] || 1;
  // A step line: ping holds until it changes.
  let path = "";
  points.forEach(([s, ms], i) => {
    const x = (s / end) * W;
    const y = H - (Math.min(ms, top) / top) * H;
    path += i === 0 ? `M${x.toFixed(1)},${y.toFixed(1)}` : `H${x.toFixed(1)}V${y.toFixed(1)}`;
  });
  path += `H${W}`;
  return (
    <svg className="ping-spark" width={W} height={H} viewBox={`0 0 ${W} ${H}`} aria-hidden>
      <path d={path} />
    </svg>
  );
}

const OUTCOME: Record<ReflectOutcome, string> = { hit: "hit", miss: "missed", sentBack: "sent back", unknown: "not seen" };

export function ReflectPanel({ d }: { d: MatchDetail }) {
  const q = useDemoStats(d);
  const who = useWho(d);
  const [open, setOpen] = useState(false);
  if (q.isError) {
    return (
      <section className="panel">
        <h2>{t("Reflects")}</h2>
        <p className="error">{errorMessage(q.error)}</p>
      </section>
    );
  }
  const r = q.data;
  const pyros = useSort("reflect-pyros", r?.pyros ?? [], {
    name: (p) => p.name, reflects: (p) => p.reflects, hit: (p) => (p.hits + p.misses > 0 ? p.hits / (p.hits + p.misses) : null), missed: (p) => p.misses,
    back: (p) => p.sentBack, unseen: (p) => p.unknown, damage: (p) => p.damage, kills: (p) => p.kills, threat: (p) => (p.judged > 0 ? p.threats / p.judged : null),
  });
  const each = useSort("reflect-each", r?.reflects ?? [], { time: (x) => x.atS, what: (x) => x.what, ended: (x) => x.outcome, damage: (x) => x.damage, threat: (x) => (x.threat === null ? null : x.threat ? 1 : 0) });
  if (!r) return null;
  const pct = (a: number, b: number) => (b > 0 ? `${Math.round((a / b) * 100)}%` : "–");
  return (
    <section className="panel reflect-panel">
      <h2>{t("Reflects")}</h2>
      <p className="hint">
        {t("Every rocket, pipe and other projectile a Pyro sent back, and how it ended: a hit when it damaged someone as it landed. Headed at the team is an estimate: the projectile carried on along its path would have passed close to the Pyro or a teammate (walls are not known).")}
      </p>
      {!r.stv && (
        <p className="hint" style={{ marginTop: 6 }}>
          {t("From your own recording: only the reflects near you are in it, and those that flew out of view end as not seen. The STV demo has them all.")}
        </p>
      )}
      <TooOld r={r} />
      {r.pyros.length === 0 ? (
        r.tooOld < r.demos && <p className="hint" style={{ marginTop: 8 }}>{t("No reflects in this match's demo.")}</p>
      ) : (
        <div className="table-wrap">
          <table className="match-table reflect-table">
            <thead>
              <tr>
                <SortTh k="name" s={pyros}>{t("Pyro")}</SortTh>
                <SortTh k="reflects" s={pyros} className="num">{t("Reflects")}</SortTh>
                <SortTh k="hit" s={pyros} className="num" tip={tip(t("Hit"), t("Hits out of those that landed (hits and misses)"), t("Read from this match's STV demo."), "STV demo")}>{t("Hit")}</SortTh>
                <SortTh k="missed" s={pyros} className="num">{t("Missed")}</SortTh>
                <SortTh k="back" s={pyros} className="num" tip={tip(t("Sent back"), t("Reflected back again by the other side"), t("Read from this match's STV demo."), "STV demo")}>{t("Sent back")}</SortTh>
                <SortTh k="unseen" s={pyros} className="num" tip={tip(t("Not seen"), t("Out of view, or the demo ended"), t("Read from this match's STV demo."), "STV demo")}>{t("Not seen")}</SortTh>
                <SortTh k="damage" s={pyros} className="num">{t("Damage")}</SortTh>
                <SortTh k="kills" s={pyros} className="num">{t("Kills")}</SortTh>
                <SortTh k="threat" s={pyros} className="num" tip={tip(t("Headed at team (est.)"), t("Of the reflects the demo showed flying long enough to tell"), t("Read from this match's STV demo."), "STV demo")}>{t("Headed at team (est.)")}</SortTh>
              </tr>
            </thead>
            <tbody>
              {pyros.sorted.map((p) => (
                <tr key={p.accountId}>
                  <td><Name {...who(p.accountId, p.name)} /></td>
                  <td className="num"><strong>{p.reflects}</strong></td>
                  <td className="num">{p.hits} <span className="muted">{pct(p.hits, p.hits + p.misses)}</span></td>
                  <td className="num">{p.misses}</td>
                  <td className="num">{p.sentBack}</td>
                  <td className="num">{p.unknown}</td>
                  <td className="num">{p.damage}</td>
                  <td className="num">{p.kills}</td>
                  <td className="num">{p.threats} <span className="muted">{tx("of {0}", { "0": p.judged })}</span></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {r.reflects.length > 0 && (
        <>
          <button className="linkish" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
            {open ? t("Hide every reflect") : tx("Show every reflect ({0})", { "0": r.reflects.length })}
          </button>
          {open && (
            <div className="table-wrap">
              <table className="match-table spy-table">
                <thead>
                  <tr>
                    <SortTh k="time" s={each} className="num">{t("Time")}</SortTh>
                    <th>{t("By")}</th>
                    <SortTh k="what" s={each}>{t("What")}</SortTh>
                    <SortTh k="ended" s={each}>{t("Ended")}</SortTh>
                    <SortTh k="damage" s={each}>{t("On")}</SortTh>
                    <SortTh k="threat" s={each}>{t("Headed at team")}</SortTh>
                  </tr>
                </thead>
                <tbody>
                  {each.sorted.map((x, i) => (
                    <tr
                      key={i}
                      className="spy-row"
                      title={t("Copy demo_gototick {tick}", { tick: x.jumpTick })}
                      onClick={() => void copy(`demo_gototick ${x.jumpTick}`, t("Reflect at {0}", { "0": clock(x.atS) }))}
                    >
                      <td className="num">{clock(x.atS)}</td>
                      <td><Name {...who(x.by)} /></td>
                      <td>{t(x.what)}</td>
                      <td>
                        {t(OUTCOME[x.outcome])}
                        {x.killed && <span className="badge">{t("killed")}</span>}
                      </td>
                      <td>
                        {x.victims.map((v) => (
                          <Name key={v} {...who(v)} />
                        ))}
                        {x.damage > 0 && <span className="muted"> {x.damage}</span>}
                      </td>
                      <td>{x.threat === null ? <span className="muted">–</span> : x.threat ? t("yes") : t("no")}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </section>
  );
}
