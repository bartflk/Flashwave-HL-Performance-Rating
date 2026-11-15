import { useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglRanking, RglSeasonItem } from "../../api/types";
import { t } from "../../lib/i18n";
import { openOnKey } from "../../lib/rows";
import { changeOf, divisionsOf, inDivision, isMine } from "../../lib/rglrank";
import { useShowResults } from "../../lib/spoilers";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { fullDate } from "../etf2l/Countdown";
import { usePlayer, useRanking } from "./data";

/** How many matches a season is, to say what reading it costs. */
const SEASON_READS = 150;

/** RGL Highlander's power ranking (Flashy: "i want a lot of the site features copied", tf2esports' Rankings). */
export function Rankings({ steamid, onTeam }: { steamid: string | null; onTeam: (id: number) => void }) {
  const [shown] = useShowResults();
  const q = useRanking(shown);
  const [division, setDivision] = useState<string | null>(null);
  const me = usePlayer(steamid);
  const mine = useMemo(() => new Set((me.data?.seasons ?? []).map((s) => s.teamId)), [me.data]);
  const client = useQueryClient();
  const include = useMutation({ mutationFn: (id: number) => api.getRglRanking(id), onSuccess: (data) => client.setQueryData(["rgl_ranking"], data) });
  const data = q.data;
  const rows = data?.rows ?? [];
  const divisions = divisionsOf(rows);
  const chosen = division !== null && divisions.includes(division) ? division : null;
  const list = inDivision(rows, chosen);

  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <div className="e2-titles">
          <h2>{t("Rankings")}</h2>
          <span className="muted">{data?.season ? `${data.season.name} · ${t("NA Highlander")}` : t("NA Highlander")}</span>
        </div>
        {data?.updated != null && (
          <span className="muted" title={t("The newest match counted in the ranking")}>
            {t("Last updated {0}", { "0": fullDate(data.updated) })}
          </span>
        )}
      </div>
      <p className="hint rgl-rank-how">
        {t("An Elo rating worked out here from the matches this app has: each team starts from its division's seed, every decided match moves both teams by how far the result beat what was expected, a forfeit moves a quarter as much, and a new season pulls each team back toward its division's seed.")}
      </p>
      {!shown ? (
        <p className="hint">{t("The ranking is made of results, so it is hidden. Turn on Show results to see it.")}</p>
      ) : q.error ? (
        <ApiError error={q.error} />
      ) : q.isPending ? (
        <Loading>{t("Working out the ranking from the matches already read…")}</Loading>
      ) : rows.length === 0 ? (
        <p className="hint">{t("No team to rank yet: the season has no match.")}</p>
      ) : (
        <>
          {divisions.length > 1 && (
            <div className="e2-rounds rgl-rank-filter" role="tablist" aria-label={t("Division")}>
              <button role="tab" aria-selected={chosen === null} className={chosen === null ? "on" : undefined} onClick={() => setDivision(null)}>
                {t("All")}
              </button>
              {divisions.map((d) => (
                <button key={d} role="tab" aria-selected={chosen === d} className={chosen === d ? "on" : undefined} onClick={() => setDivision(d)}>
                  {d}
                </button>
              ))}
            </div>
          )}
          <table className="match-table rgl-table rgl-rank">
            <thead>
              <tr>
                <th className="num">#</th>
                <th>{t("Team")}</th>
                <th>{t("Division")}</th>
                <th className="num" title={t("Elo points")}>
                  {t("Points")}
                </th>
                <th className="num" title={t("Places gained or lost since a week ago, ranking without the last seven days of matches")}>
                  +/-
                </th>
                <th className="num" title={t("Matches played and counted, forfeits apart")}>
                  {t("Matches")}
                </th>
              </tr>
            </thead>
            <tbody>
              {list.map((r) => {
                const c = changeOf(r.change);
                const open = () => onTeam(r.id);
                return (
                  <tr key={r.id} className={isMine(r, mine) ? "mine rgl-open" : "rgl-open"} role="link" tabIndex={0} onClick={open} onKeyDown={openOnKey(open)} title={t("Open the team")}>
                    <td className="num muted">#{r.rank}</td>
                    <td>
                      <span className="rgl-rank-team">
                        <strong>
                          {r.name}
                          {r.tag && <span className="rgl-tag">{r.tag}</span>}
                        </strong>
                        {r.players.length > 0 && <span className="muted rgl-rank-roster">{r.players.join(", ")}</span>}
                      </span>
                    </td>
                    <td>
                      <span className="rgl-chip div" title={r.tier ? t("{0} is about ETF2L {1}", { "0": r.division ?? "", "1": t(r.tier) }) : undefined}>
                        {r.division ?? "–"}
                        {r.tier && <span className="muted"> ≈ {t(r.tier)}</span>}
                      </span>
                    </td>
                    <td className="num rgl-st-rating">{r.points}</td>
                    <td className="num" title={r.pointsChange === 0 ? undefined : t("{0} points this week", { "0": `${r.pointsChange > 0 ? "+" : ""}${r.pointsChange}` })}>
                      {c.kind === "none" ? (
                        <span className="muted" title={t("It had played nothing a week ago")}>
                          –
                        </span>
                      ) : c.kind === "same" ? (
                        <span className="muted">–</span>
                      ) : (
                        <span className={c.kind === "up" ? "rgl-st-up" : "rgl-st-down"}>
                          <span aria-hidden>{c.kind === "up" ? "▲" : "▼"}</span> {c.places}
                        </span>
                      )}
                    </td>
                    <td className="num muted">{r.matches}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <Counted data={data} reading={include.isPending ? include.variables : null} onInclude={(id) => include.mutate(id)} error={include.error} />
        </>
      )}
    </section>
  );
}

/** Which seasons the ranking counts, and the offer of one more, with what it costs. */
function Counted({ data, reading, onInclude, error }: { data: RglRanking | undefined; reading: number | null | undefined; onInclude: (id: number) => void; error: unknown }) {
  if (!data) return null;
  const offer: RglSeasonItem | null = data.offer;
  return (
    <div className="rgl-rank-foot">
      <p className="hint">
        {t("Counted: {0}. Only seasons whose matches this app already holds are used; a team's earlier seasons are joined to it by RGL's links where its page was opened, and by name otherwise.", { "0": data.used.map((s) => s.name).join(", ") })}
      </p>
      {error != null && <ApiError error={error} />}
      {reading != null ? (
        <Loading>{t("Reading the season from RGL… about 40 seconds the first time. You can look at another page meanwhile: the read carries on in the background.")}</Loading>
      ) : (
        offer && (
          <div className="rgl-rank-offer">
            <button onClick={() => onInclude(offer.id)} title={t("Reads every match of the season from RGL, 4 a second, and keeps them for good")}>
              {t("Include {0}", { "0": offer.name })}
            </button>
            <span className="muted">{t("About {0} match reads the first time, at 4 a second (under a minute), kept for good after.", { "0": SEASON_READS })}</span>
          </div>
        )
      )}
    </div>
  );
}
