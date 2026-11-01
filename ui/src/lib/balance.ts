/**
 * A wrapping row that never leaves one piece alone on its last line (Flashy:
 * "a lot of stuff like this has to happen in the app"; the Columns button,
 * or the Class group, dropping under a full line of everything else). Used
 * as a ref, `ref={balanceWrap}`: React 19 runs the cleanup it returns.
 *
 * Where the row would end with a single child under a line of two or more,
 * the last child of the line above goes down with it: the child before it
 * gets a right margin filling the rest of its line, so the next one no
 * longer fits. Widths are read from the children, which the margin does not
 * change, so the answer is stable; it is worked out again on every resize of
 * the row or a child.
 */
export function balanceWrap(el: HTMLElement | null): (() => void) | undefined {
  if (!el) return undefined;
  let held: HTMLElement | null = null;
  const measure = () => {
    if (held) held.style.marginRight = "";
    held = null;
    const s = getComputedStyle(el);
    const room = el.clientWidth - (parseFloat(s.paddingLeft) || 0) - (parseFloat(s.paddingRight) || 0);
    const gap = parseFloat(s.columnGap) || 0;
    const kids = ([...el.children] as HTMLElement[]).filter((k) => {
      const p = getComputedStyle(k).position;
      return k.getBoundingClientRect().width > 0 && p !== "absolute" && p !== "fixed";
    });
    if (room <= 0 || kids.length < 3) return;
    // The lines the row breaks into, greedily, as flex-wrap does.
    const lines: Array<{ items: HTMLElement[]; used: number }> = [];
    for (const k of kids) {
      const w = k.getBoundingClientRect().width;
      const line = lines[lines.length - 1];
      if (line && line.used + gap + w <= room + 0.5) {
        line.items.push(k);
        line.used += gap + w;
      } else lines.push({ items: [k], used: w });
    }
    if (lines.length < 2) return;
    const last = lines[lines.length - 1];
    const above = lines[lines.length - 2];
    if (last.items.length !== 1 || above.items.length < 2) return;
    // Bring the line above's last child down, if the two fit together.
    const moving = above.items[above.items.length - 1];
    const movingW = moving.getBoundingClientRect().width;
    if (movingW + gap + last.used > room + 0.5) return;
    // The margin fills what the line leaves after `keep`, as a share of the
    // row's width, so it stays right if the row resizes before the next
    // measurement (a scrollbar arriving): fixed pixels pushed `keep` itself
    // down.
    const keep = above.items[above.items.length - 2];
    const upToKeep = above.used - gap - movingW;
    held = keep;
    keep.style.marginRight = `max(0px, calc(100% - ${Math.ceil(upToKeep) + 1}px))`;
  };
  measure();
  const ro = new ResizeObserver(measure);
  ro.observe(el);
  for (const k of el.children) ro.observe(k);
  return () => {
    ro.disconnect();
    if (held) held.style.marginRight = "";
  };
}
