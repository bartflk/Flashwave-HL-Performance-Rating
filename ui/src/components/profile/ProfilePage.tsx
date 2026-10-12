import { useRef, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import {
  errorMessage,
  type ComponentSummary,
  type ContextKind,
  type ContextSplit,
  type GameRef,
  type OppositionBand, type DivisionBand,
  type PlayerProfile,
  type Profile,
} from "../../api/types";
import { Achievements, PlayerHeader, RecentOfficials, SeasonsTable, TeamStays, TrendsCareer } from "../players/PlayerProfile";
import "../players/players.css";
import { TeammatesPanel } from "./TeammatesPanel";
import { LivesOnMapTab } from "../players/LivesOnMap";
import { capitalize, formatDate, rating, ratingPercent, splitMap } from "../../lib/format";
import { KIND_LABEL, KIND_PLURAL } from "../ContextBadge";
import { bounds, usePeriod } from "../../lib/period";
import { PeriodPicker } from "../PeriodPicker";
import { AimPanel } from "./AimPanel";
import { FightsPanel } from "./FightsPanel";
import { SeasonsPanel } from "./SeasonsPanel";
import { TrendChart } from "./TrendChart";
import { ClassIcon } from "../ClassIcon";
import { Fold } from "../Fold";
import "./profile.css";
import { t, tx, k } from "../../lib/i18n";
import { classLabel } from "../analysis/common";

/** Below this many rated games a profile is shown, but flagged as thin. */
const THIN_SAMPLE = 20;

export function ProfilePage({ steamid, onOpenMatch }: { steamid: string | null; onOpenMatch: (logId: number) => void }) {
  const [cls, setCls] = useState<string | null>(null);
  const [kind, setKind] = useState<ContextKind | null>(null);
  const period = usePeriod();
  const { from, to } = bounds(period);
  const q = useQuery({
    queryKey: ["profile", cls, kind, from, to],
    queryFn: () => api.getProfile(cls, kind, from, to),
    placeholderData: keepPreviousData,
  });
  // The split is over every game, so the last one seen stays valid while a
  // filter with no games (no Engineer officials, say) shows an empty profile.
  const lastSplit = useRef<ContextSplit[]>([]);
  if (q.data?.profile) lastSplit.current = q.data.profile.contexts;

  const me = useOwnProfile(steamid);
  const ownAccount = steamid ? accountOf(steamid) : null;

  if (q.isPending) return <div className="profile-page"><p className="hint">{t("Loading profile…")}</p></div>;
  if (q.isError) return <div className="profile-page"><p className="error">{errorMessage(q.error)}</p></div>;

  const { classes, profile, fights } = q.data;
  const active = cls ?? profile?.class ?? classes[0]?.[0] ?? null;
  const hero = me ? <PlayerHeader p={me} own aside={profile && <Signature p={profile} />} /> : null;

  if (classes.length === 0) {
    return (
      <div className="profile-page">
        {hero}
        <div className="panel">
          <h2>{t("No ratings yet")}</h2>
          <p className="hint" style={{ marginTop: 6 }}>{t("Press Sync, or Rebuild in Settings.")}</p>
        </div>
      </div>
    );
  }

  return (
    <div className={q.isPlaceholderData ? "profile-page refetching" : "profile-page"}>
      {hero}
      <JumpBar />

      <h2 className="profile-section-title" id="profile-rating">{t("Rating")}</h2>
      <nav className="class-tabs" aria-label={t("Class")}>
        {classes.map(([c, n]) => (
          <button
            key={c}
            className={c === active ? "class-tab active" : "class-tab"}
            onClick={() => {
              setCls(c);
              lastSplit.current = [];
            }}
            title={n < THIN_SAMPLE ? t("Only {n} rated games — read with care", { n: n }) : undefined}
          >
            <ClassIcon cls={c} size={20} /> {classLabel(c)} <span className="count">{n}</span>
          </button>
        ))}
      </nav>

      <div className="profile-filters">
        <KindFilter kind={kind} onChange={setKind} split={lastSplit.current} />
        <PeriodPicker />
      </div>

      {profile ? (
        <ProfileBody p={profile} onOpenMatch={onOpenMatch} onKind={setKind} />
      ) : (
        <div className="panel">
          <p className="hint">{tx("No rated {active} {1}{2}.", { "1": kind ? t(KIND_PLURAL[kind]).toLowerCase() : t("games"), "2": period.kind !== "all" ? t(" in this period") : "", active: classLabel(active) })}</p>
        </div>
      )}

      {profile && fights && (
        <Fold id="profile-fights">
          <FightsPanel card={fights} cls={profile.class} />
        </Fold>
      )}

      {profile && (q.data?.aim || q.data?.life) && (
        <Fold id="profile-aim">
          <AimPanel aim={q.data.aim} life={q.data.life} aimAll={q.data.aimAll} lifeAll={q.data.lifeAll} cls={profile.class} />
        </Fold>
      )}

      {active && (
        <Fold id="profile-seasons">
          <SeasonsPanel cls={active} />
        </Fold>
      )}

      {me && (
        <>
          <h2 className="profile-section-title" id="profile-career">{t("Career")}</h2>
          <div className="career-grid">
            <section className="panel">
              <h3>{t("Teams by season")}</h3>
              <SeasonsTable p={me} />
              <TeamStays accountId={me.accountId} />
              <h3 className="career-sub">{t("Recent officials")}</h3>
              <RecentOfficials p={me} count={6} />
            </section>
            <section className="panel">
              <h3>{t("ETF2L officials on trends.tf")}</h3>
              <TrendsCareer accountId={me.accountId} />
              <h3 className="career-sub">{tx("Achievements ({0})", { "0": me.medals.length + (me.mvps?.length ?? 0) })}</h3>
              <Achievements medals={me.medals} mvps={me.mvps ?? []} />
            </section>
          </div>
        </>
      )}

      {/* Q68 (Emiel): your own routes, without looking yourself up under Players. */}
      {ownAccount !== null && (
        <>
          <h2 className="profile-section-title" id="profile-lives">{t("Lives on a map")}</h2>
          <section className="panel">
            <LivesOnMapTab accountId={ownAccount} name={me?.name ?? t("you")} />
          </section>
        </>
      )}

      <h2 className="profile-section-title" id="profile-teammates">{t("Teammates")}</h2>
      <section className="panel">
        <TeammatesPanel />
      </section>
    </div>
  );
}

/** Your own player profile: the one anyone could look up, with you in it. */
function useOwnProfile(steamid: string | null): PlayerProfile | null {
  const id = steamid ? accountOf(steamid) : null;
  const q = useQuery({ queryKey: ["player_profile", id], queryFn: () => api.getPlayerProfile(id!), enabled: id !== null });
  return q.data ?? null;
}

/** SteamID64 to the account number the catalogue keys players by. */
function accountOf(steamid64: string): number | null {
  try {
    return Number(BigInt(steamid64) - 76561197960265728n);
  } catch {
    return null;
  }
}

/** The number you are, on the class you are looking at: beside your name. */
function Signature({ p }: { p: Profile }) {
  const delta = p.prevFormAvg === null ? null : p.formAvg - p.prevFormAvg;
  return (
    <div className="pp-sig">
      <span className="pp-sig-label">
        <ClassIcon cls={p.class} size={18} /> {tx("{0} rating", { "0": classLabel(p.class) })}
        {p.filter && <span className="muted"> · {t(KIND_PLURAL[p.filter]).toLowerCase()}</span>}
      </span>
      <span className="pp-sig-value">{rating(p.careerAvg)}</span>
      <span className="pp-sig-sub">
        {tx("form {0}", { "0": rating(p.formAvg) })}
        {delta !== null && <span className={delta >= 0 ? "kpi-delta up" : "kpi-delta down"}> {delta >= 0 ? "▲" : "▼"}{Math.abs(delta).toFixed(2)}</span>}
      </span>
      <span className="pp-sig-sub muted">
        {tx("{0} rated games", { "0": p.games })}
        {p.winRate !== null && ` · ${t("{0}% won", { "0": p.winRate.toFixed(0) })}`}
      </span>
    </div>
  );
}

/** Rating, Career, Teammates: one page, so a way to jump within it. */
function JumpBar() {
  const go = (id: string) => document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  return (
    <nav className="profile-jump" aria-label={t("On this page")}>
      <button className="linkish" onClick={() => go("profile-rating")}>{t("Rating")}</button>
      <button className="linkish" onClick={() => go("profile-career")}>{t("Career")}</button>
      <button className="linkish" onClick={() => go("profile-lives")}>{t("Lives on a map")}</button>
      <button className="linkish" onClick={() => go("profile-teammates")}>{t("Teammates")}</button>
    </nav>
  );
}

/** All games, or one kind. Counts come from the unfiltered split. */
function KindFilter(props: { kind: ContextKind | null; onChange: (k: ContextKind | null) => void; split: ContextSplit[] }) {
  const { kind, onChange, split } = props;
  const count = (k: ContextKind) => split.find((s) => s.kind === k)?.games;
  const total = split.reduce((n, s) => n + s.games, 0);
  const opts: Array<[ContextKind | null, string, number | undefined]> = [
    [null, k("All games"), total || undefined],
    ["official", KIND_PLURAL.official, count("official")],
    ["scrim", KIND_PLURAL.scrim, count("scrim")],
    ["pug", KIND_PLURAL.pug, count("pug")],
  ];
  return (
    <div className="kind-filter">
      <div className="segmented" role="tablist" aria-label={t("Kind of game")}>
        {opts.map(([k, label, n]) => (
          <button
            key={label}
            role="tab"
            aria-selected={kind === k}
            className={kind === k ? "seg active" : "seg"}
            onClick={() => onChange(k)}
          >
            {t(label)}
            {n !== undefined && <span className="count"> {n}</span>}
          </button>
        ))}
      </div>
      {kind && <span className="hint">{tx("Everything below counts {0} only.", { "0": t(KIND_PLURAL[kind]).toLowerCase() })}</span>}
    </div>
  );
}

/**
 * The same class across officials, scrims and pugs: one row each, average
 * rating on a track with the 1.00 line, games and win rate beside it.
 * Always over every game, whatever the filter.
 */
function KindSplit(props: { split: ContextSplit[]; active: ContextKind | null; onKind: (k: ContextKind | null) => void }) {
  const { split, active, onKind } = props;
  if (split.length < 2) return null;
  return (
    <section className="panel kind-split">
      <header>
        <h2>{t("Officials, scrims and pugs")}</h2>
        <p className="hint">{t("Average rating by kind of game, over every game on this class. Click one to filter.")}</p>
      </header>
      <div className="ks-rows">
        {split.map((s) => (
          <button
            key={s.kind}
            className={active === s.kind ? "ks-row active" : "ks-row"}
            onClick={() => onKind(active === s.kind ? null : s.kind)}
            aria-pressed={active === s.kind}
          >
            <span className="ks-label">{t(KIND_PLURAL[s.kind])}</span>
            <span className="ks-track" aria-hidden>
              <span className="comp-mid" />
              <span className={`ks-fill ks-${s.kind}`} style={{ width: `${ratingPercent(s.avg)}%` }} />
            </span>
            <span className="ks-value">{rating(s.avg)}</span>
            <span className="ks-meta muted">
              {tx("{games} game{1}{2}", { "1": s.games === 1 ? "" : "s", "2": s.winRate !== null && t(" · {0}% won", { "0": s.winRate.toFixed(0) }), games: s.games })}
            </span>
          </button>
        ))}
      </div>
    </section>
  );
}

/**
 * Who the games were against.
 *
 * The opposite number on your class, averaged over their *other* games, is
 * the only honest read of how hard a match was — and Highlander hands it to
 * us for nothing, because there is exactly one of each class a side.
 *
 * It is shown rather than folded into the rating. Measured over 8,679
 * performances: facing an opponent 0.20 better costs 0.076 rating points,
 * and correcting for it leaves how well a player's games predict each other
 * completely unchanged. Opponent strength swings more between one of your
 * own games and the next than it does between players, so there is no
 * standing difficulty to subtract — only something worth knowing.
 */
function Opposition({ bands, byDivision }: { bands: OppositionBand[]; byDivision: DivisionBand[] }) {
  if (bands.length < 2 && byDivision.length === 0) return null;
  const label: Record<OppositionBand["band"], string> = {
    weaker: t("Weaker opponents"),
    even: t("An even match"),
    stronger: t("Stronger opponents"),
  };
  return (
    <section className="panel kind-split">
      <header>
        <h2>{t("Who you played")}</h2>
        <p className="hint">{t("Your rating by how good the opposite number was, averaged over their other games.")}</p>
      </header>
      <div className="ks-rows">
        {bands.map((b) => (
          <div key={b.band} className="ks-row">
            <span className="ks-label">{label[b.band]}</span>
            <span className="ks-track" aria-hidden>
              <span className="comp-mid" />
              <span className={`ks-fill ks-${b.band}`} style={{ width: `${ratingPercent(b.avg)}%` }} />
            </span>
            <span className="ks-value">{rating(b.avg)}</span>
            <span className="ks-meta muted">{tx("vs {0} · {games} game{2}{3}", { "0": rating(b.opponentAvg), "2": b.games === 1 ? "" : "s", "3": b.winRate !== null && t(" · {0}% won", { "0": b.winRate.toFixed(0) }), games: b.games })}
            </span>
          </div>
        ))}
      </div>
      {byDivision.length > 0 && (
        <>
          <p className="hint ks-sub">{t("By their ETF2L division at the time: the opposite number's, from the officials read.")}</p>
          <div className="ks-rows">
            {byDivision.map((b) => (
              <div key={b.tier} className="ks-row">
                <span className="ks-label">
                  <span className={`div-badge div-t${Math.min(b.tier, 4)}`}>{b.division}</span>
                </span>
                <span className="ks-track" aria-hidden>
                  <span className="comp-mid" />
                  <span className="ks-fill ks-even" style={{ width: `${ratingPercent(b.avg)}%` }} />
                </span>
                <span className="ks-value">{rating(b.avg)}</span>
                <span className="ks-meta muted">
                  {tx("{games} game{1}{2}", { games: b.games, "1": b.games === 1 ? "" : "s", "2": b.winRate !== null && t(" · {0}% won", { "0": b.winRate.toFixed(0) }) })}
                </span>
              </div>
            ))}
          </div>
        </>
      )}
    </section>
  );
}

/**
 * Where you play (Q28): per map with drawn callouts, where your fights on
 * this class ended -- every match with a server log -- and, where STVs
 * exist, where you stood and how you moved. Zones read by your own side:
 * "Own Left" is your team's Left whichever colour you were.
 */
function Tendencies({ cls }: { cls: string }) {
  const q = useQuery({ queryKey: ["tendencies", cls], queryFn: () => api.getTendencies(cls), staleTime: 5 * 60_000 });
  const maps = q.data ?? [];
  if (maps.length === 0) return null;
  return (
    <section className="panel tendencies">
      <header>
        <h2>{t("Where you play")}</h2>
        <p className="hint">{t("By callout, on your own side of the map. Fights: where your kills and deaths happened, every match. Standing: where you spent your time alive, from STV demos.")}</p>
      </header>
      <div className="tend-maps">
        {maps.map((m) => {
          const fights = m.fights.reduce((a, f) => a + f.kills + f.deaths, 0) + m.unzoned;
          const top = m.fights.slice(0, 6);
          const most = Math.max(1, ...top.map((f) => f.kills + f.deaths));
          return (
            <article key={m.map} className="tend-map">
              <h3>
                {capitalize(m.map)}
                {m.draft && <span className="muted tend-draft"> · {t("draft callouts")}</span>}
              </h3>
              <p className="hint">
                {tx("{0} matches", { "0": m.matches })}
                {m.stvs > 0 && ` · ${t("{0} STV · {1} min alive", { "0": m.stvs, "1": Math.round(m.aliveS / 60) })}`}
              </p>
              {top.length > 0 && (
                <ul className="tend-bars">
                  {top.map((f) => (
                    <li key={f.zone} title={t("{0} kills, {1} deaths", { "0": f.kills, "1": f.deaths })}>
                      <span className="tend-zone">{f.zone}</span>
                      <span className="tend-track" aria-hidden>
                        <i className="tend-k" style={{ width: `${(100 * f.kills) / most}%` }} />
                        <i className="tend-d" style={{ width: `${(100 * f.deaths) / most}%` }} />
                      </span>
                      <span className="tend-n muted">{Math.round((100 * (f.kills + f.deaths)) / Math.max(1, fights))}%</span>
                    </li>
                  ))}
                </ul>
              )}
              {m.time.length > 0 && (
                <p className="tend-stand">
                  <span className="muted">{t("Standing")}: </span>
                  {m.time
                    .slice(0, 4)
                    .map((z) => `${z.zone} ${Math.round(z.share * 100)}%`)
                    .join(" · ")}
                </p>
              )}
              {m.paths.length > 0 && (
                <p className="tend-stand">
                  <span className="muted">{t("Moves")}: </span>
                  {m.paths
                    .slice(0, 3)
                    .map((p) => `${p.from} → ${p.to} (${p.times}×)`)
                    .join(" · ")}
                </p>
              )}
            </article>
          );
        })}
      </div>
      <p className="hint tend-legend">
        <i className="tend-k" /> {t("kills")} <i className="tend-d" /> {t("deaths")}
      </p>
    </section>
  );
}

function ProfileBody(props: {
  p: Profile;
  onOpenMatch: (logId: number) => void;
  onKind: (k: ContextKind | null) => void;
}) {
  const { p, onOpenMatch, onKind } = props;
  const delta = p.prevFormAvg === null ? null : p.formAvg - p.prevFormAvg;
  const thin = p.games < THIN_SAMPLE;
  const scope = p.filter ? t(KIND_PLURAL[p.filter]).toLowerCase() : t("games");

  return (
    <>
      {thin && (
        <p className="thin-note">{tx("Only {games} rated {1} {scope}. Treat these numbers as a rough sketch, not a verdict.", { "1": classLabel(p.class), games: p.games, scope: scope })}</p>
      )}

      <section className="kpis">
        <div className="kpi hero">
          <span className="kpi-label">{tx("Form · last {0} {scope}", { "0": Math.min(p.formWindow, p.games), scope: scope })}</span>
          <span className="kpi-value">{rating(p.formAvg)}</span>
          {delta !== null && (
            <span className={delta >= 0 ? "kpi-delta up" : "kpi-delta down"}>
              {tx("{0} {1} vs the {formWindow} before", { "0": delta >= 0 ? "▲" : "▼", "1": Math.abs(delta).toFixed(2), formWindow: p.formWindow })}</span>
          )}
        </div>
        <div className="kpi">
          <span className="kpi-label">{t("Career")}</span>
          <span className="kpi-value">{rating(p.careerAvg)}</span>
          <span className="kpi-sub">
            {tx("{games} rated {scope}", { games: p.games, scope: scope })}
          </span>
        </div>
        {p.winRate !== null && (
          <div className="kpi">
            <span className="kpi-label">{t("Win rate")}</span>
            <span className="kpi-value">{p.winRate.toFixed(0)}%</span>
            <span className="kpi-sub">{t("ties excluded")}</span>
          </div>
        )}
        {/* Career records span every kind of game, so they only show unfiltered. */}
        {p.filter === null && p.extras.map((e) => (
          <div className="kpi" key={e.label} title={e.hint ? extraText(e.hint) : undefined}>
            <span className="kpi-label">{extraText(e.label)}</span>
            <span className="kpi-value">{e.value}</span>
            {e.detail && <span className="kpi-sub">{extraText(e.detail)}</span>}
          </div>
        ))}
      </section>

      <KindSplit split={p.contexts} active={p.filter} onKind={onKind} />

      <Opposition bands={p.opposition} byDivision={p.byDivision ?? []} />

      <Tendencies cls={p.class} />

      <Fold id="profile-components">
        <Components items={p.components} formWindow={Math.min(p.formWindow, p.games)} />
      </Fold>

      <Fold id="profile-trend">
        <TrendChart points={p.trend} rollingWindow={p.rollingWindow} onOpen={onOpenMatch} />
      </Fold>

      <section className="games-grid">
        <GameList title={t("Best games")} games={p.best} onOpen={onOpenMatch} />
        <GameList title={t("Worst games")} games={p.worst} onOpen={onOpenMatch} />
      </section>
    </>
  );
}

/**
 * Where you sit on each part of the rating, as a percentile of the players you
 * face. The bar is recent form; the tick is your career. Ordered by weight, so
 * the parts that move the rating most come first.
 */
function Components({ items, formWindow }: { items: ComponentSummary[]; formWindow: number }) {
  if (items.length === 0) return null;
  const weakest = items.reduce((a, b) => (b.formPct < a.formPct ? b : a));
  const strongest = items.reduce((a, b) => (b.formPct > a.formPct ? b : a));

  return (
    <section className="panel comps">
      <header className="comps-head">
        <div>
          <h2>{t("What the rating is made of")}</h2>
          <p className="hint">{t("Percentile against the players you face, 0–100. Deaths are flipped: higher always means better.")}</p>
        </div>
        <div className="legend">
          <span>
            {tx("{0} last {formWindow} games", { "0": <span className="swatch-bar" />, formWindow: formWindow })}</span>
          <span>
            {tx("{0} career", { "0": <span className="swatch-tick" /> })}</span>
        </div>
      </header>

      <div className="comp-rows">
        {items.map((c) => (
          <div className="comp-row" key={c.component}>
            <div className="comp-label">
              <span>{t(c.label)}</span>
              {c === weakest && <span className="tag weak">{t("weakest")}</span>}
              {c === strongest && <span className="tag strong">{t("strongest")}</span>}
            </div>
            <span className="comp-weight muted">{Math.round(c.weight * 100)}%</span>
            <div
              className="comp-track"
              role="img"
              aria-label={t("{label}: {1} recently, {2} career", { "1": c.formPct.toFixed(0), "2": c.careerPct.toFixed(0), label: t(c.label) })}
            >
              <span className="comp-mid" />
              <span className="comp-fill" style={{ width: `${c.formPct}%` }} />
              <span className="comp-career" style={{ left: `${c.careerPct}%` }} />
            </div>
            <span className="comp-value">{c.formPct.toFixed(0)}</span>
            <span className="comp-raw muted">
              {fmtRaw(c)} <span className="unit">{t(c.unit)}</span>
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

function GameList(props: { title: string; games: GameRef[]; onOpen: (logId: number) => void }) {
  const { title, games, onOpen } = props;
  return (
    <section className="panel">
      <h2>{title}</h2>
      <div className="table-wrap" style={{ marginTop: 12 }}>
        <table className="match-table">
          <tbody>
            {games.map((g) => {
              const { mode, name } = splitMap(g.map);
              return (
                <tr key={g.logId} className="clickable" tabIndex={0} onClick={() => onOpen(g.logId)} onKeyDown={(e) => e.key === "Enter" && onOpen(g.logId)}>
                  <td className="num game-score">{rating(g.score)}</td>
                  <td className="muted nowrap">{formatDate(g.playedAt, true)}</td>
                  <td className="nowrap" title={g.map ?? undefined}>
                    {mode && <span className="mode">{mode}</span>}
                    {name ?? <span className="muted">{t("unknown")}</span>}
                  </td>
                  <td>{g.result && <span className={`result result-${g.result}`}>{g.result}</span>}</td>
                  <td>
                    {g.kind ? (
                      <span className={`badge badge-${g.kind}`}>{g.kind === "official" ? t("ETF2L") : KIND_LABEL[g.kind].toUpperCase()}</span>
                    ) : (
                      g.league && <span className="badge badge-league">{g.league.toUpperCase()}</span>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function fmtRaw(c: ComponentSummary): string {
  if (c.component === "headshot_share" || c.component === "untraded") return `${c.formRaw.toFixed(0)}%`;
  if (c.component === "heal" || c.component === "dpm") return c.formRaw.toFixed(0);
  return c.formRaw.toFixed(2);
}

/**
 * The career cards come from Rust with their numbers and class already in
 * the text (rating.rs, career extras), so the few shapes it builds are
 * recognised here and translated whole. Anything else is looked up as is.
 */
function extraText(s: string): string {
  let m = /^([\d.]+) per game$/.exec(s);
  if (m) return t("{0} per game", { "0": m[1] });
  m = /^([+-]?\d+) net$/.exec(s);
  if (m) return t("{0} net", { "0": m[1] });
  m = /^vs enemy (\w+), career$/.exec(s);
  if (m) return t("vs enemy {0}, career", { "0": classLabel(m[1]) });
  m = /^Your kills on the enemy (\w+) against their kills on you, across every rated game as \w+\.$/.exec(s);
  if (m) return t("Your kills on the enemy {0} against their kills on you, across every rated game as {0}.", { "0": classLabel(m[1]) });
  return t(s);
}
