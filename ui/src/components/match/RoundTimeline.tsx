import type { EventRow, MatchDetail, RoundRow, Team } from "../../api/types";
import { clock, teamLabel } from "../../lib/format";

/**
 * Every round on its own track, with caps, ubers, drops and medic deaths
 * placed where they happened. Medic deaths are the only kills logs.tf
 * timestamps, so they are the only picks that can appear here.
 *
 * Teams are stable teams throughout: stopwatch swaps colours between halves,
 * and the backend has already mapped every round back to who was who. So a
 * team keeps one colour down the whole page, and each round says which colour
 * it actually wore.
 */
export function RoundTimeline({ d }: { d: MatchDetail }) {
  if (d.rounds.length === 0) {
    return (
      <section className="panel">
        <h2>Rounds</h2>
        <p className="hint" style={{ marginTop: 6 }}>
          This log has no round data.
        </p>
      </section>
    );
  }

  const left = d.leftTeam;
  const right: Team = left === "Red" ? "Blue" : "Red";
  const us = d.myTeam !== null;
  const anySwapped = d.rounds.some((r) => r.coloursSwapped);

  return (
    <section className="panel rounds">
      <header className="rounds-head">
        <div>
          <h2>Rounds</h2>
          {anySwapped && (
            <p className="hint" style={{ marginTop: 4 }}>
              Sides swap between stopwatch halves. Colours here follow the team, not the side it
              played that half.
            </p>
          )}
        </div>
        <Legend />
      </header>
      {d.rounds.map((r) => (
        <Round key={r.roundNum} r={r} left={left} right={right} us={us} />
      ))}
    </section>
  );
}

function Round(props: { r: RoundRow; left: Team; right: Team; us: boolean }) {
  const { r, left, right, us } = props;
  const len = r.lengthS ?? Math.max(1, ...r.events.map((e) => e.atS));
  const events = r.events.filter((e) => e.kind !== "round_win");

  const outcome =
    r.winner === null ? "no winner" : !us ? `${teamLabel(r.winner)} won` : r.winner === left ? "won" : "lost";
  const outcomeClass = r.winner === null || !us ? "" : r.winner === left ? "result-W" : "result-L";

  // The colour the left team actually wore this round.
  const leftWore: Team = r.coloursSwapped ? right : left;

  const ticks = Array.from({ length: Math.floor(len / 60) }, (_, i) => (i + 1) * 60);
  const stat = (t: Team, red: number | null, blue: number | null) => (t === "Red" ? red : blue);

  return (
    <div className="round">
      <div className="round-meta">
        <span className="round-num">R{r.roundNum}</span>
        <span className={`round-outcome ${outcomeClass}`}>{outcome}</span>
        <span className="muted">{clock(r.lengthS)}</span>
        {us && (
          <span
            className={`wore wore-${leftWore.toLowerCase()}`}
            title={r.coloursSwapped ? "Sides swapped this half" : undefined}
          >
            as {teamLabel(leftWore)}
          </span>
        )}
      </div>

      <div className="track">
        {ticks.map((t) => (
          <span key={t} className="tick" style={{ left: `${(t / len) * 100}%` }} />
        ))}
        {events.map((e, i) => (
          <Marker key={i} e={e} len={len} left={left} us={us} />
        ))}
      </div>

      <div className="round-stats">
        <Pair
          label="kills"
          a={stat(left, r.redKills, r.blueKills)}
          b={stat(right, r.redKills, r.blueKills)}
          left={left}
          right={right}
        />
        <Pair
          label="ubers"
          a={stat(left, r.redUbers, r.blueUbers)}
          b={stat(right, r.redUbers, r.blueUbers)}
          left={left}
          right={right}
        />
        {r.firstcap && (
          <span className="muted">
            first cap{" "}
            <span className={`team-${r.firstcap.toLowerCase()}`}>
              {us ? (r.firstcap === left ? "us" : "them") : teamLabel(r.firstcap)}
            </span>
          </span>
        )}
      </div>
    </div>
  );
}

function Marker(props: { e: EventRow; len: number; left: Team; us: boolean }) {
  const { e, len, left, us } = props;
  const pos = `${Math.min(100, (e.atS / len) * 100)}%`;
  const team = e.team?.toLowerCase() ?? "none";
  const at = clock(e.atS);
  const who = e.team === null ? "" : us ? (e.team === left ? "Our" : "Their") : teamLabel(e.team);

  switch (e.kind) {
    case "pointcap":
      return (
        <span
          className={`mk mk-cap team-bg-${team}`}
          style={{ left: pos }}
          title={`${at} — ${who} cap${e.point !== null ? `, point ${e.point}` : ""}`}
        />
      );
    case "charge":
      return (
        <span
          className={`mk mk-uber team-border-${team}`}
          style={{ left: pos }}
          title={`${at} — ${who} uber${e.medigun && e.medigun !== "medigun" ? ` (${e.medigun})` : ""}${e.player ? `, ${e.player}` : ""}`}
        >
          U
        </span>
      );
    case "drop":
      return (
        <span className="mk mk-drop" style={{ left: pos }} title={`${at} — ${who} drop: ${e.player ?? "medic"} died with uber ready`}>
          D
        </span>
      );
    case "medic_death":
      return (
        <span
          className={`mk mk-pick ${e.killerIsMe ? "by-me" : ""} team-border-${team}`}
          style={{ left: pos }}
          title={`${at} — ${who} Medic ${e.player ?? ""} killed${e.killer ? ` by ${e.killer}` : ""}${e.killerIsMe ? " (you)" : ""}`}
        >
          ✚
        </span>
      );
    default:
      return null;
  }
}

function Pair(props: { label: string; a: number | null; b: number | null; left: Team; right: Team }) {
  const { label, a, b, left, right } = props;
  if (a === null && b === null) return null;
  return (
    <span className="muted">
      {label} <span className={`team-${left.toLowerCase()}`}>{a ?? "–"}</span>
      <span className="sep"> : </span>
      <span className={`team-${right.toLowerCase()}`}>{b ?? "–"}</span>
    </span>
  );
}

function Legend() {
  return (
    <div className="legend">
      <span>
        <span className="mk-demo mk-cap team-bg-none" /> cap
      </span>
      <span>
        <span className="mk-demo mk-uber">U</span> uber
      </span>
      <span>
        <span className="mk-demo mk-drop">D</span> drop
      </span>
      <span>
        <span className="mk-demo mk-pick">✚</span> medic killed
      </span>
      <span>
        <span className="mk-demo mk-pick by-me">✚</span> by you
      </span>
    </div>
  );
}
