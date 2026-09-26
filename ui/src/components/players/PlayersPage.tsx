import { useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type PlayerHit } from "../../api/types";
import { capitalize, formatDate, rating, ratingPercent, splitMap } from "../../lib/format";
import { ClassIcon } from "../ClassIcon";
import "./players.css";

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
  const [picked, setPicked] = useState<number | null>(null);

  // Only search once there is enough to search for: one letter matches half
  // the database and the answer is never what anyone wanted.
  const term = query.trim();
  const hits = useQuery({
    queryKey: ["search_players", term],
    queryFn: () => api.searchPlayers(term),
    enabled: term.length >= 2,
    placeholderData: keepPreviousData,
  });

  return (
    <div className="content players">
      <div className="panel">
        <h2>Look someone up</h2>
        <p className="hint" style={{ marginTop: 6 }}>
          Anyone who has played in a match you have stored — teammates, opponents, mercs. Search by name
          or paste a Steam ID. Their record here is their record <strong>in your matches</strong>, not
          their whole career.
        </p>
        <input
          className="player-search"
          value={query}
          placeholder="A name, or 76561198… / [U:1:…]"
          onChange={(e) => {
            setQuery(e.target.value);
            setPicked(null);
          }}
          autoFocus
        />
        {term.length >= 2 && (
          <Results hits={hits.data ?? []} loading={hits.isFetching} onPick={setPicked} picked={picked} />
        )}
      </div>

      {picked !== null && <PlayerCard accountId={picked} onOpenMatch={onOpenMatch} />}
    </div>
  );
}

function Results({
  hits,
  loading,
  picked,
  onPick,
}: {
  hits: PlayerHit[];
  loading: boolean;
  picked: number | null;
  onPick: (id: number) => void;
}) {
  if (hits.length === 0) {
    return (
      <p className="hint" style={{ marginTop: 14 }}>
        {loading ? "Looking…" : "Nobody by that name has played in your matches."}
      </p>
    );
  }
  return (
    <div className="player-hits">
      {hits.map((h) => (
        <button
          key={h.accountId}
          className={picked === h.accountId ? "player-hit active" : "player-hit"}
          onClick={() => onPick(h.accountId)}
          aria-pressed={picked === h.accountId}
        >
          {h.topClass ? <ClassIcon cls={h.topClass} size={22} /> : <span style={{ width: 22 }} />}
          <span className="ph-name">{h.name}</span>
          <span className="ph-meta muted">
            {h.games} game{h.games === 1 ? "" : "s"}
            {h.lastSeen !== null && ` · last ${formatDate(h.lastSeen, true)}`}
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

  if (q.isPending) return <div className="panel"><p className="hint">Loading…</p></div>;
  if (q.isError) return <div className="panel"><p className="error">{errorMessage(q.error)}</p></div>;

  const { summary: s, profile } = q.data!;
  const shown = q.data!.class;

  return (
    <>
      <div className="panel player-head">
        <div className="pl-top">
          <div>
            <h2>{s.name}</h2>
            <p className="hint">
              {s.games} game{s.games === 1 ? "" : "s"} in your matches
              {s.firstSeen !== null && s.lastSeen !== null && (
                <>
                  , {formatDate(s.firstSeen, true)} to {formatDate(s.lastSeen, true)}
                </>
              )}
            </p>
            {s.alsoKnownAs.length > 0 && (
              <p className="hint muted">Also as {s.alsoKnownAs.join(", ")}</p>
            )}
          </div>
          <div className="pl-ids">
            <a href={`https://logs.tf/profile/${s.steamid64}`} target="_blank" rel="noreferrer">
              logs.tf ↗
            </a>
            <a href={`https://steamcommunity.com/profiles/${s.steamid64}`} target="_blank" rel="noreferrer">
              Steam ↗
            </a>
          </div>
        </div>

        <dl className="kv pl-met">
          <dt>On your team</dt>
          <dd>{s.withYou}</dd>
          <dt>Against you</dt>
          <dd>
            {s.againstYou}
            {s.againstYou > 0 && (
              <span className="muted">
                {" "}
                · {s.youBeatThem}–{s.theyBeatYou} to you
              </span>
            )}
          </dd>
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
        <p className="hint pl-note">
          Rated by the same model as you, against the same pool — which is built from these players.
          One difference worth knowing: you are held out of your own baseline so you are never compared
          with yourself, and everyone else is in it.
        </p>
      </div>

      {profile && (
        <div className="panel">
          <h2>
            {capitalize(shown ?? "")} · {rating(profile.careerAvg)}
          </h2>
          <p className="hint" style={{ marginTop: 6 }}>
            Over {profile.games} rated game{profile.games === 1 ? "" : "s"}
            {profile.winRate !== null && `, ${profile.winRate.toFixed(0)}% won`}.
          </p>
          <div className="pl-lists">
            <GameList title="Their best" games={profile.best} onOpen={onOpenMatch} />
            <GameList title="Their worst" games={profile.worst} onOpen={onOpenMatch} />
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
          <span className="pl-game-map">{splitMap(g.map ?? "").name ?? "unknown"}</span>
          <span className="muted">{g.playedAt === null ? "" : formatDate(g.playedAt, true)}</span>
        </button>
      ))}
    </section>
  );
}
