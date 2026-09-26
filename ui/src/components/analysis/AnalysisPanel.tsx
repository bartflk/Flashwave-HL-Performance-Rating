import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type Analysis, type MatchDetail } from "../../api/types";
import { capitalize, splitMap, teamLabel } from "../../lib/format";
import type { Slice } from "./common";
import { Aim } from "./Aim";
import { Fights } from "./Fights";
import { KillMap } from "./KillMap";
import { PlayByPlay } from "./PlayByPlay";
import { Spread } from "./Spread";
import { TimelineChart } from "./TimelineChart";
import "./analysis.css";

type Tab = "map" | "feed" | "fights" | "spread" | "aim" | "timeline";

const TABS: Array<[Tab, string]> = [
  ["map", "Kill map"],
  ["feed", "Play-by-play"],
  ["fights", "Fights"],
  ["spread", "Damage and kills by class"],
  ["aim", "Aim"],
  ["timeline", "Timeline"],
];

/**
 * The match seen kill by kill, from its raw server log. One filter row (the
 * player, the map of a combined log, the round) scopes every view below it;
 * the tabs are four ways of looking at the same slice.
 */
export function AnalysisPanel({ d, onlyRounds }: { d: MatchDetail; onlyRounds?: number[] | null }) {
  const q = useQuery({ queryKey: ["analysis", d.logId], queryFn: () => api.getMatchAnalysis(d.logId) });

  return (
    <section className="panel analysis">
      <header className="an-head">
        <h2>Kill by kill</h2>
      </header>
      {q.isPending && <p className="hint">Reading the raw log…</p>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data === null && (
        <p className="hint">Raw log not stored yet — run a sync.</p>
      )}
      {q.data && (
        <Body
          a={q.data}
          stv={{ demosTfId: d.demosTfId, hasStv: d.demos.some((x) => x.kind === "stv") }}
          onlyRounds={onlyRounds ?? null}
        />
      )}
    </section>
  );
}

/** What the match page knows about demos, which the map offers to fetch. */
export interface StvInfo {
  demosTfId: number | null;
  hasStv: boolean;
}

