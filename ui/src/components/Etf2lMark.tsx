/**
 * ETF2L's logo -- the emblem from its rules site, rules.etf2l.org --
 * beside anything that is ETF2L's: the league tab, officials, links to its
 * pages (Flashy: "use the etf2l logo in places where appropriate to signal
 * what league it is").
 *
 * The file is drawn in dark slate for light pages, so it is used as a mask
 * filled with the text colour around it: it reads on every theme and takes
 * the colour of the badge or link it sits in. `size` is its height; the
 * emblem is wider than tall.
 */
export function Etf2lMark({ size = 14, title }: { size?: number; title?: string }) {
  return <span className="etf2l-mark" role="img" aria-label={title ?? "ETF2L"} title={title} style={{ height: size, width: Math.round(size * 1.26) }} />;
}
