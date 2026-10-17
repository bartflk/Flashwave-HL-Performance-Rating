import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type CatDivision, type Etf2lRoster, type Medal, type Mvp, type PlayerProfile as Profile } from "../../api/types";
import { formatDate, formatMonth, formatStay } from "../../lib/format";
import { openLeagueMatch } from "../../lib/goto";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";
import { locale, t, tx } from "../../lib/i18n";
import { PlayerStatsCard, RankChip } from "./PlayerStats";
import { MedalGlyph } from "./MedalGlyph";
import { Country } from "../Country";
import { BookmarkButton } from "../Bookmarks";
import { LivesOnMapTab } from "./LivesOnMap";
import { Etf2lMark } from "../Etf2lMark";

/**
 * A player's profile, HLTV-style (Q35, Flashy; PLAN §26): who they are,
 * their teams season by season, their divisions and medals, their
 * officials -- from the ETF2L officials the league job downloaded and who
 * played them, and ETF2L's own page for the country and declared classes.
 * The medals are worked out from playoff finals and final tables; ETF2L
 * has no awards to fetch.
 */

type Tab = "overview" | "teams" | "achievements" | "yours" | "lives";

const PLACE = ["gold", "silver", "bronze"] as const;

export function PlayerProfile({ accountId, yours }: { accountId: number; yours: React.ReactNode }) {
  const q = useQuery({ queryKey: ["player_profile", accountId], queryFn: () => api.getPlayerProfile(accountId) });
  const [tab, setTab] = useState<Tab>("overview");
  if (q.isPending) return <div className="panel"><p className="hint">{t("Loading…")}</p></div>;
  if (q.isError) return <div className="panel"><p className="error">{errorMessage(q.error)}</p></div>;
  const p = q.data;
  return (
    <>
      <PlayerHeader p={p} />
      <div className="panel pp-body">
        <nav className="pp-tabs" role="tablist">
          {(
            [
              ["overview", t("Overview")],
              ["teams", t("Teams")],
              ["achievements", tx("Achievements ({0})", { "0": p.medals.length + (p.mvps?.length ?? 0) })],
              ["yours", t("In your matches")],
              ["lives", t("Lives on a map")],
            ] as [Tab, React.ReactNode][]
          ).map(([id, label]) => (
            <button key={id} role="tab" aria-selected={tab === id} className={tab === id ? "pp-tab on" : "pp-tab"} onClick={() => setTab(id)}>
              {label}
            </button>
          ))}
        </nav>
        {tab === "overview" && <Overview p={p} />}
        {tab === "teams" && (
          <>
            <TeamStays accountId={p.accountId} />
            <SeasonsTable p={p} />
          </>
        )}
        {tab === "achievements" && <Achievements medals={p.medals} mvps={p.mvps ?? []} />}
        {tab === "yours" && yours}
        {tab === "lives" && <LivesOnMapTab accountId={p.accountId} name={p.name} />}
      </div>
    </>
  );
}

/**
 * Who they are: picture, name, division, team, medals. `aside` takes the
 * right-hand column (your own profile puts your numbers there, above the
 * links), and `own` dresses it as yours: the main class behind the name.
 */
