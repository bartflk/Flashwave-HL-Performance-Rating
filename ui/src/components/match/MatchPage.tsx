import { HeadToHead } from "./HeadToHead";
import { ReflectPanel } from "./DemoStats";
import { MatchTabs } from "./MatchTabs";
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type Ledger, type MatchDetail, type PartScore } from "../../api/types";
import { capitalize, splitMap } from "../../lib/format";
import { MatchHero } from "./MatchHero";
import { BoxScore } from "./BoxScore";
import { Fold } from "../Fold";
import { DemoPanel } from "./DemoPanel";
import { CartPanel } from "./CartPanel";
import { PositionsPanel } from "./PositionsPanel";
import { Spychecks } from "./Spychecks";
import { StvBanner, StvLocked } from "./StvGate";
import { StvPrompt } from "./StvPrompt";
import { AnalysisPanel } from "../analysis/AnalysisPanel";
import { Matchups } from "./Matchups";
import { RoundTimeline } from "./RoundTimeline";
import { t, tx } from "../../lib/i18n";
import { Loading, Spinner } from "../Spinner";
import { OpponentStrength, type Side } from "./OpponentStrength";

export function MatchPage({ logId, onBack }: { logId: number; onBack: () => void }) {
  const q = useQuery({ queryKey: ["match", logId], queryFn: () => api.getMatch(logId) });
  // A combined log can be read whole, or one of its logs at a time: picking
  // one scopes the whole page, not just the scoreboard.
  const [part, setPart] = useState<number | null>(null);
  const qc = useQueryClient();
  const partsQ = useQuery({
    queryKey: ["parts", logId],
    queryFn: () => api.getParts(logId),
    enabled: (q.data?.parts.length ?? 0) > 0,
    staleTime: 5 * 60_000,
  });
  // Q84: each part's map, game and sides, each map beside ETF2L's result.
  const ledgerQ = useQuery({
    queryKey: ["ledger", logId],
    queryFn: () => api.getCombinedLedger(logId),
    enabled: (q.data?.parts.length ?? 0) > 0,
    staleTime: 5 * 60_000,
  });
  const ledger = ledgerQ.data ?? null;
  const [fetching, setFetching] = useState(false);
  const [partError, setPartError] = useState<string | null>(null);

  const chosen = part === null ? null : (partsQ.data ?? []).find((p) => p.logId === part) ?? null;
  const shown = chosen?.detail ?? q.data ?? null;
  // The rounds of the combined log this part covers, for the kill-by-kill
  // views, which read the whole match's raw log.
  const onlyRounds = chosen?.parentRounds.length ? chosen.parentRounds : null;
  // A deleted STV still counts: what was read from it is kept.
  const hasStv = q.data?.demos.some((x) => x.kind === "stv") ?? false;
  const isPayload = /^pl_/i.test(shown?.map ?? "");

  async function pick(next: number | null) {
    setPart(next);
    setPartError(null);
    if (next === null) return;
    if ((partsQ.data ?? []).find((p) => p.logId === next)?.detail) return;
    setFetching(true);
    try {
      await api.fetchPart(next);
      await qc.invalidateQueries({ queryKey: ["parts", logId] });
    } catch (e) {
      setPartError(errorMessage(e));
      setPart(null);
    } finally {
      setFetching(false);
    }
  }

  return (
    <div className="match-page">
      <button className="ts-back" onClick={onBack}>
        <span aria-hidden>‹</span> {t("All matches")}
      </button>

      {q.isPending && <Loading>{t("Loading match…")}</Loading>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data === null && (
        <p className="hint">{t("This log is not stored yet. Run a sync, then open it again.")}</p>
      )}
      {q.data && shown && (
        <>
          <MatchHero d={shown} ledger={part === null ? ledger : null} />
          <StrengthOf d={shown} />
          <MatchTabs />
          {q.data.standIn && (
            <div className="stv-banner stand-in-banner">
              <span className="stv-banner-icon" aria-hidden>!</span>
              <p>
                <strong>{t("Built from {source}'s copy: logs.tf was refusing us.", { source: q.data.standIn })}</strong>{" "}
                <span className="hint">{t("The scoreboard and every kill are the same as logs.tf's. It lacks who assisted each kill and when each point was capped after the first, so ratings may move a little when a later sync swaps in the real log.")}</span>
              </p>
            </div>
          )}
          <StvBanner d={q.data} />
          {/* The scoreboard first, as on logs.tf; the matchups read it next. */}
          <Fold id="scoreboard">
            <BoxScore
              d={shown}
              reading={
                q.data.parts.length > 0 || looksCombined(q.data) ? (
                  // One cluster on the header's right: which log is read,
                  // and finding more of them (Flashy: "the buttons are just
                  // missplaced").
                  <div className="box-tools">
                    {q.data.parts.length > 0 && (
                      <PartPicker
                        ledger={ledger}
                        d={q.data}
                        parts={partsQ.data ?? null}
                        part={part}
                        onPick={(id) => void pick(id)}
                        fetching={fetching}
                        error={partError}
                      />
                    )}
                    {looksCombined(q.data) && <FindParts d={q.data} />}
                  </div>
                ) : undefined
              }
            />
          </Fold>
          <Fold id="matchups">
            <Matchups d={shown} />
          </Fold>
          <Fold id="h2h">
            <HeadToHead d={q.data} />
          </Fold>
          <Fold id="demos">
            <DemoPanel d={q.data} />
            <StvPrompt d={q.data} />
          </Fold>
          {/* The STV-only panels: blurred, with the reason and the download,
              where there is no STV rather than missing without a word. */}
          <Fold id="spychecks">
            {hasStv ? <Spychecks d={q.data} /> : <StvLocked d={q.data} kind="spychecks" />}
          </Fold>
          {/* Reflects: any demo with a timeline, STV or the owner's own;
              nothing at all without one. Ping is a scoreboard column (Q83). */}
          <Fold id="reflects">
            <ReflectPanel d={q.data} />
          </Fold>
          {(hasStv || isPayload) && (
            <Fold id="cart">
              {hasStv ? <CartPanel d={q.data} /> : <StvLocked d={q.data} kind="cart" />}
            </Fold>
          )}
          <Fold id="positions">
            {hasStv ? <PositionsPanel d={shown} /> : <StvLocked d={q.data} kind="positions" />}
          </Fold>
          <Fold id="rounds">
            <RoundTimeline d={shown} />
          </Fold>
          <Fold id="analysis">
            <AnalysisPanel d={q.data} onlyRounds={onlyRounds} />
          </Fold>
        </>
      )}
    </div>
  );
}

