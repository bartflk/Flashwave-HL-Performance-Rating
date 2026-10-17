import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { LogsTfCareer as Career, LogsTfClassCareer } from "../../api/types";
import { rating } from "../../lib/format";
import { locale, t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";

/**
 * Thirteen years of logs.tf in a card (Flashy: "build the bundled logs.tf
 * summary"): every Highlander game on each class, competitive -- officials
 * and other team games -- apart from pickup (TF2Center lobbies and pugs),
 * each rated against all of logs.tf history, with a rank among the class's
 * regulars and a rating a year. From the summary built into the app: no
 * download, nothing sent anywhere.
 */
export function LogsTfCareer({ accountId, compact }: { accountId: number; compact?: boolean }) {
  const q = useQuery({ queryKey: ["logstf_career", accountId], queryFn: () => api.logstfCareer(accountId), staleTime: Infinity });
  if (q.isPending) return <p className="hint">{t("Reading the logs.tf summary…")}</p>;
  const c = q.data;
  if (!c) return <p className="hint">{t("Not among logs.tf's Highlander regulars: fewer than five Highlander logs, or none.")}</p>;
  const comp = c.classes.filter((x) => x.pool === "competitive");
  const pickup = c.classes.filter((x) => x.pool === "pickup");
  const main = comp[0]?.class ?? null;
  const years = c.years.filter((y) => y.class === main);
  return (
    <div className="lt-career">
      <p className="lt-summary">
        <strong>{c.player.logs.toLocaleString()}</strong> {t("Highlander logs")}
        <span className="muted">
          {" "}
          · {tx("{0} competitive, {1} lobbies and pugs", { "0": c.player.competitive.toLocaleString(), "1": c.player.pickup.toLocaleString() })} · {c.player.firstYear}–{c.player.lastYear}
        </span>
      </p>
      {comp.length > 0 && <CareerTable title={t("Competitive")} rows={comp} />}
      {!compact && pickup.length > 0 && <CareerTable title={t("Lobbies and pugs")} rows={pickup} muted />}
      {!compact && main && years.length >= 2 && <YearBars cls={main} years={years} />}
      <p className="hint lt-note">
        {tx("From every Highlander log on logs.tf, duplicates out, rated against all of them on logs.tf's own numbers: without the server-log parts, so a little apart from the ratings above. Summary of {0}.", {
          "0": c.builtAt ? new Date(c.builtAt * 1000).toLocaleDateString(locale(), { day: "numeric", month: "short", year: "numeric" }) : "–",
        })}
      </p>
    </div>
  );
}

function CareerTable({ title, rows, muted }: { title: string; rows: LogsTfClassCareer[]; muted?: boolean }) {
  return (
    <table className={muted ? "pp-seasons lt-table muted-pool" : "pp-seasons lt-table"}>
      <thead>
        <tr>
          <th>{title}</th>
          <th className="num">{t("Games")}</th>
          <th className="num">{t("Rating")}</th>
          <th className="num">{t("Form")}</th>
          <th className="num">{t("Best")}</th>
          <th className="num">{t("W–L")}</th>
          <th>{t("Years")}</th>
          <th className="num">{t("Rank")}</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={r.class}>
            <td>
              <span className="lt-class">
                <ClassIcon cls={r.class} size={18} /> {classLabel(r.class)}
              </span>
            </td>
            <td className="num">{r.games.toLocaleString()}</td>
            <td className={`num lt-rating ${r.rating >= 1.05 ? "up" : r.rating <= 0.95 ? "down" : ""}`}>{rating(r.rating)}</td>
            <td className="num">{rating(r.form)}</td>
            <td className="num muted">{rating(r.best)}</td>
            <td className="num">
              <span className="pf-w">{r.won}</span>–<span className="pf-l">{r.lost}</span>
            </td>
            <td className="muted">{r.firstYear === r.lastYear ? r.firstYear : `${r.firstYear}–${r.lastYear}`}</td>
            <td className="num" title={r.rank ? t("Among {0} with 100 or more games on the class", { "0": classLabel(r.class) }) : t("Ranked from 100 games on the class")}>
              {r.rank && r.of ? (
                <>
                  <strong>#{r.rank.toLocaleString()}</strong> <span className="muted">/ {r.of.toLocaleString()}</span>
                </>
              ) : (
                <span className="muted">–</span>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** The main class's competitive rating, a bar a year, about the 1.00 line. */
function YearBars({ cls, years }: { cls: string; years: Career["years"] }) {
  const lo = 0.6;
  const hi = 1.4;
  const at = (r: number) => ((Math.min(hi, Math.max(lo, r)) - lo) / (hi - lo)) * 100;
  return (
    <div className="lt-years">
      <h4>
        <ClassIcon cls={cls} size={16} /> {tx("{0}, a year at a time", { "0": classLabel(cls) })}
      </h4>
      <div className="lt-bars" role="img" aria-label={years.map((y) => `${y.year}: ${rating(y.rating)}`).join(", ")}>
        <span className="lt-mid" style={{ bottom: `${at(1)}%` }} />
        {years.map((y) => (
          <div key={y.year} className="lt-bar-col" title={t("{0}: {1} over {2} games", { "0": y.year, "1": rating(y.rating), "2": y.games })}>
            <span className="lt-val">{rating(y.rating)}</span>
            <span className={`lt-bar ${y.rating >= 1 ? "up" : "down"}`} style={{ height: `${Math.max(4, at(y.rating))}%` }} />
            <span className="lt-year">{String(y.year).slice(2)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
