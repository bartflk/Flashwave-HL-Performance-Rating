import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { CupResult, LeagueCup } from "../../api/types";
import { formatDate } from "../../lib/format";
import { k, t, tx } from "../../lib/i18n";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { Seg } from "../analysis/MapChrome";
import { MedalGlyph } from "../players/MedalGlyph";
import { TeamAvatar } from "../teams/TeamsPage";
import { useYourTeams } from "../teams/YourTeams";
import { ResultLine, UpcomingLine } from "./Matches";

/**
 * ETF2L's other Highlander tournaments, beside the seasons (Flashy: "a
 * section for 1 day cups and smaller tournaments added as a side thing ...
 * maybe a tournaments tab so you could explore them since seasons are the
 * main"): the one-night cups, the Experimental and Nations' Cups, the
 * Highlander Open and the preseason cups, each opening its brackets round by
 * round and who won them (hl_ingest::cups).
 */

type Kind = LeagueCup["kind"];

/** What each kind is called. */
const KIND_LABEL: Record<Kind, string> = {
  "one-night": k("One-night cup"),
  experimental: k("Experimental Cup"),
  nations: k("Nations' Cup"),
  preseason: k("Preseason cup"),
  open: k("Highlander Open"),
  other: k("Cup"),
};

const KIND_ORDER: Kind[] = ["one-night", "experimental", "nations", "preseason", "open", "other"];

/** The big letters on a card: the kind, and its number where it has one. */
function artLabel(x: LeagueCup): string {
  const n = /#(\d+)/.exec(x.name)?.[1];
  const abbr: Record<Kind, string> = { "one-night": "1 night", experimental: "EC", nations: "NC", preseason: "Pre", open: "Open", other: "Cup" };
  return n ? `${abbr[x.kind]} #${n}` : abbr[x.kind];
}

function dates(x: { first: number | null; last: number | null }): string {
  if (x.first === null || x.last === null) return "";
  const a = formatDate(x.first, true);
  const b = formatDate(x.last, true);
  return a === b ? a : `${a} – ${b}`;
}

export function TournamentsGrid({ onOpen }: { onOpen: (name: string) => void }) {
  const q = useQuery({ queryKey: ["tournaments"], queryFn: api.getTournaments, staleTime: 5 * 60_000 });
  const [kind, setKind] = useState<Kind | "all">("all");
  const all = q.data ?? [];
  const kinds = KIND_ORDER.filter((x) => all.some((y) => y.kind === x));
  const shown = kind === "all" ? all : all.filter((x) => x.kind === kind);
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">{t("ETF2L Highlander")}</span>
        <h1>{t("Tournaments")}</h1>
        <p className="hint">{t("The cups beside the seasons: one-night cups, the Experimental and Nations' Cups and the preseason cups. They give no medal or division; pick one for its brackets and who won them.")}</p>
      </header>
      {q.isPending && <Loading>{t("Loading tournaments…")}</Loading>}
      {q.isError && <ApiError error={q.error} />}
      {q.data && all.length === 0 && (
        <section className="panel">
          <p className="hint">{t("No tournaments yet. They are read with the seasons, on the next sync.")}</p>
        </section>
      )}
      {kinds.length > 1 && (
        <div className="e2-filters">
          <Seg
            label={t("Kind")}
            value={kind}
            onChange={setKind}
            options={[{ v: "all" as const, label: tx("All ({0})", { "0": all.length }) }, ...kinds.map((x) => ({ v: x, label: `${t(KIND_LABEL[x])} (${all.filter((y) => y.kind === x).length})` }))]}
          />
        </div>
      )}
      {shown.length > 0 && (
        <div className="ts-grid">
          {shown.map((x) => (
            <TournamentCard key={x.name} x={x} onOpen={() => onOpen(x.name)} />
          ))}
        </div>
      )}
    </>
  );
}

