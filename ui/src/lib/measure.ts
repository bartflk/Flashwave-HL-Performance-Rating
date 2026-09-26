import { useCallback, useRef, useState } from "react";

/**
 * The width of an element, kept up to date as it changes.
 *
 * The obvious version of this — a `useRef` plus a `useEffect(..., [])` that
 * attaches a ResizeObserver — has a bug that took a bug report to find. If
 * the observed element is conditionally rendered (the trend chart's "Show as
 * table" toggle unmounts it), then:
 *
 *   1. the element unmounts, and a detached element reports 0 x 0, so the
 *      observer fires once with a width of zero;
 *   2. the element comes back as a *new* node, and the effect never re-runs,
 *      so the observer is still watching the old detached one.
 *
 * The chart is then stuck at whatever width step 1 left behind, until the
 * page is reloaded. A ref callback follows the node across unmount and
 * remount by construction, which an effect with no dependencies cannot.
 *
 * Zero is also ignored outright. A width of zero means "not laid out" — the
 * element is detached, or inside something with `display: none` — and it is
 * never a size to draw at. Clamping it up to the minimum instead is what
 * turns a moment of being hidden into a permanently squashed chart.
 */
export function useMeasuredWidth(min: number, initial = min): [number, (el: HTMLElement | null) => void] {
  const [width, setWidth] = useState(initial);
  const observer = useRef<ResizeObserver | null>(null);

  const ref = useCallback(
    (el: HTMLElement | null) => {
      observer.current?.disconnect();
      observer.current = null;
      if (!el) return;
      // Measure at once: an observer only reports after the next frame, and
      // one frame at the wrong width is a visible jump. The content box, so
      // that this first measurement and the observer's agree — `clientWidth`
      // includes padding and `contentRect` does not.
      const style = getComputedStyle(el);
      const pad = (parseFloat(style.paddingLeft) || 0) + (parseFloat(style.paddingRight) || 0);
      const now = el.clientWidth - pad;
      if (now > 0) setWidth(Math.max(min, now));
      const ro = new ResizeObserver((entries) => {
        const w = entries[0]?.contentRect.width ?? 0;
        if (w > 0) setWidth(Math.max(min, w));
      });
      ro.observe(el);
      observer.current = ro;
    },
    [min],
  );

  return [width, ref];
}
