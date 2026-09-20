import type { AimTotals, LifeTotals } from "../../api/types";

/**
 * What your demos say about your aim, over whatever the profile is filtered
 * to (PLAN §14). Each figure sits against the same figure over every demo
 * read, so a season or a kind of game can be compared with your usual play.
 *
 * Only matches with a demo on this machine count, so the sample is smaller
 * than the rating's: the caption says how much smaller.
 */
export function AimPanel(props: {
  aim: AimTotals | null;
  life: LifeTotals | null;
  aimAll: AimTotals | null;
  lifeAll: LifeTotals | null;
  cls: string;
}) {
  const { aim, life, aimAll, lifeAll, cls } = props;
  if (!aim && !life) return null;

  const rows: Array<{ label: string; value: string; note: string; usual: string | null; better?: "low" | "high"; here?: number; all?: number }> = [];
  if (aim) {
    rows.push(
      { label: "Crosshair error", value: `${aim.errorDeg.toFixed(1)}°`, note: "when the kill landed", usual: aimAll ? `${aimAll.errorDeg.toFixed(1)}°` : null, better: "low", here: aim.errorDeg, all: aimAll?.errorDeg },
      { label: "A second before", value: `${aim.beforeDeg.toFixed(1)}°`, note: "how far the crosshair had to travel", usual: aimAll ? `${aimAll.beforeDeg.toFixed(1)}°` : null, better: "low", here: aim.beforeDeg, all: aimAll?.beforeDeg },
      { label: "Angle already held", value: `${(aim.heldShare * 100).toFixed(0)}%`, note: "kills where it was within 3° a second before", usual: aimAll ? `${(aimAll.heldShare * 100).toFixed(0)}%` : null, better: "high", here: aim.heldShare, all: aimAll?.heldShare },
      { label: "Flick", value: `${aim.flickDeg.toFixed(1)}°`, note: "turn in the half second before the shot", usual: aimAll ? `${aimAll.flickDeg.toFixed(1)}°` : null },
      { label: "Range", value: aim.rangeUnits.toFixed(0), note: "map units to the player you killed", usual: aimAll ? aimAll.rangeUnits.toFixed(0) : null },
    );
  }
  if (life) {
    rows.push(
      { label: "Scoped", value: `${(life.scopedShare * 100).toFixed(0)}%`, note: "of your time alive", usual: lifeAll ? `${(lifeAll.scopedShare * 100).toFixed(0)}%` : null },
      {
        label: "Nearest teammate",
        value: life.nearestMate === null ? "—" : life.nearestMate.toFixed(0),
        note: "units away when you died",
        usual: lifeAll?.nearestMate ? lifeAll.nearestMate.toFixed(0) : null,
        better: "low",
        here: life.nearestMate ?? undefined,
        all: lifeAll?.nearestMate ?? undefined,
      },
      { label: "Died alone", value: `${(life.aloneShare * 100).toFixed(0)}%`, note: "with nobody within 900 units", usual: lifeAll ? `${(lifeAll.aloneShare * 100).toFixed(0)}%` : null, better: "low", here: life.aloneShare, all: lifeAll?.aloneShare },
      { label: "Scoped when you died", value: `${(life.scopedShareDeaths * 100).toFixed(0)}%`, note: "in the second before it", usual: lifeAll ? `${(lifeAll.scopedShareDeaths * 100).toFixed(0)}%` : null },
    );
  }

  return (
    <section className="panel aim-panel">
      <header>
        <h2>Aim, from your demos</h2>
        <p className="hint">
          Read from the demos on this machine, for the {cls} games this filter covers
          {aim ? `: ${aim.kills} kills` : ""}
          {life && life.deaths > 0 ? ` and ${life.deaths} deaths` : ""}
          {life && life.minutes > 0 ? `, ${life.minutes.toFixed(0)} minutes alive` : ""}. Matches without a demo are
          not in here.
        </p>
      </header>
      <div className="table-wrap">
        <table className="match-table">
          <thead>
            <tr>
              <th>Measure</th>
              <th className="num">Here</th>
              <th className="num">Usually</th>
              <th>What it means</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.label}>
                <td className="nowrap">{r.label}</td>
                <td className={`num ${verdict(r)}`}>{r.value}</td>
                <td className="num muted">{r.usual ?? "–"}</td>
                <td className="muted">{r.note}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

/** Green when this filter beats your usual figure, red when it falls short. */
function verdict(r: { better?: "low" | "high"; here?: number; all?: number }): string {
  if (!r.better || r.here === undefined || r.all === undefined) return "";
  const diff = r.here - r.all;
  // Within a twentieth of the usual figure is the same figure.
  if (Math.abs(diff) < Math.abs(r.all) * 0.05) return "";
  const good = r.better === "low" ? diff < 0 : diff > 0;
  return good ? "deg-good" : "deg-far";
}