export function PlayerHeader({ p, aside, own }: { p: Profile; aside?: React.ReactNode; own?: boolean }) {
  const titles = medalTitles(p.medals);
  const played = p.mainClass;
  const declared = p.declaredClasses.map((c) => c.toLowerCase()).filter((c) => c !== played);
  // Every name they went by, as its own pill (Flashy: "the names need to
  // have a different background so its easier to differentiate").
  const [allAliases, setAllAliases] = useState(false);
  const shownAliases = allAliases ? p.aliases : p.aliases.slice(0, 8);
  // When the profile opened: a ban that runs out while it is open can wait.
  const [openedAt] = useState(() => Date.now() / 1000);
  const banned = p.bans.find((b) => b.end === null || b.end > openedAt);
  return (
    <div className={own ? "panel pp-head pp-own" : "panel pp-head"}>
      {own && played && (
        <span className="pp-watermark" aria-hidden>
          <ClassIcon cls={played} size={240} />
        </span>
      )}
      <div className="pp-top">
        <div className="pp-avatar">{p.avatar ? <img src={p.avatar} alt="" /> : <span>{p.name.slice(0, 1).toUpperCase()}</span>}</div>
        <div className="pp-who">
          <div className="pp-name-row">
            <h2 className="pp-name">{p.name}</h2>
            <BookmarkButton b={{ kind: "player", id: p.accountId, label: p.name, sub: p.highest?.name ?? null }} />
            {p.highest && <DivisionBadge d={p.highest} />}
            {p.etf2lTitle && p.etf2lTitle !== "Player" && <span className="pp-role">{p.etf2lTitle}</span>}
            {banned && (
              <span className="pp-banned" title={banned.reason ?? undefined}>
                {banned.end ? t("Banned until {0}", { "0": formatDate(banned.end, true) }) : t("Banned")}
              </span>
            )}
          </div>
          {shownAliases.length > 0 && (
            <div className="pp-aka">
              <span className="pp-aka-label">{t("Also known as")}</span>
              {shownAliases.map((a, i) => (
                <span key={`${a}-${i}`} className={`pp-alias pp-alias-${i % 5}`}>
                  {a}
                </span>
              ))}
              {p.aliases.length > shownAliases.length && (
                <button className="pp-alias pp-alias-more" onClick={() => setAllAliases(true)}>
                  {t("+{0} more", { "0": p.aliases.length - shownAliases.length })}
                </button>
              )}
            </div>
          )}
          <div className="pp-meta">
            {p.country && (
              <span className="pp-chip" title={t("Country")}>
                <Country raw={p.country} />
              </span>
            )}
            <span className="pp-chip" title={t("Most played in the logs held: your matches and the league sample")}>
              {played ? (
                <>
                  <ClassIcon cls={played} size={16} /> {classLabel(played)}
                </>
              ) : (
                <span className="muted">{t("Main class")} –</span>
              )}
              {declared.length > 0 && (
                <span className="muted" title={t("Signed up as on ETF2L")}>
                  {" "}· {t("signed up as")} {declared.map((c) => classLabel(c)).join(", ")}
                </span>
              )}
            </span>
            {p.current ? (
              <span className="pp-chip pp-chip-team" title={t("Current team")}>
                <TeamAvatar src={p.current.team.avatar} />
                <strong>{p.current.team.name}</strong>
                <span className="muted">
                  {p.current.division} · {seasonShort(p.current.season, p.current.seasonName)}
                </span>
              </span>
            ) : (
              <span className="pp-chip muted">{t("No ETF2L official in the seasons read")}</span>
            )}
            <span className="pp-chip" title={t("Officials")}>
              <Etf2lMark size={13} />
              {tx("{0} in {1} season{2}", { "0": p.officials.length, "1": new Set(p.seasons.map((s) => s.season)).size, "2": new Set(p.seasons.map((s) => s.season)).size === 1 ? "" : "s" })}
              {p.registered !== null && <span className="muted"> · {t("on ETF2L since {0}", { "0": monthYear(p.registered) })}</span>}
            </span>
          </div>
          {p.etf2lTeams.length > 0 && (
            <div className="pp-rosters-row">
              <span className="pp-rosters-label">{t("ETF2L rosters")}</span>
              {p.etf2lTeams.map((r) => (
                <RosterChip key={r.id} r={r} />
              ))}
            </div>
          )}
          <HeaderRanks accountId={p.accountId} />
          {(titles.length > 0 || mvpTitles(p.mvps ?? []).length > 0) && (
            <div className="pp-titles">
              {titles.map(([label, n, place]) => (
                <span key={label} className={`pp-title pp-${PLACE[place - 1]}`}>
                  <MedalGlyph size={18} place={place} />
                  {n}× {label}
                </span>
              ))}
              {mvpTitles(p.mvps ?? []).map(([label, n]) => (
                <span key={label} className="pp-title pp-mvp">
                  <span className="pp-mvp-star" aria-hidden>★</span>
                  {n}× {label}
                </span>
              ))}
            </div>
          )}
        </div>
        {aside && <div className="pp-side">{aside}</div>}
      </div>
      <div className="pp-foot">
        {p.medals.length + (p.mvps?.length ?? 0) > 0 ? (
          <div className="pp-trophies" aria-label={t("Medals")}>
            {p.medals.map((m, i) => (
              <MedalIcon key={i} m={m} />
            ))}
            {(p.mvps ?? []).map((m, i) => (
              <MvpIcon key={`mvp-${i}`} m={m} />
            ))}
          </div>
        ) : (
          <span />
        )}
        <ProfileLinks p={p} />
      </div>
    </div>
  );
}

