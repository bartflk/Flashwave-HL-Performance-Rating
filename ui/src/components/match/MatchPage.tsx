import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Parts } from "./Parts";
import { api } from "../../api/client";
import { errorMessage, type MatchContext, type MatchDetail, type PartScore } from "../../api/types";
import { capitalize, formatDate, minutes, splitMap, teamLabel } from "../../lib/format";
import { ContextBadge, kindReason } from "../ContextBadge";
import { BoxScore } from "./BoxScore";
import { Fold } from "../Fold";
import { DemoPanel } from "./DemoPanel";
import { AnalysisPanel } from "../analysis/AnalysisPanel";
import { Matchups } from "./Matchups";
import { RoundTimeline } from "./RoundTimeline";

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
      <button className="linkish back" onClick={onBack}>
        ← All matches
      </button>

      {q.isPending && <p className="hint">Loading match…</p>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data === null && (
        <p className="hint">This log is not stored yet. Run a sync, then open it again.</p>
      )}
      {q.data && shown && (
        <>
          <Header d={shown} />
          {q.data.parts.length > 0 && (
            <PartPicker
              d={q.data}
              parts={partsQ.data ?? null}
              part={part}
              onPick={(id) => void pick(id)}
              fetching={fetching}
              error={partError}
            />
          )}
          {/* The scoreboard first, as on logs.tf; the matchups read it next. */}
          <Fold id="scoreboard">
            <BoxScore d={shown} />
          </Fold>
          <Fold id="matchups">
            <Matchups d={shown} />
          </Fold>
          <Fold id="demos">
            <DemoPanel d={q.data} />
          </Fold>
          {part === null && (
            <Fold id="parts">
              <Parts d={q.data} />
            </Fold>
          )}
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
    <section className="panel part-picker">
      <label className="an-field">
        <span className="an-label">Reading</span>
        <select value={part ?? ""} onChange={(e) => onPick(e.target.value === "" ? null : Number(e.target.value))}>
          <option value="">The whole match · {d.parts.length} logs combined</option>
          {rows.map((p) => (
            <option key={p.logId} value={p.logId}>
              {capitalize(splitMap(p.map).name ?? "unknown")} · log {p.logId}
              {"detail" in p && p.detail ? "" : " (fetches)"}
            </option>
          ))}
        </select>
      </label>
      <p className="hint">
        {fetching
          ? "Fetching that log from logs.tf…"
          : part === null
            ? "This upload holds several logs. Pick one to read the whole page as that map alone."
            : "Every panel below is this log alone, scored on its own."}
      </p>
      {error && <p className="error">{error}</p>}
    </section>
  );
}

