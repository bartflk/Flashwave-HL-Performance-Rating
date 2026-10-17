import type { AimTotals, LifeTotals } from "../../api/types";
import { t, tx, k } from "../../lib/i18n";
import { classLabel } from "../analysis/common";
import { SortTh, useSort } from "../../lib/sort";
import { tip } from "../../lib/glossary";

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

  const rows: Array<{ label: string; value: string; note: string; usual: string | null; better?: "low" | "high"; here?: number; all?: number }> = [];
  if (aim) {
    rows.push(
      { label: k("Crosshair error"), value: `${aim.errorDeg.toFixed(1)}°`, note: k("when the kill landed"), usual: aimAll ? `${aimAll.errorDeg.toFixed(1)}°` : null, better: "low", here: aim.errorDeg, all: aimAll?.errorDeg },
      { label: k("A second before"), value: `${aim.beforeDeg.toFixed(1)}°`, note: k("how far the crosshair had to travel"), usual: aimAll ? `${aimAll.beforeDeg.toFixed(1)}°` : null, better: "low", here: aim.beforeDeg, all: aimAll?.beforeDeg },
      { label: k("Angle already held"), value: `${(aim.heldShare * 100).toFixed(0)}%`, note: k("kills where it was within 3° a second before"), usual: aimAll ? `${(aimAll.heldShare * 100).toFixed(0)}%` : null, better: "high", here: aim.heldShare, all: aimAll?.heldShare },
      { label: k("Flick"), value: `${aim.flickDeg.toFixed(1)}°`, note: k("turn in the half second before the shot"), usual: aimAll ? `${aimAll.flickDeg.toFixed(1)}°` : null },
      { label: k("Range"), value: aim.rangeUnits.toFixed(0), note: k("map units to the player you killed"), usual: aimAll ? aimAll.rangeUnits.toFixed(0) : null },
    );
  }
  if (life) {
    rows.push(
      { label: k("Scoped"), value: `${(life.scopedShare * 100).toFixed(0)}%`, note: k("of your time alive"), usual: lifeAll ? `${(lifeAll.scopedShare * 100).toFixed(0)}%` : null },
      {
        label: k("Nearest teammate"),
        value: life.nearestMate === null ? "—" : life.nearestMate.toFixed(0),
        note: k("units away when you died"),
        usual: lifeAll?.nearestMate ? lifeAll.nearestMate.toFixed(0) : null,
        better: "low",
        here: life.nearestMate ?? undefined,
        all: lifeAll?.nearestMate ?? undefined,
      },
      { label: k("Died alone"), value: `${(life.aloneShare * 100).toFixed(0)}%`, note: k("with nobody within 900 units"), usual: lifeAll ? `${(lifeAll.aloneShare * 100).toFixed(0)}%` : null, better: "low", here: life.aloneShare, all: lifeAll?.aloneShare },
      { label: k("Scoped when you died"), value: `${(life.scopedShareDeaths * 100).toFixed(0)}%`, note: k("in the second before it"), usual: lifeAll ? `${(lifeAll.scopedShareDeaths * 100).toFixed(0)}%` : null },
      {
        label: k("Never saw them"),
        value: life.behindShare === null ? "—" : `${(life.behindShare * 100).toFixed(0)}%`,
        note: k("killed from more than 90° off your crosshair"),
        usual: lifeAll?.behindShare != null ? `${(lifeAll.behindShare * 100).toFixed(0)}%` : null,
        better: "low",
        here: life.behindShare ?? undefined,
        all: lifeAll?.behindShare ?? undefined,
      },
    );
  }

  const sort = useSort("profile-aim", rows, {
    label: (r) => t(r.label),
    here: (r) => r.here ?? null,
    usual: (r) => r.all ?? null,
  });
  if (!aim && !life) return null;
  return (
    <section className="panel aim-panel">
      <header>
        <h2>{t("Aim, from your demos")}</h2>
        <p className="hint">{tx("Read from the demos on this machine, for the {cls} games this filter covers{1}{2}{3}. Matches without a demo are not in here.", { "1": aim ? t(": {kills} kills", { kills: aim.kills }) : "", "2": life && life.deaths > 0 ? t(" and {deaths} deaths", { deaths: life.deaths }) : "", "3": life && life.minutes > 0 ? t(", {0} minutes alive", { "0": life.minutes.toFixed(0) }) : "", cls: classLabel(cls) })}</p>
      </header>
      <div className="table-wrap">
        <table className="match-table">
          <thead>
            <tr>
              <SortTh k="label" s={sort}>{t("Measure")}</SortTh>
              <SortTh k="here" s={sort} className="num" tip={tip(t("Here"), t("The measure over the games this filter covers."), t("Read from the STV demos on this machine; matches without a demo are not in it."), "STV demo")}>{t("Here")}</SortTh>
              <SortTh k="usual" s={sort} className="num" tip={tip(t("Usually"), t("The same measure over every demo read on this class."), t("Green when this filter beats it by more than a twentieth, red when it falls short."), "STV demo")}>{t("Usually")}</SortTh>
              <th>{t("What it means")}</th>
            </tr>
          </thead>
          <tbody>
            {sort.sorted.map((r) => (
              <tr key={r.label}>
                <td className="nowrap">{t(r.label)}</td>
                <td className={`num ${verdict(r)}`}>{r.value}</td>
                <td className="num muted">{r.usual ?? "–"}</td>
                <td className="muted">{t(r.note)}</td>
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
