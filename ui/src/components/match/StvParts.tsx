import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { type GappyMatch, type StvCoverage } from "../../api/types";
import { friendlyText } from "../../lib/apierror";
import { beginDownload, failDownload, useDownload } from "../../lib/downloads";
import { capitalize, formatDate, splitMap } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { Spinner } from "../Spinner";

/**
 * The other parts of a match's SourceTV (Flashy: "make it fetch the missing
 * STV parts automatically, give a prompt and also a possibility to check for
 * missing demos"). A server splits its recording into files of about
 * eighteen minutes and a log is linked to one: a koth match had rounds 4 to
 * 6 on this machine and no routes for the rest. Downloading a match's STV
 * now takes every part demos.tf has; these are for the matches whose demos
 * came before that, or whose parts were uploaded late.
 */

/** "1–3, 7–9" from [1, 2, 3, 7, 8, 9]. */
function roundRanges(rounds: number[]): string {
  const out: string[] = [];
  let start: number | null = null;
  let prev: number | null = null;
  for (const r of [...rounds].sort((a, b) => a - b)) {
    if (start !== null && prev !== null && r === prev + 1) {
      prev = r;
      continue;
    }
    if (start !== null) out.push(start === prev ? `${start}` : `${start}–${prev}`);
    start = prev = r;
  }
  if (start !== null) out.push(start === prev ? `${start}` : `${start}–${prev}`);
  return out.join(", ");
}

function missingOf(c: StvCoverage | null | undefined): number[] {
  return c ? c.rounds.filter((r) => !c.covered.includes(r)) : [];
}

/** Look for a match's other parts on demos.tf, and download those not here. */
function useFetchParts(logId: number, label: string) {
  const qc = useQueryClient();
  const download = useDownload(logId);
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // When the parts are in and read, what this match draws from them.
  useEffect(() => {
    if (download?.state !== "done") return;
    for (const key of ["stv_coverage", "match_openings", "match", "analysis", "positions"]) void qc.invalidateQueries({ queryKey: [key, logId] });
    void qc.invalidateQueries({ queryKey: ["missing_stv_parts"] });
  }, [download?.state, logId, qc]);

  async function run(): Promise<boolean> {
    setBusy(true);
    setError(null);
    setSaid(null);
    try {
      const parts = await api.findStvParts(logId);
      const get = parts.filter((p) => !p.here);
      if (get.length === 0) {
        // Which rounds no part on demos.tf holds: the server never uploaded
        // that file, so no fetch can fill them.
        const c = await api.stvCoverage(logId);
        const held = new Set([...(c?.covered ?? []), ...parts.flatMap((p) => p.rounds)]);
        const none = (c?.rounds ?? []).filter((r) => !held.has(r));
        setSaid(
          none.length > 0
            ? t("demos.tf has no part with rounds {0}: the server never uploaded it.", { "0": roundRanges(none) })
            : t("demos.tf has no other part of this match's SourceTV."),
        );
        return false;
      }
      setSaid(t("Downloading {0} part{1}: rounds {2}.", { "0": get.length, "1": get.length === 1 ? "" : "s", "2": roundRanges(get.flatMap((p) => p.rounds)) }));
      beginDownload(logId, label);
      try {
        await api.fetchStv(logId);
      } catch (e) {
        failDownload(logId, friendlyText(e));
        throw e;
      }
      return true;
    } catch (e) {
      setError(friendlyText(e));
      return false;
    } finally {
      setBusy(false);
    }
  }

  return { run, busy: busy || (download !== null && download.state !== "done" && download.state !== "failed"), said, error };
}

/**
 * On the match page, where the parts matter: the Demo panel and the round
 * openings. Says which rounds this machine has, and fetches the rest.
 */
export function StvPartsNotice({ logId, label }: { logId: number; label: string }) {
  const q = useQuery({ queryKey: ["stv_coverage", logId], queryFn: () => api.stvCoverage(logId), staleTime: 60_000 });
  const parts = useFetchParts(logId, label);
  const missing = missingOf(q.data);
  if (!q.data || missing.length === 0) return null;
  return (
    <div className="stv-parts">
      <p>
        {tx("The SourceTV on this machine covers rounds {0} of {1}. The server splits its recording into parts; rounds {2} are in the others.", {
          "0": <strong>{roundRanges(q.data.covered)}</strong>,
          "1": q.data.rounds.length,
          "2": <strong>{roundRanges(missing)}</strong>,
        })}
      </p>
      <div className="stv-parts-actions">
        <button onClick={() => void parts.run()} disabled={parts.busy}>
          {parts.busy && <Spinner />}
          {t("Find and download the other parts")}
        </button>
        {parts.said && <span className="hint">{parts.said}</span>}
        {parts.error && <span className="error">{parts.error}</span>}
      </div>
    </div>
  );
}

