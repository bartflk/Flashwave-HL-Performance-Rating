import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { MatchDivisions } from "../../api/types";
import { rating } from "../../lib/format";
import { openPlayer } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";
import { MedalGlyph } from "./MedalGlyph";
import { Country } from "../Country";
import { DivisionBadge, medalTitles, seasonShort } from "./PlayerProfile";
import "./players.css";
import { Etf2lMark } from "../Etf2lMark";

/**
 * A player card from the scoreboard (Q40, Flashy; PLAN §26): click a name
 * on a match page and a small card opens beside it -- who they are, their
 * medals, their main class rating and best rank, you and them -- with a way
 * through to the full profile.
 *
 * It asks for what the profile asks for, under the same query keys, so a
 * card opened once makes the profile open at once, and the other way round.
 */

export interface CardTarget {
  accountId: number;
  name: string;
  /** The class they played in this match, if known. */
  cls: string | null;
  isMe: boolean;
  /** The name that was clicked: the card sits beside it, and focus goes back to it. */
  anchor: HTMLElement;
}

/** One card at a time, for a whole match page. */
export function usePlayerCard() {
  const [target, setTarget] = useState<CardTarget | null>(null);
  const open = (t: Omit<CardTarget, "anchor">) => (e: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>) => {
    const anchor = e.currentTarget;
    setTarget((cur) => (cur?.accountId === t.accountId && cur.anchor === anchor ? null : { ...t, anchor }));
  };
  const close = useCallback(() => setTarget(null), []);
  return { target, open, close };
}

/** Props for a clickable name: a button in all but looks. */
export function nameProps(onOpen: (e: React.MouseEvent<HTMLElement> | React.KeyboardEvent<HTMLElement>) => void) {
  return {
    role: "button",
    tabIndex: 0,
    className: "pc-name",
    onClick: (e: React.MouseEvent<HTMLElement>) => {
      e.stopPropagation();
      onOpen(e);
    },
    onKeyDown: (e: React.KeyboardEvent<HTMLElement>) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        e.stopPropagation();
        onOpen(e);
      }
    },
  };
}

const PLACE = ["gold", "silver", "bronze"] as const;
const WIDTH = 330;

