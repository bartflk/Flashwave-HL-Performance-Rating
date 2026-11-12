import { useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglTeamHistory, RglTeamPage } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { headToHead, sharedMaps, trophyCount, winRate } from "../../lib/rglteam";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { Monogram } from "../etf2l/Teams";
import { KEEP, SEARCH_FROM, useSettled, useTeam, placing } from "./data";
import { WeekLine } from "./bits";
import { useTeamHistory, forfeitNote } from "./Team";

/**
 * Two RGL teams side by side (tf2esports' Compare): this season's division
 * and place, the record across their seasons, titles, the maps both have
 * played and the matches they played each other. Everything is read from
 * what a team's own page reads, held and kept the same way: opening Compare
 * on a team already looked at asks RGL nothing new for it; the second team
 * costs what opening its page and its history would (hl_ingest::rgl, about
 * 13 requests a season the first time, none after for a finished season).
 * Choosing the second team is the Teams page's own name search (one request
 * once typing stops, three letters at least), through the same query.
 */

/** Pick the second team: the Teams page's search, its results as the same tiles. */
function Pick({ a, onPick }: { a: RglTeamPage | undefined; onPick: (b: number) => void }) {
  const [q, setQ] = useState("");
  const term = useSettled(q.trim());
  const on = term.length >= SEARCH_FROM;
  const hits = useQuery({ queryKey: ["rgl_search_teams", term.toLowerCase()], queryFn: () => api.searchRglTeams(term), enabled: on, staleTime: KEEP, retry: false });
  return (
    <>
      <header className="tm-head">
        <div className="ts-pagehead">
          <span className="ts-eyebrow">{t("Compare")}</span>
          <h1>{a ? tx("{0} against…", { "0": a.name }) : t("Pick a team")}</h1>
        </div>
        <div className="e2-search tm-search">
          <input type="search" value={q} placeholder={t("Search the team to compare with")} onChange={(e) => setQ(e.target.value)} aria-label={t("Search teams")} autoFocus />
        </div>
      </header>
      {q.trim().length === 0 && <p className="hint">{t("Search any RGL team by name, then pick it.")}</p>}
      {q.trim().length > 0 && q.trim().length < SEARCH_FROM && <p className="hint">{t("Three letters at least.")}</p>}
      {on && (
        <section className="tm-section">
          <h2 className="ts-section">{tx("Teams matching “{0}”", { "0": term })}</h2>
          {hits.error ? (
            <ApiError error={hits.error} />
          ) : hits.isPending ? (
            <Loading>{t("Asking RGL…")}</Loading>
          ) : (hits.data ?? []).length === 0 ? (
            <p className="hint">{t("No team by that name.")}</p>
          ) : (
            <div className="tm-grid">
              {(hits.data ?? []).map((x) => (
                <button key={x.id} className="tm-tile" onClick={() => onPick(x.id)}>
                  <Monogram name={x.name} />
                  <span className="tm-tile-text">
                    <strong>{x.name}</strong>
                    <span className="muted">{[x.seasonName, x.division].filter(Boolean).join(" · ")}</span>
                  </span>
                </button>
              ))}
            </div>
          )}
        </section>
      )}
    </>
  );
}

/** One row of the table: what it is, then each team's figure. */
function Row({ label, a, b, title }: { label: string; a: ReactNode; b: ReactNode; title?: string }) {
  return (
    <tr title={title}>
      <td className="rgl-cmp-label">{label}</td>
      <td>{a}</td>
      <td>{b}</td>
    </tr>
  );
}

const recordCell = (d: RglTeamHistory) => {
  const rate = winRate(d.total);
  return (
    <>
      <strong>
        <span className="team-won">{d.total.won}</span>–<span className="team-lost">{d.total.lost}</span>
      </strong>
      {rate !== null && <span className="muted"> {rate.toFixed(0)}%</span>}
      {forfeitNote(d.total) && <span className="muted rgl-cmp-note">{forfeitNote(d.total)}</span>}
    </>
  );
};

const seasonCell = (v: RglTeamPage) => (
  <>
    {placing(v.finalRank) || t("In progress")}
    <span className="muted rgl-cmp-note">{[v.seasonName, v.division && (v.tier ? `${v.division} ≈ ${t(v.tier)}` : v.division)].filter(Boolean).join(" · ")}</span>
  </>
);

