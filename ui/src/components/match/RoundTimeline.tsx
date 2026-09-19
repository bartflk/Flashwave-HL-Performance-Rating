import type { EventRow, Jump, MatchDetail, RoundRow, Team } from "../../api/types";
import { copy } from "../../lib/toast";
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
  // Only worth naming the demo in a copy confirmation when there are several.
  const demoName = (j: Jump) =>
    d.demos.length > 1 ? d.demos.find((x) => x.demoId === j.demoId)?.fileName : undefined;
  const hasJumps = d.rounds.some((r) => r.jump !== null);
  const hasMine = d.rounds.some((r) => r.events.some((e) => MINE.has(e.kind)));
  const won = d.rounds.filter((r) => r.winner === left).length;
  const lost = d.rounds.filter((r) => r.winner === right).length;
  const leftName = us ? "Us" : teamLabel(left);
  const rightName = us ? "Them" : teamLabel(right);

  return (
    <section className="panel rounds">
      <header className="rounds-head">
        <div>
          <h2>Rounds</h2>
          <p className="hint" style={{ marginTop: 4 }}>
            {us ? `You won ${won} of ${d.rounds.length} rounds.` : `${teamLabel(left)} ${won}, ${teamLabel(right)} ${lost}.`}{" "}
            Each round has a lane per team: that team&apos;s caps and ubers, and its Medic going down.
            {anySwapped && " Sides swap between stopwatch halves; colours follow the team, not the side."}
          </p>
        </div>
        <Legend jumps={hasJumps} mine={hasMine} />
      </header>
      <div className="round-list">
        {d.rounds.map((r) => (
          <Round
            key={r.roundNum}
            r={r}
            left={left}
            right={right}
            us={us}
            names={[leftName, rightName]}
            showMine={hasMine}
            demoName={demoName}
          />
        ))}
      </div>
    </section>
  );
}

/** Events about the owner alone: their kills, deaths and demo killstreaks. */
const MINE = new Set(["my_kill", "my_death", "killstreak"]);

function Round(props: {
  r: RoundRow;
  left: Team;
  right: Team;
  us: boolean;
  names: [string, string];
  showMine: boolean;
  demoName: (j: Jump) => string | undefined;
}) {
  const { r, left, right, us, names, showMine, demoName } = props;
  const len = r.lengthS ?? Math.max(1, ...r.events.map((e) => e.atS));
  const events = r.events.filter((e) => e.kind !== "round_win");
  // An event with no team (rare) goes in the second lane rather than nowhere.
  const lane = (team: Team) =>
    events.filter((e) => !MINE.has(e.kind) && (e.team === team || (team === right && e.team === null)));
  const mine = events.filter((e) => MINE.has(e.kind));

  const result = r.winner === null ? "–" : !us ? `${teamLabel(r.winner)}` : r.winner === left ? "Won" : "Lost";
  const resultClass = r.winner === null || !us ? "round-result" : r.winner === left ? "round-result result-W" : "round-result result-L";

  // The colour the left team actually wore this round.
  const leftWore: Team = r.coloursSwapped ? right : left;

  const ticks = Array.from({ length: Math.floor(len / 60) }, (_, i) => (i + 1) * 60);
  const stat = (t: Team, red: number | null, blue: number | null) => (t === "Red" ? red : blue);
  const markers = (list: EventRow[]) =>
    list.map((e, i) => <Marker key={i} e={e} len={len} left={left} us={us} demoName={demoName} />);

  return (
    <div className="round">
      <div className="round-meta">
        {r.jump ? (
          <button
            className="round-num jumpable"
            title={`Copy demo_gototick ${r.jump.tick} (round start)`}
            onClick={() => jumpTo(r.jump!, `round ${r.roundNum} start`, demoName)}
          >
            R{r.roundNum}
          </button>
        ) : (
          <span className="round-num">R{r.roundNum}</span>
        )}
        <span className={resultClass}>{result}</span>
        <span className="round-sub">
          {clock(r.lengthS)}
          {us && (
            <span className={`wore wore-${leftWore.toLowerCase()}`} title={r.coloursSwapped ? "Sides swapped this half" : undefined}>
              {teamLabel(leftWore)}
            </span>
          )}
        </span>
      </div>

      <div className="round-lanes">
        <span className={`lane-label team-${left.toLowerCase()}`}>{names[0]}</span>
        <div className={`lane lane-team lane-${left.toLowerCase()}`}>{markers(lane(left))}</div>
        {showMine && (
          <>
            <span className="lane-label">You</span>
            <div className="lane lane-mine">{markers(mine)}</div>
          </>
        )}
        <span className={`lane-label team-${right.toLowerCase()}`}>{names[1]}</span>
        <div className={`lane lane-team lane-${right.toLowerCase()}`}>{markers(lane(right))}</div>
        <div className="lane-ticks" aria-hidden>
          {ticks.map((t) => (
            <span key={t} className="tick" style={{ left: `${(t / len) * 100}%` }} />
          ))}
        </div>
      </div>

      <div className="round-stats">
        <StatRow label="Kills" a={stat(left, r.redKills, r.blueKills)} b={stat(right, r.redKills, r.blueKills)} left={left} right={right} />
        <StatRow label="Ubers" a={stat(left, r.redUbers, r.blueUbers)} b={stat(right, r.redUbers, r.blueUbers)} left={left} right={right} />
        {r.firstcap && (
          <div className="rs-row">
            <span className="rs-label">First cap</span>
            <span className={`team-${r.firstcap.toLowerCase()}`}>{names[r.firstcap === left ? 0 : 1]}</span>
          </div>
        )}
      </div>
    </div>
  );
}

