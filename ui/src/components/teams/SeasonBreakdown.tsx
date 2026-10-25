import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage } from "../../api/types";
import { rating } from "../../lib/format";
import { openLeagueMatch } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { Loading } from "../Spinner";

/**
 * Why a player's season rating is what it is (Flashy: "add the breakdown to
 * the season rankings rows too"): every official with its rating, the
 * opponents' strength and what it added, its stage weight; the sum that
 * makes the season; and the rating's parts over the season's logs.
 */
export function SeasonBreakdown({ season, tier, cls, accountId }: { season: number; tier: number; cls: string; accountId: number }) {
  const q = useQuery({
    queryKey: ["season_breakdown", season, tier, cls, accountId],
    queryFn: () => api.getSeasonBreakdown(season, tier, cls, accountId),
    staleTime: 5 * 60_000,
  });
  if (q.isPending) return <Loading>{t("Rating the season's logs again…")}</Loading>;
  if (q.isError) return <p className="error">{errorMessage(q.error)}</p>;
  const b = q.data;
  const signed = (x: number) => `${x >= 0 ? "+" : "−"}${Math.abs(x).toFixed(2)}`;
  return (
    <div className="sb">
      <div className="sb-officials">
        <table className="sb-table">
          <thead>
            <tr>
              <th>{t("Official")}</th>
              <th>{t("Opponent")}</th>
              <th>{t("Maps")}</th>
              <th className="num">{t("Rating")}</th>
              <th className="num" title={t("How strong the opponents were from what they won that season, this match left out: 1.00 an average team")}>
                {t("Opp.")}
              </th>
              <th className="num" title={t("{0} for every 0.10 the opponents were above 1.00", { "0": (b.opponentSlope * 0.1).toFixed(3) })}>
                {t("Adj.")}
              </th>
              <th className="num">{t("Counts as")}</th>
              <th className="num">{t("Weight")}</th>
            </tr>
          </thead>
          <tbody>
            {b.officials.map((o) => (
              <tr key={o.matchId} className={o.ownTeam ? undefined : "sb-merc"} title={o.ownTeam ? undefined : t("As a merc for another team: shown, not counted for awards")}>
                <td>
                  <button className="linkish" onClick={() => openLeagueMatch(o.matchId)}>
                    {o.round}
                  </button>
                  {!o.ownTeam && <span className="sb-tag">{t("merc")}</span>}
                </td>
                <td>
                  {o.opponent?.name ?? "?"} {o.result && <span className={`sb-res sb-${o.result}`}>{t(o.result)}</span>}
                </td>
                <td className="muted">{o.maps.join(", ")}</td>
                <td className="num">{rating(o.rating)}</td>
                <td className="num muted">{o.opponentStrength === null ? "–" : rating(o.opponentStrength)}</td>
                <td className={`num ${o.adjustment > 0 ? "sb-up" : o.adjustment < 0 ? "sb-down" : "muted"}`}>{o.adjustment === 0 ? "–" : signed(o.adjustment)}</td>
                <td className="num strong">{rating(o.adjusted)}</td>
                <td className="num muted">×{o.stageWeight}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <td colSpan={8}>
                {tx("Season: ({0} + {1} × 1.00) ÷ ({2} + {1}) = {3}", {
                  "0": b.sumWeighted.toFixed(2),
                  "1": b.shrink,
                  "2": b.sumWeights.toFixed(1),
                  "3": <strong key="r">{rating(b.rating)}</strong>,
                })}
                <span className="muted">
                  {" "}
                  {t("(each official counted with its weight, then pulled towards 1.00 by three average officials so a short season counts for less)")}
                </span>
              </td>
            </tr>
          </tfoot>
        </table>
      </div>
      {b.parts.length > 0 && (
        <div className="sb-parts">
          <strong>{t("What the rating was made of this season")}</strong>
          <table className="sb-table">
            <thead>
              <tr>
                <th>{t("Part")}</th>
                <th className="num">{t("Average")}</th>
                <th>{t("Against the pool")}</th>
                <th className="num">{t("Weight")}</th>
                <th className="num">{t("Logs")}</th>
              </tr>
            </thead>
            <tbody>
              {b.parts.map((p) => (
                <tr key={p.component}>
                  <td>{t(p.label)}</td>
                  <td className="num">
                    {p.raw.toFixed(p.raw >= 100 ? 0 : 2)} <span className="muted">{t(p.unit)}</span>
                  </td>
                  <td>
                    <span className="sb-bar" title={t("Better than {0}% of games on this class and map", { "0": Math.round(p.percentile) })}>
                      <span style={{ width: `${Math.max(2, p.percentile)}%` }} className={p.percentile >= 60 ? "good" : p.percentile <= 40 ? "poor" : ""} />
                    </span>
                    <span className="muted"> {Math.round(p.percentile)}</span>
                  </td>
                  <td className="num">{Math.round(p.weight * 100)}%</td>
                  <td className="num muted">{p.logs}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
