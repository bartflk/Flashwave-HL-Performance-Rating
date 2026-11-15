import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { SixesMatch, SixesSeason, SixesTeam } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { formatDate } from "../../lib/format";
import { useMapBanner } from "../../lib/mapbanner";
import { Loading } from "../Spinner";
import { DivisionBadge } from "../players/PlayerProfile";
import { TeamAvatar, useBanner } from "../teams/TeamsPage";
import { tierOf } from "../etf2l/Brackets";
import { Countdown, clockTime, fullDate } from "../etf2l/Countdown";
import { NewsCard, useNews } from "../etf2l/News";
import { RecruitPreview, type Kind } from "../etf2l/Recruitment";
import { firstWhen, fixturesFor, leaderOf, type When } from "../../lib/sixeshome";
import type { RecruitPost } from "../../api/types";

export type SixesGo = "season" | "matches" | "seasons" | "news" | "recruitment";

/**
 * The ETF2L tab's home in 6v6, as Highlander's (Flashy: "the etf2l 6's page
 * doesnt have a home, please match this with the highlander"): the season
 * across the top with its banner, the officials of today, tomorrow or the
 * week and the latest 6v6 news, and in the rail each division's leader, the
 * latest results and 6v6 recruitment.
 */
export function SixesHome(props: {
  s: SixesSeason;
  onTeam: (id: number) => void;
  onPost: (id: number) => void;
  onRecruit: (p: RecruitPost, kind: Kind) => void;
  go: (to: SixesGo) => void;
}) {
  const { s, onTeam, onPost, onRecruit, go } = props;
  const matchesQ = useQuery({ queryKey: ["sixes_matches"], queryFn: api.getSixesMatches, staleTime: 30 * 60_000, retry: false });
  const matches = matchesQ.data?.matches ?? [];
  const upcoming = matches.filter((m) => !m.played && m.time !== null).sort((a, b) => a.time! - b.time!);
  const played = matches.filter((m) => m.played).sort((a, b) => (b.time ?? 0) - (a.time ?? 0));
  const [picked, setWhen] = useState<When | null>(null);
  const when: When = picked ?? firstWhen(upcoming);
  const { shown, overdue } = fixturesFor(upcoming, when);
  return (
    <div className="e2-home">
      <SixesStrip s={s} matches={matches} upcoming={upcoming.length} go={go} />
      <div className="e2-main">
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <div className="e2-titles">
              <h2>{t("Officials")}</h2>
              <span className="muted">{s.name}</span>
            </div>
            <button className="e2-more e2-more-left" onClick={() => go("matches")}>
              {t("All matches")} ›
            </button>
            <div className="km-seg" role="radiogroup" aria-label={t("When")}>
              {(
                [
                  ["today", t("Today")],
                  ["tomorrow", t("Tomorrow")],
                  ["week", t("This week")],
                ] as Array<[When, string]>
              ).map(([id, label]) => (
                <button key={id} role="radio" aria-checked={when === id} className={when === id ? "on" : undefined} onClick={() => setWhen(id)}>
                  {label}
                </button>
              ))}
            </div>
          </div>
          {matchesQ.isPending ? (
            <Loading>{t("Loading the officials…")}</Loading>
          ) : shown.length === 0 ? (
            <p className="hint">{when === "today" ? t("No officials today.") : when === "tomorrow" ? t("No officials tomorrow.") : t("No officials in the next week.")}</p>
          ) : (
            <ol className="e2-fixtures">
              {shown.map((m) => (
                <Fixture key={m.id} m={m} onTeam={onTeam} showDay={when === "week"} />
              ))}
            </ol>
          )}
          {/* Due on an earlier day and not reported yet: kept apart from today's. */}
          {!matchesQ.isPending && when === "today" && overdue.length > 0 && (
            <details className="e2-overdue">
              <summary className="hint">{tx("{0} from earlier days not played yet", { "0": overdue.length })}</summary>
              <ol className="e2-fixtures">
                {overdue.slice(0, 10).map((m) => (
                  <Fixture key={m.id} m={m} onTeam={onTeam} showDay />
                ))}
              </ol>
            </details>
          )}
        </section>
        <LatestNews onPost={onPost} onAll={() => go("news")} />
      </div>
      <aside className="e2-rail">
        <Leaders s={s} avatars={avatarsOf(matches)} onTeam={onTeam} onAll={() => go("season")} />
        <Results rows={played.slice(0, 8)} waiting={matchesQ.isPending} onTeam={onTeam} onAll={() => go("matches")} />
        <RecruitPreview format="sixes" onAll={() => go("recruitment")} onOpen={onRecruit} />
      </aside>
    </div>
  );
}