function Marker(props: {
  e: EventRow;
  len: number;
  left: Team;
  us: boolean;
  demoName: (j: Jump) => string | undefined;
}) {
  const { e, len, left, us, demoName } = props;
  const pos = `${Math.min(100, (e.atS / len) * 100)}%`;
  const team = e.team?.toLowerCase() ?? "none";
  const at = clock(e.atS);
  const who = e.team === null ? "" : us ? (e.team === left ? "Our" : "Their") : teamLabel(e.team);

  // A marker with a jump is a button: clicking it copies the tick.
  const jump = e.jump;
  const act = jump
    ? {
        role: "button" as const,
        tabIndex: 0,
        onClick: () => jumpTo(jump, `${e.kind.replace("_", " ")} at ${at}`, demoName),
        onKeyDown: (k: React.KeyboardEvent) => {
          if (k.key === "Enter" || k.key === " ") {
            k.preventDefault();
            jumpTo(jump, `${e.kind.replace("_", " ")} at ${at}`, demoName);
          }
        },
      }
    : {};
  const hint = jump ? " — click to copy demo_gototick" : "";
  const jumpCls = jump ? " jumpable" : "";

  switch (e.kind) {
    case "my_kill":
      return (
        <span
          className={`mk mk-mine mk-my-kill${jumpCls}`}
          style={{ left: pos }}
          title={`${at} — you killed ${e.player ?? "?"}${e.value ? ` (${e.value})` : ""}${hint}`}
          {...act}
        />
      );
    case "my_death":
      return (
        <span
          className={`mk mk-mine mk-my-death${jumpCls}`}
          style={{ left: pos }}
          title={`${at} — ${e.killer ?? "?"} killed you${e.value ? ` (${e.value})` : ""}${hint}`}
          {...act}
        />
      );
    case "killstreak":
      return (
        <span
          className={`mk mk-streak${jumpCls}`}
          style={{ left: pos }}
          title={`${at} — killstreak of ${e.value ?? "?"} (from your demo)${hint}`}
          {...act}
        >
          {e.value ?? "K"}
        </span>
      );
    case "pointcap":
      return (
        <span
          className={`mk mk-cap team-bg-${team}${jumpCls}`}
          style={{ left: pos }}
          title={`${at} — ${who} cap${e.point !== null ? `, point ${e.point}` : ""}${hint}`}
          {...act}
        />
      );
    case "charge":
      return (
        <span
          className={`mk mk-uber team-border-${team}${jumpCls}`}
          style={{ left: pos }}
          title={`${at} — ${who} uber${e.medigun && e.medigun !== "medigun" ? ` (${e.medigun})` : ""}${e.player ? `, ${e.player}` : ""}${hint}`}
          {...act}
        >
          U
        </span>
      );
    case "drop":
      return (
        <span
          className={`mk mk-drop${jumpCls}`}
          style={{ left: pos }}
          title={`${at} — ${who} drop: ${e.player ?? "medic"} died with uber ready${hint}`}
          {...act}
        >
          D
        </span>
      );
    case "medic_death":
      return (
        <span
          className={`mk mk-pick ${e.killerIsMe ? "by-me" : ""} team-border-${team}${jumpCls}`}
          style={{ left: pos }}
          title={`${at} — ${who} Medic ${e.player ?? ""} killed${e.killer ? ` by ${e.killer}` : ""}${e.killerIsMe ? " (you)" : ""}${hint}`}
          {...act}
        >
          ✚
        </span>
      );
    default:
      return null;
  }
}

function StatRow(props: { label: string; a: number | null; b: number | null; left: Team; right: Team }) {
  const { label, a, b, left, right } = props;
  if (a === null && b === null) return null;
  return (
    <div className="rs-row">
      <span className="rs-label">{label}</span>
      <span>
        <strong className={`team-${left.toLowerCase()}`}>{a ?? "–"}</strong>
        <span className="sep"> – </span>
        <strong className={`team-${right.toLowerCase()}`}>{b ?? "–"}</strong>
      </span>
    </div>
  );
}

function jumpTo(j: Jump, what: string, demoName: (j: Jump) => string | undefined) {
  const name = demoName(j);
  void copy(`demo_gototick ${j.tick}`, name ? `${what} (in ${name})` : what);
}

function Legend({ jumps, mine }: { jumps: boolean; mine: boolean }) {
  return (
    <div className="legend">
      {jumps && <span className="legend-note">click any marker to copy its tick</span>}
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
        <span className="mk-demo mk-pick">✚</span> Medic down
      </span>
      <span>
        <span className="mk-demo mk-pick by-me">✚</span> killed by you
      </span>
      {jumps && (
        <span>
          <span className="mk-demo mk-streak">4</span> your killstreak
        </span>
      )}
      {mine && (
        <>
          <span>
            <span className="mk-demo mk-mine mk-my-kill" /> your kill
          </span>
          <span>
            <span className="mk-demo mk-mine mk-my-death" /> your death
          </span>
        </>
      )}
    </div>
  );
}
