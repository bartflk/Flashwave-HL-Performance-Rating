import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { SixesDivision, SixesRow } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { drawn, mapMode } from "../../lib/gameformat";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { Etf2lMark } from "../Etf2lMark";
import { LeagueHead } from "../LeagueHead";
import { DivisionBadge } from "../players/PlayerProfile";
import { tierOf } from "../etf2l/Brackets";
import "../etf2l/etf2l.css";
import "./sixes.css";

/**
 * The ETF2L tab in 6v6 (prototype; Flashy: "make a prototype for etf2l
 * 6v6"). The current season's tables, read live from ETF2L's API: the
 * round-robin groups (Premiership to Division 3, and Fresh) and the Swiss
 * divisions (Division 4, Low, Open) since Season 53, with the map pool and
 * the match rules that make 6v6's tables read differently from Highlander's
 * (two maps a week, 5CP maps that can be drawn, golden caps).
 *
 * Everything else the Highlander tab has (matches, teams, players, news,
 * recruitment, seasons, tournaments) is listed under "Coming in 6v6", with
 * the plan in the 6v6 artifact.
 */
export function SixesPage() {
  const q = useQuery({ queryKey: ["sixes_season"], queryFn: api.getSixesSeason, staleTime: 30 * 60_000, retry: false });
  const stats = useQuery({ queryKey: ["index_stats"], queryFn: api.indexStats });
  const s = q.data;
  const groups = useMemo(() => (s?.divisions ?? []).filter((d) => d.system === "roundRobin"), [s]);
  const swiss = useMemo(() => (s?.divisions ?? []).filter((d) => d.system === "swiss"), [s]);

  return (
    <div className="content e2-page sx-page">
      <LeagueHead>
        <div className="e2-brand">
          <span className="e2-eyebrow" data-fit="">
            <Etf2lMark size={16} /> {t("ETF2L 6v6")}
          </span>
          <nav className="e2-menu" data-fit="" aria-label={t("ETF2L 6v6")}>
            <button className="on" aria-current="page">
              {t("Season")}
            </button>
            {[t("Matches"), t("Teams"), t("Players"), t("News"), t("Recruitment")].map((x) => (
              <button key={x} disabled title={t("Coming in 6v6")}>
                {x}
              </button>
            ))}
          </nav>
        </div>
        <span className="sx-proto" data-fit="" title={t("A first look at 6v6: the season's tables are live, the rest is planned")}>
          {t("Prototype")}
        </span>
      </LeagueHead>

      {q.error ? (
        <ApiError error={q.error} />
      ) : q.isPending ? (
        <Loading>{t("Reading ETF2L's 6v6 season…")}</Loading>
      ) : !s ? (
        <p className="hint">{t("ETF2L lists no 6v6 season.")}</p>
      ) : (
        <div className="e2-match-grid">
          <div className="e2-main">
            <header className="panel pp-head sx-head">
              <span className="ts-eyebrow">{t("ETF2L 6v6")}</span>
              <h1 className="sx-title">{s.name}</h1>
              <div className="sx-facts">
                <Fact label={t("Divisions")} value={String(s.divisions.length)} sub={tx("{0} round robin · {1} Swiss", { "0": groups.length, "1": swiss.length })} />
                <Fact label={t("Teams")} value={String(s.divisions.reduce((n, d) => n + d.teams.length, 0))} sub={t("signed up")} />
                <Fact label={t("Map pool")} value={String(s.pool.length)} sub={tx("{0} 5CP · {1} KOTH", { "0": s.pool.filter((m) => mapMode(m) === "5cp").length, "1": s.pool.filter((m) => mapMode(m) === "koth").length })} />
                <Fact label={t("A match")} value={t("2 maps")} sub={t("a week, fixed rotation")} />
              </div>
              <div className="sx-pool">
                {s.pool.map((m) => (
                  <span key={m} className={`sx-map sx-${mapMode(m)}`}>
                    {m}
                  </span>
                ))}
              </div>
            </header>

            {groups.length > 0 && (
              <>
                <h2 className="ts-section">{t("Round robin")}</h2>
                <p className="hint sx-explain">{t("Groups of eight: every team plays every other once over the seven weeks.")}</p>
                <div className="sx-grid">
                  {groups.map((d) => (
                    <DivisionTable key={d.name} d={d} />
                  ))}
                </div>
              </>
            )}
            {swiss.length > 0 && (
              <>
                <h2 className="ts-section">{t("Swiss")}</h2>
                <p className="hint sx-explain">{t("Each week a team plays one with a similar record, so the big divisions need no groups. Teams level on points are split by head-to-head, then the Sonneborn-Berger score, then rounds.")}</p>
                <div className="sx-stack">
                  {swiss.map((d) => (
                    <DivisionTable key={d.name} d={d} wide />
                  ))}
                </div>
              </>
            )}
          </div>

          <aside className="e2-rail">
            <section className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{t("How 6v6 is played")}</h2>
              </div>
              <ul className="sx-rules">
                <li>
                  <strong>{t("Six a side")}</strong>
                  <span className="muted">{t("Two Scouts, two Soldiers (pocket and roamer), a Demoman and a Medic; other classes are off-classes for a moment.")}</span>
                </li>
                <li>
                  <strong>{t("5CP")}</strong>
                  <span className="muted">{t("First to 5 rounds in 30 minutes, 4 minutes a round. Level at the limit is a draw in the regular season.")}</span>
                </li>
                <li>
                  <strong>{t("KOTH")}</strong>
                  <span className="muted">{t("First to 3 rounds, no time limit.")}</span>
                </li>
                <li>
                  <strong>{t("Points")}</strong>
                  <span className="muted">{t("As ETF2L counts them, per map. ETF2L changed them for this season, so the app shows its numbers rather than working them out.")}</span>
                </li>
              </ul>
              <p className="hint">{t("From ETF2L's 6v6 rules and the Season 53 home page.")}</p>
            </section>

            <section className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{t("Coming in 6v6")}</h2>
              </div>
              <ul className="sx-rules">
                <li>
                  <strong>{t("Your 6v6 matches")}</strong>
                  <span className="muted">
                    {stats.data
                      ? tx("Your index lists {0} 6v6 logs; they are not downloaded yet.", { "0": stats.data.sixes.toLocaleString() })
                      : t("Your 6v6 logs, downloaded and read like your Highlander ones.")}
                  </span>
                </li>
                <li>
                  <strong>{t("A 6v6 rating")}</strong>
                  <span className="muted">{t("Its own pools, roles (pocket and roamer, flank and combo Scout) and 6v6's stats: midfights, uber advantage, forced drops.")}</span>
                </li>
                <li>
                  <strong>{t("The rest of this tab")}</strong>
                  <span className="muted">{t("Matches, teams, players, playoffs, Nations' Cup, news and recruitment, as the Highlander tab has them.")}</span>
                </li>
              </ul>
            </section>
          </aside>
        </div>
      )}
    </div>
  );
}

