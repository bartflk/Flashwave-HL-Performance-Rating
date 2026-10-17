import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { PeriodStats, Season } from "../../api/types";
import { setPeriod, usePeriod } from "../../lib/period";
import { locale, t } from "../../lib/i18n";
import { SortTh, useSort } from "../../lib/sort";
import { tip } from "../../lib/glossary";

const shortDate = (t: number) =>
  new Date(t * 1000).toLocaleDateString(locale(), { day: "numeric", month: "short", year: "2-digit", timeZone: "UTC" });

const num = (v: number | null, digits = 0) => (v === null ? "–" : v.toFixed(digits));
const pct = (v: number | null) => (v === null ? "–" : `${Math.round(v * 100)}%`);

/**
 * One class, season by season: every game played in each season's window,
 * officials, scrims and pugs alike. Click a season to look at just it.
 */
export function SeasonsPanel({ cls }: { cls: string }) {
  const q = useQuery({ queryKey: ["seasons-view", cls], queryFn: () => api.getSeasons(cls) });
  const period = usePeriod();
  const sort = useSort("profile-seasons", q.data?.seasons ?? [], {
    season: (x) => x.season.from,
    games: (x) => x.stats.games,
    record: (x) => (x.stats.wins + x.stats.losses > 0 ? x.stats.wins / (x.stats.wins + x.stats.losses) : null),
    rating: (x) => x.stats.rating,
    dpm: (x) => x.stats.dpm,
    kd: (x) => x.stats.kd,
    kills: (x) => x.stats.killsPer10,
    deaths: (x) => x.stats.deathsPer10,
    opening: (x) => x.stats.openingWon,
    traded: (x) => x.stats.traded,
    kast: (x) => x.stats.fightKast,
  });
  if (!q.data || q.data.seasons.length === 0) return null;
  const { allTime } = q.data;

  const pick = (s: Season) =>
    period.kind === "season" && period.key === s.key
      ? setPeriod({ kind: "all" })
      : setPeriod({ kind: "season", key: s.key, name: s.name, from: s.from, to: s.to });

  return (
    <section className="panel seasons-panel">
      <header>
        <h2>{t("By season")}</h2>
        <p className="hint">{t("Click a season to filter everything to it.")}</p>
      </header>
      <div className="table-wrap">
        <table className="match-table seasons-table">
          <thead>
            <tr>
              <SortTh k="season" s={sort}>{t("Season")}</SortTh>
              <SortTh k="games" s={sort} className="num" tip={tip(t("Games"), t("Officials / scrims / pugs"), t("Over the season's rated games on the class."), "this app")}>{t("Games")}</SortTh>
              <SortTh k="record" s={sort} className="num">{t("Record")}</SortTh>
              <SortTh k="rating" s={sort} className="num" tip={tip(t("Rating"), t("Average rating"), t("Over the season's rated games on the class."), "logs.tf", "server log")}>{t("Rating")}</SortTh>
              <SortTh k="dpm" s={sort} className="num" tip={tip(t("DPM"), t("Damage per minute on the class"), t("Over the season's rated games on the class."), "logs.tf")}>{t("DPM")}</SortTh>
              <SortTh k="kd" s={sort} className="num" tip="kd">K/D</SortTh>
              <SortTh k="kills" s={sort} className="num" tip={tip(t("Kills/10"), t("Kills per 10 minutes"), t("Over the season's rated games on the class."), "logs.tf")}>{t("Kills/10")}</SortTh>
              <SortTh k="deaths" s={sort} className="num" tip={tip(t("Deaths/10"), t("Deaths per 10 minutes"), t("Over the season's rated games on the class."), "logs.tf")}>{t("Deaths/10")}</SortTh>
              <SortTh k="opening" s={sort} className="num" tip={tip(t("Opening duels"), t("First kills of fights: yours against you dying"), t("Over the season's rated games on the class."), "server log")}>{t("Opening duels")}</SortTh>
              <SortTh k="traded" s={sort} className="num" tip={tip(t("Traded"), t("Your kills where your team lost someone within 3 s"), t("Over the season's rated games on the class."), "server log")}>{t("Traded")}</SortTh>
              <SortTh k="kast" s={sort} className="num" tip={tip(t("Fight KAST"), t("Fights with a kill or assist, survival, or a traded death"), t("Over the season's rated games on the class."), "server log")}>{t("Fight KAST")}</SortTh>
            </tr>
          </thead>
          <tbody>
            {sort.sorted.map(({ season: s, stats }) => {
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
                      {s.ongoing && <span className="badge badge-now">{t("now")}</span>}
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
              <td>{t("All time")}</td>
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
      <td className="hint" colSpan={10}>{t("No rated games on this class")}</td>
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
