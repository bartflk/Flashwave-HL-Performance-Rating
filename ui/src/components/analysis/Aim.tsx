import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type Analysis, type AimRow, type AimTotals } from "../../api/types";
import { playerMap } from "./common";

/**
 * Your aim, read from the match's own demo (PLAN §14).
 *
 * The log says who you killed; the demo says where you were looking a second
 * before, how far the view travelled, and how far away they were. Only kills
 * the demo carried both players through get an answer, so a Spy killed round
 * a corner is left out.
 */
export function Aim({ a, logId }: { a: Analysis; logId: number }) {
  const q = useQuery({ queryKey: ["aim", logId], queryFn: () => api.getAim(logId) });
  const names = playerMap(a);

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

  const kills = [...d.kills].sort((x, y) => x.tick - y.tick);

  return (
    <div className="aim">
      {d.totals && <Summary t={d.totals} career={d.career} />}
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
      <p className="hint aim-foot">
        Angles are measured to the middle of the victim&apos;s head. A small crosshair error a second before the kill
        means the angle was already held; a large one followed by a flick means it was a reaction.
      </p>
    </div>
  );
}

function Summary({ t, career }: { t: AimTotals; career: AimTotals | null }) {
  const cards: Array<[string, string, string, string | null]> = [
    ["Crosshair error", `${t.errorDeg.toFixed(1)}°`, "when the kill landed", career && `${career.errorDeg.toFixed(1)}° usually`],
    ["A second before", `${t.beforeDeg.toFixed(1)}°`, "how far it had to travel", career && `${career.beforeDeg.toFixed(1)}° usually`],
    ["Flick", `${t.flickDeg.toFixed(1)}°`, "turn in the last half second", career && `${career.flickDeg.toFixed(1)}° usually`],
    ["Range", t.rangeUnits.toFixed(0), "map units", career && `${career.rangeUnits.toFixed(0)} usually`],
    ["Angle already held", `${(t.heldShare * 100).toFixed(0)}%`, "within 3° a second before", career && `${(career.heldShare * 100).toFixed(0)}% usually`],
  ];
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

/** Degrees, tinted: close is good, far is not. */
function Deg({ v, good, bad }: { v: number; good: number; bad: number }) {
  const cls = v <= good ? "deg-good" : v >= bad ? "deg-far" : "";
  return <span className={cls}>{v.toFixed(1)}°</span>;
}

export type { AimRow };