function ProfileLinks({ p }: { p: Profile }) {
  const id = p.steamid64;
  const links: Array<[string, string, boolean]> = [
    ...(p.etf2lId !== null ? [["ETF2L", `https://etf2l.org/forum/user/${p.etf2lId}/`, true] as [string, string, boolean]] : []),
    ["trends.tf", `https://trends.tf/player/${id}/`, false],
    ["logs.tf", `https://logs.tf/profile/${id}`, false],
    ["demos.tf", `https://demos.tf/profiles/${id}`, false],
    ["RGL", `https://rgl.gg/Public/PlayerProfile?p=${id}`, false],
    ["UGC", `https://www.ugcleague.com/players_page.cfm?player_id=${id}`, false],
    ["Steam", `https://steamcommunity.com/profiles/${id}`, false],
  ];
  return (
    <nav className="pp-links" aria-label={t("Elsewhere")}>
      {links.map(([label, href, etf2l]) => (
        <a key={label} href={href} target="_blank" rel="noreferrer">
          {etf2l && <Etf2lMark size={12} />}
          {label} <span aria-hidden>↗</span>
        </a>
      ))}
    </nav>
  );
}

/** "[9S] The 9 Stooges · hl fun", linking to the team on ETF2L. */
export function RosterChip({ r }: { r: Etf2lRoster }) {
  const kind = r.kind?.replace("Highlander", "HL").replace(" Team", "").toLowerCase();
  return (
    <a className="pp-roster" href={`https://etf2l.org/teams/${r.id}/`} target="_blank" rel="noreferrer" title={[r.name, r.kind, r.country].filter(Boolean).join(" · ")}>
      <TeamAvatar src={r.avatar} />
      {r.tag && <span className="pp-roster-tag">{r.tag}</span>}
      {r.name}
      {kind && <span className="muted"> · {kind}</span>}
    </a>
  );
}

/** "Jun 2014". */
function monthYear(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString(locale(), { month: "short", year: "numeric" });
}

/** The newest season's ranks, best first, beside the name like HLTV's Top 20. */
function HeaderRanks({ accountId }: { accountId: number }) {
  const q = useQuery({ queryKey: ["player_stats", accountId], queryFn: () => api.getPlayerStats(accountId) });
  const ranks = q.data?.ranks ?? [];
  if (ranks.length === 0) return null;
  const newest = ranks[0].season;
  return (
    <div className="pp-ranks">
      {ranks
        .filter((r) => r.season === newest)
        .slice(0, 3)
        .map((r) => (
          <RankChip key={r.class} r={r} accountId={accountId} />
        ))}
    </div>
  );
}

/** "Low winner" x2, "High runner-up" x1, ... best first. */
export function medalTitles(medals: Medal[]): [string, number, number][] {
  const count = new Map<string, [number, number, number]>();
  for (const m of medals) {
    const what = m.place === 1 ? t("{0} winner", { "0": m.division }) : m.place === 2 ? t("{0} runner-up", { "0": m.division }) : t("{0} third", { "0": m.division });
    const e = count.get(what) ?? [0, m.place, m.tier ?? 9];
    e[0] += 1;
    count.set(what, e);
  }
  return [...count.entries()]
    .sort((a, b) => a[1][2] - b[1][2] || a[1][1] - b[1][1])
    .map(([label, [n, place]]) => [label, n, place]);
}

/** "Open Sniper MVP" x1, "Low MVP" x1: the event's first, then by class. */
function mvpTitles(mvps: Mvp[]): [string, number][] {
  const count = new Map<string, number>();
  for (const m of [...mvps].sort((a, b) => Number(b.event) - Number(a.event))) {
    const what = m.event ? t("{0} MVP", { "0": m.division }) : t("{0} {1} MVP", { "0": m.division, "1": classLabel(m.class) });
    count.set(what, (count.get(what) ?? 0) + 1);
  }
  return [...count.entries()];
}