function Body({ a, stv, onlyRounds }: { a: Analysis; stv: StvInfo; onlyRounds: number[] | null }) {
  const me = a.players.find((p) => p.isMe) ?? null;
  const [tab, setTab] = useState<Tab>("map");
  const [player, setPlayer] = useState<number>(me?.accountId ?? a.players[0]?.accountId ?? 0);
  const [round, setRound] = useState<number | null>(null);
  // A combined log opens on its first map: a kill map across three maps
  // would overlay three different places.
  const mapCount = new Set(a.segments.map((s) => s.map)).size;
  const multiMap = mapCount > 1;
  const [seg, setSeg] = useState<number | null>(multiMap ? 0 : null);

  // The page can be reading one log of a combined upload: then every view
  // here is held to that log's rounds, whatever else is picked.
  const only = useMemo(() => (onlyRounds ? new Set(onlyRounds) : null), [onlyRounds]);

  const slice: Slice = useMemo(() => {
    const segment = seg === null ? null : a.segments[seg] ?? null;
    if (round !== null) {
      const r = a.rounds.find((x) => x.roundNum === round);
      const on = a.segments.find((s) => s.rounds.includes(round));
      return {
        rounds: new Set([round]),
        startS: r?.startS ?? 0,
        endS: r?.endS ?? a.durationS,
        oneRound: true,
        map: on?.map ?? a.map,
        multiMap,
      };
    }
    if (segment) {
      return {
        rounds: new Set(segment.rounds),
        startS: segment.startS,
        endS: segment.endS,
        oneRound: false,
        map: segment.map,
        multiMap,
      };
    }
    if (only) {
      // The log's own rounds, laid on the combined log's clock.
      const mine = a.rounds.filter((r) => only.has(r.roundNum));
      return {
        rounds: only,
        startS: mine[0]?.startS ?? 0,
        endS: mine[mine.length - 1]?.endS ?? a.durationS,
        oneRound: mine.length === 1,
        map: a.segments.find((sg) => sg.rounds.some((n) => only.has(n)))?.map ?? a.map,
        multiMap: false,
      };
    }
    return { rounds: null, startS: 0, endS: a.durationS, oneRound: false, map: multiMap ? null : a.segments[0]?.map ?? a.map, multiMap };
  }, [a, seg, round, multiMap, only]);

  // The round buttons show the chosen map's rounds, or only the rounds of
  // the log the page is reading.
  const roundChoices = (seg === null ? a.rounds : a.rounds.filter((r) => a.segments[seg].rounds.includes(r.roundNum))).filter(
    (r) => !only || only.has(r.roundNum),
  );

  const teams = useMemo(() => {
    const byTeam = (t: "Red" | "Blue") => a.players.filter((p) => p.team === t);
    // The owner's team first, like everywhere else on the page.
    return me?.team === "Red" ? [byTeam("Red"), byTeam("Blue")] : [byTeam("Blue"), byTeam("Red")];
  }, [a.players, me]);

  return (
    <>
      <div className="an-filters">
        <label className="an-field">
          <span className="an-label">Player</span>
          <select value={player} onChange={(e) => setPlayer(Number(e.target.value))}>
            {teams.map((team) =>
              team.length === 0 ? null : (
                <optgroup key={team[0].team} label={teamLabel(team[0].team)}>
                  {team.map((p) => (
                    <option key={p.accountId} value={p.accountId}>
                      {p.name}
                      {p.mainClass ? ` · ${capitalize(p.mainClass)}` : ""}
                      {p.isMe ? " (you)" : ""}
                    </option>
                  ))}
                </optgroup>
              ),
            )}
          </select>
        </label>
        {multiMap && !only && (
          <div className="segmented" role="tablist" aria-label="Map">
            <button
              role="tab"
              aria-selected={seg === null}
              className={seg === null ? "seg active" : "seg"}
              onClick={() => {
                setSeg(null);
                setRound(null);
              }}
            >
              All maps
            </button>
            {a.segments.map((s, i) => (
              <button
                key={i}
                role="tab"
                aria-selected={seg === i}
                className={seg === i ? "seg active" : "seg"}
                onClick={() => {
                  setSeg(i);
                  setRound(null);
                }}
                title={s.map ?? "map not known"}
              >
                {capitalize(splitMap(s.map).name ?? "unknown map")}
              </button>
            ))}
          </div>
        )}
        <div className="segmented" role="tablist" aria-label="Round">
          <button role="tab" aria-selected={round === null} className={round === null ? "seg active" : "seg"} onClick={() => setRound(null)}>
            All rounds
          </button>
          {roundChoices.map((r) => (
            <button
              key={r.roundNum}
              role="tab"
              aria-selected={round === r.roundNum}
              className={round === r.roundNum ? "seg active" : "seg"}
              onClick={() => setRound(r.roundNum)}
            >
              R{r.roundNum}
            </button>
          ))}
        </div>
      </div>

      <nav className="an-tabs" role="tablist" aria-label="View">
        {TABS.map(([id, label]) => (
          <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? "an-tab active" : "an-tab"} onClick={() => setTab(id)}>
            {label}
          </button>
        ))}
      </nav>

      {tab === "map" && <KillMap a={a} player={player} slice={slice} stv={stv} />}
      {tab === "feed" && <PlayByPlay a={a} player={player} slice={slice} />}
      {tab === "fights" && <Fights a={a} player={player} slice={slice} onPick={setPlayer} />}
      {tab === "spread" && <Spread a={a} player={player} slice={slice} />}
      {tab === "aim" && <Aim a={a} logId={a.logId} player={player} slice={slice} />}
      {tab === "timeline" && <TimelineChart a={a} player={player} slice={slice} onPick={setPlayer} />}
    </>
  );
}
