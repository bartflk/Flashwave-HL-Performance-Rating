import { rating } from "../lib/format";
import { usePalette } from "../lib/tiers";

/**
 * A rating coloured the way HLTV colours its ratings (Flashy: "can you add
 * the coloring just like HLTV has ... for the rating"): green above 1.00,
 * red below, plain at 1.00, the average game. Off with Settings > Stat
 * colours.
 */
export function RatingValue({ v }: { v: number }) {
  const palette = usePalette();
  const r = Math.round(v * 100);
  const tone = palette === "off" ? "" : r > 100 ? " rt-pos" : r < 100 ? " rt-neg" : " rt-even";
  return <span className={`rt${tone}`}>{rating(v)}</span>;
}
