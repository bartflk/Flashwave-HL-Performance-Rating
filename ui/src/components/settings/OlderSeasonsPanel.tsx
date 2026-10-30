import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { api, inTauri } from "../../api/client";
import { errorMessage, type OldSeasonAdded } from "../../api/types";
import { t } from "../../lib/i18n";
import { Spinner } from "../Spinner";
import { Help } from "../StatTip";

/** The seasons before the league sample's window, oldest last on screen. */
const OLDEST = 1;
const FIRST_IN_WINDOW = 24;
/** logs.tf began in 2013: before Season 5, results and medals but no logs. */
const FIRST_WITH_LOGS = 5;
/** Season 19 (summer 2019): ETF2L gives a deadline for every match's time
 *  and nobody who played, and its teams' rosters are in no log near it
 *  (Flashy's import, October 2026): results and medals only. */
const NO_LOGS = new Set([19]);

/**
 * Seasons before the league sample's six years (Flashy, October 2026, for
 * Clark: "are you still working on adding seasons prior to s24?"). ETF2L has
 * every season's results and who played; their logs are found in the
 * logs.tf dump's index, then downloaded from drops.tf and rated like the
 * rest of the league. On a computer without the dump it says so.
 */
export function OlderSeasonsPanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["old_seasons"], queryFn: api.oldSeasonsStatus });
  const [busy, setBusy] = useState<number | null>(null);
  const [step, setStep] = useState<string | null>(null);
  const [result, setResult] = useState<{ season: number; r: OldSeasonAdded } | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function add(season: number) {
    setBusy(season);
    setError(null);
    setResult(null);
    const off = await api.onOldSeasonProgress((p) => setStep(p.what));
    try {
      const r = await api.importOldSeason(season);
      setResult({ season, r });
      void qc.invalidateQueries();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      off();
      setStep(null);
      setBusy(null);
    }
  }

  async function chooseIndex() {
    setError(null);
    const picked = inTauri
      ? await open({ title: t("Choose the logs.tf dump's index"), filters: [{ name: "SQLite", extensions: ["sqlite3", "sqlite", "db"] }] })
      : window.prompt(t("Path to the logs.tf dump's index"));
    if (typeof picked !== "string" || !picked) return;
    try {
      await api.setLogsDumpIndex(picked);
      void qc.invalidateQueries({ queryKey: ["old_seasons"] });
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  const s = q.data;
  const held = new Map((s?.seasons ?? []).map((x) => [x.season, x]));
  const seasons = Array.from({ length: FIRST_IN_WINDOW - OLDEST }, (_, i) => FIRST_IN_WINDOW - 1 - i);

  return (
    <div className="panel os">
      <h2>
        {t("Older seasons")}
        <Help>
          {t(
            "ETF2L keeps every Highlander season's results and who played, back to Season 1 in 2012. Their logs were never linked to the matches, so they are found in the logs.tf dump's index: the logs on a match's maps, near its time, with most of both lineups in them. They are then downloaded from drops.tf and rated like every other official. logs.tf began in 2013, so the first seasons have results and medals but no ratings.",
          )}
        </Help>
      </h2>
      {q.isPending && <Spinner />}
      {s && (
        <div className="os-index">
          {s.index ? (
            <>
              <span className="muted">{t("Dump index")}</span> <code>{s.index}</code>{" "}
              <button className="linkish" onClick={() => void chooseIndex()}>
                {t("Change…")}
              </button>
            </>
          ) : (
            <>
              <span className="muted">{t("The logs.tf dump's index is not on this computer, so older seasons cannot be added here.")}</span>{" "}
              <button onClick={() => void chooseIndex()}>{t("Choose the index…")}</button>
            </>
          )}
        </div>
      )}
      {step && (
        <p className="hint os-step">
          <Spinner /> {t("Season {0}: {1}", { "0": busy ?? "", "1": step })}
        </p>
      )}
      {result && (
        <p className="hint os-step">
          {t("Season {0}: {1} of {2} matches with lineups linked to {3} logs; {4} downloaded and {5} rated.", {
            "0": result.season,
            "1": result.r.imported.linked,
            "2": result.r.imported.withLineups,
            "3": result.r.imported.logs,
            "4": result.r.caught.logs,
            "5": result.r.caught.rated,
          })}
        </p>
      )}
      {error && <p className="error os-step">{error}</p>}
      {s && (
        <table className="os-table">
          <thead>
            <tr>
              <th>{t("Season")}</th>
              <th className="num">{t("Played")}</th>
              <th className="num">{t("With logs")}</th>
              <th className="num">{t("Logs rated")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {seasons.map((n) => {
              const h = held.get(n);
              const logsExist = n >= FIRST_WITH_LOGS && !NO_LOGS.has(n);
              const complete = h && h.logs > 0 && h.rated >= h.logs;
              return (
                <tr key={n}>
                  <td>{t("Season {0}", { "0": n })}</td>
                  <td className="num">{h ? h.played : "–"}</td>
                  <td className="num">{h && logsExist ? `${h.linked}` : "–"}</td>
                  <td className="num">{h && logsExist ? `${h.rated} / ${h.logs}` : "–"}</td>
                  <td className="os-act">
                    {!logsExist ? (
                      <span className="muted">{NO_LOGS.has(n) ? t("no logs found") : t("before logs.tf")}</span>
                    ) : complete ? (
                      <span className="os-done">{t("Added")}</span>
                    ) : (
                      <button disabled={!s.index || busy !== null} onClick={() => void add(n)}>
                        {busy === n ? t("Adding…") : h && h.logs > 0 ? t("Finish") : t("Add")}
                      </button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}
