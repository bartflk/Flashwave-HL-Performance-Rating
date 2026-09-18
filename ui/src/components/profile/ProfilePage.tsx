import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type ComponentSummary, type GameRef, type Profile } from "../../api/types";
import { capitalize, formatDate, splitMap } from "../../lib/format";
import { TrendChart } from "./TrendChart";
import "./profile.css";

/** Below this many rated games a profile is shown, but flagged as thin. */
const THIN_SAMPLE = 20;

export function ProfilePage({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
  const [cls, setCls] = useState<string | null>(null);
  const q = useQuery({
    queryKey: ["profile", cls],
    queryFn: () => api.getProfile(cls),
    placeholderData: keepPreviousData,
  });

  if (q.isPending) return <div className="profile-page"><p className="hint">Loading profile…</p></div>;
  if (q.isError) return <div className="profile-page"><p className="error">{errorMessage(q.error)}</p></div>;

  const { classes, profile } = q.data;
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
            onClick={() => setCls(c)}
            title={n < THIN_SAMPLE ? `Only ${n} rated games — read with care` : undefined}
          >
            {capitalize(c)} <span className="count">{n}</span>
          </button>
        ))}
      </nav>

      {profile ? (
        <ProfileBody p={profile} onOpenMatch={onOpenMatch} />
      ) : (
        <div className="panel">
          <p className="hint">No rated {active} games.</p>
        </div>
      )}
    </div>
  );
}

function ProfileBody({ p, onOpenMatch }: { p: Profile; onOpenMatch: (logId: number) => void }) {
  const delta = p.prevFormAvg === null ? null : p.formAvg - p.prevFormAvg;
  const thin = p.games < THIN_SAMPLE;

  return (
    <>
      {thin && (
        <p className="thin-note">
          Only {p.games} rated {capitalize(p.class)} games. Treat these numbers as a rough sketch, not a
          verdict.
        </p>
      )}

      <section className="kpis">
        <div className="kpi hero">
          <span className="kpi-label">Form · last {Math.min(p.formWindow, p.games)} games</span>
          <span className="kpi-value">{p.formAvg.toFixed(0)}</span>
          {delta !== null && (
            <span className={delta >= 0 ? "kpi-delta up" : "kpi-delta down"}>
              {delta >= 0 ? "▲" : "▼"} {Math.abs(delta).toFixed(1)} vs the {p.formWindow} before
            </span>
          )}
        </div>
        <div className="kpi">
          <span className="kpi-label">Career</span>
          <span className="kpi-value">{p.careerAvg.toFixed(0)}</span>
          <span className="kpi-sub">{p.games} rated games</span>
        </div>
        {p.winRate !== null && (
          <div className="kpi">
            <span className="kpi-label">Win rate</span>
            <span className="kpi-value">{p.winRate.toFixed(0)}%</span>
            <span className="kpi-sub">ties excluded</span>
          </div>
        )}
        {p.extras.map((e) => (
          <div className="kpi" key={e.label} title={e.hint ?? undefined}>
            <span className="kpi-label">{e.label}</span>
            <span className="kpi-value">{e.value}</span>
            {e.detail && <span className="kpi-sub">{e.detail}</span>}
          </div>
        ))}
      </section>

      <Components items={p.components} formWindow={Math.min(p.formWindow, p.games)} />

      <TrendChart points={p.trend} rollingWindow={p.rollingWindow} onOpen={onOpenMatch} />

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
                  <td className="num game-score">{g.score.toFixed(0)}</td>
                  <td className="muted nowrap">{formatDate(g.playedAt, true)}</td>
                  <td className="nowrap" title={g.map ?? undefined}>
                    {mode && <span className="mode">{mode}</span>}
                    {name ?? <span className="muted">unknown</span>}
                  </td>
                  <td>{g.result && <span className={`result result-${g.result}`}>{g.result}</span>}</td>
                  <td>{g.league && <span className="badge badge-league">{g.league.toUpperCase()}</span>}</td>
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
  if (c.component === "headshot_share") return `${c.formRaw.toFixed(0)}%`;
  if (c.component === "heal" || c.component === "dpm") return c.formRaw.toFixed(0);
  return c.formRaw.toFixed(2);
}
