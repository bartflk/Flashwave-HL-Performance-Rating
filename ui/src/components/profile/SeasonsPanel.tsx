import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { PeriodStats, Season } from "../../api/types";
import { setPeriod, usePeriod } from "../../lib/period";

const shortDate = (t: number) =>
  new Date(t * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "2-digit", timeZone: "UTC" });

const num = (v: number | null, digits = 0) => (v === null ? "–" : v.toFixed(digits));
const pct = (v: number | null) => (v === null ? "–" : `${Math.round(v * 100)}%`);

/**
 * One class, season by season: every game played in each season's window,
 * officials, scrims and pugs alike. Click a season to look at just it.
 */
export function SeasonsPanel({ cls }: { cls: string }) {
  const q = useQuery({ queryKey: ["seasons-view", cls], queryFn: () => api.getSeasons(cls) });
  const period = usePeriod();
  if (!q.data || q.data.seasons.length === 0) return null;
  const { seasons, allTime } = q.data;

  const pick = (s: Season) =>
    period.kind === "season" && period.key === s.key
      ? setPeriod({ kind: "all" })
      : setPeriod({ kind: "season", key: s.key, name: s.name, from: s.from, to: s.to });

  return (
    <section className="panel seasons-panel">
      <header>
        <h2>By season</h2>
        <p className="hint">Click a season to filter everything to it.</p>
      </header>
      <div className="table-wrap">
        <table className="match-table seasons-table">
          <thead>
            <tr>
              <th>Season</th>
              <th className="num" title="Officials / scrims / pugs">Games</th>
              <th className="num">Record</th>
              <th className="num" title="Average rating">Rating</th>
              <th className="num" title="Damage per minute on the class">DPM</th>
              <th className="num">K/D</th>
              <th className="num" title="Kills per 10 minutes">Kills/10</th>
              <th className="num" title="Deaths per 10 minutes">Deaths/10</th>
              <th className="num" title="First kills of fights: yours against you dying">Opening duels</th>
              <th className="num" title="Your kills where your team lost someone within 3 s">Traded</th>
              <th className="num" title="Fights with a kill or assist, survival, or a traded death">Fight KAST</th>
            </tr>
          </thead>
          <tbody>
            {seasons.map(({ season: s, stats }) => {
              const active = period.kind === "season" && period.key === s.key;
              return (
                <tr
                  key={s.key}
                  className={`clickable${active ? " sel" : ""}${stats.games === 0 ? " muted-row" : ""}`}
                  onClick={() => pick(s)}
                  aria-pressed={active}
                >
                  <td>
                    <div className="season-name">
                      {s.name}
                      {s.ongoing && <span className="badge badge-now">now</span>}
                    </div>
                    <div className="hint season-dates">
                      {shortDate(s.from)} – {shortDate(s.to)}
                      {s.divisions.length > 0 && ` · ${s.divisions.join(", ")}`}
                    </div>
                  </td>
                  <Cells stats={stats} />
                </tr>
              );
            })}
            <tr className="seasons-total">
              <td>All time</td>
              <Cells stats={allTime} />
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  );
}

function Cells({ stats: s }: { stats: PeriodStats }) {
  if (s.games === 0) {
    return (
      <td className="hint" colSpan={10}>
        No rated games on this class
      </td>
    );
  }
  return (
    <>
      <td className="num">
        {s.games}{" "}
        <span className="muted">
          ({s.officials}/{s.scrims}/{s.pugs})
        </span>
      </td>
      <td className="num">
        {s.wins}–{s.losses}
        {s.ties > 0 && `–${s.ties}`}
      </td>
      <td className="num">{num(s.rating)}</td>
      <td className="num">{num(s.dpm)}</td>
      <td className="num">{num(s.kd, 2)}</td>
      <td className="num">{num(s.killsPer10, 1)}</td>
      <td className="num">{num(s.deathsPer10, 1)}</td>
      <td className="num">{pct(s.openingWon)}</td>
      <td className="num">{pct(s.traded)}</td>
      <td className="num">{pct(s.fightKast)}</td>
    </>
  );
}
