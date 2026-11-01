/**
 * RGL's logo -- the white mark from the footer of its docs, docs.rgl.gg --
 * beside anything that is RGL's: the league tab, its page, links to rgl.gg
 * (Flashy: "have it just like the etf2l logo ... used everywhere for rgl as
 * well").
 *
 * Used as a mask filled with the text colour around it, as Etf2lMark is: it
 * reads on every theme and takes the colour of the badge or link it sits in.
 * `size` is its height; the mark is square.
 */
export function RglMark({ size = 14, title }: { size?: number; title?: string }) {
  return <span className="rgl-mark" role="img" aria-label={title ?? "RGL"} title={title} style={{ height: size, width: size }} />;
}
