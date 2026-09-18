import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type MatchDetail } from "../../api/types";
import { formatDate, minutes, splitMap, teamLabel } from "../../lib/format";
import { BoxScore } from "./BoxScore";
import { Matchups } from "./Matchups";
import { RoundTimeline } from "./RoundTimeline";

export function MatchPage({ logId, onBack }: { logId: number; onBack: () => void }) {
  const q = useQuery({ queryKey: ["match", logId], queryFn: () => api.getMatch(logId) });

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
      {q.data && (
        <>
          <Header d={q.data} />
          <Matchups d={q.data} />
          <RoundTimeline d={q.data} />
          <BoxScore d={q.data} />
        </>
      )}
    </div>
  );
}

function Header({ d }: { d: MatchDetail }) {
  const { mode, name } = splitMap(d.map);
  const mine = d.myTeam;
  const [myScore, theirScore] = mine === "Blue" ? [d.blueScore, d.redScore] : [d.redScore, d.blueScore];

  const links: Array<[string, string]> = [["logs.tf", `https://logs.tf/${d.logId}`]];
  if (d.demosTfId) links.push(["demos.tf", `https://demos.tf/${d.demosTfId}`]);
  if (d.etf2lMatchId) links.push(["ETF2L", `https://etf2l.org/matches/${d.etf2lMatchId}/`]);

  return (
    <header className="panel match-header">
      <div className="mh-main">
        <div>
          <div className="mh-map">
            {mode && <span className={`mode mode-${mode}`}>{mode}</span>}
            <h1>{name ?? "Unknown map"}</h1>
          </div>
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

      <div className="mh-foot">
        <div>
          {d.league && <span className="badge badge-league">{d.league.toUpperCase()}</span>}
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
