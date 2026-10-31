import { useState } from "react";
import { ApiError } from "../ApiError";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { type DataIssue } from "../../api/types";
import { openLeagueMatch } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { Loading } from "../Spinner";

/**
 * A season's data check (data_audit.rs): every played official against
 * ETF2L's record of it and the logs held, so a season table or an award can
 * be trusted -- or the official that is missing a map can be found.
 */

const WHAT: Record<string, string> = {
  no_logs: "No logs",
  own_only: "Only your upload",
  missing_map: "Map missing",
  extra_map: "Unexpected map",
  duplicate: "Uploaded twice",
  combined: "Combined upload",
  overlap: "Overlapping logs",
  not_downloaded: "Not downloaded",
  unrated: "Not rated",
  players: "Odd player count",
  score: "Score differs",
  split_map: "Map in many logs",
  own_combined: "Your combined upload",
};

export function DataCheck({ season }: { season: number }) {
  const [open, setOpen] = useState(false);
  const [all, setAll] = useState(false);
  const q = useQuery({ queryKey: ["data_audit", season], queryFn: () => api.getDataAudit(season, null), staleTime: 5 * 60_000 });
  const r = q.data;
  const errors = r?.issues.filter((i) => i.severity === "error").length ?? 0;
  const warns = r?.issues.filter((i) => i.severity === "warn").length ?? 0;
  const shown: DataIssue[] = (r?.issues ?? []).filter((i) => all || i.severity !== "info");
  return (
    <section className="panel dc">
      <button className="dc-head" onClick={() => setOpen(!open)} aria-expanded={open}>
        <span className="dc-title">{t("Data check")}</span>
        {q.isPending && <Loading>{t("Checking the season…")}</Loading>}
        {r && (
          <span className="dc-sum">
            {tx("{0} of {1} officials clean", { "0": <strong key="c">{r.clean}</strong>, "1": r.officials })}
            {errors > 0 && <span className="dc-chip dc-error">{tx("{0} to fix", { "0": errors })}</span>}
            {warns > 0 && <span className="dc-chip dc-warn">{tx("{0} doubtful", { "0": warns })}</span>}
          </span>
        )}
        <span className="dc-caret" aria-hidden>
          {open ? "▴" : "▾"}
        </span>
      </button>
      {q.isError && <ApiError error={q.error} />}
      {open && r && (
        <>
          <p className="hint">
            {t(
              "Each played official checked against ETF2L's record and the logs held. To fix changes who is counted: a map with no log, two games in the same minutes. Doubtful does not change the tables: uploads counted once, a score that differs from ETF2L's, a log not downloaded yet (a sync fetches it).",
            )}
          </p>
          <label className="dc-all">
            <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} /> {t("Show notes too (maps split into many logs, your combined uploads)")}
          </label>
          {shown.length === 0 ? (
            <p className="hint">{t("Nothing to report: every official has its maps, once each.")}</p>
          ) : (
            <table className="match-table dc-table">
              <thead>
                <tr>
                  <th>{t("Division")}</th>
                  <th>{t("Round")}</th>
                  <th>{t("Match")}</th>
                  <th>{t("Found")}</th>
                  <th>{t("What it means")}</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((i, n) => (
                  <tr key={n} className={`dc-${i.severity}`}>
                    <td>{i.division}</td>
                    <td>{i.round}</td>
                    <td>
                      <button className="linkish" onClick={() => openLeagueMatch(i.matchId)}>
                        {i.teams}
                      </button>
                    </td>
                    <td className="dc-code">{t(WHAT[i.code] ?? i.code)}</td>
                    <td>{i.detail}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}
    </section>
  );
}
