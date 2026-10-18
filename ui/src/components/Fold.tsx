import { useEffect, useRef, useState, type ReactNode } from "react";
import { ErrorBoundary } from "./ErrorBoundary";

const KEY = (id: string) => `hl.fold.${id}`;

function stored(id: string): boolean {
  try {
    return localStorage.getItem(KEY(id)) === "1";
  } catch {
    return false;
  }
}

/** Open a section from elsewhere (the match page's tabs, Q82). */
export function openFold(id: string) {
  window.dispatchEvent(new CustomEvent("hl:fold-open", { detail: id }));
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

  useEffect(() => {
    const on = (e: Event) => {
      if ((e as CustomEvent<string>).detail !== id) return;
      setClosed(false);
      try {
        localStorage.setItem(KEY(id), "0");
      } catch {
        // Blocked storage: open for this session.
      }
    };
    window.addEventListener("hl:fold-open", on);
    return () => window.removeEventListener("hl:fold-open", on);
  }, [id]);

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
      id={`sec-${id}`}
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
      <ErrorBoundary what={`This panel (${id})`}>{children}</ErrorBoundary>
    </div>
  );
}
