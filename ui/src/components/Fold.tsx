import { useEffect, useRef, useState, type ReactNode } from "react";

const KEY = (id: string) => `hl.fold.${id}`;

function stored(id: string): boolean {
  try {
    return localStorage.getItem(KEY(id)) === "1";
  } catch {
    return false;
  }
}

/**
 * Makes a panel collapsible. Wraps a component that renders one
 * `<section className="panel">`: clicking the panel's heading (its first
 * `h2`) folds everything below the panel's first child, its header. The
 * choice is remembered per section across matches and restarts.
 */
export function Fold({ id, children }: { id: string; children: ReactNode }) {
  const [closed, setClosed] = useState(() => stored(id));
  const box = useRef<HTMLDivElement>(null);

  const toggle = () =>
    setClosed((c) => {
      try {
        localStorage.setItem(KEY(id), c ? "0" : "1");
      } catch {
        // Storage can be blocked; folding still works for this session.
      }
      return !c;
    });

  // The heading acts as the toggle button, for the keyboard and screen readers too.
  useEffect(() => {
    const h = box.current?.querySelector("h2");
    if (!h) return;
    h.tabIndex = 0;
    h.setAttribute("role", "button");
    h.setAttribute("aria-expanded", String(!closed));
  });

  return (
    <div
      ref={box}
      className={closed ? "fold closed" : "fold"}
      onClick={(e) => {
        // Only the heading toggles: buttons and links in a header keep working.
        if ((e.target as HTMLElement).closest("h2")) toggle();
      }}
      onKeyDown={(e) => {
        if ((e.key === "Enter" || e.key === " ") && (e.target as HTMLElement).matches("h2")) {
          e.preventDefault();
          toggle();
        }
      }}
    >
      {children}
    </div>
  );
}