function mvpHow(m: Mvp): string {
  const final = t("Grand Final {0} over {1} logs", { "0": m.finalRating.toFixed(2), "1": m.finalMaps });
  return m.playoffsRating === null ? final : `${final} · ${t("playoffs {0} over {1}", { "0": m.playoffsRating.toFixed(2), "1": m.playoffsMaps })}`;
}

function MvpIcon({ m }: { m: Mvp }) {
  const what = m.event ? t("Event MVP") : t("{0} MVP", { "0": classLabel(m.class) });
  return (
    <span className="pp-medal pp-mvp" title={`${what} · ${m.division} · ${seasonLong(m.season, m.seasonName)} · ${m.team.name} · ${mvpHow(m)}`}>
      <span className="pp-medal-tile pp-mvp-tile">
        <span className="pp-mvp-star" aria-hidden>★</span>
        {!m.event && <ClassIcon cls={m.class} size={18} />}
      </span>
      <span className="pp-medal-label">
        {seasonShort(m.season, m.seasonName)} {shortDivision(m.division)} {m.event ? "MVP" : ""}
      </span>
    </span>
  );
}

function MedalIcon({ m }: { m: Medal }) {
  const kind = PLACE[m.place - 1];
  return (
    <span className={`pp-medal pp-${kind}`} title={`${m.place === 1 ? t("Winner") : m.place === 2 ? t("Runner-up") : t("Third")} · ${m.division} · ${seasonLong(m.season, m.seasonName)} · ${m.team.name} · ${m.how}`}>
      <span className="pp-medal-tile">
        <MedalGlyph size={38} place={m.place} />
      </span>
      <span className="pp-medal-label">
        {seasonShort(m.season, m.seasonName)} {shortDivision(m.division)}
      </span>
    </span>
  );
}

/** "S34", or a season between two numbered ones by its name ("AFA 2025"). */
export function seasonShort(season: number, name?: string): string {
  return season >= 100 ? (name ?? "AFA") : `S${season}`;
}

/** "Season 34 (Summer 2025)", or just "AFA 2025". */
export function seasonLong(season: number, name: string): string {
  return season >= 100 ? name : `${t("Season {0}", { "0": season })} (${name})`;
}

function shortDivision(d: string): string {
  return d === "Premiership" ? "Prem" : d.replace("Division ", "Div ");
}

export function DivisionBadge({ d }: { d: CatDivision }) {
  return (
    <span className={`div-badge div-t${Math.min(d.tier, 4)}`} title={t("Highest division with three or more officials played")}>
      {shortDivision(d.name)}
    </span>
  );
}

export function TeamAvatar({ src }: { src: string | null }) {
  return src ? <img className="pp-team-avatar" src={src} alt="" /> : <span className="pp-team-avatar" />;
}

function Overview({ p }: { p: Profile }) {
  return (
    <div className="pp-overview">
      <section>
        <h3>{t("Recent officials")}</h3>
        <RecentOfficials p={p} />
      </section>
      <section>
        <h3>{t("Rating")}</h3>
        <PlayerStatsCard accountId={p.accountId} />
        <h3 className="pp-career-title">{t("ETF2L officials on trends.tf")}</h3>
        <TrendsCareer accountId={p.accountId} />
      </section>
    </div>
  );
}

/** The newest officials, with the score. */
export function RecentOfficials({ p, count = 10 }: { p: Profile; count?: number }) {
  const recent = p.officials.slice(0, count);
  return (
    <>
        {recent.length === 0 && <p className="hint">{t("No ETF2L official in the seasons read.")}</p>}
        <ul className="pp-matches">
          {recent.map((o) => (
            <li
              key={o.matchId}
              className={`pp-open${o.won === true ? " won" : o.won === false ? " lost" : ""}`}
              role="link"
              tabIndex={0}
              onClick={() => openLeagueMatch(o.matchId)}
              onKeyDown={(e) => e.key === "Enter" && openLeagueMatch(o.matchId)}
              title={t("Open the match and its logs")}
            >
              <span className="pp-vs">
                <TeamAvatar src={o.team.avatar} /> {o.team.name} <span className="muted">{t("vs")}</span> <TeamAvatar src={o.opponent.avatar} /> <strong>{o.opponent.name}</strong>
              </span>
              <span className="muted pp-where">
                {o.division && o.tier !== null && <DivisionBadge d={{ name: o.division, tier: o.tier }} />}{" "}
                {o.stage === "Cup" ? o.competition.replace(/:.*$/, "") : seasonShort(o.season)}
                {o.division && o.tier === null ? ` ${o.division}` : ""}
                {o.stage !== "regular" && o.stage !== "Cup" ? ` · ${o.round ?? o.stage}` : o.stage === "Cup" && o.round ? ` · ${o.round}` : ""}
                {o.time ? ` · ${formatDate(o.time, true)}` : ""}
              </span>
              <span className="pp-score">
                {o.scoreFor ?? "–"} : {o.scoreAgainst ?? "–"}
              </span>
              <span className="pp-go" aria-hidden>
                ›
              </span>
            </li>
          ))}
        </ul>
    </>
  );
}

