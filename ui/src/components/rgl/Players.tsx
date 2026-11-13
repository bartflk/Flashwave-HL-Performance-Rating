import { useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglFormMatch, RglMedal, RglPlayer } from "../../api/types";
import { t } from "../../lib/i18n";
import { openOnKey } from "../../lib/rows";
import { byYear, career, formLine, medalCounts, resultFor } from "../../lib/rglplayer";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { RglMark } from "../RglMark";
import { fullDate } from "../etf2l/Countdown";
import { MedalGlyph } from "../players/MedalGlyph";
import { PlayerStatsCard } from "../players/PlayerStats";
import { KEEP, SEARCH_FROM, useNameSearch, usePlayer, day, looksLikeSteamId, secs } from "./data";
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

// ---- Players ---------------------------------------------------------------

// Transfers (every roster move of the live season) are left out here on
// purpose: they need each of the season's teams' rosters, about a hundred
// team reads this tab does not make now, and the first view would ask RGL for
// all of them. A team's own page lists its joins and leaves instead.
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

/** The medals RGL's final ranks and the player's stints add up to, and the
 *  results of their newest teams: one read of RGL, kept on disk by the
 *  backend, so this asks once a view and says so while it waits. */
const useMore = (steamId: string) => useQuery({ queryKey: ["rgl_player_more", steamId], queryFn: () => api.getRglPlayerMore(steamId), staleTime: KEEP, retry: false });

/** A SteamID64 as the account id the app's own tables use. */
const accountOf = (steamId: string): number | null => {
  try {
    const n = BigInt(steamId) - 76561197960265728n;
    return n > 0n && n < 4294967296n ? Number(n) : null;
  } catch {
    return null;
  }
};

const PLACE_LABEL: Record<number, string> = { 1: "Champion", 2: "Runner-up", 3: "Third" };

/** How many of each medal they won, beside the name. */
function MedalBadges({ medals }: { medals: RglMedal[] }) {
  const counts = medalCounts(medals);
  return (
    <>
      {counts.map(
        (n, i) =>
          n > 0 && (
            <span key={i} className="badge rgl-medal" title={t("{0} × {1}", { "0": n, "1": t(PLACE_LABEL[i + 1]) })}>
              <MedalGlyph size={15} place={i + 1} />
              {n}
            </span>
          ),
      )}
    </>
  );
}

function Kpi({ label, value, sub, title }: { label: string; value: ReactNode; sub?: ReactNode; title?: string }) {
  return (
    <div className="kpi" title={title}>
      <span className="kpi-label">{label}</span>
      <span className="kpi-value">{value}</span>
      {sub && <span className="kpi-sub">{sub}</span>}
    </div>
  );
}

/** The headline cards: what their RGL seasons add up to. */
function Headline({ p, more }: { p: RglPlayer; more: ReturnType<typeof useMore> }) {
  const c = career(p.seasons, more.data?.medals ?? []);
  const span = c.firstYear === null || c.lastYear === null ? "–" : c.firstYear === c.lastYear ? `${c.firstYear}` : `${c.firstYear}–${c.lastYear}`;
  const years = c.firstYear === null || c.lastYear === null ? null : c.lastYear - c.firstYear + 1;
  return (
    <section className="kpis rgl-kpis">
      <Kpi
        label={t("Titles · podiums")}
        value={more.data ? `${c.titles} · ${c.podiums}` : more.error ? "–" : "…"}
        sub={more.data ? t("With a team they ended the season on") : more.error ? t("Not read") : t("Reading their teams…")}
        title={t("Seasons won, and seasons finished in the top three, with a team they were still on when the season ended")}
      />
      <Kpi label={t("Seasons · teams")} value={`${c.seasons} · ${c.teams}`} sub={t("NA Highlander")} />
      <Kpi label={t("Matches")} value={`${c.wins}-${c.losses}`} sub={c.winPct === null ? t("None played") : t("{0}% won", { "0": c.winPct.toFixed(0) })} title={t("Matches won and lost with their teams, across their RGL seasons")} />
      <Kpi label={t("Career")} value={span} sub={years === null ? undefined : years === 1 ? t("1 year") : t("{0} years", { "0": years })} />
      <Kpi
        label={t("Highest division")}
        value={c.highest?.division ?? "–"}
        sub={c.highest?.tier ? t("≈ {0} on ETF2L", { "0": t(c.highest.tier) }) : undefined}
        title={t("The best division they played in, with where RGL's division sits on ETF2L's ladder")}
      />
    </section>
  );
}

/** Their teams' newest results while they were on the roster. */
function Results({ more, onTeam, onMatch }: { more: ReturnType<typeof useMore>; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const form = more.data?.form ?? [];
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Recent results")}</h2>
        {form.length > 0 && (
          <span className="rgl-form" title={t("Their newest results, the latest on the right")}>
            {formLine(form).map((r, i) => (
              <i key={i} className={r === "W" ? "w" : r === "L" ? "l" : ""}>
                {r ?? "–"}
              </i>
            ))}
          </span>
        )}
      </div>
      {more.error ? (
        <ApiError error={more.error} />
      ) : more.isPending ? (
        <Loading>{t("Reading their teams from RGL… about 30 seconds the first time, then kept. You can look at another page meanwhile.")}</Loading>
      ) : form.length === 0 ? (
        <p className="hint">{t("No decided matches while they were on a roster, in their newest three seasons.")}</p>
      ) : (
        <>
          <ul className="rgl-results">
            {form.map((f) => (
              <ResultLine key={f.match.matchId} f={f} onTeam={onTeam} onOpen={() => onMatch(f.match.matchId)} />
            ))}
          </ul>
          <p className="hint">{t("Matches their teams played from the day they joined until the day they left, in their newest three seasons: RGL lists rosters, not who played.")}</p>
        </>
      )}
    </section>
  );
}