export function PlayerPopCard({ target, divisions, onClose }: { target: CardTarget; divisions: MatchDivisions | undefined; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const { accountId, anchor } = target;

  const profile = useQuery({ queryKey: ["player_profile", accountId], queryFn: () => api.getPlayerProfile(accountId) });
  const stats = useQuery({ queryKey: ["player_stats", accountId], queryFn: () => api.getPlayerStats(accountId) });
  const together = useQuery({ queryKey: ["player", accountId, null], queryFn: () => api.getPlayer(accountId, null), enabled: !target.isMe });

  // Beside the name, flipped to stay in the window; under it when narrow.
  useLayoutEffect(() => {
    const place = () => {
      const r = anchor.getBoundingClientRect();
      const h = ref.current?.offsetHeight ?? 300;
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      let left: number;
      let top: number;
      if (vw < 720) {
        left = Math.max(8, Math.min(r.left, vw - WIDTH - 8));
        top = r.bottom + 6;
      } else {
        left = r.right + 10 + WIDTH < vw ? r.right + 10 : Math.max(8, r.left - WIDTH - 10);
        top = r.top - 8;
      }
      top = Math.max(8, Math.min(top, vh - h - 8));
      setPos({ left, top });
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [anchor, profile.data, stats.data, together.data]);

  // Click elsewhere or Escape closes it; Escape gives focus back to the name.
  useEffect(() => {
    const down = (e: MouseEvent) => {
      const n = e.target as Node;
      if (!ref.current?.contains(n) && !anchor.contains(n)) onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        anchor.focus();
      }
    };
    document.addEventListener("mousedown", down);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", down);
      document.removeEventListener("keydown", key);
    };
  }, [anchor, onClose]);

  const p = profile.data;
  const s = stats.data;
  const then = divisions?.players[accountId];
  const main = p?.mainClass ?? s?.classes[0]?.class ?? null;
  const mainStats = s?.classes.find((c) => c.class === main) ?? null;
  const here = target.cls && target.cls !== main ? (s?.classes.find((c) => c.class === target.cls) ?? null) : null;
  const best = s?.ranks.filter((r) => r.class === main).sort((a, b) => a.rank - b.rank || b.season - a.season)[0];
  const counts = [0, 0, 0];
  for (const m of p?.medals ?? []) counts[m.place - 1] += 1;
  const title = p ? medalTitles(p.medals)[0] : undefined;
  const sum = together.data?.summary;

  return (
    <div
      ref={ref}
      className="pc-card"
      role="dialog"
      aria-label={t("{0}'s card", { "0": target.name })}
      style={{ width: WIDTH, left: pos?.left ?? -9999, top: pos?.top ?? 0 }}
    >
      {/* 1. Who */}
      <div className="pc-who">
        <div className="pp-avatar pc-avatar">{p?.avatar ? <img src={p.avatar} alt="" /> : <span>{target.name.slice(0, 1).toUpperCase()}</span>}</div>
        <div className="pc-who-text">
          <div className="pc-name-row">
            <strong>{p?.name ?? target.name}</strong>
            {target.isMe && <span className="you-tag">{t("you")}</span>}
          </div>
          <div className="pc-divs">
            {then && (
              <span title={then.exact ? undefined : t("The nearest season they played, not the one this was in")}>
                <DivisionBadge d={{ name: then.division, tier: then.tier }} />{" "}
                <span className="muted">
                  {then.exact ? t("then") : seasonShort(then.season)}
                </span>
              </span>
            )}
            {p?.highest && (!then || (p.highest.tier ?? 9) < then.tier) && (
              <span>
                <span className="muted">{t("best")}</span> <DivisionBadge d={p.highest} />
              </span>
            )}
            {p?.country && (
              <span className="muted">
                <Country raw={p.country} />
              </span>
            )}
          </div>
        </div>
      </div>

      {profile.isPending && <p className="hint">{t("Loading…")}</p>}
      {p && p.officials.length === 0 && <p className="hint">{t("No ETF2L officials in the seasons read.")}</p>}

      {/* 2. Medals */}
      {p && p.medals.length > 0 && (
        <div className="pc-medals">
          {counts.map((n, i) =>
            n > 0 ? (
              <span key={i} className={`pp-medal-tile pc-medal pp-${PLACE[i]}`} title={[t("Gold"), t("Silver"), t("Bronze")][i]}>
                <MedalGlyph size={24} place={i + 1} />
                <span className="pc-medal-n">{n}</span>
              </span>
            ) : null,
          )}
          {title && (
            <span className="pc-title">
              {title[1]}× {title[0]}
            </span>
          )}
        </div>
      )}

      {/* 3. Main class, and 4. the class played here when it is another */}
      {mainStats && (
        <div className="pc-class">
          <ClassIcon cls={mainStats.class} size={22} />
          <span className="pc-class-text">
            <span>
              {classLabel(mainStats.class)} <span className="muted">· {mainStats.recent !== null ? t("last 3 months") : t("career")}</span>
            </span>
            <span className="muted pc-small">
              {tx("{0} games", { "0": mainStats.recent !== null ? mainStats.recentGames : mainStats.games })}
              {best && <> · {tx("#{0} of {1} in {2}, {3}", { "0": best.rank, "1": best.of, "2": best.division, "3": seasonShort(best.season, best.seasonName) })}</>}
            </span>
          </span>
          <span className="pc-rating">{rating(mainStats.recent ?? mainStats.career)}</span>
        </div>
      )}
      {here && (
        <div className="pc-class pc-here">
          <ClassIcon cls={here.class} size={18} />
          <span className="pc-class-text">
            <span>
              {classLabel(here.class)} <span className="muted">· {t("played here, not their main")}</span>
            </span>
            <span className="muted pc-small">{tx("{0} games", { "0": here.games })}</span>
          </span>
          <span className="pc-rating pc-rating-small">{rating(here.recent ?? here.career)}</span>
        </div>
      )}
      {s && s.classes.length === 0 && <p className="hint">{t("None of their games are in the logs held, so there is no rating yet.")}</p>}

      {/* 5. Four stat bars */}
      {mainStats && mainStats.groups.length > 0 && (
        <ul className="ps-bars pc-bars">
          {mainStats.groups.slice(0, 4).map(([g, v]) => (
            <li key={g}>
              <span className="ps-bar-label">{t(GROUP_LABEL[g] ?? g)}</span>
              <span className="ps-bar-value">{Math.round(v)}</span>
              <span className="ps-bar-track" aria-hidden>
                <i className={`rg-g-${g}`} style={{ width: `${Math.max(2, v)}%` }} />
              </span>
            </li>
          ))}
        </ul>
      )}

      {/* 6. You and them */}
      {sum && (sum.withYou > 0 || sum.againstYou > 0) && (
        <p className="pc-together">
          {tx("With you {0} · against you {1}", { "0": sum.withYou, "1": sum.againstYou })}
          {sum.againstYou > 0 && <span className="muted"> · {tx("you won {0}–{1}", { "0": sum.youBeatThem, "1": sum.theyBeatYou })}</span>}
        </p>
      )}

      {/* 7. Links */}
      <div className="pc-links">
        <button
          className="primary"
          onClick={() => {
            onClose();
            openPlayer(accountId);
          }}
        >
          {t("Open profile")}
        </button>
        {p?.etf2lId != null && (
          <a href={`https://etf2l.org/forum/user/${p.etf2lId}/`} target="_blank" rel="noreferrer">
            <Etf2lMark size={13} /> {t("ETF2L ↗")}
          </a>
        )}
        {p && (
          <a href={`https://trends.tf/player/${p.steamid64}/`} target="_blank" rel="noreferrer">
            {t("trends.tf ↗")}
          </a>
        )}
      </div>
    </div>
  );
}

const GROUP_LABEL: Record<string, string> = {
  fragging: "Kills and damage",
  survival: "Staying alive",
  teamplay: "Playing for the team",
  objective: "The objective",
  medic: "Medic",
  speciality: "Class speciality",
};
