import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { t, tx } from "../../lib/i18n";
import { roleLabel } from "../../lib/gameformat";
import { classLabel } from "../analysis/common";
import { ClassIcon } from "../ClassIcon";
import { Loading } from "../Spinner";
import { BanTag } from "../BanTag";

const ROLES = ["pocket", "roamer", "pocket_scout", "flank_scout", "demoman", "medic", "off:engineer", "off:heavy", "off:sniper", "off:pyro", "off:spy"];
const CLASS: Record<string, string> = { pocket: "soldier", roamer: "soldier", pocket_scout: "scout", flank_scout: "scout", demoman: "demoman", medic: "medic" };
const classOf = (role: string) => CLASS[role] ?? role.replace(/^off:/, "");
const short = (role: string) => roleLabel(role)?.short ?? classLabel(classOf(role));

/**
 * The 6v6 Players board (the 6v6 plan, P7): the best average preview
 * ratings in each role and off-class, three games or more, over everyone
 * rated -- the players in your 6v6 logs, and the league sample once it is
 * downloading. A name opens the player's ratings by role.
 */
export function SixesPlayers() {
  const [role, setRole] = useState("pocket");
  const [player, setPlayer] = useState<{ id: number; name: string } | null>(null);
  const q = useQuery({ queryKey: ["sixes_board", role], queryFn: () => api.getSixesBoard(role), staleTime: 5 * 60_000 });
  if (player) return <SixesPlayer id={player.id} name={player.name} onBack={() => setPlayer(null)} />;
  return (
    <div className="sx-matches">
      <div className="sx-week-bar">
        <div className="segmented sx-role-pick" role="tablist" aria-label={t("Role")}>
          {ROLES.map((r) => (
            <button key={r} role="tab" aria-selected={r === role} className={r === role ? "seg active" : "seg"} onClick={() => setRole(r)} title={roleLabel(r)?.long}>
              <ClassIcon cls={classOf(r)} size={14} /> {short(r)}
            </button>
          ))}
        </div>
      </div>
      <p className="hint sx-explain">{t("Preview ratings, three games or more in the role: the players in your 6v6 logs, and the whole league once the 6v6 league sample is downloading.")}</p>
      {q.isPending ? (
        <Loading>{t("Loading players…")}</Loading>
      ) : !q.data || q.data.length === 0 ? (
        <p className="hint">{t("No one has three rated games in this role yet.")}</p>
      ) : (
        <section className="panel e2-panel">
          <div className="table-wrap">
            <table className="match-table">
              <thead>
                <tr>
                  <th className="num">#</th>
                  <th>{t("Player")}</th>
                  <th className="num">{t("Games")}</th>
                  <th className="num">{t("Average")}</th>
                  <th className="num">{t("Best")}</th>
                </tr>
              </thead>
              <tbody>
                {q.data.map((r, i) => (
                  <tr key={r.accountId}>
                    <td className="num muted">{i + 1}</td>
                    <td>
                      <button className="linkish sx-team-link" onClick={() => setPlayer({ id: r.accountId, name: r.name ?? String(r.accountId) })}>
                        {r.name ?? r.accountId}
                      </button>
                      <BanTag accountId={r.accountId} short />
                    </td>
                    <td className="num muted">{r.games}</td>
                    <td className="num">
                      <strong>{r.average.toFixed(2)}</strong>
                    </td>
                    <td className="num muted">{r.best.toFixed(2)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      )}
    </div>
  );
}

/** One player's 6v6 preview ratings by role. */
function SixesPlayer({ id, name, onBack }: { id: number; name: string; onBack: () => void }) {
  const q = useQuery({ queryKey: ["sixes_ratings_of", id], queryFn: () => api.getSixesRatings({ accountId: id }) });
  const byRole = useMemo(() => {
    const m = new Map<string, number[]>();
    for (const r of q.data ?? []) m.set(r.role, [...(m.get(r.role) ?? []), r.rating]);
    return [...m.entries()].map(([role, xs]) => ({ role, games: xs.length, avg: xs.reduce((a, b) => a + b, 0) / xs.length })).sort((a, b) => b.games - a.games);
  }, [q.data]);
  return (
    <div className="sx-team-page">
      <button className="linkish sx-back" onClick={onBack}>
        {t("‹ Back to the players")}
      </button>
      <section className="panel sx-soon-panel">
        <span className="ts-eyebrow">{t("6v6")}</span>
        <h1 className="sx-title">{name}</h1>
        {q.isPending ? (
          <Loading>{t("Loading player…")}</Loading>
        ) : (
          <table className="match-table sx-roles">
            <thead>
              <tr>
                <th>{t("Role")}</th>
                <th className="num">{t("Games")}</th>
                <th className="num">{t("Average")}</th>
              </tr>
            </thead>
            <tbody>
              {byRole.map((r) => (
                <tr key={r.role}>
                  <td>
                    <ClassIcon cls={classOf(r.role)} size={14} /> {roleLabel(r.role)?.long ?? tx("{0}, off-class", { "0": classLabel(classOf(r.role)) })}
                  </td>
                  <td className="num muted">{r.games}</td>
                  <td className="num">
                    <strong>{r.avg.toFixed(2)}</strong>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <a className="sx-ext" href={`https://logs.tf/profile/${BigInt(id) + 76561197960265728n}`} target="_blank" rel="noreferrer">
          {t("Their logs on logs.tf")}
        </a>
      </section>
    </div>
  );
}
