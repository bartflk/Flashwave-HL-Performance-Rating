import { useState } from "react";
import type { Fixture, ResultRow } from "../../api/types";
import { locale, t, tx } from "../../lib/i18n";
import { DivisionBadge } from "../players/PlayerProfile";
import { TeamAvatar } from "../teams/TeamsPage";
import { clockTime, fullDate } from "./Countdown";

/**
 * A week of officials as a calendar (Flashy: "is it a good idea to maybe
 * have a calendar style overview for the current week"): Monday to Sunday,
 * a column a day, the week's results and what is still to come together,
 * so the shape of the week reads at a glance. The arrows step a week.
 */

type Item = {
  matchId: number;
  time: number;
  division: string | null;
  tier: number | null;
  clan1: { id: number; name: string; avatar: string | null };
  clan2: { id: number; name: string; avatar: string | null };
  r1: number | null;
  r2: number | null;
  fixture?: Fixture;
};

/** Midnight, local time, of the Monday of the week `offset` weeks from this one. */
function monday(offset: number): Date {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7) + offset * 7);
  return d;
}

export function WeekCalendar(props: {
  fixtures: Fixture[];
  results: ResultRow[];
  mine: Set<number>;
  onMatch: (id: number, f?: Fixture) => void;
}) {
  const { fixtures, results, mine, onMatch } = props;
  const [offset, setOffset] = useState(0);
  const start = monday(offset);
  const days = Array.from({ length: 7 }, (_, i) => {
    const d = new Date(start);
    d.setDate(start.getDate() + i);
    return d;
  });
  const end = new Date(start);
  end.setDate(start.getDate() + 7);
  const from = start.getTime() / 1000;
  const to = end.getTime() / 1000;

  // A match is listed once: its result once played, else its fixture.
  const items = new Map<number, Item>();
  for (const f of fixtures) if (f.time >= from && f.time < to) items.set(f.matchId, { ...f, r1: null, r2: null, fixture: f });
  for (const r of results) if (r.time >= from && r.time < to) items.set(r.matchId, { ...r });
  const all = [...items.values()].sort((a, b) => a.time - b.time);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const range = `${days[0].toLocaleDateString(locale(), { day: "numeric", month: "short" })} – ${days[6].toLocaleDateString(locale(), { day: "numeric", month: "short", year: "numeric" })}`;

  return (
    <section className="panel e2-panel e2-cal">
      <div className="e2-panel-head">
        <h2>{offset === 0 ? t("This week") : offset === 1 ? t("Next week") : offset === -1 ? t("Last week") : range}</h2>
        <span className="e2-cal-nav">
          <span className="muted">{range}</span>
          <button className="km-chip" onClick={() => setOffset((o) => o - 1)} aria-label={t("Previous week")}>
            ‹
          </button>
          {offset !== 0 && (
            <button className="km-chip" onClick={() => setOffset(0)}>
              {t("Today")}
            </button>
          )}
          <button className="km-chip" onClick={() => setOffset((o) => o + 1)} aria-label={t("Next week")}>
            ›
          </button>
        </span>
      </div>
      <div className="e2-cal-grid">
        {days.map((d) => {
          const a = d.getTime() / 1000;
          const rows = all.filter((x) => x.time >= a && x.time < a + 86_400);
          const isToday = d.getTime() === today.getTime();
          const past = d.getTime() < today.getTime();
          return (
            <div key={a} className={`e2-cal-day${isToday ? " today" : ""}${past ? " past" : ""}`}>
              <header>
                <span className="e2-cal-wd">{d.toLocaleDateString(locale(), { weekday: "short" })}</span>
                <span className="e2-cal-date">{d.getDate()}</span>
                {rows.length > 0 && <span className="e2-cal-n">{rows.length}</span>}
              </header>
              {rows.length === 0 ? (
                <p className="e2-cal-none">{t("No officials")}</p>
              ) : (
                <ol>
                  {rows.map((x) => {
                    const ours = mine.has(x.clan1.id) || mine.has(x.clan2.id);
                    const played = x.r1 !== null && x.r2 !== null;
                    const side = (c: Item["clan1"], score: number | null, other: number | null) => (
                      <span className={played && score !== null && other !== null && score > other ? "e2-cal-team won" : "e2-cal-team"}>
                        <TeamAvatar src={c.avatar} />
                        <span className="e2-cal-name">{c.name}</span>
                        {played && <span className="e2-cal-score">{score}</span>}
                      </span>
                    );
                    return (
                      <li key={x.matchId}>
                        <button className={`e2-cal-match${ours ? " ours" : ""}${played ? " played" : ""}`} onClick={() => onMatch(x.matchId, x.fixture)} title={fullDate(x.time)}>
                          <span className="e2-cal-top">
                            <span className="e2-cal-time">{clockTime(x.time)}</span>
                            {x.division && x.tier !== null && <DivisionBadge d={{ name: x.division, tier: x.tier }} />}
                          </span>
                          {side(x.clan1, x.r1, x.r2)}
                          {side(x.clan2, x.r2, x.r1)}
                        </button>
                      </li>
                    );
                  })}
                </ol>
              )}
            </div>
          );
        })}
      </div>
      {all.length === 0 && <p className="hint">{tx("No officials in the week of {0} in what is read so far.", { "0": range })}</p>}
    </section>
  );
}
