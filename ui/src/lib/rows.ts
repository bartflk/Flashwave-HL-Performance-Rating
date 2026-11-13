import type { KeyboardEvent } from "react";

/**
 * The key handler of a row that opens something when it is clicked. Enter or
 * Space opens it only when the row itself has the focus: on a button inside
 * the row (a team's name) the key belongs to that button, which does its own
 * thing, instead of the row opening its page first and the button's page on
 * top of it.
 */
export function openOnKey(open: () => void) {
  return (e: KeyboardEvent<HTMLElement>) => {
    if (e.target !== e.currentTarget) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      open();
    }
  };
}
