import { useEffect, useState } from "react";
import { api } from "../api/client";
import { friendlyText } from "./apierror";
import { t as tr } from "./i18n";

/**
 * One app-wide confirmation line. Copying a demo command is invisible
 * otherwise, and the command is exactly what you need to read back when
 * pasting it into the TF2 console.
 */

type Toast = { id: number; text: string; detail?: string; kind: "ok" | "error" };

/** How long a toast stays: an error longer, so its report can be copied. */
const SHOW_MS = { ok: 4000, error: 12000 };

let listeners: Array<(t: Toast) => void> = [];
let nextId = 1;

export function toast(text: string, detail?: string, kind: Toast["kind"] = "ok") {
  const t = { id: nextId++, text, detail, kind };
  listeners.forEach((l) => l(t));
}

/** A failed click, said out loud: an action that throws must not look like nothing happened. */
export function toastError(e: unknown) {
  toast(friendlyText(e), undefined, "error");
}

/** Copy to the clipboard and say so. `what` is already in the user's language. */
export async function copy(text: string, what: string) {
  try {
    await api.copyText(text);
    // A path or a command is read back; a log or a report is only counted.
    const lines = text.split("\n").length;
    const short = lines === 1 && text.length <= 160;
    toast(tr("Copied {0}", { "0": what }), short ? text : tr("{0} lines", { "0": lines.toLocaleString() }));
  } catch (e) {
    toast(tr("Could not copy to the clipboard"), String(e), "error");
  }
}

export function ToastHost() {
  const [current, setCurrent] = useState<Toast | null>(null);

  useEffect(() => {
    const onToast = (t: Toast) => setCurrent(t);
    listeners.push(onToast);
    return () => {
      listeners = listeners.filter((l) => l !== onToast);
    };
  }, []);

  useEffect(() => {
    if (!current) return;
    const timer = setTimeout(() => setCurrent(null), SHOW_MS[current.kind]);
    return () => clearTimeout(timer);
  }, [current]);

  if (!current) return null;
  return (
    <div className={`toast toast-${current.kind}`} role="status" aria-live="polite">
      <span className="toast-text">{current.text}</span>
      {current.detail && <code className="toast-detail">{current.detail}</code>}
      {current.kind === "error" && (
        <button
          className="toast-report"
          title={tr("The error, the app's details and the log's last lines, ready to paste")}
          onClick={() => {
            const failure = { what: current.text, message: current.detail };
            setCurrent(null);
            // Loaded on use: the report copies through this module.
            void import("./report").then((m) => m.copyReport(failure));
          }}
        >
          {tr("Copy report")}
        </button>
      )}
    </div>
  );
}
