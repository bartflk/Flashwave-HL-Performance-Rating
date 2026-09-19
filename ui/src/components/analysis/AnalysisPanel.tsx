import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type Analysis, type MatchDetail } from "../../api/types";
import { capitalize, teamLabel } from "../../lib/format";
import { KillMap } from "./KillMap";
import { PlayByPlay } from "./PlayByPlay";
import { Spread } from "./Spread";
import { TimelineChart } from "./TimelineChart";
import "./analysis.css";

type Tab = "map" | "feed" | "spread" | "timeline";

const TABS: Array<[Tab, string]> = [
  ["map", "Kill map"],
  ["feed", "Play-by-play"],
  ["spread", "Damage and kills by class"],
  ["timeline", "Timeline"],
];

/**
 * The match seen kill by kill, from its raw server log. One filter row (the
 * player and the round) scopes every view below it; the tabs are four ways of
 * looking at the same slice.
 */
export function AnalysisPanel({ d }: { d: MatchDetail }) {
  const q = useQuery({ queryKey: ["analysis", d.logId], queryFn: () => api.getMatchAnalysis(d.logId) });

  return (
    <section className="panel analysis">
      <header className="an-head">
        <h2>Kill by kill</h2>
        <p className="hint">From the match&apos;s raw server log: every kill with where both players stood.</p>
      </header>
      {q.isPending && <p className="hint">Reading the raw log…</p>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data === null && (
        <p className="hint">
          This match&apos;s raw log is not stored yet. Sync fetches it; logs.tf has none for a few very old matches.
        </p>
      )}
      {q.data && <Body a={q.data} />}
    </section>
  );
}

function Body({ a }: { a: Analysis }) {
  const me = a.players.find((p) => p.isMe) ?? null;
  const [tab, setTab] = useState<Tab>("map");
  const [player, setPlayer] = useState<number>(me?.accountId ?? a.players[0]?.accountId ?? 0);
  const [round, setRound] = useState<number | null>(null);

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
        <div className="segmented" role="tablist" aria-label="Round">
          <button role="tab" aria-selected={round === null} className={round === null ? "seg active" : "seg"} onClick={() => setRound(null)}>
            All rounds
          </button>
          {a.rounds.map((r) => (
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

      {tab === "map" && <KillMap a={a} player={player} round={round} />}
      {tab === "feed" && <PlayByPlay a={a} player={player} round={round} />}
      {tab === "spread" && <Spread a={a} player={player} />}
      {tab === "timeline" && <TimelineChart a={a} player={player} round={round} onPick={setPlayer} />}
    </>
  );
}
