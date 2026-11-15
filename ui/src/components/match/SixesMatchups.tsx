import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { MatchDetail, Team } from "../../api/types";
import { partsOf, roleRows, type Pick, type RoleRow } from "../../lib/sixesmatchups";
import { rating, RATING_GAP_FULL, teamLabel } from "../../lib/format";
import { roleClass, roleLabel, unitLabel } from "../../lib/gameformat";
import { openRatingGuide, sixesComponent } from "../../lib/sixesguide";
import { t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";
import { BanTag } from "../BanTag";
import { PlayerPopCard, nameProps, usePlayerCard } from "../players/PlayerPopCard";
import { DivTag, useMatchDivisions } from "./Divisions";
import { Help } from "../StatTip";

/**
 * A 6v6 match's matchups (audit P2, Flashy: "the audit [should] check for
 * differences between highlander and 6's"): role against role -- the
 * pockets, the roamers, the two Scouts by role, the Demomen and the Medics --
 * on the 6v6 rating, where the class matchups paired nine Highlander classes
 * and said "not rated yet" for ratings 6v6 never gets.
 */
export function SixesMatchups({ d }: { d: MatchDetail }) {
  const roles = useQuery({ queryKey: ["match_roles", d.logId], queryFn: () => api.getMatchRoles(d.logId), staleTime: 5 * 60_000 }).data ?? [];
  const ratingsQ = useQuery({ queryKey: ["sixes_ratings", d.logId], queryFn: () => api.getSixesRatings({ logId: d.logId }), staleTime: 5 * 60_000 });
  const guide = useQuery({ queryKey: ["sixes_rating_guide"], queryFn: api.getSixesRatingGuide, staleTime: 5 * 60_000 }).data;
  const divisions = useMatchDivisions(d.logId);
  const card = usePlayerCard();
  const [open, setOpen] = useState<string | null>(null);
  const rows = roleRows(d, roles, ratingsQ.data ?? []);
  if (rows.length === 0) return null;
  const left = d.leftTeam;
  const right: Team = left === "Red" ? "Blue" : "Red";
  const us = d.myTeam !== null;
  const maxGap = Math.max(RATING_GAP_FULL, ...rows.map((r) => Math.abs(r.diff ?? 0)));
  const won = (w: RoleRow["winner"]) => rows.filter((r) => r.winner === w).length;
  const rated = rows.some((r) => r.diff !== null);
  return (
    <section className="panel mu">
      <header className="mu-head">
        <div>
          <h2>
            {t("Role matchups")}
            <Help>{t("Ratings measure individual play, not who won the match. Click a row for the working.")}</Help>
          </h2>
          <p className="hint">
            {rated
              ? tx("{0} won {1}, {2} won {3}{4}.", {
                  "0": us ? t("You") : teamLabel(left),
                  "1": won("left"),
                  "2": us ? t("they") : teamLabel(right),
                  "3": won("right"),
                  "4": won("even") > 0 ? t(", {even} even", { even: won("even") }) : "",
                })
              : t("No 6v6 ratings for this match yet: a role needs 5 minutes or more.")}
          </p>
        </div>
        <div className="mu-head-right">
          {guide && <span className="model-tag">{tx("6v6 model {0} · one model per role", { "0": guide.modelVersion })}</span>}
          <button className="km-chip mu-guide-link" onClick={() => openRatingGuide("6s")}>
            {t("How ratings work →")}
          </button>
        </div>
      </header>
      <div className="mu-table">
        <div className="mu-cols" aria-hidden>
          <span />
          <span className={`team-${left.toLowerCase()} mu-col-l`}>
            {us ? t("Us") : ""} {teamLabel(left)}
          </span>
          <span className="mu-col-c">{t("gap")}</span>
          <span className={`team-${right.toLowerCase()} mu-col-r`}>
            {teamLabel(right)} {us ? t("Them") : ""}
          </span>
          <span />
        </div>
        {rows.map((r) => {
          const pct = r.diff === null ? 0 : Math.min(50, (Math.abs(r.diff) / maxGap) * 50);
          const cls = ["mu-row"];
          if (r.involvesMe) cls.push("mine");
          if (r.decisive) cls.push("decisive");
          if (open === r.role) cls.push("open");
          const side = (p: Pick | null, align: "left" | "right", winning: boolean) => {
            if (!p) return <span className={`mu-side ${align} muted`}>{t("nobody")}</span>;
            const name = (
              <span className="mu-name">
                <span {...nameProps(card.open({ accountId: p.accountId, name: p.name, cls: roleClass(r.role), isMe: d.players.some((x) => x.accountId === p.accountId && x.isMe) }))}>{p.name}</span>
                <BanTag accountId={p.accountId} short />
                <DivTag divisions={divisions} accountId={p.accountId} />
              </span>
            );
            const score = (
              <span className={winning ? "mu-score win" : "mu-score"} title={p.rating ? undefined : t("Not rated: under 5 minutes in the role")}>
                {p.rating ? rating(p.rating.rating) : "—"}
              </span>
            );
            return <span className={`mu-side ${align}`}>{align === "left" ? <>{name}{score}</> : <>{score}{name}</>}</span>;
          };
          return (
            <div key={r.role}>
              <button className={cls.join(" ")} onClick={() => setOpen(open === r.role ? null : r.role)} aria-expanded={open === r.role}>
                <span className="mu-class" title={roleLabel(r.role)?.long ?? unitLabel(r.role)}>
                  <ClassIcon cls={roleClass(r.role)} size={22} />
                  {roleLabel(r.role)?.short ?? unitLabel(r.role)}
                  {r.involvesMe && <span className="you-tag">{t("you")}</span>}
                </span>
                {side(r.left, "left", r.winner === "left")}
                <span className="mu-center">
                  <span className="bar">
                    <span className="bar-mid" />
                    {r.diff !== null && r.winner !== "even" && (
                      <span
                        className={`bar-fill team-bg-${(r.winner === "left" ? left : right).toLowerCase()}`}
                        style={r.winner === "left" ? { right: "50%", width: `${pct}%` } : { left: "50%", width: `${pct}%` }}
                      />
                    )}
                  </span>
                  <span className="h2h">{r.diff !== null ? <b>{Math.abs(r.diff).toFixed(2)}</b> : <span className="muted">–</span>}</span>
                </span>
                {side(r.right, "right", r.winner === "right")}
                <span className="mu-tag">
                  {r.decisive ? <span className="decisive-tag">{t("decisive")}</span> : r.winner === "even" ? <span className="muted">{t("even")}</span> : null}
                </span>
              </button>
              {open === r.role && <RoleBreakdown row={r} />}
            </div>
          );
        })}
      </div>
      {card.target && <PlayerPopCard target={card.target} divisions={divisions} onClose={card.close} />}
    </section>
  );
}

/** Each part of the role's rating for both players: where each sits, 0-100. */
function RoleBreakdown({ row }: { row: RoleRow }) {
  const a = partsOf(row.left?.rating ?? null);
  const b = partsOf(row.right?.rating ?? null);
  const keys = [...new Set([...a, ...b].sort((x, y) => y[2] - x[2]).map((p) => p[0]))];
  if (keys.length === 0) {
    return (
      <div className="mu-breakdown">
        <p className="hint">{t("Neither side is rated here.")}</p>
      </div>
    );
  }
  const at = (parts: Array<[string, number, number]>, k: string) => parts.find((p) => p[0] === k) ?? null;
  return (
    <div className="mu-breakdown">
      <table className="match-table sx-mu-parts">
        <thead>
          <tr>
            <th className="num">{row.left?.name ?? "—"}</th>
            <th>{t("Part")}</th>
            <th className="num">{t("Share")}</th>
            <th className="num">{row.right?.name ?? "—"}</th>
          </tr>
        </thead>
        <tbody>
          {keys.map((k) => {
            const l = at(a, k);
            const r = at(b, k);
            const share = (l ?? r)![2];
            return (
              <tr key={k}>
                <td className={l && r && l[1] > r[1] ? "num h2h-win" : "num"}>{l ? Math.round(l[1]) : "–"}</td>
                <td>{sixesComponent(k).label}</td>
                <td className="num muted">{Math.round(share * 100)}%</td>
                <td className={l && r && r[1] > l[1] ? "num h2h-win" : "num"}>{r ? Math.round(r[1]) : "–"}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <p className="hint">{t("Each number is where the game sits among the role's league games, 0 to 100; higher is better. The share is the part's weight in the rating.")}</p>
    </div>
  );
}
