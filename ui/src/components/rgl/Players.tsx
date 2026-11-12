import { useState } from "react";
import type { RglPlayer } from "../../api/types";
import { t } from "../../lib/i18n";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { RglMark } from "../RglMark";
import { SEARCH_FROM, useNameSearch, usePlayer, day, looksLikeSteamId } from "./data";
import { OnRgl, SearchHits } from "./bits";

/** You on RGL, from the app's SteamID: your last season and team. */
export function YourRgl({ steamid, onTeam, onPlayer }: { steamid: string; onTeam: (id: number) => void; onPlayer: (s: string) => void }) {
  const q = usePlayer(steamid);
  if (q.isPending) return null;
  const p = q.data;
  const last = p?.seasons[0];
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>
          <RglMark size={14} /> {t("You on RGL")}
        </h2>
        {p && (
          <button className="e2-more" onClick={() => onPlayer(steamid)}>
            {t("Profile")} ›
          </button>
        )}
      </div>
      {q.error ? (
        <ApiError error={q.error} />
      ) : !p ? (
        <p className="hint">{t("No RGL profile for your SteamID.")}</p>
      ) : !last ? (
        <p className="hint">{t("{0} has no NA Highlander seasons.", { "0": p.name })}</p>
      ) : (
        <div className="rgl-you">
          <span className="muted">{last.seasonName}</span>
          <button className="linkish" onClick={() => onTeam(last.teamId)}>
            {last.teamName}
          </button>
          <span className="muted">
            {last.division}
            {last.tier && ` ≈ ${t(last.tier)}`} · {last.wins}-{last.losses}
          </span>
        </div>
      )}
    </section>
  );
}

// ---- Teams -----------------------------------------------------------------

export function PlayersLanding({ steamid, onPlayer, onTeam }: { steamid: string | null; onPlayer: (s: string) => void; onTeam: (id: number) => void }) {
  const [q, setQ] = useState("");
  const ok = looksLikeSteamId(q.trim());
  const found = useNameSearch(q);
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Players")}</h2>
      </div>
      <p className="hint">{t("A name, or a SteamID, a Steam profile link or an rgl.gg profile link.")}</p>
      <form
        className="rgl-lookup"
        onSubmit={(e) => {
          e.preventDefault();
          if (ok) onPlayer(q.trim());
        }}
      >
        <input type="search" value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("Name, SteamID or profile link")} aria-label={t("Name, SteamID or profile link")} />
        <button type="submit" disabled={!ok}>
          {t("Open")}
        </button>
        {steamid && (
          <button type="button" onClick={() => onPlayer(steamid)}>
            {t("You")}
          </button>
        )}
      </form>
      {q.trim().length > 0 && q.trim().length < SEARCH_FROM && !ok && <p className="hint">{t("Type at least {0} letters.", { "0": SEARCH_FROM })}</p>}
      {found.on && (
        <div className="rgl-found">
          <SearchHits found={found} onPlayer={onPlayer} onTeam={onTeam} />
        </div>
      )}
    </section>
  );
}

export function PlayerView({ steamId, onTeam }: { steamId: string; onTeam: (id: number) => void }) {
  const q = usePlayer(steamId);
  const p: RglPlayer | null | undefined = q.data;
  if (q.error) return <ApiError error={q.error} />;
  if (q.isPending) return <Loading>{t("Loading the player…")}</Loading>;
  if (!p) return <p className="hint">{t("No RGL profile for this SteamID.")}</p>;
  const wins = p.seasons.reduce((n, s) => n + s.wins, 0);
  const losses = p.seasons.reduce((n, s) => n + s.losses, 0);
  return (
    <>
      <div className="panel pp-head">
        <div className="pp-top">
          <div className="pp-avatar">{p.avatar ? <img src={p.avatar} alt="" /> : <span>{p.name.slice(0, 1).toUpperCase()}</span>}</div>
          <div className="pp-who">
            <div className="pp-name-row">
              <h2 className="pp-name">{p.name}</h2>
              {p.banned && <span className="badge rgl-banned">{t("Banned")}</span>}
              {p.probation && <span className="badge">{t("On probation")}</span>}
            </div>
            <span className="muted">
              {p.seasons.length === 1 ? t("1 NA Highlander season") : t("{0} NA Highlander seasons", { "0": p.seasons.length })} · {wins}-{losses}
            </span>
          </div>
          <OnRgl href={`https://rgl.gg/Public/PlayerProfile?p=${p.steamId}`} />
        </div>
      </div>
      <section className="panel e2-panel">
        <div className="e2-panel-head">
          <h2>{t("Seasons")}</h2>
        </div>
        {p.seasons.length === 0 ? (
          <p className="hint">{t("No NA Highlander seasons.")}</p>
        ) : (
          <div className="table-wrap">
            <table className="match-table">
              <thead>
                <tr>
                  <th>{t("Season")}</th>
                  <th>{t("Division")}</th>
                  <th>{t("Team")}</th>
                  <th>{t("Joined")}</th>
                  <th className="num">{t("W-L")}</th>
                </tr>
              </thead>
              <tbody>
                {p.seasons.map((s) => (
                  <tr key={`${s.teamId}-${s.startedAt}`}>
                    <td className="nowrap">{s.seasonName}</td>
                    <td className="nowrap muted">
                      {s.division ?? "–"}
                      {s.tier && ` ≈ ${t(s.tier)}`}
                    </td>
                    <td>
                      <button className="linkish" onClick={() => onTeam(s.teamId)}>
                        {s.teamName}
                      </button>
                    </td>
                    <td className="nowrap muted">{day(s.startedAt)}</td>
                    <td className="num">
                      {s.wins}-{s.losses}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </>
  );
}
