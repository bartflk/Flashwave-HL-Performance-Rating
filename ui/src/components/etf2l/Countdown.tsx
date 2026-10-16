import { useEffect, useState } from "react";
import { locale, t } from "../../lib/i18n";

/**
 * How long until an official starts (Q74, Flashy): "in 2 d 4 h", "in 3 h
 * 12 m", "in 14:32" ticking through the last hour, LIVE for three hours
 * after the start (the fixtures' own rule), then played. The local time
 * sits beside it in the lists; the full date is on hover.
 */

/** As long as a match counts as on after its start, like `fixtures.rs`. */
const LIVE_FOR = 3 * 3600;

export type Until = { kind: "soon" | "later" | "live" | "past"; label: string };

export function until(time: number, now: number): Until {
  const d = Math.round(time - now);
  if (d <= -LIVE_FOR) return { kind: "past", label: t("played") };
  if (d <= 0) return { kind: "live", label: t("LIVE") };
  const days = Math.floor(d / 86400);
  const hours = Math.floor((d % 86400) / 3600);
  const mins = Math.floor((d % 3600) / 60);
  if (d < 3600) return { kind: "soon", label: t("in {0}", { "0": `${mins}:${String(d % 60).padStart(2, "0")}` }) };
  if (d < 86400) return { kind: "later", label: t("in {0} h {1} m", { "0": hours, "1": mins }) };
  return { kind: "later", label: t("in {0} d {1} h", { "0": days, "1": hours }) };
}

/** The clock, ticking each second in the last hour and each half minute before. */
function useNow(fast: boolean): number {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now() / 1000), fast ? 1000 : 30_000);
    return () => window.clearInterval(id);
  }, [fast]);
  return now;
}

export function fullDate(time: number): string {
  return new Date(time * 1000).toLocaleString(locale(), { weekday: "long", day: "numeric", month: "long", hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
}

export function Countdown({ time, big }: { time: number; big?: boolean }) {
  const first = Date.now() / 1000;
  const now = useNow(time - first < 3700 && time - first > -60);
  const u = until(time, now);
  return (
    <span className={`cd cd-${u.kind}${big ? " cd-big" : ""}`} title={fullDate(time)}>
      {u.kind === "live" && <span className="cd-dot" aria-hidden />}
      {u.label}
    </span>
  );
}

/** "20:00", the local time of day. */
export function clockTime(time: number): string {
  return new Date(time * 1000).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
}
