import type { Analysis, StateSeries } from "../../api/types";

/** Row heights, in pixels. */
const ALIVE_H = 36;
const UBER_H = 18;
const AD_H = 8;
const GAP = 8;
/** A difference this big fills the alive row's half-height. */
const ALIVE_MAX = 4;

const STRIP_H = ALIVE_H + GAP + UBER_H * 2 + GAP + AD_H + 4;

/** The chosen side's view of the state series: "mine" is the chosen player's team. */
export function sides(s: StateSeries, mine: "Red" | "Blue") {
  const red = mine === "Red";
  return {
    alive: red ? s.redAlive : s.blueAlive,
    theirAlive: red ? s.blueAlive : s.redAlive,
    charge: red ? s.redCharge : s.blueCharge,
    theirCharge: red ? s.blueCharge : s.redCharge,
    /** 1 when the chosen side holds the uber advantage, -1 the other side. */
    advantage: red ? s.advantage : s.advantage.map((v) => -v),
  };
}

export function chargeLabel(c: number | undefined): string {
  if (c === undefined || c < 0) return "no Medic";
  if (c === 100) return "ready";
  if (c === 101) return "in use";
  return `${c}%`;
}

/**
 * The game state under the timeline, on the same time axis. Three rows:
 * players up or down (blue when the chosen side has more alive, orange when
 * fewer), each side's uber charge, and who holds the uber advantage.
 */
export function StateStrip(props: {
  a: Analysis;
  mine: "Red" | "Blue";
  t0: number;
  t1: number;
  x: (t: number) => number;
  left: number;
  plotW: number;
  width: number;
  hoverT: number | null;
  onHover: (t: number | null) => void;
}) {
  const { a, mine, t0, t1, x, left, plotW, width, hoverT, onHover } = props;
  const s = sides(a.state, mine);
  const i0 = Math.max(0, Math.floor(t0));
  const i1 = Math.min(s.alive.length, Math.ceil(t1));

  // Runs of equal value, so a 40-minute match is a few hundred shapes, not thousands.
  const runs = (values: number[]) => {
    const out: Array<{ from: number; to: number; v: number }> = [];
    for (let i = i0; i < i1; i++) {
      const v = values[i];
      const last = out[out.length - 1];
      if (last && last.v === v && last.to === i) last.to = i + 1;
      else out.push({ from: i, to: i + 1, v });
    }
    return out;
  };

  const diff = s.alive.map((n, i) => n - s.theirAlive[i]);
  const aliveTop = 0;
  const mid = aliveTop + ALIVE_H / 2;
  const uberTop = ALIVE_H + GAP;
  const adTop = uberTop + UBER_H * 2 + GAP;
  const w = (r: { from: number; to: number }) => Math.max(0.5, x(Math.min(r.to, t1)) - x(Math.max(r.from, t0)));
  const rx = (r: { from: number }) => x(Math.max(r.from, t0));

  const uberRow = (values: number[], top: number, key: string) =>
    runs(values.map((c) => (c < 0 ? -1 : c >= 100 ? c : Math.round(c / 5) * 5))).map((r) => {
      if (r.v < 0) return null;
      const h = (Math.min(r.v, 100) / 100) * (UBER_H - 2);
      const cls = r.v === 101 ? "ss-uber-used" : r.v === 100 ? "ss-uber-ready" : "ss-uber-build";
      return <rect key={`${key}${r.from}`} x={rx(r)} width={w(r)} y={top + UBER_H - 1 - h} height={h} className={cls} />;
    });

  // A plain-language summary of the slice: the chart's text alternative.
  const n = Math.max(1, i1 - i0);
  const share = (pred: (i: number) => boolean) => {
    let k = 0;
    for (let i = i0; i < i1; i++) if (pred(i)) k++;
    return Math.round((k / n) * 100);
  };
  const up = share((i) => diff[i] > 0);
  const down = share((i) => diff[i] < 0);
  const adMine = share((i) => s.advantage[i] > 0);
  const adTheirs = share((i) => s.advantage[i] < 0);

  const tFromEvent = (e: React.MouseEvent<SVGSVGElement>) => {
    const mx = e.clientX - e.currentTarget.getBoundingClientRect().left;
    return Math.max(t0, Math.min(t1, t0 + ((mx - left) / plotW) * (t1 - t0)));
  };

  const label = (y: number, text: string) => (
    // In the right margin: the left one is too narrow for words.
    <text x={left + plotW + 8} y={y} className="tl-axis" dominantBaseline="middle">
      {text}
    </text>
  );

  return (
    <div className="ss">
      <svg
        width={width}
        height={STRIP_H}
        role="img"
        aria-label={`Game state: up players ${up}% of the time, down ${down}%. Uber advantage yours ${adMine}%, theirs ${adTheirs}%.`}
        onMouseMove={(e) => onHover(tFromEvent(e))}
        onMouseLeave={() => onHover(null)}
      >
        <defs>
          {/* A charge in use: hatched, so it never reads as the orange of "behind". */}
          <pattern id="ss-hatch" width="4" height="4" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
            <rect width="4" height="4" className="ss-uber-ready" />
            <line x1="0" y1="0" x2="0" y2="4" className="ss-hatch-line" />
          </pattern>
        </defs>
        {label(mid, "Players ±")}
        <line x1={left} x2={left + plotW} y1={mid} y2={mid} className="tl-grid" />
        {runs(diff).map((r) => {
          if (r.v === 0) return null;
          const h = (Math.min(Math.abs(r.v), ALIVE_MAX) / ALIVE_MAX) * (ALIVE_H / 2 - 1);
          return (
            <rect
              key={`d${r.from}`}
              x={rx(r)}
              width={w(r)}
              y={r.v > 0 ? mid - h : mid}
              height={h}
              className={r.v > 0 ? "ss-up" : "ss-down"}
            />
          );
        })}

        {label(uberTop + UBER_H / 2, "Your uber")}
        {uberRow(s.charge, uberTop, "m")}
        {label(uberTop + UBER_H * 1.5, "Their uber")}
        {uberRow(s.theirCharge, uberTop + UBER_H, "t")}

        {label(adTop + AD_H / 2, "Advantage")}
        <rect x={left} width={plotW} y={adTop} height={AD_H} className="ss-ad-none" />
        {runs(s.advantage).map((r) =>
          r.v === 0 ? null : (
            <rect key={`a${r.from}`} x={rx(r)} width={w(r)} y={adTop} height={AD_H} className={r.v > 0 ? "ss-up" : "ss-down"} />
          ),
        )}

        {a.rounds
          .filter((r) => r.startS > t0 && r.startS < t1)
          .map((r) => (
            <line key={r.roundNum} x1={x(r.startS)} x2={x(r.startS)} y1={0} y2={adTop + AD_H} className="tlc-round" />
          ))}
        {hoverT !== null && <line x1={x(hoverT)} x2={x(hoverT)} y1={0} y2={adTop + AD_H} className="tl-cross" />}
      </svg>
      <p className="hint ss-summary">
        <span className="km-key km-key-kill ss-key" /> ahead <span className="km-key ss-key ss-key-down" /> behind ·{" "}
        Up a player or more {up}% of the time, down {down}%. Uber advantage: yours {adMine}%, theirs {adTheirs}%.
        Charge between two known moments (ready, used, death) is interpolated.
      </p>
    </div>
  );
}