/**
 * What only trends.tf knows (Q37): every Highlander game they played, not
 * only the ones held here. Read when the profile opens, kept a day.
 */
export function TrendsCareer({ accountId }: { accountId: number }) {
  const q = useQuery({ queryKey: ["trends_career", accountId], queryFn: () => api.getTrendsCareer(accountId), staleTime: 60 * 60_000 });
  if (q.isPending) return <p className="hint">{t("Reading trends.tf…")}</p>;
  if (q.isError) return <p className="hint">{errorMessage(q.error)}</p>;
  const v = q.data;
  const c = v.career;
  const hours = (s: number) => Math.round(s / 3600);
  if (!c || (c.wins + c.losses + c.ties === 0 && c.classes.length === 0)) {
    return <p className="hint">{v.error ?? t("trends.tf has no ETF2L Highlander officials for them.")}</p>;
  }
  return (
    <div className="pp-career">
      <p className="pp-career-head">
        <strong>
          {c.wins}–{c.losses}–{c.ties}
        </strong>{" "}
        {c.winrate !== null && <span>· {tx("{0}% won", { "0": c.winrate.toFixed(1) })}</span>}{" "}
        <span className="muted">· {tx("{0} h in officials", { "0": hours(c.timeS) })}</span>
      </p>
      <table className="pp-career-classes">
        <thead>
          <tr>
            <th>{t("Class")}</th>
            <th className="num">{t("W–L–T")}</th>
            <th className="num">{t("Won")}</th>
            <th className="num" title={t("Damage per minute")}>{t("DPM")}</th>
            <th className="num" title={t("Accuracy")}>{t("Acc")}</th>
            <th className="num">{t("Hours")}</th>
          </tr>
        </thead>
        <tbody>
          {c.classes
            .filter((x) => x.timeS >= 600)
            .slice(0, 6)
            .map((x) => (
              <tr key={x.class}>
                <td>
                  <ClassIcon cls={x.class} size={16} /> {classLabel(x.class)}
                </td>
                <td className="num">{x.wins + x.losses + x.ties > 0 ? `${x.wins}–${x.losses}–${x.ties}` : "–"}</td>
                <td className="num">{x.winrate !== null ? `${Math.round(x.winrate)}%` : "–"}</td>
                <td className="num">{x.dpm ?? "–"}</td>
                <td className="num">{x.accuracy !== null ? `${x.accuracy}%` : "–"}</td>
                <td className="num">{hours(x.timeS)}</td>
              </tr>
            ))}
        </tbody>
      </table>
      <p className="hint pp-career-foot">
        {v.error && <>{v.error} · </>}
        {v.fetchedAt !== null && <>{tx("Read {0}", { "0": formatDate(v.fetchedAt, true) })} · </>}
        <a href={v.url} target="_blank" rel="noreferrer">
          {t("From trends.tf ↗")}
        </a>
      </p>
    </div>
  );
}

