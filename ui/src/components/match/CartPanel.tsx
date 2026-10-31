import { useQuery } from "@tanstack/react-query";
import { ApiError } from "../ApiError";
import { Help } from "../StatTip";
import { api } from "../../api/client";
import { type CartFight, type CartHold, type CartRound, type MatchDetail, type RoundFight } from "../../api/types";
import { clock } from "../../lib/format";
import { copy } from "../../lib/toast";
import { t, tx } from "../../lib/i18n";
import { SortTh, useSort } from "../../lib/sort";
import { tip } from "../../lib/glossary";

/**
 * Q11 (Flashy): the cart in a numbers advantage. Seconds BLU was up three
 * or more and the cart stood still -- nobody on it, or a defender alive on
 * it that nobody killed -- and how much it moved after each won fight.
 * Payload only, and only where an STV was read.
 */
export function CartPanel({ d }: { d: MatchDetail }) {
  const q = useQuery({ queryKey: ["cart", d.logId], queryFn: () => api.getCart(d.logId, d.map ?? undefined) });
  // Every hook before any return: the error answer used to come first,
  // so the sorts were skipped on that render and React's hook order broke
  // when the query recovered.
  const v = q.data;
  const stalls = useSort("cart-stalls", v?.stalls ?? [], { round: (x) => x.round * 100000 + x.atS, at: (x) => x.atS, still: (x) => x.seconds, up: (x) => x.mostUp, why: (x) => (x.emptyS * 2 >= x.seconds ? 0 : 1) });
  const holds = useSort("cart-holds", v?.holds ?? [], { round: (x) => x.round * 100000 + x.fromS, at: (x) => x.fromS, held: (x) => x.seconds, where: (x) => x.zone ?? null, pushes: (x) => x.pushesFailed, lost: (x) => x.lostAttackers - x.lostDefenders, then: (x) => (x.broke ? 1 : 0) });
  if (q.isError) {
    return (
      <section className="panel cart-panel">
        <h2>{t("The cart")}</h2>
        <ApiError error={q.error} />
      </section>
    );
  }
  if (!v) return null;
  const wasted = v.rounds.reduce((a, r) => a + r.upStillS, 0);
  const empty = v.rounds.reduce((a, r) => a + r.upStillEmptyS, 0);
  const jump = (tick: number, what: string) => void copy(`demo_gototick ${tick}`, what);

  return (
    <section className="panel cart-panel">
      <h2>{t("The cart")}</h2>
      <p className="hint">
        {tx("Seconds BLU was up {0} or more players and the cart stood still: {1} in this match, {2} of them with nobody on the cart and the rest with a defender blocking it.", {
          "0": v.up,
          "1": <strong>{wasted}s</strong>,
          "2": <strong>{empty}s</strong>,
        })}
      </p>

      <div className="cart-rounds">
        {v.rounds.map((r, i) => (
          <div key={i} className="cart-round">
            <div className="cart-round-head">
              <strong>{t("Round {0}", { "0": i + 1 })}</strong>
              <span className="hint">
                {t("{0} live · cart moving {1}%", { "0": clock(r.seconds), "1": Math.round((100 * r.movingS) / Math.max(1, r.seconds)) })}
              </span>
            </div>
            <div
              className="cart-bar"
              title={t("Up {0}+: {1}s moving, {2}s still with nobody on it, {3}s blocked", {
                "0": v.up,
                "1": r.upS - r.upStillS,
                "2": r.upStillEmptyS,
                "3": r.upStillS - r.upStillEmptyS,
              })}
            >
              <div className="cart-seg cart-seg-moving" style={{ flex: Math.max(0, r.upS - r.upStillS) }} />
              <div className="cart-seg cart-seg-empty" style={{ flex: r.upStillEmptyS }} />
              <div className="cart-seg cart-seg-blocked" style={{ flex: r.upStillS - r.upStillEmptyS }} />
            </div>
            <span className="hint">{t("{0}s up {1}+ · {2}s of it still", { "0": r.upS, "1": v.up, "2": r.upStillS })}</span>
          </div>
        ))}
      </div>
      <div className="cart-legend hint">
        <span><i className="cart-seg-moving" /> {t("moving")}</span>
        <span><i className="cart-seg-empty" /> {t("still, nobody on it")}</span>
        <span><i className="cart-seg-blocked" /> {t("still, blocked")}</span>
      </div>

      {v.stalls.length > 0 && (
        <>
          <h3>{t("Stalls while up")}</h3>
          <div className="table-wrap">
            <table className="match-table cart-table">
              <thead>
                <tr>
                  <SortTh k="round" s={stalls}>{t("Round")}</SortTh>
                  <SortTh k="at" s={stalls} className="num">{t("At")}</SortTh>
                  <SortTh k="still" s={stalls} className="num">{t("Still for")}</SortTh>
                  <SortTh k="up" s={stalls} className="num">{t("Up")}</SortTh>
                  <SortTh k="why" s={stalls}>{t("Why")}</SortTh>
                </tr>
              </thead>
              <tbody>
                {stalls.sorted.map((s, i) => (
                  <tr
                    key={i}
                    className="spy-row"
                    title={t("Copy demo_gototick {tick}", { tick: s.jumpTick })}
                    onClick={() => jump(s.jumpTick, t("Stall at {0}", { "0": clock(s.atS) }))}
                  >
                    <td>{s.round + 1}</td>
                    <td className="num">{clock(s.atS)}</td>
                    <td className="num">{s.seconds}s</td>
                    <td className="num">+{s.mostUp}</td>
                    <td>{s.emptyS * 2 >= s.seconds ? t("nobody on the cart") : t("a defender blocking")}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}

      {(v.holds?.length ?? 0) + (v.allFights?.length ?? 0) > 0 && (
        <>
          <h3>
            {t("Momentum")}
            <Help>{t("How far BLU pushed the cart through each round. A flat stretch with fights in it is a hold; dots are fights, blue where BLU won it and red where the defence did.")}</Help>
          </h3>
          {v.rounds.map((r, i) => (
            <Momentum key={i} n={i} r={r} fights={v.allFights.filter((f) => f.round === i)} holds={v.holds.filter((h) => h.round === i)} />
          ))}
          {v.holds.length > 0 && (
            <div className="table-wrap">
              <table className="match-table cart-table">
                <thead>
                  <tr>
                    <SortTh k="round" s={holds}>{t("Round")}</SortTh>
                    <SortTh k="at" s={holds} className="num">{t("At")}</SortTh>
                    <SortTh k="held" s={holds} className="num">{t("Held for")}</SortTh>
                    <SortTh k="where" s={holds}>{t("Where")}</SortTh>
                    <SortTh k="pushes" s={holds} className="num" tip={tip(t("Pushes turned back"), t("Fights in the hold that BLU did not win"), t("From the STV demo: the cart's position and who stood near it, every tick."), "STV demo")}>{t("Pushes turned back")}</SortTh>
                    <SortTh k="lost" s={holds} className="num" tip={tip(t("Lost"), t("Players each side lost in the hold: BLU – RED"), t("From the STV demo: the cart's position and who stood near it, every tick."), "STV demo")}>{t("Lost")}</SortTh>
                    <SortTh k="then" s={holds}>{t("Then")}</SortTh>
                  </tr>
                </thead>
                <tbody>
                  {holds.sorted.map((h, i) => (
                    <tr key={i} className="spy-row" title={t("Copy demo_gototick {tick}", { tick: h.jumpTick })} onClick={() => jump(h.jumpTick, t("Hold at {0}", { "0": clock(h.fromS) }))}>
                      <td>{h.round + 1}</td>
                      <td className="num">{clock(h.fromS)}</td>
                      <td className="num">{h.seconds}s</td>
                      <td>{h.zone ?? <span className="muted">–</span>}</td>
                      <td className="num">
                        {h.pushesFailed} <span className="muted">/ {h.fights}</span>
                      </td>
                      <td className="num">
                        {h.lostAttackers}–{h.lostDefenders}
                      </td>
                      <td>{h.broke ? t("broke") : t("held to the end")}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}

      {v.fights.length > 0 && (
        <>
          <h3>{tx("After a won fight: seconds of the next {0} the cart moved", { "0": v.afterS })}</h3>
          <div className="cart-after">
            {[1, 2, 3, 0].map((n) => (
              <After key={n} n={n} fights={v.fights.filter((f) => n === 0 || f.nth === n)} />
            ))}
          </div>
        </>
      )}
    </section>
  );
}

/**
 * Q12 (Flashy: "when the cart gets stuck and you start killing or dying a
 * lot at one spot"): the cart's progress through one round, the holds
 * shaded, each fight a dot on the line.
 */
function Momentum({ n, r, fights, holds }: { n: number; r: CartRound; fights: RoundFight[]; holds: CartHold[] }) {
  const p = r.progress ?? [];
  if (p.length < 2) return null;
  const W = 600;
  const H = 70;
  const span = Math.max(1, (p.length - 1) * 2);
  const top = Math.max(1, ...p);
  const x = (s: number) => (Math.min(s, span) / span) * W;
  const y = (s: number) => H - 4 - (p[Math.min(p.length - 1, Math.floor(s / 2))] / top) * (H - 10);
  const line = p.map((v, i) => `${((i * 2) / span) * W},${H - 4 - (v / top) * (H - 10)}`).join(" ");
  return (
    <figure className="momentum">
      <figcaption className="hint">{t("Round {0}", { "0": n + 1 })}</figcaption>
      <div className="mom-chart">
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label={t("Cart progress, round {0}", { "0": n + 1 })}>
        {holds.map((h, i) => (
          <rect key={i} className="mom-hold" x={x(h.fromS)} y={0} width={Math.max(2, x(h.fromS + h.seconds) - x(h.fromS))} height={H}>
            <title>{t("Held {0}s{1}: {2} of {3} pushes turned back", { "0": h.seconds, "1": h.zone ? ` · ${h.zone}` : "", "2": h.pushesFailed, "3": h.fights })}</title>
          </rect>
        ))}
        <polyline className="mom-line" points={line} vectorEffect="non-scaling-stroke" />
      </svg>
      {/* Dots over the stretched chart, so they stay round. */}
      {fights.map((f, i) => (
        <span
          key={i}
          className={f.lostDefenders > f.lostAttackers ? "mom-dot mom-blu" : "mom-dot mom-red"}
          style={{ left: `${(x(f.toS) / W) * 100}%`, top: `${y(f.toS)}px` }}
          title={t("{0}: BLU lost {1}, RED lost {2}", { "0": clock(f.toS), "1": f.lostAttackers, "2": f.lostDefenders })}
        />
      ))}
      </div>
    </figure>
  );
}

function After({ n, fights }: { n: number; fights: CartFight[] }) {
  const label = n === 0 ? t("Every fight won") : n === 1 ? t("1st fight won") : n === 2 ? t("2nd fight won") : t("3rd fight won");
  if (fights.length === 0) {
    return (
      <div className="cart-after-cell">
        <span className="hint">{label}</span>
        <strong className="muted">–</strong>
      </div>
    );
  }
  const moving = fights.reduce((a, f) => a + f.movingS, 0) / fights.length;
  const window = fights.reduce((a, f) => a + f.windowS, 0) / fights.length;
  return (
    <div className="cart-after-cell">
      <span className="hint">{label}</span>
      <strong>{moving.toFixed(1)}s</strong>
      <span className="hint">
        {fights.length === 1
          ? t("of {0}s · 1 fight", { "0": window.toFixed(0) })
          : t("of {0}s · {1} fights", { "0": window.toFixed(0), "1": fights.length })}
      </span>
    </div>
  );
}
