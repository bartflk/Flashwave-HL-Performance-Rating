import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type Analysis, type AimRow, type AimTotals, type DeathRow, type LifeTotals } from "../../api/types";
import { AimCharts } from "./AimCharts";
import { playerMap, type Slice } from "./common";

/**
 * Your aim, read from the match's own demo (PLAN §14).
 *
 * The log says who you killed; the demo says where you were looking a second
 * before, how far the view travelled, and how far away they were. Only kills
 * the demo carried both players through get an answer, so a Spy killed round
 * a corner is left out.
 */
export function Aim({ a, logId, slice }: { a: Analysis; logId: number; slice: Slice }) {
  const q = useQuery({ queryKey: ["aim", logId], queryFn: () => api.getAim(logId) });
  const names = playerMap(a);
  // The filter row above applies here too: a round, or a map of a combined
  // log, narrows the kills and deaths the demo is read for.
  const inSlice = (round: number | null) => slice.rounds === null || (round !== null && slice.rounds.has(round));

  if (q.isPending) return <p className="hint an-empty">Reading the demo…</p>;
  if (q.isError) return <p className="error">{errorMessage(q.error)}</p>;
  const d = q.data;
  if (!d || d.kills.length === 0) {
    return (
      <p className="hint an-empty">
        No demo is linked to this match, so there is nothing to read. Point the app at your TF2 folder in Settings,
        and demos you recorded are matched to your logs automatically.
      </p>
    );
  }

  const kills = d.kills.filter((k) => inSlice(k.roundNum)).sort((x, y) => x.tick - y.tick);
  const deaths = d.deaths.filter((k) => inSlice(k.roundNum));
  const filtered = slice.rounds !== null;
  if (kills.length === 0 && deaths.length === 0) {
    return <p className="hint an-empty">No kills or deaths of yours in this round.</p>;
  }

  return (
    <div className="aim">
      {d.totals && !filtered && <Summary t={d.totals} career={d.career} life={d.life} careerLife={d.careerLife} />}
      {filtered && (
        <p className="hint">
          {kills.length} kill{kills.length === 1 ? "" : "s"} and {deaths.length} death
          {deaths.length === 1 ? "" : "s"} in this round. The cards above the tabs cover the whole match.
        </p>
      )}
      <AimCharts kills={kills} deaths={deaths} />
      <details className="aim-details">
        <summary>Every kill, in numbers</summary>
        <div className="table-wrap">
        <table className="match-table aim-table">
          <thead>
            <tr>
              <th>Victim</th>
              <th className="num" title="Degrees between your view and their head when the kill landed">
                Crosshair
              </th>
              <th className="num" title="The same, one second before the kill: how far the crosshair had to travel">
                A second before
              </th>
              <th className="num" title="How far your view turned in the half second before the shot">
                Flick
              </th>
              <th className="num" title="Distance in map units; a Sniper sightline is around 1,500">
                Range
              </th>
              <th className="num" title="How far above you they stood">
                Height
              </th>
              <th>Shot</th>
            </tr>
          </thead>
          <tbody>
            {kills.map((k) => (
              <tr key={k.tick} className={k.victimSeen ? undefined : "muted-row"}>
                <td className="nowrap player-name">{(k.victim !== null && names.get(k.victim)?.name) || "—"}</td>
                <td className="num">
                  <Deg v={k.errorDeg} good={3} bad={15} />
                </td>
                <td className="num">
                  <Deg v={k.beforeDeg} good={3} bad={30} />
                </td>
                <td className="num">{k.flickDeg.toFixed(1)}°</td>
                <td className="num">{k.rangeUnits.toFixed(0)}</td>
                <td className="num">{k.height.toFixed(0)}</td>
                <td className="nowrap">
                  {k.headshot && <span className="badge badge-hs">headshot</span>}
                  {!k.victimSeen && (
                    <span className="muted" title="The demo did not carry them the whole time, so these numbers are stale">
                      not on screen
                    </span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        </div>
      </details>
      {deaths.length > 0 && (
        <details className="aim-details">
          <summary>Every death, in numbers</summary>
          <Deaths rows={deaths} names={names} />
        </details>
      )}
      <p className="hint aim-foot">
        Angles are measured to the middle of the victim&apos;s head. A small crosshair error a second before the kill
        means the angle was already held; a large one followed by a flick means it was a reaction.
      </p>
    </div>
  );
}

function Summary(props: { t: AimTotals; career: AimTotals | null; life: LifeTotals | null; careerLife: LifeTotals | null }) {
  const { t, career, life, careerLife } = props;
  const cards: Array<[string, string, string, string | null]> = [
    ["Crosshair error", `${t.errorDeg.toFixed(1)}°`, "when the kill landed", career && `${career.errorDeg.toFixed(1)}° usually`],
    ["A second before", `${t.beforeDeg.toFixed(1)}°`, "how far it had to travel", career && `${career.beforeDeg.toFixed(1)}° usually`],
    ["Flick", `${t.flickDeg.toFixed(1)}°`, "turn in the last half second", career && `${career.flickDeg.toFixed(1)}° usually`],
    ["Range", t.rangeUnits.toFixed(0), "map units", career && `${career.rangeUnits.toFixed(0)} usually`],
    ["Angle already held", `${(t.heldShare * 100).toFixed(0)}%`, "within 3° a second before", career && `${(career.heldShare * 100).toFixed(0)}% usually`],
  ];
  if (life) {
    cards.push(
      ["Scoped", `${(life.scopedShare * 100).toFixed(0)}%`, "of your time alive", careerLife && `${(careerLife.scopedShare * 100).toFixed(0)}% usually`],
      [
        "Nearest teammate",
        life.nearestMate === null ? "—" : life.nearestMate.toFixed(0),
        "units away when you died",
        careerLife?.nearestMate ? `${careerLife.nearestMate.toFixed(0)} usually` : null,
      ],
      ["Died alone", `${(life.aloneShare * 100).toFixed(0)}%`, "nobody within 900 units", careerLife && `${(careerLife.aloneShare * 100).toFixed(0)}% usually`],
    );
  }
  return (
    <div className="aim-cards">
      {cards.map(([label, value, note, vs]) => (
        <div className="aim-card" key={label}>
          <span className="aim-label">{label}</span>
          <span className="aim-value">{value}</span>
          <span className="aim-note">{note}</span>
          {vs && <span className="aim-vs">{vs}</span>}
        </div>
      ))}
      <p className="hint aim-count">From {t.kills} kills the demo could answer for.</p>
    </div>
  );
}

/** Your deaths: who got you, from how far, and who was close enough to help. */
function Deaths({ rows, names }: { rows: DeathRow[]; names: ReturnType<typeof playerMap> }) {
  return (
    <div className="table-wrap">
      <table className="match-table aim-table">
        <thead>
          <tr>
            <th>Killed by</th>
            <th className="num" title="How far away they were; blank when the demo never carried them">
              Their range
            </th>
            <th className="num" title="Distance to the nearest living teammate the demo carried">
              Nearest teammate
            </th>
            <th className="num" title="Teammates within 900 units">
              Cover
            </th>
            <th>State</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.tick}>
              <td className="nowrap player-name">{(r.killer !== null && names.get(r.killer)?.name) || "—"}</td>
              <td className="num">{r.killerRange === null ? <span className="muted">–</span> : r.killerRange.toFixed(0)}</td>
              <td className="num">
                {r.nearestMate === null ? <span className="muted">–</span> : <span className={r.nearestMate > 900 ? "deg-far" : ""}>{r.nearestMate.toFixed(0)}</span>}
              </td>
              <td className="num">{r.matesNear}</td>
              <td className="nowrap">
                {r.scoped && <span className="badge badge-hs">scoped</span>}
                {r.matesNear === 0 && <span className="muted">alone</span>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Degrees, tinted: close is good, far is not. */
function Deg({ v, good, bad }: { v: number; good: number; bad: number }) {
  const cls = v <= good ? "deg-good" : v >= bad ? "deg-far" : "";
  return <span className={cls}>{v.toFixed(1)}°</span>;
}

export type { AimRow };
