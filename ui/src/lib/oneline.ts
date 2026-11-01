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
export function useOneLine(gap: number): [(el: HTMLElement | null) => void, Lines] {
  const [lines, setLines] = useState<Lines>("one");
  const observer = useRef<ResizeObserver | null>(null);

  const ref = useCallback(
    (el: HTMLElement | null) => {
      observer.current?.disconnect();
      observer.current = null;
      if (!el) return;
      const parts = () => [...el.querySelectorAll<HTMLElement>("[data-fit]")];
      // The widest room one line has been seen to overflow in: up to it the
      // header stays on two lines, so the check below cannot flip it back
      // and forth.
      let overflowAt = 0;
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
        // And the layout's own word: on one line, anything running past the
        // header's edge means it does not fit, whatever the sums said (a
        // part's width can be off by a font or a rounding).
        if (!el.classList.contains("stacked") && el.scrollWidth > el.clientWidth + 1) overflowAt = Math.max(overflowAt, room);
        if (need <= room + 0.5 && room > overflowAt) return setLines("one");
        // On two lines the first part has the first to itself; whether the
        // rest share the second, or the last moves up beside the first.
        const rest = ps.slice(1);
        const second = rest.reduce((n, p) => n + natural(p), 0) + gap * Math.max(0, rest.length - 1);
        setLines(second <= room + 0.5 ? "two" : "split");
      };
      measure();
      const ro = new ResizeObserver(measure);
      ro.observe(el);
      for (const p of parts()) for (const k of [p, ...p.children]) ro.observe(k);
      observer.current = ro;
    },
    [gap],
  );

  return [ref, lines];
}

/**
 * How a header lays out: "one" line; "two", the first part alone above the
 * rest; or "split", where even the rest do not fit a line: the last part
 * (a search) moves up beside the first, and the middle (a menu) has the
 * second line to itself, rather than the search hanging alone on a third.
 */
export type Lines = "one" | "two" | "split";
