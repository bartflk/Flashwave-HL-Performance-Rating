import { useCallback, useRef, useState } from "react";

/**
 * Whether a header's parts fit on one line, for headers that go to two lines
 * as a whole rather than letting one piece wrap on its own (Flashy: "if the
 * title makes it so the nav buttons and the search cant be on the same line,
 * just put the nav and search 1 line under. A lot of stuff like this has to
 * happen in the app").
 *
 * The parts are the elements marked `data-fit` inside the header. Each counts
 * at its natural width, the same on one line or two, so the answer does not
 * flip back and forth as the layout changes: a flex row is the sum of its
 * children and its gaps, anything else its own width, and `data-fit="300"`
 * a fixed minimum (a search box that can shrink to that). `gap` is the space
 * between parts on one line.
 *
 * Measured at once and again on every resize of the header or a part (a
 * language switch, a font arriving), as useMeasuredWidth is.
 */
export function useOneLine(gap: number): [(el: HTMLElement | null) => void, boolean] {
  const [stacked, setStacked] = useState(false);
  const observer = useRef<ResizeObserver | null>(null);

  const ref = useCallback(
    (el: HTMLElement | null) => {
      observer.current?.disconnect();
      observer.current = null;
      if (!el) return;
      const parts = () => [...el.querySelectorAll<HTMLElement>("[data-fit]")];
      const natural = (p: HTMLElement): number => {
        const fixed = Number(p.dataset.fit);
        if (p.dataset.fit && Number.isFinite(fixed)) return fixed;
        const s = getComputedStyle(p);
        if (s.display.includes("flex")) {
          const kids = [...p.children] as HTMLElement[];
          const between = (parseFloat(s.columnGap) || 0) * Math.max(0, kids.length - 1);
          const pad = (parseFloat(s.paddingLeft) || 0) + (parseFloat(s.paddingRight) || 0);
          return kids.reduce((n, k) => n + k.getBoundingClientRect().width, 0) + between + pad;
        }
        return p.getBoundingClientRect().width;
      };
      const measure = () => {
        const s = getComputedStyle(el);
        const room = el.clientWidth - (parseFloat(s.paddingLeft) || 0) - (parseFloat(s.paddingRight) || 0);
        if (room <= 0) return; // not laid out: hidden, or detached
        const ps = parts();
        const need = ps.reduce((n, p) => n + natural(p), 0) + gap * Math.max(0, ps.length - 1);
        setStacked(need > room + 0.5);
      };
      measure();
      const ro = new ResizeObserver(measure);
      ro.observe(el);
      for (const p of parts()) for (const k of [p, ...p.children]) ro.observe(k);
      observer.current = ro;
    },
    [gap],
  );

  return [ref, stacked];
}