/**
 * Which log the page is reading: the whole combined upload, or one of the
 * logs it was built from. Picking one scopes every panel below, and a log
 * whose data is not stored yet is fetched from logs.tf on the spot.
 */
/** Several logs in one: maps in the title or the map field, "combined", or
 *  longer than one map runs. */
function looksCombined(d: MatchDetail): boolean {
  const m = d.map ?? "";
  return /combined/i.test(d.title ?? "") || m.includes("/") || m.includes("+") || d.durationS >= 45 * 60;
}

/**
 * Find the logs a combined log was built from (Flashy: "this log doesn't
 * list the other logs ... i want this system expanded ... so it can detect
 * combined games"): logs.tf's logs with the same players whose every round
 * is one of this log's, fetched and folded in.
 */
function FindParts({ d }: { d: MatchDetail }) {
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  async function find() {
    setBusy(true);
    setMsg(null);
    try {
      const r = await api.findParts(d.logId);
      setMsg(
        r.added > 0
          ? t("Found {0} of its logs on logs.tf.", { "0": r.added })
          : r.candidates > 0
            ? t("logs.tf has {0} logs with these players then, but none is part of this one.", { "0": r.candidates })
            : t("logs.tf has no other logs of this match."),
      );
      if (r.added > 0) {
        void qc.invalidateQueries({ queryKey: ["match", d.logId] });
        void qc.invalidateQueries({ queryKey: ["parts", d.logId] });
        void qc.invalidateQueries({ queryKey: ["ledger", d.logId] });
      }
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  // With its parts already known it is a small "look again"; without, the
  // one thing to do here.
  const known = d.parts.length > 0;
  return (
    <>
      <button
        className={known ? "km-chip find-parts find-parts-quiet" : "km-chip find-parts"}
        onClick={() => void find()}
        disabled={busy}
        title={t("Look on logs.tf for the maps and halves this log was combined from")}
      >
        {busy ? <Spinner /> : <span aria-hidden>⟳</span>} {known ? t("Look again") : t("Find its other logs")}
      </button>
      {msg && <span className="hint box-tools-msg">{msg}</span>}
    </>
  );
}

function PartPicker(props: {
  ledger: Ledger | null;
  d: MatchDetail;
  parts: PartScore[] | null;
  part: number | null;
  onPick: (id: number | null) => void;
  fetching: boolean;
  error: string | null;
}) {
  const { d, parts, part, onPick, fetching, error, ledger } = props;
  const rows = parts ?? d.parts.map((p) => ({ ...p, detail: null, parentRounds: [] }));
  // Ordered and named by the ledger where there is one: each map, then its
  // games ("Upward · game 2 · log 4138738"); a combine of other parts, which
  // adds nothing, last.
  const at = (id: number) => {
    const i = ledger?.parts.findIndex((x) => x.logId === id) ?? -1;
    return i < 0 ? 999 : i;
  };
  const ordered = [...rows].sort((a, b) => at(a.logId) - at(b.logId));
  const lp = (id: number) => ledger?.parts.find((x) => x.logId === id) ?? null;
  const flipped = part !== null && lp(part)?.teamsFlipped === true;
  return (
    <div className="part-picker">
      <label className="an-field">
        <span className="an-label">{t("Reading")}</span>
        <select value={part ?? ""} onChange={(e) => onPick(e.target.value === "" ? null : Number(e.target.value))}>
          <option value="">{tx("The whole match · {parts} logs combined", { parts: d.parts.length })}</option>
          {ordered.map((p) => {
            const l = lp(p.logId);
            const map = capitalize(splitMap(p.map).name ?? "unknown");
            return (
              <option key={p.logId} value={p.logId}>
                {l
                  ? tx("{0} · {1} · log {logId}{2}", { "0": map, "1": l.label === "whole map" ? t("whole map") : t("game {0}", { "0": l.label.replace("game ", "") }), "2": "detail" in p && p.detail ? "" : t(" (fetches)"), logId: p.logId })
                  : tx("{0} · log {logId}{2}", { "0": map, "2": "detail" in p && p.detail ? "" : t(" (fetches)"), logId: p.logId })}
              </option>
            );
          })}
        </select>
      </label>
      {fetching && <span className="hint"><Spinner /> {t("Fetching…")}</span>}
      {part !== null && !fetching && (
        <span className="hint" title={flipped ? t("Each log names its own RED: whoever was RED when it began. The whole match keeps each team on one colour.") : undefined}>
          {flipped ? t("this log alone; its RED is the whole match's BLU") : t("this log alone")}
        </span>
      )}
      {error && <p className="error">{error}</p>}
    </div>
  );
}

/** Opponent strength, your side first as the hero has it (Q78). */
function StrengthOf({ d }: { d: MatchDetail }) {
  const side = (team: "Red" | "Blue"): Side => ({
    label: team === "Red" ? t("RED") : t("BLU"),
    tone: team === "Red" ? "red" : "blu",
    players: d.players.filter((p) => p.team === team).map((p) => ({ accountId: p.accountId, class: p.mainClass })),
  });
  const blueFirst = d.myTeam === "Blue";
  const sides: [Side, Side] = blueFirst ? [side("Blue"), side("Red")] : [side("Red"), side("Blue")];
  return <OpponentStrength sides={sides} score={blueFirst ? [d.blueScore, d.redScore] : [d.redScore, d.blueScore]} />;
}