function Header({ d }: { d: MatchDetail }) {
  // The resolved maps: a combined log's own map field is whatever its
  // uploader typed.
  const maps = [...new Set(d.segments.map((s) => s.map).filter((m): m is string => m !== null))];
  const { mode, name } = splitMap(maps.length === 1 ? maps[0] : d.map);
  const mine = d.myTeam;
  const [myScore, theirScore] = mine === "Blue" ? [d.blueScore, d.redScore] : [d.redScore, d.blueScore];

  const etf2lId = d.context?.etf2lMatchId ?? d.etf2lMatchId;
  const links: Array<[string, string]> = [["logs.tf", `https://logs.tf/${d.logId}`]];
  if (d.demosTfId) links.push(["demos.tf", `https://demos.tf/${d.demosTfId}`]);
  if (etf2lId) links.push(["ETF2L", `https://etf2l.org/matches/${etf2lId}/`]);

  return (
    <header className="panel match-header">
      <div className="mh-main">
        <div>
          <div className="mh-map">
            {maps.length > 1 ? (
              <h1>{maps.map((m) => capitalize(splitMap(m).name ?? "?")).join(" · ")}</h1>
            ) : (
              <>
                {mode && <span className={`mode mode-${mode}`}>{mode}</span>}
                <h1>{name ?? "Unknown map"}</h1>
              </>
            )}
          </div>
          {maps.length > 1 && <MapResults d={d} />}
          <p className="muted mh-sub">
            {formatDate(d.playedAt, true)} · {minutes(d.durationS)}
            {d.title && <> · {d.title}</>}
          </p>
        </div>

        <div className="mh-score">
          {d.result && <span className={`mh-result result-${d.result}`}>{d.result === "W" ? "Win" : d.result === "L" ? "Loss" : "Tie"}</span>}
          {mine ? (
            <span className="mh-numbers">
              {myScore}
              <span className="dash">–</span>
              {theirScore}
            </span>
          ) : (
            <span className="mh-numbers">
              <span className="team-red">{teamLabel("Red")} {d.redScore}</span>
              <span className="dash">–</span>
              <span className="team-blue">{d.blueScore} {teamLabel("Blue")}</span>
            </span>
          )}
          {mine && <span className={`muted team-${mine.toLowerCase()}`}>you played {teamLabel(mine)}</span>}
        </div>
      </div>

      {d.context && <ContextLine c={d.context} logScore={mine ? [myScore, theirScore] : null} />}

      <div className="mh-foot">
        <div>
          {!d.context && d.league && <span className="badge badge-league">{d.league.toUpperCase()}</span>}
          {d.demos.some((x) => x.kind === "pov") && <span className="badge badge-pov">POV demo</span>}
          {d.demosTfId && <span className="badge badge-demo">STV demo</span>}
          {d.format && d.format !== "highlander" && <span className="badge">{d.format}</span>}
        </div>
        <div className="mh-links">
          {links.map(([label, url]) => (
            <button key={label} className="linkish" onClick={() => void api.openExternal(url)}>
              {label} ↗
            </button>
          ))}
        </div>
      </div>
    </header>
  );
}

/**
 * What kind of game this was, and against whom. For an official, the ETF2L
 * competition, and ETF2L's own score when it differs from the log's (a
 * match played over several logs, or a result changed after the fact).
 */
function ContextLine({ c, logScore }: { c: MatchContext; logScore: [number, number] | null }) {
  const o = c.official;
  const parts: string[] = [];
  if (o?.competition) parts.push(o.competition);
  if (o?.round) parts.push(o.round);
  const sides = c.teamName || c.oppName ? `${c.teamName ?? "your team"} vs ${c.oppName ?? "unknown"}` : null;

  return (
    <div className="mh-context">
      <ContextBadge c={c} />
      {sides && <strong className="mh-sides">{sides}</strong>}
      {parts.length > 0 && <span className="muted">{parts.join(" · ")}</span>}
      {o?.score && !(logScore && o.score[0] === logScore[0] && o.score[1] === logScore[1]) && (
        <span className="muted" title="ETF2L's score for the match. In stopwatch this is not the same as rounds won.">
          ETF2L result <strong className={o.score[0] > o.score[1] ? "result-W" : o.score[0] < o.score[1] ? "result-L" : ""}>{o.score[0]}–{o.score[1]}</strong>
        </span>
      )}
      {o?.defaultWin && <span className="warn-text">default win</span>}
      {c.kind !== "official" && <span className="hint">{kindReason(c)}</span>}
      {c.linkMethod === "roster" && <span className="hint">found by roster; trends.tf had not tagged it</span>}
    </div>
  );
}

/**
 * A combined log's maps, each with the rounds won on it: from your side when
 * you played, otherwise RED–BLU. Rounds, not ETF2L's score, which counts
 * stopwatch and golden caps its own way.
 */
function MapResults({ d }: { d: MatchDetail }) {
  return (
    <p className="mh-maps">
      {d.segments.map((s, i) => {
        const [a, b] = d.myTeam === "Blue" ? [s.blueWins, s.redWins] : [s.redWins, s.blueWins];
        const cls = d.myTeam ? (a > b ? "result-W" : a < b ? "result-L" : "") : "";
        return (
          <span key={i} className="mh-map-result" title={`Rounds ${s.firstRound}–${s.lastRound}`}>
            {capitalize(splitMap(s.map).name ?? "unknown")} <strong className={cls}>{a}–{b}</strong>
          </span>
        );
      })}
      <span className="hint">rounds won{d.myTeam ? ", yours first" : ", RED–BLU"}</span>
    </p>
  );
}
