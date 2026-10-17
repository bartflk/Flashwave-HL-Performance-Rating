import { PingPanel, ReflectPanel } from "./DemoStats";
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type MatchDetail, type PartScore } from "../../api/types";
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
          <MatchHero d={shown} />
          <StrengthOf d={shown} />
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
                q.data.parts.length > 0 ? (
                  <PartPicker
                    d={q.data}
                    parts={partsQ.data ?? null}
                    part={part}
                    onPick={(id) => void pick(id)}
                    fetching={fetching}
                    error={partError}
                  />
                ) : undefined
              }
            />
          </Fold>
          <Fold id="matchups">
            <Matchups d={shown} />
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
          {/* Ping and reflects: any demo with a timeline, STV or the
              owner's own; nothing at all without one. */}
          <Fold id="ping">
            <PingPanel d={q.data} />
          </Fold>
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
function PartPicker(props: {
  d: MatchDetail;
  parts: PartScore[] | null;
  part: number | null;
  onPick: (id: number | null) => void;
  fetching: boolean;
  error: string | null;
}) {
  const { d, parts, part, onPick, fetching, error } = props;
  const rows = parts ?? d.parts.map((p) => ({ ...p, detail: null, parentRounds: [] }));
  return (
    <div className="part-picker">
      <label className="an-field">
        <span className="an-label">{t("Reading")}</span>
        <select value={part ?? ""} onChange={(e) => onPick(e.target.value === "" ? null : Number(e.target.value))}>
          <option value="">{tx("The whole match · {parts} logs combined", { parts: d.parts.length })}</option>
          {rows.map((p) => (
            <option key={p.logId} value={p.logId}>
              {tx("{0} · log {logId}{2}", { "0": capitalize(splitMap(p.map).name ?? "unknown"), "2": "detail" in p && p.detail ? "" : t(" (fetches)"), logId: p.logId })}
            </option>
          ))}
        </select>
      </label>
      {fetching && <span className="hint"><Spinner /> {t("Fetching…")}</span>}
      {part !== null && !fetching && <span className="hint">{t("this log alone")}</span>}
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
