/**
 * The theme's map, faded behind a big header (Flashy: "can you put images
 * like this into the app per theme, would be a cool flare"). Shows only while
 * the theme has a map and Settings › Theme puts it in headers; the header
 * needs the class `has-backdrop`, and this goes first inside it.
 */
export function Backdrop() {
  return <div className="backdrop" aria-hidden />;
}