function Fact({ label, value, sub }: { label: string; value: string; sub: ReactNode }) {
  return (
    <div className="sx-fact">
      <span className="sx-fact-label">{label}</span>
      <strong className="sx-fact-value">{value}</strong>
      <span className="muted">{sub}</span>
    </div>
  );
}

type SortKey = "place" | "won" | "score";

/** One division's table: maps won, drawn and lost, golden caps, points. */
function DivisionTable({ d, wide }: { d: SixesDivision; wide?: boolean }) {
  const [open, setOpen] = useState(!wide || d.teams.length <= 12);
  const [sort, setSort] = useState<SortKey>("place");
  const rows = useMemo(() => {
    const placed = d.teams.map((r, i) => ({ r, place: r.dropped ? null : i + 1 }));
    if (sort === "won") return [...placed].sort((a, b) => b.r.mapsWon - a.r.mapsWon || (a.place ?? 99) - (b.place ?? 99));
    if (sort === "score") return [...placed].sort((a, b) => b.r.score - a.r.score || (a.place ?? 99) - (b.place ?? 99));
    return placed;
  }, [d.teams, sort]);
  const shown = open ? rows : rows.slice(0, 10);
  const hasPen = d.teams.some((r) => r.penalty !== 0);
  const hasBye = d.teams.some((r) => r.byes !== 0);
  const hasSeed = d.teams.some((r) => r.seededPoints !== 0);
  const th = (k: SortKey, label: string, title: string) => (
    <th className="num">
      <button className={sort === k ? "sx-sort on" : "sx-sort"} onClick={() => setSort(sort === k ? "place" : k)} title={title}>
        {label}
      </button>
    </th>
  );
  return (
    <section className="panel e2-panel sx-div">
      <div className="e2-panel-head">
        <h2 className="sx-div-name">
          <DivisionBadge d={{ name: d.name, tier: Math.floor(tierOf(d.name)) }} />
        </h2>
        <span className="muted">{d.teams.length === 1 ? t("1 team") : tx("{0} teams", { "0": d.teams.length })}</span>
      </div>
      <div className="table-wrap">
        <table className="match-table sx-table">
          <thead>
            <tr>
              <th className="num">#</th>
              <th>{t("Team")}</th>
              <th className="num" title={t("Maps played")}>{t("P")}</th>
              {th("won", t("W"), t("Maps won"))}
              <th className="num" title={t("Maps drawn")}>{t("D")}</th>
              <th className="num" title={t("Maps lost")}>{t("L")}</th>
              <th className="num" title={t("Golden caps won and lost")}>{t("GC")}</th>
              {hasSeed && <th className="num" title={t("Seeded points")}>{t("SP")}</th>}
              {hasBye && <th className="num" title={t("Byes")}>{t("B")}</th>}
              {hasPen && <th className="num" title={t("Penalty points")}>{t("Pen")}</th>}
              {th("score", t("Pts"), t("League points, as ETF2L counts them"))}
            </tr>
          </thead>
          <tbody>
            {shown.map(({ r, place }) => (
              <Row key={r.id} r={r} place={place} hasPen={hasPen} hasBye={hasBye} hasSeed={hasSeed} />
            ))}
          </tbody>
        </table>
      </div>
      {!open && (
        <button className="linkish sx-more" onClick={() => setOpen(true)}>
          {tx("Show all {0}", { "0": d.teams.length })}
        </button>
      )}
    </section>
  );
}

function Row({ r, place, hasPen, hasBye, hasSeed }: { r: SixesRow; place: number | null; hasPen: boolean; hasBye: boolean; hasSeed: boolean }) {
  return (
    <tr className={r.dropped ? "sx-dropped" : undefined}>
      <td className="num muted">{place ?? "–"}</td>
      <td className="sx-team">
        <a href={`https://etf2l.org/teams/${r.id}/`} target="_blank" rel="noreferrer" title={t("Open on ETF2L")}>
          {r.name}
        </a>
        {r.dropped && <span className="muted"> {t("dropped")}</span>}
      </td>
      <td className="num muted">{r.mapsPlayed}</td>
      <td className="num team-won">{r.mapsWon}</td>
      <td className="num muted">{drawn(r)}</td>
      <td className="num team-lost">{r.mapsLost}</td>
      <td className="num muted">{r.gcWon || r.gcLost ? `${r.gcWon}-${r.gcLost}` : "–"}</td>
      {hasSeed && <td className="num muted">{r.seededPoints || ""}</td>}
      {hasBye && <td className="num muted">{r.byes || ""}</td>}
      {hasPen && <td className="num muted">{r.penalty || ""}</td>}
      <td className="num sx-pts">{r.score}</td>
    </tr>
  );
}
