import { useMemo, useState } from "react";
import type { Analysis, Jump, KillView, PlayEvent, Team } from "../../api/types";
import { capitalize, teamLabel } from "../../lib/format";
import { CLASS_SHORT, inSlice, jumpTo, playerMap, roundClock, type Slice } from "./common";

type Kind = "kills" | "ubers" | "caps" | "chat" | "streaks";

const FILTERS: Array<[Kind, string]> = [
  ["kills", "Kills"],
  ["ubers", "Ubers"],
  ["caps", "Caps"],
  ["chat", "Chat"],
  ["streaks", "Streaks"],
];

type Row = { t: number; roundNum: number; kind: Kind; kill?: KillView; ev?: PlayEvent; jump: Jump | null };

/**
 * The match as a feed, grouped by round. Kills come from the raw log; ubers,
 * drops and caps from logs.tf; streaks are three or more kills without dying.
 * Every row with a demo behind it copies its tick.
 */
export function PlayByPlay({ a, player, slice }: { a: Analysis; player: number; slice: Slice }) {
  const [on, setOn] = useState<Set<Kind>>(new Set(["kills", "ubers", "caps", "streaks"]));
  const [onlyPlayer, setOnlyPlayer] = useState(false);
  const players = useMemo(() => playerMap(a), [a]);
  const who = players.get(player);

  const rows: Row[] = useMemo(() => {
    const out: Row[] = [];
    for (const k of a.kills) {
      out.push({ t: k.t, roundNum: k.roundNum, kind: "kills", kill: k, jump: k.jump });
    }
    for (const e of a.events) {
      const kind: Kind | null =
        e.kind === "charge" || e.kind === "drop"
          ? "ubers"
          : e.kind === "pointcap"
            ? "caps"
            : e.kind === "chat"
              ? "chat"
              : e.kind === "streak"
                ? "streaks"
                : null;
      if (kind) out.push({ t: e.t, roundNum: e.roundNum, kind, ev: e, jump: e.jump });
    }
    return out.sort((x, y) => x.t - y.t);
  }, [a]);

  const involves = (r: Row) =>
    r.kill
      ? r.kill.killer === player || r.kill.victim === player || r.kill.assister === player
      : r.ev?.player === player || (r.ev?.victims ?? []).includes(player);

  const shown = rows.filter((r) => on.has(r.kind) && inSlice(r.roundNum, slice) && (!onlyPlayer || involves(r)));
  const byRound = new Map<number, Row[]>();
  for (const r of shown) byRound.set(r.roundNum, [...(byRound.get(r.roundNum) ?? []), r]);

  const toggle = (k: Kind) =>
    setOn((s) => {
      const n = new Set(s);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });

  const name = (id: number | null | undefined) => (id == null ? "?" : players.get(id)?.name ?? "?");
  const team = (id: number | null | undefined): Team | null => (id == null ? null : players.get(id)?.team ?? null);
  const Who = ({ id }: { id: number | null | undefined }) => (
    <span className={`pbp-name team-${(team(id) ?? "none").toLowerCase()}${id === player ? " pbp-me" : ""}`}>{name(id)}</span>
  );

  return (
    <div className="pbp">
      <div className="pbp-filters">
        {FILTERS.map(([k, label]) => (
          <button key={k} className={on.has(k) ? "chip on" : "chip"} aria-pressed={on.has(k)} onClick={() => toggle(k)}>
            {label}
          </button>
        ))}
        <label className="check">
          <input type="checkbox" checked={onlyPlayer} onChange={(e) => setOnlyPlayer(e.target.checked)} />
          Only rows with {who?.name ?? "the player"}
        </label>
        <span className="hint">{shown.length} rows</span>
      </div>

      {shown.length === 0 && <p className="hint an-empty">Nothing for this filter.</p>}

      {[...byRound.entries()].map(([rn, list]) => (
        <section key={rn} className="pbp-round">
          <h3 className="pbp-round-head">Round {rn}</h3>
          <ol className="pbp-list">
            {list.map((r, i) => (
              <li
                key={i}
                className={`pbp-row pbp-${r.kind}${r.jump ? " jumpable" : ""}${involves(r) ? " pbp-involved" : ""}`}
                onClick={() => r.jump && jumpTo(r.jump, `${r.kind} at ${roundClock(r.t, a.rounds)}`)}
                title={r.jump ? `Copy demo_gototick ${r.jump.tick}` : undefined}
              >
                <span className="pbp-time">{roundClock(r.t, a.rounds).replace(/^R\d+ /, "")}</span>
                <span className="pbp-body">
                  {r.kill && (
                    <>
                      <Who id={r.kill.killer} /> <span className="muted">{cls(r.kill.killerClass)}</span>
                      <span className="pbp-arrow"> → </span>
                      <Who id={r.kill.victim} /> <span className="muted">{cls(r.kill.victimClass)}</span>
                      <KillTagChips k={r.kill} />
                      {r.kill.victim === player && <DeathTagChips k={r.kill} />}
                      <span className="pbp-meta">
                        {r.kill.weapon}
                        {r.kill.custom && ` · ${r.kill.custom}`}
                        {r.kill.assister !== null && (
                          <>
                            {" · assist "}
                            <Who id={r.kill.assister} />
                          </>
                        )}
                      </span>
                    </>
                  )}
                  {r.ev?.kind === "charge" && (
                    <>
                      <Who id={r.ev.player} /> popped{" "}
                      {r.ev.text && r.ev.text !== "medigun" ? capitalize(r.ev.text) : "uber"}
                    </>
                  )}
                  {r.ev?.kind === "drop" && (
                    <>
                      <Who id={r.ev.player} /> <strong className="warn-text">dropped uber</strong>
                    </>
                  )}
                  {r.ev?.kind === "pointcap" && (
                    <>
                      <span className={`team-${(r.ev.team ?? "none").toLowerCase()}`}>{r.ev.team ? teamLabel(r.ev.team) : "?"}</span>{" "}
                      captured{r.ev.text ? ` point ${r.ev.text}` : ""}
                    </>
                  )}
                  {r.ev?.kind === "chat" && (
                    <>
                      <Who id={r.ev.player} />
                      {r.ev.teamChat && <span className="muted"> (team)</span>}: <span className="pbp-chat">{r.ev.text}</span>
                    </>
                  )}
                  {r.ev?.kind === "streak" && (
                    <>
                      <Who id={r.ev.player} /> <strong>{r.ev.text}-kill streak</strong>
                      <span className="pbp-meta">
                        {r.ev.victims.map((v, j) => (
                          <span key={j}>
                            {j > 0 && ", "}
                            <Who id={v} />
                          </span>
                        ))}
                      </span>
                    </>
                  )}
                </span>
              </li>
            ))}
          </ol>
        </section>
      ))}
    </div>
  );
}