function ResultLine({ f, onTeam, onOpen }: { f: RglFormMatch; onTeam: (id: number) => void; onOpen: () => void }) {
  const m = f.match;
  const mine = m.teams.find((x) => x.teamId === f.teamId);
  const other = m.teams.find((x) => x.teamId !== f.teamId);
  // The score as their team saw it: map rounds won and lost.
  const homeMine = mine?.isHome ?? true;
  const a = m.maps.reduce((n, x) => n + ((homeMine ? x.homeScore : x.awayScore) ?? 0), 0);
  const b = m.maps.reduce((n, x) => n + ((homeMine ? x.awayScore : x.homeScore) ?? 0), 0);
  const r = resultFor(f);
  return (
    <li className="rgl-pr rgl-open" role="link" tabIndex={0} onClick={onOpen} onKeyDown={openOnKey(onOpen)} title={t("Open the match")}>
      <i className={`rgl-sq ${r === "W" ? "w" : r === "L" ? "l" : ""}`}>{r ?? "–"}</i>
      <span className="muted rgl-pr-day" title={m.matchDate ? fullDate(secs(m.matchDate)) : undefined}>
        {day(m.matchDate)}
      </span>
      <span className="rgl-pr-vs">
        <span className="muted">{t("vs")}</span>{" "}
        {other ? (
          <button
            className="linkish"
            onClick={(e) => {
              e.stopPropagation();
              onTeam(other.teamId);
            }}
          >
            {other.teamName}
          </button>
        ) : (
          "–"
        )}
      </span>
      <span className="rgl-pr-score">{m.isForfeit ? t("forfeit") : m.maps.length ? `${a}:${b}` : "–"}</span>
      <span className="muted rgl-pr-week">{m.matchName ?? ""}</span>
    </li>
  );
}

/** Each year's teams with division and record, opening the team. */
function Timeline({ p, medals, onTeam }: { p: RglPlayer; medals: RglMedal[]; onTeam: (id: number) => void }) {
  const rows = byYear(p.seasons);
  const placed = new Map(medals.map((m) => [m.teamId, m.place]));
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Career by year")}</h2>
      </div>
      {rows.length === 0 ? (
        <p className="hint">{t("No NA Highlander seasons.")}</p>
      ) : (
        <div className="rgl-years">
          {rows.map((y) => (
            <div key={y.year ?? "none"} className="rgl-year">
              <div className="rgl-year-head">
                <strong>{y.year ?? t("Undated")}</strong>
                <span className="muted" title={t("Matches won and lost that year")}>
                  {y.wins}-{y.losses}
                </span>
              </div>
              <ul>
                {y.stints.map((s) => (
                  <li key={`${s.teamId}-${s.startedAt}`}>
                    <span className="muted rgl-yr-season">{s.seasonName}</span>
                    <span className="rgl-yr-team">
                      <button className="linkish" onClick={() => onTeam(s.teamId)}>
                        {s.teamName}
                      </button>
                      {placed.has(s.teamId) && <MedalGlyph size={14} place={placed.get(s.teamId)} />}
                    </span>
                    <span className="muted rgl-yr-div">
                      {s.division ?? "–"}
                      {s.tier && ` ≈ ${t(s.tier)}`}
                    </span>
                    <span className="num">
                      {s.wins}-{s.losses}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function Cabinet({ more, onTeam }: { more: ReturnType<typeof useMore>; onTeam: (id: number) => void }) {
  const medals = [...(more.data?.medals ?? [])].sort((a, b) => b.seasonId - a.seasonId);
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Trophy cabinet")}</h2>
      </div>
      {more.error ? (
        <ApiError error={more.error} />
      ) : more.isPending ? (
        <Loading>{t("Reading their teams from RGL…")}</Loading>
      ) : medals.length === 0 ? (
        <p className="hint">{t("No podium finishes with a team they ended the season on.")}</p>
      ) : (
        <ul className="rgl-cabinet">
          {medals.map((m) => (
            <li key={m.teamId}>
              <MedalGlyph size={20} place={m.place} />
              <span className="rgl-cab-what">
                <button className="linkish" onClick={() => onTeam(m.teamId)}>
                  {m.teamName}
                </button>
                <span className="muted">
                  {m.seasonName}
                  {m.division && ` · ${m.division}`} · {t(PLACE_LABEL[m.place])}
                </span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** The app's own rating of them, when it has their games: the same card
 *  the app's player page shows, from the logs it holds (no request). */
function AppRating({ steamId }: { steamId: string }) {
  const id = accountOf(steamId);
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("In the app")}</h2>
      </div>
      {id === null ? <p className="hint">{t("The app has no games of theirs.")}</p> : <PlayerStatsCard accountId={id} />}
      <p className="hint">{t("The app's own rating, from the logs it holds of their games: their own matches and the league sample.")}</p>
    </section>
  );
}

export function PlayerView({ steamId, onTeam, onMatch }: { steamId: string; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const q = usePlayer(steamId);
  const p: RglPlayer | null | undefined = q.data;
  const more = useMore(steamId);
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
              {more.data && <MedalBadges medals={more.data.medals} />}
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
      <Headline p={p} more={more} />
      <div className="rgl-cols">
        <div className="rgl-col">
          <Results more={more} onTeam={onTeam} onMatch={onMatch} />
          <Timeline p={p} medals={more.data?.medals ?? []} onTeam={onTeam} />
        </div>
        <div className="rgl-col">
          <AppRating steamId={p.steamId} />
          <Cabinet more={more} onTeam={onTeam} />
        </div>
      </div>
    </>
  );
}
