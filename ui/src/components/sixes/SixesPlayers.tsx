import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { t } from "../../lib/i18n";
import { roleLabel } from "../../lib/gameformat";
import { classLabel } from "../analysis/common";
import { ClassIcon } from "../ClassIcon";
import { Loading } from "../Spinner";
import { BanTag } from "../BanTag";
import { ProfilePage } from "../profile/ProfilePage";
import { DivisionBadge } from "../players/PlayerProfile";

const ROLES = ["pocket", "roamer", "pocket_scout", "flank_scout", "demoman", "medic", "off:engineer", "off:heavy", "off:sniper", "off:pyro", "off:spy"];
const CLASS: Record<string, string> = { pocket: "soldier", roamer: "soldier", pocket_scout: "scout", flank_scout: "scout", demoman: "demoman", medic: "medic" };
const classOf = (role: string) => CLASS[role] ?? role.replace(/^off:/, "");
const short = (role: string) => roleLabel(role)?.short ?? classLabel(classOf(role));

/**
 * The 6v6 Players board (the 6v6 plan, P7): the best average preview
 * ratings in each role and off-class, three games or more, over everyone
 * rated -- the players in your 6v6 logs, and the league sample once it is
 * downloading. A name, or a search above the board (Flashy: "the player
 * lookup isn't finished yet and its just a list"), opens the player's full
 * 6v6 profile: the Highlander profile's page, by role.
 */
export function SixesPlayers({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
  const [role, setRole] = useState("pocket");
  const [player, setPlayer] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const term = query.trim();
  const q = useQuery({ queryKey: ["sixes_board", role], queryFn: () => api.getSixesBoard(role), staleTime: 5 * 60_000 });
  const hits = useQuery({ queryKey: ["search_catalogue", term], queryFn: () => api.searchCatalogue(term), enabled: term.length >= 2, placeholderData: keepPreviousData });
  if (player !== null) {
    return (
      <div className="sx-team-page">
        <button className="linkish sx-back" onClick={() => setPlayer(null)}>
          {t("‹ Back to the players")}
        </button>
        <ProfilePage key={player} format="6s" accountId={player} steamid={null} onOpenMatch={onOpenMatch} />
      </div>
    );
  }
  return (
    <div className="sx-matches">
      <section className="panel e2-panel">
        <h2>{t("Look someone up")}</h2>
        <input className="player-search" value={query} placeholder={t("A name, or 76561198… / [U:1:…]")} onChange={(e) => setQuery(e.target.value)} />
        {term.length >= 2 && (
          <div className="player-hits">
            {(hits.data ?? []).length === 0 ? (
              <p className="hint">{hits.isFetching ? t("Searching…") : t("No one by that name.")}</p>
            ) : (
              (hits.data ?? []).slice(0, 12).map((h) => (
                <button key={h.accountId} className="player-hit" onClick={() => setPlayer(h.accountId)}>
                  {h.mainClass ? <ClassIcon cls={h.mainClass} size={22} /> : <span style={{ width: 22 }} />}
                  <span className="ph-name">{h.name}</span>
                  <BanTag accountId={h.accountId} short />
                  {h.highest && <DivisionBadge d={h.highest} />}
                </button>
              ))
            )}
          </div>
        )}
      </section>
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
                      <button className="linkish sx-team-link" onClick={() => setPlayer(r.accountId)}>
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