function cls(c: string | null): string {
  return c ? `(${CLASS_SHORT[c] ?? c})` : "";
}

/** The few labels worth a glance in a feed: rare enough to stand out. */
function KillTagChips({ k }: { k: KillView }) {
  const t = k.tags;
  if (!t) return null;
  const chips: Array<[string, string, string]> = [];
  if (t.firstOfRound) chips.push(["first pick", "tag-open", "The first kill of the round"]);
  else if (t.opening) chips.push(["opening", "tag-open", "The first kill of a fight: more than 10 s after the last one"]);
  if (t.drop) chips.push(["drop", "tag-charge", "The Medic died holding a ready charge"]);
  else if (t.intoCharge) chips.push(["into charge", "tag-charge", "A combo player killed while their team held a ready charge"]);
  // "Traded" and "clean-up" fit a third of all kills each: in the Fights tab, not here.
  if (t.diedAfter) chips.push(["died after", "tag-traded", "The killer died within 3 s"]);
  return (
    <>
      {chips.map(([label, cls, title]) => (
        <span key={label} className={`kill-tag ${cls}`} title={title}>
          {label}
        </span>
      ))}
    </>
  );
}

/** Why the chosen player died, on their own deaths only (PLAN §12 step 1). */
function DeathTagChips({ k }: { k: KillView }) {
  const t = k.tags;
  if (!t) return null;
  return (
    <>
      {t.deathTraded ? (
        <span className="kill-tag tag-open" title="Your team killed back within 3 s: the death opened something">
          traded
        </span>
      ) : (
        <span className="kill-tag tag-traded" title="Nobody on your team killed back within 3 s">
          untraded
        </span>
      )}
      {t.stationary && (
        <span className="kill-tag tag-plain" title="You died near a spot you had already got two kills from this life">
          stayed put
        </span>
      )}
    </>
  );
}
