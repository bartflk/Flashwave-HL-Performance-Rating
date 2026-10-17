import { useEffect, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type CatalogueHit } from "../../api/types";
import { capitalize, formatDate, rating, ratingPercent, signed, splitMap } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";
import "./players.css";
import { t, tx } from "../../lib/i18n";
import { DivisionBadge, PlayerProfile } from "./PlayerProfile";
import { TopPlayers } from "./PlayerStats";
import { MedalGlyph } from "./MedalGlyph";
import { onOpenPlayer, takePendingPlayer } from "../../lib/goto";

/**
 * Look other people up (Q14).
 *
 * Everything here is drawn from matches you played in. Seventeen other
 * players are in every Highlander log you have stored, all of them already
 * rated by the same model against the same pool — so this needed no new
 * model and no new downloading, only a different `WHERE account_id`.
 *
 * What it deliberately does not do is pretend to be trends.tf. It cannot
 * see a game you were not in, so every count is "in your matches" and says
 * so. Fetching a stranger's whole history is somebody else's seven hundred
 * logs per lookup, and logs.tf's patience is finite.
 */
export function PlayersPage({ onOpenMatch }: { onOpenMatch: (logId: number) => void }) {
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<number | null>(() => takePendingPlayer());
  // "Open profile" on a match page's player card (Q40).
  useEffect(
    () =>
      onOpenPlayer((id) => {
        setQuery("");
        setPicked(id);
        window.scrollTo({ top: 0 });
      }),
    [],
  );

  // Only search once there is enough to search for: one letter matches half
  // the database and the answer is never what anyone wanted.
  const term = query.trim();
  const hits = useQuery({
    queryKey: ["search_catalogue", term],
    queryFn: () => api.searchCatalogue(term),
    enabled: term.length >= 2,
    placeholderData: keepPreviousData,
  });
  // And everyone in thirteen years of logs.tf, from the summary built in.
  const older = useQuery({
    queryKey: ["logstf_search", term],
    queryFn: () => api.logstfSearch(term),
    enabled: term.length >= 2,
    placeholderData: keepPreviousData,
    staleTime: Infinity,
  });
  const known = new Set((hits.data ?? []).map((h) => h.accountId));
  const history = (older.data ?? []).filter((p) => !known.has(p.accountId));

  return (
    <div className="content players">
      <div className="panel">
        <h2>{t("Look someone up")}</h2>
        <p className="hint" style={{ marginTop: 6 }}>{t("Anyone who played an ETF2L official in the last six years, is in your matches, or played Highlander on logs.tf since 2012. Any name they used, or a Steam ID.")}</p>
        <input
          className="player-search"
          value={query}
          placeholder={t("A name, or 76561198… / [U:1:…]")}
          onChange={(e) => {
            setQuery(e.target.value);
            setPicked(null);
          }}
          autoFocus
        />
        {term.length >= 2 && (
          <Results hits={hits.data ?? []} loading={hits.isFetching} onPick={setPicked} picked={picked} />
        )}
        {term.length >= 2 && history.length > 0 && (
          <div className="lt-hits">
            <h3>
              {t("In all of logs.tf")} <span className="muted">{t("Highlander players since 2012, by any name they used")}</span>
            </h3>
            <div className="player-hits">
              {history.map((p) => (
                <button key={p.accountId} className={`player-hit${picked === p.accountId ? " active" : ""}`} onClick={() => setPicked(p.accountId)} aria-pressed={picked === p.accountId}>
                  {p.mainClass ? <ClassIcon cls={p.mainClass} size={22} /> : <span style={{ width: 22 }} />}
                  <span className="ph-name">
                    {p.name}
                    {p.aliases.length > 0 && <span className="lt-aka"> · {p.aliases.slice(0, 3).join(" · ")}</span>}
                  </span>
                  <span className="ph-meta muted">
                    {tx("{0} HL logs", { "0": p.logs.toLocaleString() })} · {p.firstYear === p.lastYear ? p.firstYear : `${p.firstYear}–${p.lastYear}`}
                  </span>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      {picked !== null && <PlayerProfile key={picked} accountId={picked} yours={<PlayerCard accountId={picked} onOpenMatch={onOpenMatch} />} />}
      {picked === null && <TopPlayers onPick={setPicked} />}
    </div>
  );
}

function Results({
  hits,
  loading,
  picked,
  onPick,
}: {
  hits: CatalogueHit[];
  loading: boolean;
  picked: number | null;
  onPick: (id: number) => void;
}) {
  if (hits.length === 0) {
    return (
      <p className="hint" style={{ marginTop: 14 }}>
        {loading ? t("Looking…") : t("Nobody by that name in the ETF2L officials read or in your matches.")}
      </p>
    );
  }
  return (
    <div className="player-hits">
      {hits.map((h) => (
        <button
          key={h.accountId}
          // Never played an official: from your matches only, greyed (Flashy, Q74).
          className={`player-hit${picked === h.accountId ? " active" : ""}${h.officials === 0 ? " out" : ""}`}
          title={h.officials === 0 ? t("Not in the league: only in your matches") : undefined}
          onClick={() => onPick(h.accountId)}
          aria-pressed={picked === h.accountId}
        >
          {h.mainClass ? <ClassIcon cls={h.mainClass} size={22} /> : <span style={{ width: 22 }} />}
          <span className="ph-name">{h.name}</span>
          {h.highest && <DivisionBadge d={h.highest} />}
          <span className="ph-medals" aria-label={t("Medals")}>
            {h.medals.map((n, i) =>
              n > 0 ? (
                <span key={i} className={`ph-medal pp-${["gold", "silver", "bronze"][i]}`} title={[t("Gold"), t("Silver"), t("Bronze")][i]}>
                  <MedalGlyph size={16} place={i + 1} />
                  {n}
                </span>
              ) : null,
            )}
          </span>
          <span className="ph-meta muted">
            {h.officials > 0 ? tx("{0} official{1}", { "0": h.officials, "1": h.officials === 1 ? "" : "s" }) : t("your matches")}
            {h.lastSeen !== null && t(" · last {0}", { "0": formatDate(h.lastSeen, true) })}
          </span>
        </button>
      ))}
    </div>
  );
}

/** One player, once picked. */
function PlayerCard({ accountId, onOpenMatch }: { accountId: number; onOpenMatch: (logId: number) => void }) {
  const [cls, setCls] = useState<string | null>(null);
  const q = useQuery({
    queryKey: ["player", accountId, cls],
    queryFn: () => api.getPlayer(accountId, cls),
    placeholderData: keepPreviousData,
  });
  // A regular teammate's line from your side: how you did together.
  const mates = useQuery({ queryKey: ["teammates", true], queryFn: () => api.getTeammates(true), staleTime: 5 * 60_000 });

  if (q.isPending) return <div className="panel"><p className="hint">{t("Loading…")}</p></div>;
  if (q.isError) return <div className="panel"><p className="error">{errorMessage(q.error)}</p></div>;

  const { summary: s, profile } = q.data!;
  const shown = q.data!.class;
  const mate = mates.data?.teammates.find((m) => m.accountId === accountId);

  return (
    <>
      <div className="panel player-head">
        <div className="pl-top">
          <div>
            <h2>{s.name}</h2>
            <p className="hint">
              {tx("{games} game{1} in your matches{2}", { "1": s.games === 1 ? "" : "s", "2": s.firstSeen !== null && s.lastSeen !== null && (
                <>{tx(", {0} to {1}", { "0": formatDate(s.firstSeen, true), "1": formatDate(s.lastSeen, true) })}
                </>
              ), games: s.games })}
            </p>
            {s.alsoKnownAs.length > 0 && (
              <p className="hint muted">{tx("Also as {0}", { "0": s.alsoKnownAs.join(", ") })}</p>
            )}
          </div>
          <div className="pl-ids">
            <a href={`https://logs.tf/profile/${s.steamid64}`} target="_blank" rel="noreferrer">{t("logs.tf ↗")}</a>
            <a href={`https://steamcommunity.com/profiles/${s.steamid64}`} target="_blank" rel="noreferrer">{t("Steam ↗")}</a>
          </div>
        </div>

        <dl className="kv pl-met">
          <dt>{t("On your team")}</dt>
          <dd>{s.withYou}</dd>
          <dt>{t("Against you")}</dt>
          <dd>
            {s.againstYou}
            {s.againstYou > 0 && (
              <span className="muted">
                {tx("{0}· {youBeatThem}–{theyBeatYou} to you", { "0": " ", youBeatThem: s.youBeatThem, theyBeatYou: s.theyBeatYou })}</span>
            )}
          </dd>
          {mate && (
            <>
              <dt>{t("Together")}</dt>
              <dd>
                {mate.wins}–{mate.losses}
                <span className="muted"> · {formatDate(mate.firstPlayed, true)} – {formatDate(mate.lastPlayed, true)}</span>
              </dd>
              {mate.myAvgWith !== null && (
                <>
                  <dt title={t("Your average rating in games with them, and the difference from your other games")}>{t("Your rating with them")}</dt>
                  <dd>
                    {rating(mate.myAvgWith)}{" "}
                    <span className={mate.myAvgDelta! > 0 ? "delta up" : mate.myAvgDelta! < 0 ? "delta down" : "delta"}>({signed(mate.myAvgDelta!, 2)})</span>
                  </dd>
                </>
              )}
            </>
          )}
        </dl>

        {s.classes.length > 0 && (
          <div className="pl-classes">
            {s.classes.map((c) => (
              <button
                key={c.class}
                className={shown === c.class ? "pl-class active" : "pl-class"}
                onClick={() => setCls(c.class)}
                aria-pressed={shown === c.class}
              >
                <ClassIcon cls={c.class} size={20} />
                <span className="pl-class-name">{capitalize(c.class)}</span>
                <span className="pl-class-rating">{rating(c.avg)}</span>
                <span className="pl-track" aria-hidden>
                  <span className="comp-mid" />
                  <span className="pl-fill" style={{ width: `${ratingPercent(c.avg)}%` }} />
                </span>
                <span className="muted pl-class-games">{c.games}</span>
              </button>
            ))}
          </div>
        )}
      </div>

      {profile && (
        <div className="panel">
          <h2>
            {capitalize(shown ?? "")} · {rating(profile.careerAvg)}
          </h2>
          <p className="hint" style={{ marginTop: 6 }}>{tx("Over {games} rated game{1}{2}.", { "1": profile.games === 1 ? "" : "s", "2": profile.winRate !== null && t(", {0}% won", { "0": profile.winRate.toFixed(0) }), games: profile.games })}</p>
          <div className="pl-lists">
            <GameList title={t("Their best")} games={profile.best} onOpen={onOpenMatch} />
            <GameList title={t("Their worst")} games={profile.worst} onOpen={onOpenMatch} />
          </div>
        </div>
      )}
    </>
  );
}

function GameList({
  title,
  games,
  onOpen,
}: {
  title: string;
  games: Array<{ logId: number; score: number; playedAt: number | null; map: string | null }>;
  onOpen: (logId: number) => void;
}) {
  if (games.length === 0) return null;
  return (
    <section className="pl-list">
      <h3>{title}</h3>
      {games.map((g) => (
        <button key={g.logId} className="pl-game" onClick={() => onOpen(g.logId)}>
          <span className="pl-game-rating">{rating(g.score)}</span>
          <span className="pl-game-map">{splitMap(g.map ?? "").name ?? t("unknown")}</span>
          <span className="muted">{g.playedAt === null ? "" : formatDate(g.playedAt, true)}</span>
        </button>
      ))}
    </section>
  );
}
