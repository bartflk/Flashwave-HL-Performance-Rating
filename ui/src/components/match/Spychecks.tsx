import { useState } from "react";
import { Help } from "../StatTip";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type MatchDetail, type SpyPlayer, type Team } from "../../api/types";
import { clock, teamLabel } from "../../lib/format";
import { copy } from "../../lib/toast";
import { t, tx } from "../../lib/i18n";
import { SortTh, useSort } from "../../lib/sort";

/**
 * Q27 (ivg): hits on a Spy nobody could see. Read off the match's kept STV
 * timelines, so it is there for any match whose STV was downloaded, even
 * after the file itself was deleted.
 */
export function Spychecks({ d }: { d: MatchDetail }) {
  const q = useQuery({ queryKey: ["spychecks", d.logId], queryFn: () => api.getSpychecks(d.logId) });
  const [open, setOpen] = useState(false);
  const nameOf = (id: number) => d.players.find((x) => x.accountId === id)?.name ?? null;
  const sort = useSort("spychecks", q.data?.checks ?? [], { time: (c) => c.atS, by: (c) => nameOf(c.attacker), spy: (c) => nameOf(c.spy), damage: (c) => c.damage, killed: (c) => (c.killed ? 1 : 0) });
  if (q.isError) {
    return (
      <section className="panel spy-panel">
        <h2>{t("Spychecks")}</h2>
        <p className="error">{errorMessage(q.error)}</p>
      </section>
    );
  }
  const r = q.data;
  // Nothing to say without an STV: the demo panel above already offers one.
  if (!r) return null;

  const who = (id: number, fallback?: SpyPlayer) => {
    const p = d.players.find((x) => x.accountId === id);
    return { name: p?.name ?? fallback?.name ?? String(id), team: p?.team ?? null, isMe: p?.isMe ?? false };
  };
  const checkers = r.players.filter((p) => p.checks > 0);
  const spies = r.players.filter((p) => p.found > 0).sort((a, b) => b.found - a.found);
  const most = Math.max(1, ...checkers.map((p) => p.checks));
  const mostFound = Math.max(1, ...spies.map((p) => p.found));

  return (
    <section className="panel spy-panel">
      <h2>
        {t("Spychecks")}
        <Help>{t("Hits on a Spy who had been fully cloaked for a second and was not blinking, burning, jarated, milked or bleeding. One per attacker per Spy every 2 seconds, so a held minigun counts once.")}</Help>
      </h2>
      {r.checks.length === 0 ? (
        <p className="hint" style={{ marginTop: 8 }}>{t("Nobody found a cloaked Spy in this match.")}</p>
      ) : (
        <div className="spy-cols">
          <SpyList title={t("Checks made")} rows={checkers.map((p) => ({ ...who(p.accountId, p), n: p.checks }))} most={most} />
          <SpyList title={t("Spies found")} rows={spies.map((p) => ({ ...who(p.accountId, p), n: p.found }))} most={mostFound} />
        </div>
      )}
      <p className="hint spy-skipped">
        {t("Not counted: {0} while still fading in, {1} already blinking, {2} marked by fire, jarate, milk or bleed, {3} inside the cooldown.", {
          "0": r.fading,
          "1": r.blinking,
          "2": r.marked,
          "3": r.cooldown,
        })}
      </p>
      {r.checks.length > 0 && (
        <>
          <button className="linkish" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
            {open ? t("Hide every spycheck") : tx("Show every spycheck ({0})", { "0": r.checks.length })}
          </button>
          {open && (
            <div className="table-wrap">
              <table className="match-table spy-table">
                <thead>
                  <tr>
                    <SortTh k="time" s={sort} className="num">{t("Time")}</SortTh>
                    <SortTh k="by" s={sort}>{t("By")}</SortTh>
                    <SortTh k="spy" s={sort}>{t("Spy")}</SortTh>
                    <SortTh k="damage" s={sort} className="num">{t("Damage")}</SortTh>
                    <SortTh k="killed" s={sort}>{""}</SortTh>
                  </tr>
                </thead>
                <tbody>
                  {sort.sorted.map((c, i) => {
                    const a = who(c.attacker);
                    const s = who(c.spy);
                    return (
                      <tr
                        key={i}
                        className="spy-row"
                        title={t("Copy demo_gototick {tick}", { tick: c.jumpTick })}
                        onClick={() => void copy(`demo_gototick ${c.jumpTick}`, t("Spycheck at {0}", { "0": clock(c.atS) }))}
                      >
                        <td className="num">{clock(c.atS)}</td>
                        <td><Name {...a} /></td>
                        <td><Name {...s} /></td>
                        <td className="num">{c.damage}</td>
                        <td>{c.killed && <span className="badge">{t("killed")}</span>}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </section>
  );
}

type Row = { name: string; team: Team | null; isMe: boolean; n: number };

function SpyList({ title, rows, most }: { title: string; rows: Row[]; most: number }) {
  return (
    <div className="spy-list">
      <h3>{title}</h3>
      {rows.map((p) => (
        <div key={p.name} className="spy-line">
          <Name {...p} />
          <div className="spy-bar">
            <div className={`spy-fill${p.team ? ` spy-fill-${p.team.toLowerCase()}` : ""}`} style={{ width: `${(p.n / most) * 100}%` }} />
          </div>
          <span className="num">{p.n}</span>
        </div>
      ))}
    </div>
  );
}

export function Name({ name, team, isMe }: { name: string; team: Team | null; isMe: boolean }) {
  return (
    <span className="spy-name">
      {team && <span className={`spy-team spy-team-${team.toLowerCase()}`}>{teamLabel(team)}</span>}
      {name}
      {isMe && <span className="you-tag">{t("you")}</span>}
    </span>
  );
}
