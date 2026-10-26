import { rating } from "../lib/format";
import { usePalette } from "../lib/tiers";

/**
 * A rating coloured the way HLTV colours its ratings (Flashy: "can you add
 * the coloring just like HLTV has ... for the rating"): green above 1.05,
 * red below 0.95, and a grey band between for a game around the average
 * (Flashy: "add the grey band around 1.00"), so only a clear standout
 * catches the eye. Off with Settings > Stat colours.
 */
export function RatingValue({ v }: { v: number }) {
  const palette = usePalette();
  const r = Math.round(v * 100);
  const tone = palette === "off" ? "" : r > 105 ? " rt-pos" : r < 95 ? " rt-neg" : " rt-even";
  return <span className={`rt${tone}`}>{rating(v)}</span>;
}
