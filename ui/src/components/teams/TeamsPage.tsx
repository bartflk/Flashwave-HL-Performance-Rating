import { useEffect, useMemo, useRef, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type Fixture, type LeagueRecord, type Podium, type SeasonTile, type Stay, type TeamEtf2l, type TeamHonours, type TeamInfo, type TeamTransfers, type TeamView } from "../../api/types";
import { formatDate, formatMonth, formatStay, rating } from "../../lib/format";
import { onOpenSeason, onOpenTeam, openPlayer, takePendingSeason, takePendingTeam } from "../../lib/goto";
import { BookmarkButton } from "../Bookmarks";
import { locale, t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";
import { Country } from "../Country";
import { classLabel } from "../analysis/common";
import { MedalGlyph } from "../players/MedalGlyph";
import { DivisionBadge, medalTitles, seasonLong, seasonShort } from "../players/PlayerProfile";
import "../players/players.css";
import "./teams.css";
import { YouOnThisTeam, YourTeamsStrip } from "./YourTeams";

/**
 * Teams (Q29, and Flashy's UX passes): every ETF2L Highlander season as a
 * tile -- its banner from ETF2L's news, who won it, how you did -- then a
 * season's podiums and division tables, then a team's own page, built like
 * a player's profile, with its ETF2L description and awards.
 */

type View = { kind: "seasons" } | { kind: "season"; season: number } | { kind: "team"; teamId: number; from: View };

/** One step back within Teams, as the ‹ Back button does; `null` on the grid. */
function stepBack(v: View): View | null {
  if (v.kind === "team") return v.from;
  if (v.kind === "season") return { kind: "seasons" };
  return null;
}

const PLACE = ["gold", "silver", "bronze"] as const;

export function TeamsPage({ active }: { active: boolean }) {
  // A team asked for from elsewhere (a match header) opens straight on it.
  const [view, setView] = useState<View>(() => {
    const pending = takePendingTeam();
    if (pending !== null) return { kind: "team", teamId: pending, from: { kind: "seasons" } };
    // Or a season, from a bookmark (Q60).
    const season = takePendingSeason();
    return season === null ? { kind: "seasons" } : { kind: "season", season };
  });
  const openTeam = (teamId: number) => setView((from) => ({ kind: "team", teamId, from: from.kind === "team" ? from.from : from }));
  useEffect(() => onOpenTeam((teamId) => setView({ kind: "team", teamId, from: { kind: "seasons" } })), []);
  useEffect(() => onOpenSeason((season) => setView({ kind: "season", season })), []);
  // Mouse 4 / Alt+Left step back within Teams (team → season → grid) before the
  // app's own history sees them: capture phase on window runs first, and
  // stopPropagation keeps the app from also going back a tab. On the grid, or
  // with Teams hidden, they are left alone.
  const viewRef = useRef(view);
  const activeRef = useRef(active);
  useEffect(() => {
    viewRef.current = view;
    activeRef.current = active;
  });
  useEffect(() => {
    const target = () => (activeRef.current ? stepBack(viewRef.current) : null);
    const onMouse = (e: MouseEvent) => {
      if (e.button !== 3) return; // 3 is the mouse's back button
      const to = target();
      if (!to) return;
      e.preventDefault();
      e.stopPropagation();
      // Same rule as navigation.ts: act on the way up only.
      if (e.type === "mouseup") setView(to);
    };
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || e.key !== "ArrowLeft") return;
      const to = target();
      if (!to) return;
      e.preventDefault();
      e.stopPropagation();
      setView(to);
    };
    window.addEventListener("mousedown", onMouse, true);
    window.addEventListener("mouseup", onMouse, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("mousedown", onMouse, true);
      window.removeEventListener("mouseup", onMouse, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, []);
  let body;
  if (view.kind === "team") {
    body = <TeamScreen teamId={view.teamId} onBack={() => setView(stepBack(view) ?? { kind: "seasons" })} onTeam={openTeam} backLabel={view.from.kind === "season" ? t("Season") : t("All seasons")} />;
  } else if (view.kind === "season") {
    body = <SeasonScreen season={view.season} onBack={() => setView(stepBack(view) ?? { kind: "seasons" })} onTeam={openTeam} />;
  } else {
    body = <SeasonsGrid onSeason={(season) => setView({ kind: "season", season })} onTeam={openTeam} />;
  }
  return <div className="content teams-page">{body}</div>;
}

function BackButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button className="ts-back" onClick={onClick}>
      <span aria-hidden>‹</span> {label}
    </button>
  );
}

// ---- 1. Every season as a tile ----------------------------------------

/** Every upcoming Highlander official (Q48), read from ETF2L at most every half hour. */
function useFixtures() {
  return useQuery({ queryKey: ["fixtures"], queryFn: api.getFixtures, staleTime: 10 * 60_000 }).data ?? [];
}