/** The season across the top: ETF2L's banner for it, else one of its maps. */
function SixesStrip({ s, matches, upcoming, go }: { s: SixesSeason; matches: SixesMatch[]; upcoming: number; go: (to: SixesGo) => void }) {
  const league = useQuery({ queryKey: ["leagues", "sixes", null], queryFn: () => api.getLeagues(undefined, "sixes"), staleTime: 5 * 60_000 }).data;
  const number = league?.season?.season ?? null;
  const etf2l = useBanner({ season: number ?? 0, seasonName: s.name }, true, "sixes");
  const fromMaps = useMapBanner(s.pool, number ?? s.name.length, etf2l === null);
  const banner = number !== null ? (etf2l ?? fromMaps) : fromMaps;
  const times = matches.map((m) => m.time).filter((x): x is number => x !== null);
  const from = times.length ? Math.min(...times) : null;
  const to = times.length ? Math.max(...times) : null;
  const live = upcoming > 0;
  const teams = s.divisions.reduce((n, d) => n + d.teams.length, 0);
  return (
    <section className={`e2-strip ${banner ? "" : "e2-strip-plain"}`} style={banner ? { backgroundImage: `url("${banner}")` } : undefined}>
      <div className="e2-strip-text">
        <span className="e2-strip-eyebrow">
          {t("ETF2L 6v6")}
          {live && <span className="e2-live-chip">{t("Live")}</span>}
        </span>
        <h1>{s.name}</h1>
        {from !== null && to !== null && (
          <span className="e2-strip-dates">
            {formatDate(from, true)} – {formatDate(to, true)}
          </span>
        )}
        <div className="e2-strip-stats">
          <span>
            <strong>{teams}</strong>
            <em>{t("teams")}</em>
          </span>
          <span>
            <strong>{matches.filter((m) => m.played).length}</strong>
            <em>{t("officials played")}</em>
          </span>
          {live && (
            <span>
              <strong>{upcoming}</strong>
              <em>{t("scheduled")}</em>
            </span>
          )}
          <span>
            <strong>{s.pool.length}</strong>
            <em>{t("maps in the pool")}</em>
          </span>
        </div>
        <span className="e2-strip-divs">
          {s.divisions.map((d) => (
            <DivisionBadge key={d.name} d={{ name: d.name, tier: Math.floor(tierOf(d.name)) }} />
          ))}
        </span>
      </div>
      <div className="e2-strip-actions">
        <button className="km-chip" onClick={() => go("season")}>
          {t("Standings & fixtures")} ›
        </button>
        <button className="km-chip" onClick={() => go("seasons")}>
          {t("All seasons")} ›
        </button>
      </div>
    </section>
  );
}

function TeamButton({ x, onTeam }: { x: SixesTeam; onTeam: (id: number) => void }) {
  return (
    <button
      className="e2-team"
      onClick={(e) => {
        e.stopPropagation();
        onTeam(x.id);
      }}
      title={t("Open {0}", { "0": x.name })}
    >
      <TeamAvatar src={x.avatar} />
      <span>{x.name}</span>
    </button>
  );
}

function Fixture({ m, onTeam, showDay }: { m: SixesMatch; onTeam: (id: number) => void; showDay: boolean }) {
  return (
    <li className="e2-fx with-round sx-home-fx">
      <span className="e2-fx-time" title={fullDate(m.time!)}>
        {showDay && <span className="muted">{formatDate(m.time!)} </span>}
        {clockTime(m.time!)}
      </span>
      <Countdown time={m.time!} />
      <span className="e2-fx-div">{m.division && <DivisionBadge d={{ name: m.division, tier: m.tier ?? Math.floor(tierOf(m.division)) }} />}</span>
      <span className="e2-fx-teams">
        <TeamButton x={m.clan1} onTeam={onTeam} />
        <span className="e2-vs">{t("vs")}</span>
        <TeamButton x={m.clan2} onTeam={onTeam} />
      </span>
      <span className="e2-fx-round muted">{m.week !== null ? t("Week {0}", { "0": m.week }) : (m.round ?? "")}</span>
      {/* The week's maps one under the other, beside the week (Flashy: "put
          the maps in a column, 1 below each other next to the week"). */}
      <span className="sx-home-maps muted">
        {m.maps.map((x) => (
          <span key={x} title={x}>
            {shortMap(x)}
          </span>
        ))}
      </span>
    </li>
  );
}

