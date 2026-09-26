import { useRef, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import {
  errorMessage,
  type ComponentSummary,
  type ContextKind,
  type ContextSplit,
  type GameRef,
  type OppositionBand,
  type Profile,
} from "../../api/types";
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

/** Below this many rated games a profile is shown, but flagged as thin. */
const THIN_SAMPLE = 20;

export function ProfilePage({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
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

  if (q.isPending) return <div className="profile-page"><p className="hint">Loading profile…</p></div>;
  if (q.isError) return <div className="profile-page"><p className="error">{errorMessage(q.error)}</p></div>;

  const { classes, profile, fights } = q.data;
  const active = cls ?? profile?.class ?? classes[0]?.[0] ?? null;

  if (classes.length === 0) {
    return (
      <div className="profile-page">
        <div className="panel">
          <h2>No ratings yet</h2>
          <p className="hint" style={{ marginTop: 6 }}>
            Ratings are built at the end of every sync. Press Sync, or use Rebuild in Settings.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className={q.isPlaceholderData ? "profile-page refetching" : "profile-page"}>
      <nav className="class-tabs" aria-label="Class">
        {classes.map(([c, n]) => (
          <button
            key={c}
            className={c === active ? "class-tab active" : "class-tab"}
            onClick={() => {
              setCls(c);
              lastSplit.current = [];
            }}
            title={n < THIN_SAMPLE ? `Only ${n} rated games — read with care` : undefined}
          >
            <ClassIcon cls={c} size={20} /> {capitalize(c)} <span className="count">{n}</span>
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
          <p className="hint">
            No rated {active} {kind ? KIND_PLURAL[kind].toLowerCase() : "games"}
            {period.kind !== "all" ? " in this period" : ""}.
          </p>
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
    </div>
  );
}

/** All games, or one kind. Counts come from the unfiltered split. */
function KindFilter(props: { kind: ContextKind | null; onChange: (k: ContextKind | null) => void; split: ContextSplit[] }) {
  const { kind, onChange, split } = props;
  const count = (k: ContextKind) => split.find((s) => s.kind === k)?.games;
  const total = split.reduce((n, s) => n + s.games, 0);
  const opts: Array<[ContextKind | null, string, number | undefined]> = [
    [null, "All games", total || undefined],
    ["official", KIND_PLURAL.official, count("official")],
    ["scrim", KIND_PLURAL.scrim, count("scrim")],
    ["pug", KIND_PLURAL.pug, count("pug")],
  ];
  return (
    <div className="kind-filter">
      <div className="segmented" role="tablist" aria-label="Kind of game">
        {opts.map(([k, label, n]) => (
          <button
            key={label}
            role="tab"
            aria-selected={kind === k}
            className={kind === k ? "seg active" : "seg"}
            onClick={() => onChange(k)}
          >
            {label}
            {n !== undefined && <span className="count"> {n}</span>}
          </button>
        ))}
      </div>
      {kind && <span className="hint">Everything below counts {KIND_PLURAL[kind].toLowerCase()} only.</span>}
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
        <h2>Officials, scrims and pugs</h2>
        <p className="hint">Average rating by kind of game, over every game on this class. Click one to filter.</p>
      </header>
      <div className="ks-rows">
        {split.map((s) => (
          <button
            key={s.kind}
            className={active === s.kind ? "ks-row active" : "ks-row"}
            onClick={() => onKind(active === s.kind ? null : s.kind)}
            aria-pressed={active === s.kind}
          >
            <span className="ks-label">{KIND_PLURAL[s.kind]}</span>
            <span className="ks-track" aria-hidden>
              <span className="comp-mid" />
              <span className={`ks-fill ks-${s.kind}`} style={{ width: `${ratingPercent(s.avg)}%` }} />
            </span>
            <span className="ks-value">{rating(s.avg)}</span>
            <span className="ks-meta muted">
              {s.games} game{s.games === 1 ? "" : "s"}
              {s.winRate !== null && ` · ${s.winRate.toFixed(0)}% won`}
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
function Opposition({ bands }: { bands: OppositionBand[] }) {
  if (bands.length < 2) return null;
  const label: Record<OppositionBand["band"], string> = {
    weaker: "Weaker opponents",
    even: "An even match",
    stronger: "Stronger opponents",
  };
  return (
    <section className="panel kind-split">
      <header>
        <h2>Who you played</h2>
        <p className="hint">
          Your rating by how good the opposite number on your class is, averaged over their other
          games. Games whose opponent has too little history to judge are left out.
        </p>
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
            <span className="ks-meta muted">
              vs {rating(b.opponentAvg)} · {b.games} game{b.games === 1 ? "" : "s"}
              {b.winRate !== null && ` · ${b.winRate.toFixed(0)}% won`}
            </span>
          </div>
        ))}
      </div>
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
  const scope = p.filter ? KIND_PLURAL[p.filter].toLowerCase() : "games";

  return (
    <>
      {thin && (
        <p className="thin-note">
          Only {p.games} rated {capitalize(p.class)} {scope}. Treat these numbers as a rough sketch, not a
          verdict.
        </p>
      )}

      <section className="kpis">
        <div className="kpi hero">
          <span className="kpi-label">Form · last {Math.min(p.formWindow, p.games)} {scope}</span>
          <span className="kpi-value">{rating(p.formAvg)}</span>
          {delta !== null && (
            <span className={delta >= 0 ? "kpi-delta up" : "kpi-delta down"}>
              {delta >= 0 ? "▲" : "▼"} {Math.abs(delta).toFixed(2)} vs the {p.formWindow} before
            </span>
          )}
        </div>
        <div className="kpi">
          <span className="kpi-label">Career</span>
          <span className="kpi-value">{rating(p.careerAvg)}</span>
          <span className="kpi-sub">
            {p.games} rated {scope}
          </span>
        </div>
        {p.winRate !== null && (
          <div className="kpi">
            <span className="kpi-label">Win rate</span>
            <span className="kpi-value">{p.winRate.toFixed(0)}%</span>
            <span className="kpi-sub">ties excluded</span>
          </div>
        )}
        {/* Career records span every kind of game, so they only show unfiltered. */}
        {p.filter === null && p.extras.map((e) => (
          <div className="kpi" key={e.label} title={e.hint ?? undefined}>
            <span className="kpi-label">{e.label}</span>
            <span className="kpi-value">{e.value}</span>
            {e.detail && <span className="kpi-sub">{e.detail}</span>}
          </div>
        ))}
      </section>

      <KindSplit split={p.contexts} active={p.filter} onKind={onKind} />

      <Opposition bands={p.opposition} />

      <Fold id="profile-components">
        <Components items={p.components} formWindow={Math.min(p.formWindow, p.games)} />
      </Fold>

      <Fold id="profile-trend">
        <TrendChart points={p.trend} rollingWindow={p.rollingWindow} onOpen={onOpenMatch} />
      </Fold>

      <section className="games-grid">
        <GameList title="Best games" games={p.best} onOpen={onOpenMatch} />
        <GameList title="Worst games" games={p.worst} onOpen={onOpenMatch} />
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
          <h2>What the rating is made of</h2>
          <p className="hint">
            Percentile against the players you face, 0–100. Deaths are flipped: higher always means better.
          </p>
        </div>
        <div className="legend">
          <span>
            <span className="swatch-bar" /> last {formWindow} games
          </span>
          <span>
            <span className="swatch-tick" /> career
          </span>
        </div>
      </header>

      <div className="comp-rows">
        {items.map((c) => (
          <div className="comp-row" key={c.component}>
            <div className="comp-label">
              <span>{c.label}</span>
              {c === weakest && <span className="tag weak">weakest</span>}
              {c === strongest && <span className="tag strong">strongest</span>}
            </div>
            <span className="comp-weight muted">{Math.round(c.weight * 100)}%</span>
            <div
              className="comp-track"
              role="img"
              aria-label={`${c.label}: ${c.formPct.toFixed(0)} recently, ${c.careerPct.toFixed(0)} career`}
            >
              <span className="comp-mid" />
              <span className="comp-fill" style={{ width: `${c.formPct}%` }} />
              <span className="comp-career" style={{ left: `${c.careerPct}%` }} />
            </div>
            <span className="comp-value">{c.formPct.toFixed(0)}</span>
            <span className="comp-raw muted">
              {fmtRaw(c)} <span className="unit">{c.unit}</span>
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
                    {name ?? <span className="muted">unknown</span>}
                  </td>
                  <td>{g.result && <span className={`result result-${g.result}`}>{g.result}</span>}</td>
                  <td>
                    {g.kind ? (
                      <span className={`badge badge-${g.kind}`}>{g.kind === "official" ? "ETF2L" : KIND_LABEL[g.kind].toUpperCase()}</span>
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