/** When a match is: "Sun 4 Oct, 20:15". */
function kickoff(unix: number): string {
  return new Date(unix * 1000).toLocaleString(locale(), { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
}

/** One upcoming match. `side` puts that team on the left and reads the record from its side. */
function FixtureCard({ f, side, onTeam }: { f: Fixture; side?: number; onTeam: (id: number) => void }) {
  const flip = side !== undefined && f.clan2.id === side;
  const [a, b] = flip ? [f.clan2, f.clan1] : [f.clan1, f.clan2];
  const r = flip ? { ...f.h2h, won: f.h2h.lost, lost: f.h2h.won } : f.h2h;
  const team = (x: Fixture["clan1"]) => (
    <button className="ts-fx-team" onClick={() => onTeam(x.id)} title={t("Open {0}", { "0": x.name })}>
      <TeamAvatar src={x.avatar} />
      <span>{x.name}</span>
    </button>
  );
  return (
    <article className="ts-fx">
      <header className="ts-fx-head">
        <span className="ts-fx-when">{kickoff(f.time)}</span>
        <span className="ts-fx-what">
          {f.division && f.tier !== null && <DivisionBadge d={{ name: f.division, tier: f.tier }} />}
          {f.round && <span className="muted">{f.round}</span>}
        </span>
      </header>
      <div className="ts-fx-teams">
        {team(a)}
        <span className="ts-fx-vs">{t("vs")}</span>
        {team(b)}
      </div>
      <footer className="ts-fx-foot">
        <span>
          {r.played === 0 ? (
            <span className="muted">{t("First meeting in an official")}</span>
          ) : (
            <>
              <span className="muted">{t("Head-to-head")}</span> <strong>{wl(r)}</strong>
            </>
          )}
        </span>
        <span className="muted">{f.maps.length > 0 ? f.maps.join(" · ") : t("Maps still to be picked")}</span>
      </footer>
    </article>
  );
}

function SeasonsGrid({ onSeason, onTeam }: { onSeason: (season: number) => void; onTeam: (id: number) => void }) {
  const q = useQuery({ queryKey: ["seasons_overview"], queryFn: api.getSeasonsOverview, staleTime: 5 * 60_000 });
  const fixtures = useFixtures();
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">{t("ETF2L Highlander")}</span>
        <h1>{t("Seasons")}</h1>
        <p className="hint">{t("Pick a season for its podiums, its divisions and every team in them.")}</p>
      </header>
      <YourTeamsStrip onTeam={onTeam} />
      {fixtures.length > 0 && (
        <>
          <h2 className="ts-section">{tx("Coming up ({0})", { "0": fixtures.length })}</h2>
          <section className="ts-fixtures">
            {fixtures.slice(0, 6).map((f) => (
              <FixtureCard key={f.matchId} f={f} onTeam={onTeam} />
            ))}
          </section>
        </>
      )}
      {q.isPending && <p className="hint">{t("Loading seasons…")}</p>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data && q.data.length === 0 && (
        <section className="panel">
          <p className="hint">{t("No seasons yet. The next sync reads the last year of ETF2L Highlander: every division, team and result.")}</p>
        </section>
      )}
      {q.data && q.data.length > 0 && (
        <div className="ts-grid">
          {q.data.map((s, i) => (
            <SeasonCard key={s.season} s={s} live={i === 0 && !s.champion} onOpen={() => onSeason(s.season)} />
          ))}
        </div>
      )}
    </>
  );
}

function seasonTitle(s: { season: number; seasonName: string }): string {
  if (s.season >= 100) return s.seasonName;
  return s.seasonName && !s.seasonName.startsWith("Season") ? t("Season {0} · {1}", { "0": s.season, "1": s.seasonName }) : t("Season {0}", { "0": s.season });
}

/** The season's banner, shrunk once by the app: small for a tile, large for the header. */
function useBanner(s: { season: number; seasonName: string }, large = false) {
  return useQuery({ queryKey: ["season_banner", s.season, large], queryFn: () => api.getSeasonBanner(s.season, s.seasonName, large), staleTime: Infinity }).data ?? null;
}

/** RED for even seasons, BLU for odd: the cards without a banner alternate. */
function teamTint(season: number): string {
  return season % 2 === 0 ? "ts-tint-red" : "ts-tint-blu";
}

function SeasonCard({ s, live, onOpen }: { s: SeasonTile; live: boolean; onOpen: () => void }) {
  const banner = useBanner(s);
  return (
    <button className={`ts-card ${banner ? "" : teamTint(s.season)}`} onClick={onOpen} aria-label={t("Open {0}", { "0": seasonTitle(s) })}>
      <span className="ts-art">
        <span className="ts-art-img" style={banner ? { backgroundImage: `url("${banner}")` } : undefined} aria-hidden />
        {!banner && <span className="ts-art-label">{s.season >= 100 ? s.seasonName : `S${s.season}`}</span>}
        <span className="ts-chips">
          <span className="ts-chip">{s.season >= 100 ? t("Off-season") : `S${s.season}`}</span>
          {live && <span className="ts-chip ts-live">{t("Live")}</span>}
        </span>
      </span>
      <span className="ts-body">
        <span className="ts-title">{seasonTitle(s)}</span>
        <span className="ts-dates">
          {formatDate(s.from)} – {formatDate(s.to)}
        </span>
        <span className="ts-stats">
          <span className="ts-stat">{tx("{0} teams", { "0": s.teams })}</span>
          <span className="ts-stat">{tx("{0} officials", { "0": s.matches })}</span>
        </span>
        <span className="ts-divs">
          {s.divisions.map((d) => (
            <DivisionBadge key={d.name} d={d} />
          ))}
        </span>
        <span className="ts-rows">
          <span className="ts-row ts-row-champ">
            <span className="ts-row-label">{t("Champion")}</span>
            {s.champion ? (
              <span className="ts-row-value">
                <MedalGlyph size={16} place={1} />
                <TeamAvatar src={s.champion.avatar} />
                <strong>{s.champion.name}</strong>
              </span>
            ) : (
              <span className="ts-row-value muted">{t("still being played")}</span>
            )}
          </span>
          {s.you && (
            <span className="ts-row ts-row-you">
              <span className="ts-row-label">{t("You")}</span>
              <span className="ts-row-value">
                {s.you.team.name} <DivisionBadge d={{ name: s.you.division, tier: s.you.tier ?? 4 }} />
                {s.you.place && <MedalGlyph size={16} place={s.you.place} />}
                {s.you.merc && <span className="muted">{t("merc")}</span>}
              </span>
            </span>
          )}
        </span>
        <span className="ts-open">
          {t("Open season")}
          <span className="ts-open-arrow" aria-hidden>
            ›
          </span>
        </span>
      </span>
    </button>
  );
}

// ---- 2. One season: podiums, then the division tables ----------------

function SeasonScreen({ season, onBack, onTeam }: { season: number; onBack: () => void; onTeam: (id: number) => void }) {
  const tiles = useQuery({ queryKey: ["seasons_overview"], queryFn: api.getSeasonsOverview, staleTime: 5 * 60_000 });
  const tile = tiles.data?.find((s) => s.season === season);
  const tables = useQuery({ queryKey: ["leagues", season], queryFn: () => api.getLeagues(season), placeholderData: keepPreviousData });
  const podiums = useQuery({ queryKey: ["season_podiums", season], queryFn: () => api.getSeasonPodiums(season), staleTime: 5 * 60_000 });
  const banner = useBanner(tile ?? { season, seasonName: "" }, true);
  const upcoming = useFixtures().filter((f) => f.season === season);
  const v = tables.data;
  return (
    <>
      <BackButton label={t("All seasons")} onClick={onBack} />
      <section className={`ts-hero ${banner ? "" : teamTint(season)}`} style={banner ? { backgroundImage: `url("${banner}")` } : undefined}>
        <div className="ts-hero-text">
          <span className="ts-eyebrow">{t("ETF2L Highlander")}</span>
          <h1>
            {tile ? seasonTitle(tile) : t("Season {0}", { "0": season })}{" "}
            <BookmarkButton b={{ kind: "season", id: season, label: tile ? seasonTitle(tile) : t("Season {0}", { "0": season }), sub: t("ETF2L Highlander") }} />
          </h1>
          {tile && (
            <p className="ts-meta">
              {formatDate(tile.from)} – {formatDate(tile.to)}
              <span className="ts-dot">•</span>
              {tx("{0} teams", { "0": tile.teams })}
              <span className="ts-dot">•</span>
              {tx("{0} officials", { "0": tile.matches })}
            </p>
          )}
          {v?.season && v.season.pool.length > 0 && (
            <p className="ts-pool">
              {v.season.pool.map((m) => (
                <code key={m}>{m}</code>
              ))}
            </p>
          )}
        </div>
      </section>

      {upcoming.length > 0 && (
        <>
          <h2 className="ts-section">{tx("Upcoming ({0})", { "0": upcoming.length })}</h2>
          <section className="ts-fixtures">
            {upcoming.map((f) => (
              <FixtureCard key={f.matchId} f={f} onTeam={onTeam} />
            ))}
          </section>
        </>
      )}

      <h2 className="ts-section">{t("Podiums")}</h2>
      {podiums.isPending && <p className="hint">{t("Loading podiums…")}</p>}
      {podiums.data && podiums.data.length === 0 && (
        <section className="panel ts-pending">
          <p className="hint">{t("No final has been played yet this season. The podiums appear here as each division finishes.")}</p>
        </section>
      )}
      {podiums.data && podiums.data.length > 0 && (
        <section className="ts-podiums">
          {podiums.data.map((p) => (
            <PodiumCard key={p.division} p={p} onTeam={onTeam} />
          ))}
        </section>
      )}

      <h2 className="ts-section">{t("Divisions")}</h2>
      {tables.isError && <p className="error">{errorMessage(tables.error)}</p>}
      {v && v.divisions.length > 0 && (
        <div className="teams-divisions">
          {v.divisions.map((d) => (
            <section key={d.division} className="panel ts-division">
              <h3>{d.division}</h3>
              <div className="table-wrap">
                <table className="match-table teams-table">
                  <thead>
                    <tr>
                      <th className="num">#</th>
                      <th>{t("Team")}</th>
                      <th className="num">{t("Played")}</th>
                      <th className="num">{t("W-L")}</th>
                      <th className="num">{t("Score")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {d.teams.map((row, i) => (
                      <tr key={row.teamId} className="teams-row" onClick={() => onTeam(row.teamId)} title={t("Open {0}", { "0": row.name })}>
                        <td className="num muted">{i + 1}</td>
                        <td>
                          <span className="teams-name">
                            <TeamAvatar src={row.avatar} />
                            <span>{row.name}</span>
                          </span>
                        </td>
                        <td className="num">{row.record.played}</td>
                        <td className="num nowrap">{wl(row.record)}</td>
                        <td className="num nowrap">
                          {row.scoreFor}:{row.scoreAgainst}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>
          ))}
        </div>
      )}
      {v && v.pendingDetails > 0 && <p className="hint">{t("{0} matches still to be read in detail; the per-map numbers fill in over the next syncs.", { "0": v.pendingDetails })}</p>}
    </>
  );
}

function PodiumCard({ p, onTeam }: { p: Podium; onTeam: (id: number) => void }) {
  return (
    <div className="panel ts-podium">
      <div className="ts-podium-head">
        <DivisionBadge d={{ name: p.division, tier: p.tier ?? 4 }} />
      </div>
      <ol className="ts-places">
        {p.medals.map((m, i) => (
          <li key={i} className={`pp-${PLACE[m.place - 1]}`} title={m.how}>
            <span className="pp-medal-tile ts-place-tile">
              <MedalGlyph size={20} place={m.place} />
            </span>
            <button className="ts-team" onClick={() => onTeam(m.team.id)}>
              <TeamAvatar src={m.team.avatar} />
              <span>{m.team.name}</span>
            </button>
          </li>
        ))}
      </ol>
      {p.mvp && (
        <button className="ts-mvp" onClick={() => openPlayer(p.mvp!.accountId)} title={t("Grand Final {0} over {1} logs", { "0": p.mvp.finalRating.toFixed(2), "1": p.mvp.finalMaps })}>
          <span className="pp-mvp-star" aria-hidden>
            ★
          </span>
          <span className="ts-mvp-label">{t("MVP")}</span>
          <ClassIcon cls={p.mvp.class} size={16} />
          <strong>{p.mvpName ?? "?"}</strong>
          <span className="ts-mvp-rating">{rating(p.mvp.score)}</span>
        </button>
      )}
    </div>
  );
}

// ---- 3. One team, like a player's profile ------------------------------

type TeamTab = "overview" | "lineup" | "seasons" | "history" | "results";

function TeamScreen({ teamId, onBack, onTeam, backLabel }: { teamId: number; onBack: () => void; onTeam: (id: number) => void; backLabel: string }) {
  const q = useQuery({ queryKey: ["team", teamId], queryFn: () => api.getTeam(teamId) });
  const h = useQuery({ queryKey: ["team_honours", teamId], queryFn: () => api.getTeamHonours(teamId) });
  const e = useQuery({ queryKey: ["team_etf2l", teamId], queryFn: () => api.getTeamEtf2l(teamId), staleTime: 60 * 60_000 });
  const info = useQuery({ queryKey: ["team_info", teamId], queryFn: () => api.getTeamInfo(teamId), staleTime: 60 * 60_000 });
  const roles = useMemo(() => new Map((info.data?.members ?? []).filter((m) => m.accountId !== null).map((m) => [m.accountId!, m.role])), [info.data]);
  const [tab, setTab] = useState<TeamTab>("overview");
  return (
    <>
      <BackButton label={backLabel} onClick={onBack} />
      {q.isPending && <p className="hint">{t("Loading team…")}</p>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data === null && (
        <>
          <p className="hint">{t("This team's seasons are older than the ones read, so only your own games with it are here.")}</p>
          <YouOnThisTeam teamId={teamId} named />
        </>
      )}
      {q.data && (
        <>
          <TeamHeader v={q.data} honours={h.data} info={info.data} />
          <YouOnThisTeam teamId={teamId} />
          <div className="panel pp-body">
            <nav className="pp-tabs" role="tablist">
              {(
                [
                  ["overview", t("Overview")],
                  ["lineup", tx("Lineup ({0})", { "0": q.data.roster.length })],
                  ["seasons", t("Seasons")],
                  ["history", t("Roster history")],
                  ["results", tx("Results ({0})", { "0": q.data.results.length })],
                ] as [TeamTab, React.ReactNode][]
              ).map(([id, label]) => (
                <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? "pp-tab on" : "pp-tab"} onClick={() => setTab(id)}>
                  {label}
                </button>
              ))}
            </nav>
            {tab === "overview" && <TeamOverview v={q.data} etf2l={e.data} roles={roles} onTeam={onTeam} onLineup={() => setTab("lineup")} onResults={() => setTab("results")} />}
            {tab === "lineup" && <Lineup v={q.data} roles={roles} />}
            {tab === "seasons" && <Seasons honours={h.data} etf2l={e.data} info={info.data} />}
            {tab === "history" && <RosterHistory teamId={teamId} etf2lUrl={e.data?.url} roles={roles} />}
            {tab === "results" && <Results v={q.data} onTeam={onTeam} />}
          </div>
        </>
      )}
    </>
  );
}

/** A member's ETF2L role as a chip; an ordinary member gets none. */
function RoleChip({ role }: { role: string | undefined }) {
  if (!role || role === "Member") return null;
  const label = role === "Leader" ? t("Leader") : role === "Deputy" ? t("Deputy") : role === "Inactive" ? t("Inactive") : role === "Buddy" ? t("Buddy") : role;
  return <span className={`ts-role ts-role-${role.toLowerCase()}`}>{label}</span>;
}

function TeamHeader({ v, honours, info }: { v: TeamView; honours: TeamHonours | undefined; info: TeamInfo | undefined }) {
  const latest = honours?.seasons[0];
  const titles = medalTitles(honours?.medals ?? []);
  const p = pct(v.record);
  return (
    <div className="panel pp-head ts-teamhead">
      <div className="pp-top">
        <div className="pp-avatar ts-teamavatar">{v.avatar ? <img src={v.avatar} alt="" /> : <span>{v.name.slice(0, 1).toUpperCase()}</span>}</div>
        <div className="pp-who">
          <div className="pp-name-row">
            <h2 className="pp-name">{v.name}</h2>
            {info?.tag && <span className="ts-tag">{info.tag}</span>}
            <BookmarkButton b={{ kind: "team", id: v.teamId, label: v.name, sub: latest ? `${seasonShort(latest.season)} · ${latest.division}` : null }} />
            {latest && latest.tier !== null && <DivisionBadge d={{ name: latest.division, tier: latest.tier }} />}
          </div>
          <dl className="pp-facts">
            {v.country && (
              <>
                <dt>{t("Country")}</dt>
                <dd>
                  <Country raw={v.country} />
                </dd>
              </>
            )}
            {latest && (
              <>
                <dt>{t("Latest season")}</dt>
                <dd>
                  {seasonLong(latest.season, latest.seasonName)} · {latest.division} · {latest.won}–{latest.lost}
                </dd>
              </>
            )}
            <dt>{t("Seasons")}</dt>
            <dd>{honours ? honours.seasons.length : v.seasons.length}</dd>
            {info && info.formerNames.length > 0 && (
              <>
                <dt>{t("Formerly")}</dt>
                <dd>
                  {info.formerNames.map((n, i) => (
                    <span key={i} className="ts-former" title={tx("Renamed {0}", { "0": formatDate(n.time, true) }) as string}>
                      {i > 0 && " · "}
                      {n.from} <span className="muted">({tx("until {0}", { "0": formatMonth(n.time) })})</span>
                    </span>
                  ))}
                </dd>
              </>
            )}
          </dl>
          {titles.length > 0 && (
            <div className="pp-titles">
              {titles.map(([label, n, place]) => (
                <span key={label} className={`pp-title pp-${PLACE[place - 1]}`}>
                  <MedalGlyph size={18} place={place} />
                  {n}× {label}
                </span>
              ))}
            </div>
          )}
        </div>
        <div className="ts-record">
          <span className="ts-record-label">{t("Officials")}</span>
          <strong>{wl(v.record)}</strong>
          {p !== null && (
            <span className="ts-winbar" title={t("{0}% won", { "0": p })}>
              <span style={{ width: `${p}%` }} />
            </span>
          )}
          <span className="hint">{p === null ? "" : t("{0}% won", { "0": p })}</span>
          <span className="ts-links">
            <a href={`https://etf2l.org/teams/${v.teamId}/`} target="_blank" rel="noreferrer" className="ts-link">
              {t("ETF2L ↗")}
            </a>
            {info?.homepage && (
              <a href={info.homepage} target="_blank" rel="noreferrer" className="ts-link">
                {t("Website ↗")}
              </a>
            )}
            {info?.steamGroup && (
              <a href={info.steamGroup} target="_blank" rel="noreferrer" className="ts-link">
                {t("Steam group ↗")}
              </a>
            )}
          </span>
        </div>
      </div>
      {honours && honours.medals.length > 0 && (
        <div className="pp-trophies" aria-label={t("Medals")}>
          {honours.medals.map((m, i) => (
            <span key={i} className={`pp-medal pp-${PLACE[m.place - 1]}`} title={`${m.division} · ${seasonLong(m.season, m.seasonName)} · ${m.how}`}>
              <span className="pp-medal-tile">
                <MedalGlyph size={38} place={m.place} />
              </span>
              <span className="pp-medal-label">
                {seasonShort(m.season, m.seasonName)} {m.division.replace("Division ", "Div ")}
              </span>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function TeamOverview({
  v,
  etf2l,
  roles,
  onTeam,
  onLineup,
  onResults,
}: {
  v: TeamView;
  etf2l: TeamEtf2l | undefined;
  roles: Map<number, string>;
  onTeam: (id: number) => void;
  onLineup: () => void;
  onResults: () => void;
}) {
  const newest = v.roster.flatMap((r) => r.seasons ?? []).reduce((a, s) => (a === null || s.season > a ? s.season : a), null as number | null);
  // The newest season's lineup; the whole roster where seasons are not known.
  const current = (
    newest !== null
      ? v.roster.map((r) => ({ r, s: (r.seasons ?? []).find((x) => x.season === newest) }))
      : v.roster.map((r) => ({ r, s: { season: 0, officials: r.matches, class: r.class, rating: r.rating } }))
  )
    .filter((x) => x.s && x.s.officials > 0)
    .sort((a, b) => b.s!.officials - a.s!.officials || (b.s!.rating ?? 0) - (a.s!.rating ?? 0))
    .slice(0, 9);
  const maps = v.maps.filter((m) => m.inPool);
  const next = useFixtures().filter((f) => f.clan1.id === v.teamId || f.clan2.id === v.teamId);
  return (
    <div className="pp-overview">
      <section>
        {next.length > 0 && (
          <>
            <h3>{next.length === 1 ? t("Next match") : t("Next matches")}</h3>
            <div className="ts-fixtures ts-fixtures-one">
              {next.map((f) => (
                <FixtureCard key={f.matchId} f={f} side={v.teamId} onTeam={onTeam} />
              ))}
            </div>
          </>
        )}
        {etf2l?.description && (
          <>
            <h3>{t("About")}</h3>
            <blockquote className="ts-about">{etf2l.description}</blockquote>
          </>
        )}
        <h3>{t("Recent officials")}</h3>
        <ul className="pp-matches">
          {v.results.slice(0, 8).map((r) => {
            const won = r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor > r.scoreAgainst;
            const lost = r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor < r.scoreAgainst;
            return (
              <li key={r.matchId} className={won ? "won" : lost ? "lost" : undefined}>
                <span className="pp-vs">
                  {t("vs")}{" "}
                  <button className="linkish" onClick={() => onTeam(r.opponentId)}>
                    <strong>{r.opponent}</strong>
                  </button>
                </span>
                <span className="muted pp-where">
                  {seasonShort(r.season)} {r.division}
                  {r.stage !== "regular" ? ` · ${r.round ?? r.stage}` : ""}
                  {r.time ? ` · ${formatDate(r.time)}` : ""}
                  {r.maps[0] ? ` · ${r.maps[0]}` : ""}
                </span>
                <span className="pp-score">{r.defaultWin ? t("default") : `${r.scoreFor ?? "–"} : ${r.scoreAgainst ?? "–"}`}</span>
              </li>
            );
          })}
        </ul>
        {v.results.length > 8 && (
          <button className="ts-more" onClick={onResults}>
            {tx("All {0} results", { "0": v.results.length })} ›
          </button>
        )}
      </section>
      <section>
        <h3>{newest !== null ? tx("Lineup, {0}", { "0": seasonShort(newest) }) : t("Lineup")}</h3>
        {current.length === 0 && <p className="hint">{t("None of this team's officials have been downloaded yet, so there are no classes or ratings to show.")}</p>}
        <ul className="ts-lineup">
          {current.map(({ r, s }) => (
            <li key={r.accountId} className={roles.get(r.accountId) === "Inactive" ? "ts-inactive" : undefined}>
              {s!.class ? <ClassIcon cls={s!.class} size={20} /> : <span />}
              <span className="ts-lineup-who">
                <button className="linkish ts-lineup-name" onClick={() => openPlayer(r.accountId)}>
                  {r.name}
                </button>
                <RoleChip role={roles.get(r.accountId)} />
              </span>
              <span className="muted">{tx("{0} officials", { "0": s!.officials })}</span>
              <span className="ts-lineup-rating">{s!.rating !== null ? rating(s!.rating) : "–"}</span>
            </li>
          ))}
        </ul>
        <button className="ts-more" onClick={onLineup}>
          {tx("Every season's lineup ({0} players)", { "0": v.roster.length })} ›
        </button>
        {maps.length > 0 && (
          <>
            <h3 className="ts-maps-title">{t("Current map pool")}</h3>
            <MapBars maps={maps} />
          </>
        )}
      </section>
    </div>
  );
}

function MapBars({ maps }: { maps: TeamView["maps"] }) {
  return (
    <div className="team-maps">
      {maps.map((m) => {
        const p = pct(m.record);
        return (
          <div key={m.map} className={m.inPool ? "team-map" : "team-map muted"}>
            <div className="team-map-head">
              <code>{m.map}</code>
              <span className="num">{wl(m.record)}</span>
            </div>
            <div className="team-map-bar" title={t("{0}% of {1} maps won", { "0": p ?? 0, "1": m.record.played })}>
              <div className="team-map-fill" style={{ width: `${p ?? 0}%` }} />
            </div>
            <span className="hint">
              {p === null ? "–" : t("{0}% won", { "0": p })}
              {m.roundsFor + m.roundsAgainst > 0 && t(" · rounds {0}:{1}", { "0": m.roundsFor, "1": m.roundsAgainst })}
            </span>
          </div>
        );
      })}
    </div>
  );
}

type Sort = "officials" | "rating" | "name";

/** Every player who played for the team: all seasons, or one, sorted. */
function Lineup({ v, roles }: { v: TeamView; roles: Map<number, string> }) {
  const seasons = useMemo(() => {
    const set = new Set<number>();
    for (const r of v.roster) for (const s of r.seasons ?? []) set.add(s.season);
    return [...set].sort((a, b) => order(b) - order(a));
  }, [v]);
  const [season, setSeason] = useState<number | null>(null);
  const [sort, setSort] = useState<Sort>("officials");
  const rows = v.roster
    .map((r) => {
      if (season === null) return { r, officials: r.matches, cls: r.class, rating: r.rating };
      const s = (r.seasons ?? []).find((x) => x.season === season);
      return s ? { r, officials: s.officials, cls: s.class, rating: s.rating } : null;
    })
    .filter((x): x is NonNullable<typeof x> => x !== null && x.officials > 0)
    .sort((a, b) =>
      sort === "name" ? a.r.name.localeCompare(b.r.name) : sort === "rating" ? (b.rating ?? -1) - (a.rating ?? -1) || b.officials - a.officials : b.officials - a.officials || (b.rating ?? -1) - (a.rating ?? -1),
    );
  const best = Math.max(1.4, ...rows.map((x) => x.rating ?? 0));
  return (
    <>
      <div className="ts-filters">
        <div className="ts-seasons" role="tablist" aria-label={t("Season")}>
          <button role="tab" aria-selected={season === null} className={season === null ? "ts-pill on" : "ts-pill"} onClick={() => setSeason(null)}>
            {t("All seasons")}
          </button>
          {seasons.map((s) => (
            <button key={s} role="tab" aria-selected={season === s} className={season === s ? "ts-pill on" : "ts-pill"} onClick={() => setSeason(s)}>
              {seasonShort(s, s >= 100 ? "AFA" : undefined)}
            </button>
          ))}
        </div>
        <div className="segmented" role="tablist" aria-label={t("Sort by")}>
          {(
            [
              ["officials", t("Officials")],
              ["rating", t("Rating")],
              ["name", t("Name")],
            ] as [Sort, string][]
          ).map(([id, label]) => (
            <button key={id} role="tab" aria-selected={sort === id} className={sort === id ? "seg active" : "seg"} onClick={() => setSort(id)}>
              {label}
            </button>
          ))}
        </div>
      </div>
      <p className="hint">{t("Class: what they played for this team in its officials. Rating: their average on it in those officials.")}</p>
      <div className="table-wrap">
        <table className="match-table ts-lineup-table">
          <thead>
            <tr>
              <th>{t("Player")}</th>
              <th>{t("Class")}</th>
              <th className="num">{t("Officials")}</th>
              <th className="num">{t("Last played")}</th>
              <th>{t("Rating")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(({ r, officials, cls, rating: score }) => (
              <tr key={r.accountId} className="teams-row" onClick={() => openPlayer(r.accountId)} title={t("Open {0}'s profile", { "0": r.name })}>
                <td className="nowrap">
                  <strong>{r.name}</strong> <RoleChip role={roles.get(r.accountId)} />
                </td>
                <td className="nowrap">
                  {cls ? (
                    <span className="ts-class">
                      <ClassIcon cls={cls} size={16} /> {classLabel(cls)}
                    </span>
                  ) : (
                    <span className="muted">–</span>
                  )}
                </td>
                <td className="num">{officials}</td>
                <td className="num muted nowrap">{r.lastPlayed ? formatDate(r.lastPlayed) : "–"}</td>
                <td className="ts-rating-cell">
                  {score !== null ? (
                    <>
                      <span className="ts-rating-bar" aria-hidden>
                        <span style={{ width: `${Math.min(100, (score / best) * 100)}%` }} />
                      </span>
                      <span className="num">{rating(score)}</span>
                    </>
                  ) : (
                    <span className="muted">–</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <h3 className="ts-maps-title">{t("By map")}</h3>
      <p className="hint">{t("Maps won and lost in officials. Maps of the current pool first.")}</p>
      <MapBars maps={[...v.maps.filter((m) => m.inPool), ...v.maps.filter((m) => !m.inPool)]} />
    </>
  );
}

// ---- Roster history, from ETF2L's transfers (Q48) ------------------------

function stayLength(s: Stay, now: number): number {
  return (s.to ?? now) - (s.from ?? s.to ?? now);
}

function RosterHistory({ teamId, etf2lUrl, roles }: { teamId: number; etf2lUrl: string | undefined; roles: Map<number, string> }) {
  const q = useQuery({ queryKey: ["team_transfers", teamId], queryFn: () => api.getTeamTransfers(teamId), staleTime: 60 * 60_000 });
  if (q.isPending) return <p className="hint">{t("Reading the team's transfers from ETF2L…")}</p>;
  if (q.isError) return <p className="error">{errorMessage(q.error)}</p>;
  const v: TeamTransfers = q.data;
  if (v.rows.length === 0) return <p className="hint">{t("ETF2L lists no transfers for this team.")}</p>;
  return <RosterHistoryView v={v} etf2lUrl={etf2lUrl} roles={roles} />;
}

function RosterHistoryView({ v, etf2lUrl, roles }: { v: TeamTransfers; etf2lUrl: string | undefined; roles: Map<number, string> }) {
  const now = Math.floor(Date.now() / 1000);
  const [showAll, setShowAll] = useState(false);
  const current = v.stays.filter((s) => s.to === null);
  const past = v.stays.filter((s) => s.to !== null);
  const longest = Math.max(1, ...v.stays.map((s) => stayLength(s, now)));
  const shownPast = showAll ? past : past.slice(0, 12);
  // Joins and leaves by year, newest first.
  const years = useMemo(() => {
    const out: [number, TeamTransfers["rows"]][] = [];
    for (const r of v.rows) {
      const y = new Date(r.time * 1000).getFullYear();
      const last = out[out.length - 1];
      if (last && last[0] === y) last[1].push(r);
      else out.push([y, [r]]);
    }
    return out;
  }, [v.rows]);
  const stayRow = (s: Stay, i: number) => (
    <li key={`${s.accountId ?? s.name}-${s.from ?? i}`} className={s.to === null ? "ts-stay on" : "ts-stay"}>
      <span className="ts-stay-who">
        {s.accountId !== null ? (
          <button className="linkish ts-stay-name" onClick={() => openPlayer(s.accountId!)}>
            {s.name}
          </button>
        ) : (
          <span className="ts-stay-name">{s.name}</span>
        )}
        {s.to === null && s.accountId !== null && <RoleChip role={roles.get(s.accountId)} />}
      </span>
      <span className="ts-stay-dates muted">
        {s.from !== null ? formatMonth(s.from) : t("before the records")} – {s.to !== null ? formatMonth(s.to) : t("now")}
      </span>
      <span className="ts-stay-bar" aria-hidden>
        <span style={{ width: `${Math.max(3, (stayLength(s, now) / longest) * 100)}%` }} />
      </span>
      <span className="ts-stay-length">{formatStay(stayLength(s, now))}</span>
    </li>
  );
  return (
    <div className="ts-history">
      <section>
        <h3>{tx("On the roster now ({0})", { "0": current.length })}</h3>
        <ul className="ts-stays">{current.map(stayRow)}</ul>
        {past.length > 0 && (
          <>
            <h3>{tx("Who came and went ({0})", { "0": past.length })}</h3>
            <p className="hint">{t("Longest stay first.")}</p>
            <ul className="ts-stays">{shownPast.map(stayRow)}</ul>
            {past.length > shownPast.length && (
              <button className="ts-more" onClick={() => setShowAll(true)}>
                {tx("Show all {0}", { "0": past.length })} ›
              </button>
            )}
          </>
        )}
      </section>
      <section>
        <h3>{tx("Joins and leaves ({0})", { "0": v.rows.length })}</h3>
        <div className="ts-moves">
          {years.map(([year, rows]) => (
            <div key={year} className="ts-moves-year">
              <h4>{year}</h4>
              <ul>
                {rows.map((r, i) => (
                  <li key={`${r.time}-${i}`} className={r.joined ? "ts-move in" : "ts-move out"}>
                    <span className="ts-move-mark" aria-label={r.joined ? t("joined the team") : t("left the team")}>
                      {r.joined ? "+" : "−"}
                    </span>
                    <span className="ts-move-date muted">{formatDate(r.time, true)}</span>
                    <span className="ts-move-who">
                      {r.accountId !== null ? (
                        <button className="linkish" onClick={() => openPlayer(r.accountId!)}>
                          {r.name}
                        </button>
                      ) : (
                        r.name
                      )}{" "}
                      <span className="muted">{r.joined ? t("joined the team") : t("left the team")}</span>
                      {r.by && <span className="muted"> · {r.joined ? tx("added by {0}", { "0": r.by }) : tx("removed by {0}", { "0": r.by })}</span>}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
        <p className="hint">
          {t("From ETF2L's transfer list.")}
          {v.fetchedAt !== null && <> {tx("Read {0}", { "0": formatDate(v.fetchedAt, true) })}.</>}
          {etf2lUrl && (
            <>
              {" "}
              <a href={etf2lUrl} target="_blank" rel="noreferrer">
                {t("ETF2L ↗")}
              </a>
            </>
          )}
        </p>
      </section>
    </div>
  );
}

function order(season: number): number {
  return season >= 100 ? (season - 100) * 2 + 1 : season * 2;
}

function Seasons({ honours, etf2l, info }: { honours: TeamHonours | undefined; etf2l: TeamEtf2l | undefined; info: TeamInfo | undefined }) {
  if (!honours) return <p className="hint">{t("Loading…")}</p>;
  return (
    <div className="ts-seasons-tab">
      <table className="pp-seasons">
        <thead>
          <tr>
            <th>{t("Season")}</th>
            <th>{t("Division")}</th>
            <th className="num">{t("W–L")}</th>
            <th>{t("Finish")}</th>
          </tr>
        </thead>
        <tbody>
          {honours.seasons.map((s) => (
            <tr key={s.season}>
              <td>{s.season >= 100 ? s.seasonName : <>S{s.season} <span className="muted">{s.seasonName}</span></>}</td>
              <td>{s.tier !== null ? <DivisionBadge d={{ name: s.division, tier: s.tier }} /> : s.division}</td>
              <td className="num">
                {s.won}–{s.lost}
              </td>
              <td>
                {s.place ? (
                  <span className={`pp-place pp-${PLACE[s.place - 1]}`}>
                    <MedalGlyph size={16} place={s.place} /> {s.place === 1 ? t("Winner") : s.place === 2 ? t("Runner-up") : t("Third")}
                  </span>
                ) : (
                  <span className="muted">–</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {((info && info.cups.length > 0) || (etf2l && etf2l.awards.length > 0)) && (
        <aside className="ts-awards">
          {info && info.cups.length > 0 && (
            <>
              <h3>{tx("Cups ({0})", { "0": info.cups.length })}</h3>
              <p className="hint">{t("Every Highlander cup the team entered, from ETF2L; the placing where ETF2L awarded one.")}</p>
              <ul className="ts-cups">
                {info.cups.map((c) => {
                  const place = c.place?.startsWith("1") ? 1 : c.place?.startsWith("2") ? 2 : c.place?.startsWith("3") ? 3 : null;
                  return (
                    <li key={c.competitionId} className={place ? `pp-${PLACE[place - 1]}` : undefined}>
                      {place ? <MedalGlyph size={18} place={place} /> : <span className="ts-cup-dot" aria-hidden />}
                      <span>
                        {c.name}
                        {c.division && (
                          <>
                            {" "}
                            {c.tier !== null ? <DivisionBadge d={{ name: c.division, tier: c.tier }} /> : <span className="muted">{c.division}</span>}
                          </>
                        )}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </>
          )}
          {etf2l && etf2l.awards.length > 0 && (
            <>
          <h3>{t("ETF2L awards")}</h3>
          <p className="hint">{t("As ETF2L's team page lists them, cups included.")}</p>
          <ul>
            {etf2l.awards.map((a, i) => {
              const place = a.place.startsWith("1") ? 1 : a.place.startsWith("2") ? 2 : a.place.startsWith("3") ? 3 : null;
              return (
                <li key={i} className={place ? `pp-${PLACE[place - 1]}` : undefined}>
                  {place ? <MedalGlyph size={18} place={place} /> : <span className="muted">{a.place}</span>}
                  <span>{a.competition}</span>
                </li>
              );
            })}
          </ul>
            </>
          )}
        </aside>
      )}
    </div>
  );
}

/** Each opponent's record against this team, from its officials. */
function opponents(v: TeamView): { id: number; name: string; r: LeagueRecord; last: number | null }[] {
  const by = new Map<number, { id: number; name: string; r: LeagueRecord; last: number | null }>();
  for (const x of v.results) {
    const o = by.get(x.opponentId) ?? { id: x.opponentId, name: x.opponent, r: { played: 0, won: 0, lost: 0, drawn: 0 }, last: null };
    if (!x.defaultWin && x.scoreFor !== null && x.scoreAgainst !== null && x.scoreFor + x.scoreAgainst > 0) {
      o.r.played += 1;
      if (x.scoreFor > x.scoreAgainst) o.r.won += 1;
      else if (x.scoreFor < x.scoreAgainst) o.r.lost += 1;
      else o.r.drawn += 1;
    }
    if (x.time !== null && (o.last === null || x.time > o.last)) o.last = x.time;
    by.set(x.opponentId, o);
  }
  return [...by.values()].filter((o) => o.r.played > 0).sort((a, b) => b.r.played - a.r.played || (b.last ?? 0) - (a.last ?? 0));
}

function Results({ v, onTeam }: { v: TeamView; onTeam: (id: number) => void }) {
  const [against, setAgainst] = useState<number | null>(null);
  const rivals = useMemo(() => opponents(v), [v]);
  const shown = against === null ? v.results : v.results.filter((r) => r.opponentId === against);
  return (
    <>
      {rivals.some((o) => o.r.played > 1) && (
        <section className="ts-h2h">
          <h3>{t("Head-to-head")}</h3>
          <p className="hint">{t("Every team met in more than one official, most met first. Pick one for just those matches.")}</p>
          <div className="ts-h2h-list">
            {rivals
              .filter((o) => o.r.played > 1)
              .slice(0, 12)
              .map((o) => (
                <button key={o.id} className={against === o.id ? "ts-h2h-row on" : "ts-h2h-row"} onClick={() => setAgainst(against === o.id ? null : o.id)}>
                  <span className="ts-h2h-name">{o.name}</span>
                  <span className="ts-h2h-bar" aria-hidden>
                    <span className="won" style={{ flex: o.r.won }} />
                    <span className="drawn" style={{ flex: o.r.drawn }} />
                    <span className="lost" style={{ flex: o.r.lost }} />
                  </span>
                  <strong>{wl(o.r)}</strong>
                </button>
              ))}
          </div>
          {against !== null && (
            <p className="hint">
              {tx("Only the matches against {0}.", { "0": rivals.find((o) => o.id === against)?.name ?? "" })}{" "}
              <button className="linkish" onClick={() => setAgainst(null)}>
                {t("Show all")}
              </button>{" "}
              ·{" "}
              <button className="linkish" onClick={() => onTeam(against)}>
                {t("Open their page")}
              </button>
            </p>
          )}
        </section>
      )}
      <ResultsTable rows={shown} onTeam={onTeam} />
    </>
  );
}

function ResultsTable({ rows, onTeam }: { rows: TeamView["results"]; onTeam: (id: number) => void }) {
  return (
    <div className="table-wrap">
      <table className="match-table">
        <thead>
          <tr>
            <th>{t("Date")}</th>
            <th>{t("Season")}</th>
            <th>{t("Against")}</th>
            <th>{t("Map")}</th>
            <th className="num">{t("Score")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => {
            const won = r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor > r.scoreAgainst;
            const lost = r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor < r.scoreAgainst;
            return (
              <tr key={r.matchId}>
                <td className="nowrap muted">{r.time ? formatDate(r.time) : "–"}</td>
                <td className="nowrap muted">
                  {seasonShort(r.season)} {r.division}
                </td>
                <td>
                  <button className="linkish" onClick={() => onTeam(r.opponentId)}>
                    {r.opponent}
                  </button>
                  {r.stage !== "regular" && <span className="badge">{r.round ?? r.stage}</span>}
                </td>
                <td className="nowrap">{r.maps[0] ?? "–"}</td>
                <td className={won ? "num team-won" : lost ? "num team-lost" : "num"}>{r.defaultWin ? t("default") : `${r.scoreFor ?? "–"}:${r.scoreAgainst ?? "–"}`}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function TeamAvatar({ src }: { src: string | null }) {
  return src ? <img src={src} alt="" className="teams-avatar" /> : <span className="teams-avatar" />;
}

function wl(r: LeagueRecord): string {
  return r.drawn > 0 ? `${r.won}-${r.lost}-${r.drawn}` : `${r.won}-${r.lost}`;
}

function pct(r: LeagueRecord): number | null {
  return r.played > 0 ? Math.round((100 * r.won) / r.played) : null;
}