/** Matches already answered, this session. */
const asked = new Set<number>();
const NEVER_KEY = "stv-parts-prompt-off";

function neverAsk(): boolean {
  try {
    return localStorage.getItem(NEVER_KEY) === "1";
  } catch {
    return false;
  }
}

/**
 * Opening a match whose SourceTV leaves rounds out: asked once a match, and
 * never again if you say so, as the prompt for a match with no STV at all.
 */
export function StvPartsPrompt({ logId, label }: { logId: number; label: string }) {
  const q = useQuery({ queryKey: ["stv_coverage", logId], queryFn: () => api.stvCoverage(logId), staleTime: 60_000 });
  const parts = useFetchParts(logId, label);
  const [open, setOpen] = useState(false);
  const missing = missingOf(q.data);
  const gappy = missing.length > 0;

  useEffect(() => {
    if (!gappy || asked.has(logId) || neverAsk()) return;
    asked.add(logId);
    setOpen(true);
  }, [gappy, logId]);

  if (!open || !q.data) return null;

  function stopAsking() {
    try {
      localStorage.setItem(NEVER_KEY, "1");
    } catch {
      // It asks again next time, which is the safe way to fail.
    }
    setOpen(false);
  }

  return (
    <div className="modal-scrim" role="dialog" aria-modal="true" aria-label={t("Part of this match's SourceTV is missing")}>
      <div className="modal">
        <h3>{t("Part of this match's SourceTV is missing")}</h3>
        <p>
          {tx("The SourceTV on this machine covers rounds {0} of {1}. The server splits its recording into parts of about eighteen minutes, and rounds {2} are in the others: without them those rounds have no routes, no round openings and no aim for anyone but you.", {
            "0": <strong>{roundRanges(q.data.covered)}</strong>,
            "1": q.data.rounds.length,
            "2": <strong>{roundRanges(missing)}</strong>,
          })}
        </p>
        {parts.said && <p className="muted">{parts.said}</p>}
        {parts.error && <p className="error">{parts.error}</p>}
        <div className="modal-actions">
          <button
            className="primary"
            disabled={parts.busy}
            onClick={() =>
              void parts.run().then((started) => {
                if (started) setOpen(false);
              })
            }
          >
            {parts.busy && <Spinner />}
            {t("Find and download them")}
          </button>
          <button onClick={() => setOpen(false)}>{t("Not now")}</button>
          <button className="linkish" onClick={stopAsking}>
            {t("Never ask")}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * Settings: every match whose SourceTV on this machine leaves rounds out,
 * each with its own fetch. From this machine alone, so it is quick; demos.tf
 * is asked match by match.
 */
export function MissingDemosPanel({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
  const [asked, setAsked] = useState(false);
  const q = useQuery({ queryKey: ["missing_stv_parts"], queryFn: api.missingStvParts, enabled: asked });
  const items = q.data ?? [];
  return (
    <div className="panel">
      <h2>{t("Missing demo parts")}</h2>
      <p className="hint">{t("Matches whose SourceTV on this machine leaves rounds out: the server split its recording into parts and only some were downloaded.")}</p>
      <div className="stv-parts-actions">
        <button onClick={() => (asked ? void q.refetch() : setAsked(true))} disabled={q.isFetching}>
          {q.isFetching && <Spinner />}
          {t("Check for missing demos")}
        </button>
        {asked && q.data && <span className="hint">{items.length === 0 ? t("Every match's SourceTV covers all its rounds.") : tx("{0} match{1} with rounds missing.", { "0": items.length, "1": items.length === 1 ? "" : "es" })}</span>}
        {q.isError && <span className="error">{friendlyText(q.error)}</span>}
      </div>
      {items.length > 0 && (
        <ul className="stv-gappy">
          {items.map((g) => (
            <GappyRow key={g.logId} g={g} onOpenMatch={onOpenMatch} />
          ))}
        </ul>
      )}
    </div>
  );
}

function GappyRow({ g, onOpenMatch }: { g: GappyMatch; onOpenMatch: (logId: number) => void }) {
  const name = capitalize(splitMap(g.map).name ?? t("unknown map"));
  const parts = useFetchParts(g.logId, `${name}, log ${g.logId}`);
  return (
    <li>
      <button className="linkish stv-gappy-name" onClick={() => onOpenMatch(g.logId)} title={t("Open the match")}>
        {name}
      </button>
      <span className="muted">{formatDate(g.playedAt)}</span>
      <span>{tx("rounds {0} of {1}", { "0": roundRanges(g.covered), "1": g.rounds })}</span>
      <span className="stv-gappy-act">
        {parts.said ?? (parts.error && <span className="error">{parts.error}</span>)}
        {!parts.said && (
          <button onClick={() => void parts.run()} disabled={parts.busy}>
            {parts.busy && <Spinner />}
            {t("Find the other parts")}
          </button>
        )}
      </span>
    </li>
  );
}