/** Their teams with the dates they were on them, from ETF2L's transfers (Q48). */
export function TeamStays({ accountId }: { accountId: number }) {
  const q = useQuery({ queryKey: ["player_teams", accountId], queryFn: () => api.getPlayerTeams(accountId), staleTime: 60 * 60_000 });
  const [all, setAll] = useState(false);
  if (!q.data || q.data.length === 0) return null;
  const now = Math.floor(Date.now() / 1000);
  const shown = all ? q.data : q.data.slice(0, 8);
  return (
    <section className="pp-stays">
      <h3>{t("Team history")}</h3>
      <ul>
        {shown.map((s, i) => (
          <li key={`${s.teamId}-${s.from ?? i}`} className={s.to === null ? "on" : undefined}>
            <span className="pp-stay-team">
              <strong>{s.teamName}</strong>
              {s.teamType && <span className="pp-stay-type">{s.teamType.replace("Highlander", "HL")}</span>}
            </span>
            <span className="muted">
              {s.from !== null ? formatMonth(s.from) : t("before the records")} – {s.to !== null ? formatMonth(s.to) : t("now")}
            </span>
            <span className="pp-stay-length">{formatStay((s.to ?? now) - (s.from ?? s.to ?? now))}</span>
          </li>
        ))}
      </ul>
      {q.data.length > shown.length && (
        <button className="linkish" onClick={() => setAll(true)}>
          {tx("Show all {0}", { "0": q.data.length })}
        </button>
      )}
      <p className="hint">{t("From ETF2L's transfers: every team, Highlander or not.")}</p>
    </section>
  );
}

/** Their teams, season by season. */
export function SeasonsTable({ p }: { p: Profile }) {
  if (p.seasons.length === 0) return <p className="hint">{t("No ETF2L official in the seasons read.")}</p>;
  return (
    <table className="pp-seasons">
      <thead>
        <tr>
          <th>{t("Season")}</th>
          <th>{t("Team")}</th>
          <th>{t("Division")}</th>
          <th>{t("Record")}</th>
          <th>{t("Placing")}</th>
        </tr>
      </thead>
      <tbody>
        {p.seasons.map((s) => (
          <tr key={`${s.season}-${s.team.id}`}>
            <td>
              {s.season >= 100 ? s.seasonName : <>S{s.season} <span className="muted">{s.seasonName}</span></>}
            </td>
            <td>
              <span className="pp-team">
                <TeamAvatar src={s.team.avatar} />
                {s.team.name}
                {s.merc && (
                  <span className="muted" title={t("Played for them as a merc, not on their roster: no medal of theirs")}>
                    {" "}· {t("merc")}
                  </span>
                )}
                {s.leftEarly && (
                  <span className="muted" title={t("Left the team before its last match of the season, by ETF2L's transfers: the team's medal is not theirs")}>
                    {" "}· {t("left early")}
                  </span>
                )}
              </span>
            </td>
            <td>{s.tier !== null ? <DivisionBadge d={{ name: s.division, tier: s.tier }} /> : s.division}</td>
            <td>
              {s.won}–{s.lost}
              <span className="muted"> {tx("of {0}", { "0": s.played })}</span>
            </td>
            <td>{s.place ? <span className={`pp-place pp-${PLACE[s.place - 1]}`}>{s.place === 1 ? t("Winner") : s.place === 2 ? t("Runner-up") : t("Third")}</span> : <span className="muted">–</span>}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function Achievements({ medals, mvps }: { medals: Medal[]; mvps: Mvp[] }) {
  if (medals.length === 0 && mvps.length === 0) {
    return <p className="hint">{t("No medals in the seasons read. Medals come from ETF2L playoff finals and, where a division has no playoffs, its final table.")}</p>;
  }
  return (
    <ul className="pp-achievements">
      {medals.map((m, i) => (
        <li key={i}>
          <MedalIcon m={m} />
          <span>
            <strong>{m.place === 1 ? t("Winner") : m.place === 2 ? t("Runner-up") : t("Third")}</strong> · {m.division} · {seasonLong(m.season, m.seasonName)}
          </span>
          <span className="pp-team">
            <TeamAvatar src={m.team.avatar} />
            {m.team.name}
          </span>
          <span className="muted">{m.how}</span>
        </li>
      ))}
      {mvps.map((m, i) => (
        <li key={`mvp-${i}`}>
          <MvpIcon m={m} />
          <span>
            <strong>{m.event ? t("Event MVP") : t("{0} MVP", { "0": classLabel(m.class) })}</strong> · {m.division} · {seasonLong(m.season, m.seasonName)}
          </span>
          <span className="pp-team">
            <TeamAvatar src={m.team.avatar} />
            {m.team.name}
          </span>
          <span className="muted">{mvpHow(m)}</span>
        </li>
      ))}
    </ul>
  );
}
