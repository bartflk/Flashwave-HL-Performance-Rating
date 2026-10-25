import { useState } from "react";
import { t } from "../lib/i18n";

export type RankBy = "value" | "pct";

/**
 * DPM and accuracy sorted by the number, or by where it stands among the
 * class's games on the map (Flashy: "is it possible to sort accuracy and
 * damage by percentile?"; "add the class % switch to the matches list
 * too"): the same percentile their colour shows. Remembered per table.
 */
export function useRankBy(id: string): [RankBy, (r: RankBy) => void] {
  const key = `hl.${id}.rankBy`;
  const [rank, setRank] = useState<RankBy>(() => {
    try {
      return localStorage.getItem(key) === "pct" ? "pct" : "value";
    } catch {
      return "value";
    }
  });
  const pick = (r: RankBy) => {
    setRank(r);
    try {
      localStorage.setItem(key, r);
    } catch {
      // Blocked storage: the choice holds for this session.
    }
  };
  return [rank, pick];
}

/** The Rank by switch: two halves in one control, the picked one raised. */
export function RankBySwitch({ value, onPick, stat }: { value: RankBy; onPick: (r: RankBy) => void; stat: string }) {
  return (
    <div className="rank-by" role="group" aria-label={t("Sort {0} and accuracy by", { "0": stat })} title={t("Sort {0} and accuracy by the number, or by where it stands among the class's games on the map", { "0": stat })}>
      <span className="an-label">{t("Rank by")}</span>
      <div className="rank-by-seg">
        <button className={value === "value" ? "rank-by-opt on" : "rank-by-opt"} aria-pressed={value === "value"} onClick={() => onPick("value")}>
          {t("Value")}
        </button>
        <button className={value === "pct" ? "rank-by-opt on" : "rank-by-opt"} aria-pressed={value === "pct"} onClick={() => onPick("pct")}>
          {t("Class %")}
        </button>
      </div>
    </div>
  );
}