export function CompareView({ a, b, onTeam, onMatch, onPick }: { a: number; b: number | null; onTeam: (id: number) => void; onMatch: (id: number) => void; onPick: (b: number | null) => void }) {
  const ta = useTeam(a);
  const tb = useTeam(b);
  const ha = useTeamHistory(a);
  const hb = useTeamHistory(b);
  if (b === null) return <Pick a={ta.data ?? undefined} onPick={onPick} />;
  const error = ta.error ?? tb.error ?? ha.error ?? hb.error;
  if (error) return <ApiError error={error} />;
  const va = ta.data;
  const vb = tb.data;
  if (ta.isPending || tb.isPending) return <Loading>{t("Loading the two teams…")}</Loading>;
  if (!va || !vb) return <p className="hint">{t("RGL has no such team.")}</p>;
  const da = ha.data;
  const db = hb.data;
  if (!da || !db) return <Loading>{t("Reading both teams' seasons from RGL… a minute or two the first time for a team with many seasons. You can look at another page meanwhile: the read carries on and is kept.")}</Loading>;

  const ra = winRate(da.total);
  const rb = winRate(db.total);
  const trA = trophyCount(da.seasons);
  const trB = trophyCount(db.seasons);
  const h2h = headToHead(da, db);
  const maps = sharedMaps(da, db);
  const side = (v: RglTeamPage) => (
    <th scope="col">
      <button className="rgl-cmp-team linkish" onClick={() => onTeam(v.id)}>
        <Monogram name={v.name} />
        <span>{v.name}</span>
      </button>
    </th>
  );
  // A figure's better side is marked, as the Players board heat-maps: only a clear difference.
  const lead = (x: number | null, y: number | null) => (x !== null && y !== null && x !== y ? (x > y ? "a" : "b") : "");

  return (
    <>
      <header className="tm-head">
        <div className="ts-pagehead">
          <span className="ts-eyebrow">{t("Compare")}</span>
          <h1>
            {va.name} <span className="muted">{t("vs")}</span> {vb.name}
          </h1>
        </div>
        <button onClick={() => onPick(null)}>{t("Change the second team")}</button>
      </header>
      <section className="panel e2-panel">
        <div className="e2-panel-head">
          <h2>{t("Side by side")}</h2>
          {(da.capped || db.capped) && <span className="muted">{t("Totals cover the newest 12 seasons of a team")}</span>}
        </div>
        <div className="table-wrap">
          <table className="match-table rgl-cmp">
            <thead>
              <tr>
                <th />
                {side(va)}
                {side(vb)}
              </tr>
            </thead>
            <tbody>
              <Row label={t("This season")} a={seasonCell(va)} b={seasonCell(vb)} title={t("Its place in the division this season, as RGL ranks it")} />
              <Row label={t("Record")} a={recordCell(da)} b={recordCell(db)} title={t("Matches won and lost across the seasons read, forfeits apart")} />
              <Row label={t("Win %")} a={<span className={lead(ra, rb) === "a" ? "rgl-cmp-lead" : undefined}>{ra === null ? "–" : `${ra.toFixed(0)}%`}</span>} b={<span className={lead(ra, rb) === "b" ? "rgl-cmp-lead" : undefined}>{rb === null ? "–" : `${rb.toFixed(0)}%`}</span>} />
              <Row label={t("Titles")} a={<span className={lead(trA.titles, trB.titles) === "a" ? "rgl-cmp-lead" : undefined}>{trA.titles}</span>} b={<span className={lead(trA.titles, trB.titles) === "b" ? "rgl-cmp-lead" : undefined}>{trB.titles}</span>} title={t("Seasons finished first")} />
              <Row label={t("Podiums")} a={<span className={lead(trA.podiums, trB.podiums) === "a" ? "rgl-cmp-lead" : undefined}>{trA.podiums}</span>} b={<span className={lead(trA.podiums, trB.podiums) === "b" ? "rgl-cmp-lead" : undefined}>{trB.podiums}</span>} title={t("Seasons finished in the top three")} />
              <Row label={t("Active since")} a={da.firstSeason ?? "–"} b={db.firstSeason ?? "–"} />
            </tbody>
          </table>
        </div>
      </section>

      <div className="rgl-cols rgl-cmp-cols">
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <h2>{t("Head to head")}</h2>
            <span className="muted">
              {h2h.matches.length === 0 ? "" : (
                <>
                  <span className="team-won">{h2h.won}</span>–<span className="team-lost">{h2h.lost}</span>
                  {h2h.forfeits > 0 && ` · ${h2h.forfeits === 1 ? t("1 forfeit") : tx("{0} forfeits", { "0": h2h.forfeits })}`}
                </>
              )}
            </span>
          </div>
          {h2h.matches.length === 0 ? (
            <p className="hint">{t("They have not met in the seasons read.")}</p>
          ) : (
            <>
              <p className="hint">
                {tx("From {0}'s side: {0} has won {1} and lost {2}.", { "0": va.name, "1": h2h.won, "2": h2h.lost })}
              </p>
              <ol className="rgl-results">
                {h2h.matches.map((m) => (
                  <WeekLine key={m.matchId} m={m} onTeam={onTeam} onOpen={() => onMatch(m.matchId)} />
                ))}
              </ol>
            </>
          )}
        </section>
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <h2>{t("Maps both have played")}</h2>
            <span className="muted">{t("won – lost")}</span>
          </div>
          {maps.length === 0 ? (
            <p className="hint">{t("No map in common yet.")}</p>
          ) : (
            <ul className="rgl-pool rgl-cmp-maps">
              <li className="rgl-pool-head" aria-hidden>
                <span>{t("Map")}</span>
                <span>{va.name}</span>
                <span>{vb.name}</span>
              </li>
              {maps.map((m) => (
                <li key={m.map}>
                  <span className="rgl-pool-map">{m.map}</span>
                  <span className="rgl-maprec-wl" title={t("{0} rounds won, {1} lost", { "0": m.a.roundsFor, "1": m.a.roundsAgainst })}>
                    <span className="team-won">{m.a.won}</span>–<span className="team-lost">{m.a.lost}</span>
                  </span>
                  <span className="rgl-maprec-wl" title={t("{0} rounds won, {1} lost", { "0": m.b.roundsFor, "1": m.b.roundsAgainst })}>
                    <span className="team-won">{m.b.won}</span>–<span className="team-lost">{m.b.lost}</span>
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </>
  );
}
