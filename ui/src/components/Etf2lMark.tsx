/**
 * ETF2L's own mark -- its favicon, from etf2l.org -- beside anything that is
 * ETF2L's: the league tab, officials, links to its pages (Flashy: "use the
 * etf2l logo in places where appropriate to signal what league it is").
 *
 * The icon is 16 px and dark slate, so it sits on a small light tile to read
 * on every theme; the 2x copy is the same pixels doubled, for 2K/4K screens.
 */
export function Etf2lMark({ size = 14, title }: { size?: number; title?: string }) {
  return (
    <img
      className="etf2l-mark"
      src="/icons/etf2l.png"
      srcSet="/icons/etf2l@2x.png 2x"
      width={size}
      height={size}
      alt={title ?? "ETF2L"}
      title={title}
      draggable={false}
    />
  );
}
