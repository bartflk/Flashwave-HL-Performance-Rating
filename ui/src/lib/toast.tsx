import { useEffect, useState } from "react";
import { api } from "../api/client";

/**
 * One app-wide confirmation line. Copying a demo command is invisible
 * otherwise, and the command is exactly what you need to read back when
 * pasting it into the TF2 console.
 */

type Toast = { id: number; text: string; detail?: string; kind: "ok" | "error" };

let listeners: Array<(t: Toast) => void> = [];
let nextId = 1;

export function toast(text: string, detail?: string, kind: Toast["kind"] = "ok") {
  const t = { id: nextId++, text, detail, kind };
  listeners.forEach((l) => l(t));
}

/** Copy to the clipboard and say so. */
export async function copy(text: string, what: string) {
  try {
    await api.copyText(text);
    toast(`Copied ${what}`, text);
  } catch (e) {
    toast("Could not copy to the clipboard", String(e), "error");
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
    const timer = setTimeout(() => setCurrent(null), 4000);
    return () => clearTimeout(timer);
  }, [current]);

  if (!current) return null;
  return (
    <div className={`toast toast-${current.kind}`} role="status" aria-live="polite">
      <span className="toast-text">{current.text}</span>
      {current.detail && <code className="toast-detail">{current.detail}</code>}
    </div>
  );
}
