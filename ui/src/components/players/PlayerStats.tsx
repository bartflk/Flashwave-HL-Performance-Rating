import { useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { Rank } from "../../api/types";
import { rating } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";
import { t, tx } from "../../lib/i18n";
import { seasonShort } from "./PlayerProfile";
import { DivisionBadge } from "./PlayerProfile";
import "../rating/rating.css";
import { SortTh, useSort } from "../../lib/sort";
import { Help, StatTip } from "../StatTip";
import { tip } from "../../lib/glossary";
import { Loading, Refreshing, Spinner } from "../Spinner";

/**
 * Ratings for everyone (Q36, Flashy): a player's rating per class on the
 * league scale, the HLTV-style stat bars -- each component group as a
 * percentile against the league on their class -- and their ranks.
 */

const GROUP_LABEL: Record<string, string> = {
  fragging: "Kills and damage",
  survival: "Staying alive",
  teamplay: "Playing for the team",
  objective: "The objective",
  medic: "Medic",
  speciality: "Class speciality",
};

export function PlayerStatsCard({ accountId }: { accountId: number }) {
  const q = useQuery({ queryKey: ["player_stats", accountId], queryFn: () => api.getPlayerStats(accountId) });
  const [picked, setPicked] = useState<string | null>(null);
  if (q.isPending) return <Loading>{t("Loading…")}</Loading>;
  if (q.isError || !q.data || q.data.classes.length === 0) {
    return <p className="hint">{t("None of their games are in the logs held, so there is no rating yet.")}</p>;
  }
  const s = q.data;
  const c = s.classes.find((x) => x.class === picked) ?? s.classes[0];
  const headline = c.recent ?? c.career;
  return (
    <div className="ps-card">
      <div className="ps-classes" role="tablist">
        {s.classes.slice(0, 6).map((x) => (
          <button key={x.class} role="tab" aria-selected={x.class === c.class} className={x.class === c.class ? "ps-class on" : "ps-class"} onClick={() => setPicked(x.class)} title={classLabel(x.class)}>
            <ClassIcon cls={x.class} size={18} />
            <span>{x.games}</span>
          </button>
        ))}
      </div>
      <div className="ps-rating">
        <span className="ps-rating-label">
          {classLabel(c.class)} · {c.recent !== null ? t("last 3 months played") : t("career")}
        </span>
        <span className="ps-rating-value">
          <StatTip k="rating">{rating(headline)}</StatTip>
        </span>
      </div>
      <p className="hint ps-sub">
        {c.recent !== null
          ? tx("{0} recent games · career {1} over {2} · best {3}", { "0": c.recentGames, "1": rating(c.career), "2": c.games, "3": rating(c.best) })
          : tx("{0} games · best {1}", { "0": c.games, "1": rating(c.best) })}
      </p>
      <ul className="ps-bars">
        {c.groups.map(([g, v]) => (
          <li key={g}>
            <span className="ps-bar-label">
              <StatTip entry={tip(t(GROUP_LABEL[g] ?? g), t("Where their games sit against every game on the class in the league, 0 to 100, for this group of the rating's parts."), t("The group's parts, each a percentile, averaged by their weight in the class's model."), "logs.tf", "server log")}>
                {t(GROUP_LABEL[g] ?? g)}
              </StatTip>
            </span>
            <span className="ps-bar-value">
              {Math.round(v)}
              <span className="muted">/100</span>
            </span>
            <span className="ps-bar-track" aria-hidden>
              <i className={`rg-g-${g}`} style={{ width: `${Math.max(2, v)}%` }} />
            </span>
          </li>
        ))}
      </ul>
      <p className="hint ps-foot">{t("Each bar: how their games compare with every game on the class in the league, 0 to 100. The parts are on the How ratings work page.")}</p>
      {s.ranks.length > 0 && (
        <div className="ps-ranks">
          <h4>{t("Ranks")}</h4>
          <ul>
            {s.ranks.slice(0, 6).map((r) => (
              <li key={`${r.season}-${r.class}`}>
                <RankChip r={r} accountId={accountId} />
                <span className="muted">
                  {tx("{0} over {1} officials", { "0": rating(r.avg), "1": r.games })}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/**
 * "#3 of 8 Sniper in Mid, S34", and on hover or focus the whole table it
 * comes from, the player highlighted (Flashy: "is it possible to get like
 * an overview when you hover over that").
 */
export function RankChip({ r, accountId }: { r: Rank; accountId?: number }) {
  const [open, setOpen] = useState(false);
  // Fixed to the window, beside the chip: an absolute popover near the foot
  // of the page made the page taller and the scrollbar jump (Flashy). Up
  // when there is no room below.
  const [pos, setPos] = useState<{ left: number; top?: number; bottom?: number } | null>(null);
  const place = (el: HTMLElement) => {
    const b = el.getBoundingClientRect();
    const room = window.innerHeight - b.bottom;
    const left = Math.max(8, Math.min(b.left, window.innerWidth - 430));
    setPos(room >= 340 ? { left, top: b.bottom + 6 } : { left, bottom: window.innerHeight - b.top + 6 });
  };
  const q = useQuery({ queryKey: ["rankings", r.season, r.tier, r.class], queryFn: () => api.getRankings(r.season, r.tier, r.class), enabled: open });
  return (
    <span
      className="ps-rank"
      tabIndex={0}
      onMouseEnter={(e) => {
        place(e.currentTarget);
        setOpen(true);
      }}
      onMouseLeave={() => setOpen(false)}
      onFocus={(e) => {
        place(e.currentTarget);
        setOpen(true);
      }}
      onBlur={() => setOpen(false)}
      aria-describedby={open ? `rank-${r.season}-${r.class}` : undefined}
    >
      <strong>#{r.rank}</strong>
      <span className="muted">{t("of {0}", { "0": r.of })}</span> <ClassIcon cls={r.class} size={16} /> {t("in")} <DivisionBadge d={{ name: r.division, tier: r.tier }} /> <span className="muted">{seasonShort(r.season, r.seasonName)}</span>
      {/* On the page itself, not inside the header: the profile header is a
          layer of its own (its map backdrop), and inside it the sticky tabs
          and toolbar below drew over the list (Flashy: "the overlap"). */}
      {open &&
        createPortal(
        <span className="rank-pop" role="tooltip" id={`rank-${r.season}-${r.class}`} style={pos ?? undefined}>
          <span className="rank-pop-head">
            {r.season >= 100
              ? tx("{0} in {1}, {2}", { "0": classLabel(r.class), "1": r.division, "2": r.seasonName })
              : tx("{0} in {1}, Season {2}", { "0": classLabel(r.class), "1": r.division, "2": r.season })}
          </span>
          <span className="rank-pop-sub">{tx("Officials only; {0} or more to be ranked, the rest greyed out.", { "0": 3 })}</span>
          {!q.data && <span className="hint"><Spinner /> {t("Loading…")}</span>}
          {q.data && (
            <span className="rank-pop-rows">
              {q.data.rows.map((x) => (
                <span key={x.accountId} className={`rank-pop-row${x.accountId === accountId ? " me" : ""}${x.rank === null ? " under" : ""}`}>
                  <span className="rank-pop-n">{x.rank === null ? "–" : `#${x.rank}`}</span>
                  <span className="rank-pop-name">{x.name}</span>
                  <span className="muted rank-pop-team">{x.team?.name}</span>
                  <span className="rank-pop-avg">{rating(x.avg)}</span>
                  <span className="muted rank-pop-games">{x.games}</span>
                </span>
              ))}
            </span>
          )}
        </span>,
          document.body,
        )}
    </span>
  );
}

const CLASSES = ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"];

/** A season's top players of a class in a division. */
export function TopPlayers({ onPick }: { onPick: (accountId: number) => void }) {
  const [cls, setCls] = useState("sniper");
  const [tier, setTier] = useState<number | null>(null);
  const [season, setSeason] = useState<number | null>(null);
  const q = useQuery({ queryKey: ["rankings", season, tier, cls], queryFn: () => api.getRankings(season, tier, cls), placeholderData: keepPreviousData });
  const r = q.data;
  // Everyone who played an official there (Flashy and function): the ranked,
  // then those under the threshold, greyed out.
  const top = useMemo(() => r?.rows ?? [], [r]);
  const s = useSort("top-players", top, { rank: (x) => x.rank ?? 9999, name: (x) => x.name, team: (x) => x.team?.name ?? null, games: (x) => x.games, rating: (x) => x.avg });
  return (
    <div className="panel top-players has-refresh">
      <Refreshing on={q.isPlaceholderData} />
      <h2>{t("Top players")}<Help>{tx("By average rating in one season, on one class, in the division they played, over their officials. {0} or more officials to be ranked; everyone else is listed, greyed out.", { "0": 3 })}</Help></h2>
      <div className="tp-controls">
        <div className="ps-classes">
          {CLASSES.map((c) => (
            <button key={c} className={c === cls ? "ps-class on" : "ps-class"} onClick={() => setCls(c)} title={classLabel(c)} aria-pressed={c === cls}>
              <ClassIcon cls={c} size={18} />
            </button>
          ))}
        </div>
        {r && (
          <>
            <select value={r.tier} onChange={(e) => setTier(Number(e.target.value))} aria-label={t("Division")}>
              {r.divisions.map(([tr, name]) => (
                <option key={tr} value={tr}>
                  {name}
                </option>
              ))}
            </select>
            <select value={r.season} onChange={(e) => { setSeason(Number(e.target.value)); setTier(null); }} aria-label={t("Season")}>
              {r.seasons.map(([s, name]) => (
                <option key={s} value={s}>
                  {s >= 100 ? name : tx("S{0} · {1}", { "0": s, "1": name })}
                </option>
              ))}
            </select>
          </>
        )}
      </div>
      {r && r.rows.length === 0 && <p className="hint">{t("Nobody on this class played enough rated games in this division and season.")}</p>}
      {r && r.rows.length > 0 && (
        <table className="pp-seasons tp-table">
          <thead>
            <tr>
              <SortTh k="rank" s={s}>#</SortTh>
              <SortTh k="name" s={s}>{t("Player")}</SortTh>
              <SortTh k="team" s={s}>{t("Team")}</SortTh>
              <SortTh k="games" s={s}>{t("Officials")}</SortTh>
              <SortTh k="rating" s={s} tip="rank">{t("Rating")}</SortTh>
            </tr>
          </thead>
          <tbody>
            {s.sorted.map((x) => (
              <tr
                key={x.accountId}
                className={x.rank === null ? "tp-row tp-under" : "tp-row"}
                onClick={() => onPick(x.accountId)}
                title={x.rank === null ? t("Fewer than {0} officials: listed, not ranked", { "0": 3 }) : undefined}
              >
                <td>{x.rank ?? "–"}</td>
                <td>
                  <strong>{x.name}</strong>
                </td>
                <td className="muted">{x.team?.name}</td>
                <td>{x.games}</td>
                <td>
                  <strong>{rating(x.avg)}</strong>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
