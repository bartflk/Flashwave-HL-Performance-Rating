import type { ReactNode } from "react";
import { t } from "../lib/i18n";

/**
 * Something is happening (Q81, Flashy: "a lot of actions that take a time
 * loading, i want a bunch of places to have a loading spinner"). A ring in
 * the colour of the text around it, as tall as that text, so it fits a
 * hint, a button or a heading alike.
 */
export function Spinner({ label }: { label?: string }) {
  return (
    <span className="spinner" role="status" aria-label={label ?? t("Loading")}>
      <svg viewBox="0 0 24 24" aria-hidden>
        <circle className="spinner-track" cx="12" cy="12" r="9" />
        <path className="spinner-arc" d="M21 12a9 9 0 0 0-9-9" />
      </svg>
    </span>
  );
}

/** A waiting line: the spinner and what is being waited for. */
export function Loading({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <p className={className ? `hint loading ${className}` : "hint loading"}>
      <Spinner />
      <span>{children}</span>
    </p>
  );
}
