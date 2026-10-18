/**
 * The app's logo -- a flash over two waves in a ring -- drawn in the theme's
 * own colours (Flashy: "logo doesn't change color when changing theme
 * color"): the disc in the panel colour, the ring and the lower wave in the
 * accent (the lower wave in the theme's second colour when it has one), the bolt and the upper wave in the text colour. `public/logo.svg`
 * keeps the Gravel colours for the window icon and the favicon, which a
 * theme cannot reach.
 */
export function Logo({ className, size }: { className?: string; size?: number }) {
  return (
    <svg className={className} viewBox="0 0 512 512" width={size} height={size} aria-hidden>
      <circle cx="256" cy="256" r="236" fill="var(--panel)" />
      <circle cx="256" cy="256" r="236" fill="none" stroke="var(--accent)" strokeWidth="34" />
      <path d="M96 330 C150 290 200 370 256 330 C312 290 362 370 416 330" fill="none" stroke="var(--text)" strokeWidth="30" strokeLinecap="round" />
      <path d="M120 390 C170 356 214 420 262 390 C310 360 352 418 396 392" fill="none" stroke="var(--second, var(--accent))" strokeWidth="22" strokeLinecap="round" />
      <polygon points="282,86 176,250 240,250 216,346 336,190 270,190 300,86" fill="var(--text)" />
    </svg>
  );
}