/** "cp_granary_pro" -> "granary": the map without its mode or version, for a narrow column. */
function shortMap(map: string): string {
  return map.replace(/^(cp|koth|pl|ctf|plr|arena|tc)_/i, "").split("_")[0];
}

function LatestNews({ onPost, onAll }: { onPost: (id: number) => void; onAll: () => void }) {
  const q = useNews("sixes");
  const items = (q.data ?? []).slice(0, 5);
  if (q.isPending) return <Loading>{t("Loading the news…")}</Loading>;
  if (items.length === 0) return null;
  return (
    <section className="e2-block">
      <div className="e2-block-head">
        <h2 className="ts-section">{t("Latest news")}</h2>
        <button className="e2-more" onClick={onAll}>
          {t("All news")} ›
        </button>
      </div>
      <div className="np-grid np-grid-home">
        {items.map((n, i) => (
          <NewsCard key={n.id} n={n} lead={i === 0} onOpen={() => onPost(n.id)} />
        ))}
      </div>
    </section>
  );
}

/** Each division's leader, as Highlander's rail: the top of its table. */
/** Each team's avatar from the season's matches: the tables carry none. */
function avatarsOf(matches: SixesMatch[]): Map<number, string | null> {
  const m = new Map<number, string | null>();
  for (const x of matches) {
    m.set(x.clan1.id, x.clan1.avatar);
    m.set(x.clan2.id, x.clan2.avatar);
  }
  return m;
}

function Leaders({ s, avatars, onTeam, onAll }: { s: SixesSeason; avatars: Map<number, string | null>; onTeam: (id: number) => void; onAll: () => void }) {
  const rows = s.divisions.filter((d) => d.teams.length > 0);
  if (rows.length === 0) return null;
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Division leaders")}</h2>
        <button className="e2-more" onClick={onAll}>
          {t("Tables")} ›
        </button>
      </div>
      <ul className="e2-leaders">
        {rows.map((d) => {
          const top = leaderOf(d.teams);
          if (!top) return null;
          return (
            <li key={d.name}>
              <DivisionBadge d={{ name: d.name, tier: Math.floor(tierOf(d.name)) }} />
              <TeamButton x={{ id: top.id, name: top.name, avatar: avatars.get(top.id) ?? null }} onTeam={onTeam} />
              <span className="e2-leader-wl" title={t("Leading so far")}>
                {tx("{0} pts", { "0": top.score })}
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function Results({ rows, waiting, onTeam, onAll }: { rows: SixesMatch[]; waiting: boolean; onTeam: (id: number) => void; onAll: () => void }) {
  if (waiting) {
    return (
      <section className="panel e2-panel" aria-busy>
        <div className="e2-panel-head">
          <h2>{t("Recent results")}</h2>
        </div>
        <Loading>{t("Loading results…")}</Loading>
      </section>
    );
  }
  if (rows.length === 0) return null;
  const side = (x: SixesTeam, score: number | null, won: boolean) => (
    <button
      className={won ? "e2-res-team won" : "e2-res-team"}
      onClick={(e) => {
        e.stopPropagation();
        onTeam(x.id);
      }}
      title={t("Open {0}", { "0": x.name })}
    >
      <span className="e2-res-name">
        <TeamAvatar src={x.avatar} />
        <span>{x.name}</span>
      </span>
      <span className="e2-res-score">{score ?? "–"}</span>
    </button>
  );
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Recent results")}</h2>
        <button className="e2-more" onClick={onAll}>
          {t("All results")} ›
        </button>
      </div>
      <ol className="e2-results">
        {rows.map((m) => {
          const a = m.r1 ?? 0;
          const b = m.r2 ?? 0;
          return (
            <li key={m.id} title={m.time ? fullDate(m.time) : undefined} className="e2-res-row">
              {side(m.clan1, m.r1, a > b)}
              {side(m.clan2, m.r2, b > a)}
              <span className="e2-res-meta">
                {m.division && <DivisionBadge d={{ name: m.division, tier: m.tier ?? Math.floor(tierOf(m.division)) }} />}
                <span>{m.defaultWin ? t("default") : (m.week !== null ? t("Week {0}", { "0": m.week }) : m.round) ?? ""}</span>
                {m.time !== null && <span className="e2-res-when">{formatDate(m.time)}</span>}
              </span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}