function TournamentCard({ x, onOpen }: { x: LeagueCup; onOpen: () => void }) {
  const more = x.winners.length - 3;
  return (
    <button className={`ts-card tn-card tn-k-${x.kind}`} onClick={onOpen} aria-label={t("Open {0}", { "0": x.name })}>
      <span className="ts-art">
        <span className="ts-art-img" aria-hidden />
        <span className="ts-art-label">{artLabel(x)}</span>
        <span className="ts-chips">
          <span className="ts-chip">{t(KIND_LABEL[x.kind])}</span>
        </span>
      </span>
      <span className="ts-body">
        <span className="ts-title">{x.name}</span>
        <span className="ts-dates">{dates(x)}</span>
        <span className="ts-stats">
          <span className="ts-stat">{tx("{0} teams", { "0": x.teams })}</span>
          <span className="ts-stat">{x.matches === 1 ? t("1 match") : tx("{0} matches", { "0": x.matches })}</span>
        </span>
        <span className="ts-rows">
          {x.winners.length === 0 && (
            <span className="ts-row ts-row-champ">
              <span className="ts-row-label">{t("Winner")}</span>
              <span className="ts-row-value muted">{t("no final played")}</span>
            </span>
          )}
          {x.winners.slice(0, 3).map((w) => (
            <span key={w.bracket} className="ts-row ts-row-champ">
              <span className="ts-row-label" title={w.bracket}>
                {x.brackets.length > 1 ? w.bracket : t("Winner")}
              </span>
              <span className="ts-row-value">
                <MedalGlyph size={16} place={1} />
                <TeamAvatar src={w.team.avatar} />
                <strong>{w.team.name}</strong>
              </span>
            </span>
          ))}
          {more > 0 && <span className="muted tn-more">{more === 1 ? t("and 1 more bracket") : tx("and {0} more brackets", { "0": more })}</span>}
        </span>
        <span className="ts-open">
          {t("Open tournament")}
          <span className="ts-open-arrow" aria-hidden>
            ›
          </span>
        </span>
      </span>
    </button>
  );
}

/** One tournament: who won each bracket, then each bracket round by round. */
export function TournamentScreen({ name, onTeam, onMatch }: { name: string; onTeam: (id: number) => void; onMatch: (id: number) => void }) {
  const q = useQuery({ queryKey: ["tournament", name], queryFn: () => api.getTournament(name), staleTime: 5 * 60_000 });
  const my = new Set(useYourTeams().map((e) => e.teamId));
  const [picked, setPicked] = useState<string | null>(null);
  const x = q.data ?? null;
  const bracket = x && picked && x.brackets.includes(picked) ? picked : (x?.brackets[0] ?? null);
  // The picked bracket's rounds, in the order they were played.
  const rounds = useMemo(() => {
    const out: Array<[string, CupResult[]]> = [];
    for (const r of (x?.results ?? []).filter((r) => r.bracket === bracket)) {
      const k2 = r.round ?? r.stage;
      const at = out.find(([n]) => n === k2);
      if (at) at[1].push(r);
      else out.push([k2, [r]]);
    }
    return out;
  }, [x, bracket]);
  if (q.isPending) return <Loading>{t("Loading the tournament…")}</Loading>;
  if (q.isError) return <ApiError error={q.error} />;
  if (!x) return <p className="hint">{t("This tournament is not in the league data.")}</p>;
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">{t(KIND_LABEL[x.kind])}</span>
        <h1>{x.name}</h1>
        <p className="hint">
          {dates(x)} · {tx("{0} teams", { "0": x.teams })} · {x.matches === 1 ? t("1 match") : tx("{0} matches", { "0": x.matches })}
        </p>
      </header>
      {x.winners.length > 0 && (
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <h2>{x.winners.length === 1 ? t("Winner") : t("Winners")}</h2>
          </div>
          <ul className="tn-winners">
            {x.winners.map((w) => (
              <li key={w.bracket}>
                <MedalGlyph size={20} place={1} />
                <button className="link" onClick={() => onTeam(w.team.id)}>
                  <TeamAvatar src={w.team.avatar} />
                  <strong>{w.team.name}</strong>
                </button>
                {x.brackets.length > 1 && <span className="muted">{w.bracket}</span>}
              </li>
            ))}
          </ul>
        </section>
      )}
      <h2 className="ts-section">{t("Brackets & results")}</h2>
      {x.brackets.length > 1 && (
        <div className="e2-rounds" role="tablist" aria-label={t("Bracket")}>
          {x.brackets.map((b) => (
            <button key={b} role="tab" aria-selected={b === bracket} className={b === bracket ? "on" : undefined} onClick={() => setPicked(b)}>
              {b}
            </button>
          ))}
        </div>
      )}
      <div className="e2-round-grid">
        {rounds.map(([round, list]) => (
          <section key={round} className="panel e2-panel">
            <div className="e2-panel-head">
              <h2>{round}</h2>
              <span className="muted">{list.length === 1 ? t("1 match") : tx("{0} matches", { "0": list.length })}</span>
            </div>
            <ol className="e2-results-list e2-round-list tn-round-list">
              {list.map((r) =>
                r.r1 !== null || r.defaultWin ? (
                  <ResultLine key={r.matchId} r={r} mine={my} onTeam={onTeam} onOpen={() => onMatch(r.matchId)} />
                ) : (
                  <UpcomingLine key={r.matchId} r={r} mine={my} onTeam={onTeam} onOpen={() => onMatch(r.matchId)} />
                ),
              )}
            </ol>
          </section>
        ))}
      </div>
    </>
  );
}
