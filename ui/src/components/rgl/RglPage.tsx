import { useEffect, useRef, useState, type ReactNode } from "react";
import { t } from "../../lib/i18n";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { RglMark } from "../RglMark";
import { LeagueHead } from "../LeagueHead";
import { onOpenRgl, takePendingRgl } from "../../lib/goto";
import "../players/players.css";
import "../teams/teams.css";
import "../etf2l/etf2l.css";
import "./rgl.css";
import { useSeasons, useTeam, useMatch, SEARCH_FROM, useNameSearch, usePlayer, looksLikeSteamId } from "./data";
import type { Loc, Section } from "./data";
import { SearchHits } from "./bits";
import { SeasonList, SeasonView, SeasonPlay } from "./Seasons";
import { LiveMatches, MatchesPage } from "./Matches";
import { MatchPage } from "./MatchPage";
import { TeamsLanding, TeamView } from "./Team";
import { PlayersLanding, PlayerView, YourRgl } from "./Players";
import { BansPage } from "./Bans";

/**
 * The RGL tab (PLAN §30), in the ETF2L tab's frame (Flashy: "i do want to
 * follow the template of the etf2l page"): a menu, breadcrumbs, a search,
 * and the mouse's back button stepping back within the tab. What RGL's API
 * answers by id is here -- seasons, a team's roster, results and other
 * seasons, a player's career by SteamID or name, your own RGL card -- read
 * live, kept for half an hour, and on disk once a season is over
 * (hl_ingest::rgl says how RGL is asked as little as it can be).
 */

function sectionOf(l: Loc): Section {
  if (l.at === "season") return "seasons";
  if (l.at === "team" || l.at === "player" || l.at === "match") return sectionOf(l.from);
  return l.at;
}

/*
 * A team, player or match page goes back to the page it was opened from, and
 * that page is in the trail above it: opening Team B from Team A reads
 * "... > Team A > Team B", so Back and the trail say the same thing.
 */

const same = (a: Loc, b: Loc) => JSON.stringify(a) === JSON.stringify(b);
/** What the full tab will add, shown until it is built. */
const coming = (): [string, string][] => [
  [t("Through the app"), t("RGL on profiles and the scoreboard, and RGL officials counted as officials.")],
];

const SEEN_KEY = "rgl-in-development-seen";

/** Said already in this run, for when storage is blocked and cannot say. */
let seenThisRun = false;

function seen(): boolean {
  if (seenThisRun) return true;
  try {
    return localStorage.getItem(SEEN_KEY) === "1";
  } catch {
    return false;
  }
}

/**
 * Said once, the first time the tab is opened (Flashy: "put a disclaimer rgl
 * tab is in development, a modal/dialog when you first click it, transparent
 * darker background so it focuses you").
 */
function InDevelopment() {
  const [open, setOpen] = useState(() => !seen());
  const ok = useRef<HTMLButtonElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const close = () => {
    seenThisRun = true;
    try {
      localStorage.setItem(SEEN_KEY, "1");
    } catch {
      // It says so again next time, which is harmless.
    }
    setOpen(false);
  };
  useEffect(() => {
    if (!open) return;
    ok.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
      // Tab stays inside the dialog: behind it the page is not reachable.
      if (e.key === "Tab" && card.current) {
        const items = [...card.current.querySelectorAll<HTMLElement>("button, a[href], input, select, textarea, [tabindex]:not([tabindex='-1'])")].filter((x) => !x.hasAttribute("disabled"));
        if (items.length === 0) return;
        const first = items[0];
        const last = items[items.length - 1];
        const at = document.activeElement;
        if (!card.current.contains(at) || (e.shiftKey && at === first)) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && at === last) {
          e.preventDefault();
          first.focus();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);
  if (!open) return null;
  return (
    <div className="modal-scrim focus" role="dialog" aria-modal="true" aria-labelledby="rgl-dev-title" onClick={(e) => e.target === e.currentTarget && close()}>
      <div className="modal rgl-dev" ref={card}>
        <span className="e2-eyebrow">
          <RglMark size={16} /> {t("RGL Highlander")}
        </span>
        <h3 id="rgl-dev-title">{t("The RGL tab is in development")}</h3>
        <p>{t("This is an early look. Seasons, teams, players, matches and bans are read from RGL's public API, and each match's logs are found on logs.tf; RGL on your profile and the scoreboard is not here yet.")}</p>
        <p className="muted">{t("Things may change, be missing or not work as they should. What is still to come is listed under Coming soon.")}</p>
        <div className="modal-actions">
          <button ref={ok} className="primary" onClick={close}>
            {t("Got it")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function RglPage({ active, steamid, onOpenMatch }: { active: boolean; steamid: string | null; onOpenMatch: (logId: number) => void }) {
  const [stack, setStack] = useState<Loc[]>([{ at: "home" }]);
  const loc = stack[stack.length - 1];
  const go = (l: Loc) => {
    setStack((s) => (same(s[s.length - 1], l) ? s : [...s, l].slice(-50)));
    window.scrollTo({ top: 0 });
  };
  const openTeam = (id: number) => {
    setStack((s) => [...s, { at: "team", id, from: s[s.length - 1] }]);
    window.scrollTo({ top: 0 });
  };
  const openPlayer = (steamId: string) => {
    setStack((s) => [...s, { at: "player", steamId, from: s[s.length - 1] }]);
    window.scrollTo({ top: 0 });
  };
  const openMatch = (id: number) => {
    setStack((s) => [...s, { at: "match", id, from: s[s.length - 1] }]);
    window.scrollTo({ top: 0 });
  };

  // A bookmark's team or season, asked for before or while the tab is shown.
  const goRef = useRef<(to: { kind: "team" | "season"; id: number }) => void>(() => {});
  useEffect(() => {
    goRef.current = (to) => (to.kind === "team" ? setStack((s) => [...s, { at: "team", id: to.id, from: { at: "home" } }]) : go({ at: "season", id: to.id }));
  });
  useEffect(() => {
    const pending = takePendingRgl();
    if (pending) goRef.current(pending);
    return onOpenRgl((to) => goRef.current(to));
  }, []);

  // Mouse 4 / Alt+Left step back within the tab first, as in the ETF2L tab.
  const depth = useRef(stack.length);
  const activeRef = useRef(active);
  useEffect(() => {
    depth.current = stack.length;
    activeRef.current = active;
  });
  useEffect(() => {
    const can = () => activeRef.current && depth.current > 1;
    const pop = () => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s));
    const onMouse = (e: MouseEvent) => {
      if (e.button !== 3 || !can()) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.type === "mouseup") pop();
    };
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || e.key !== "ArrowLeft" || !can()) return;
      e.preventDefault();
      e.stopPropagation();
      pop();
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

  const section = sectionOf(loc);
  const menu: Array<[Section, string]> = [
    ["home", t("Home")],
    ["matches", t("Matches")],
    ["seasons", t("Seasons")],
    ["teams", t("Teams")],
    ["players", t("Players")],
    ["bans", t("Bans")],
  ];

  return (
    <div className="content e2-page">
      {active && <InDevelopment />}
      <LeagueHead>
        <div className="e2-brand">
          <span className="e2-eyebrow" data-fit="">
            <RglMark size={16} /> {t("RGL Highlander")}
          </span>
          <nav className="e2-menu" data-fit="" aria-label={t("RGL")}>
            {menu.map(([id, label]) => (
              <button key={id} className={section === id ? "on" : undefined} aria-current={section === id ? "page" : undefined} onClick={() => go({ at: id })}>
                {label}
              </button>
            ))}
          </nav>
        </div>
        <div className="e2-head-tools" data-fit="300">
          <RglSearch onTeam={openTeam} onPlayer={openPlayer} />
        </div>
      </LeagueHead>
      <Crumbs loc={loc} go={go} />

      {loc.at === "home" && <Home steamid={steamid} onTeam={openTeam} onPlayer={openPlayer} onMatch={openMatch} go={go} />}
      {loc.at === "matches" && <MatchesPage onTeam={openTeam} onMatch={openMatch} />}
      {loc.at === "match" && <MatchPage key={loc.id} id={loc.id} onTeam={openTeam} onMatch={openMatch} onPlayer={openPlayer} onOpenLog={onOpenMatch} />}
      {loc.at === "seasons" && <SeasonList onSeason={(id) => go({ at: "season", id })} />}
      {loc.at === "season" && (
        <>
          <SeasonView id={loc.id} onTeam={openTeam} />
          <SeasonPlay id={loc.id} onTeam={openTeam} onMatch={openMatch} />
        </>
      )}
      {loc.at === "teams" && <TeamsLanding steamid={steamid} onTeam={openTeam} />}
      {loc.at === "bans" && <BansPage onPlayer={openPlayer} />}
      {loc.at === "team" && <TeamView key={loc.id} id={loc.id} onTeam={openTeam} onPlayer={openPlayer} onMatch={openMatch} />}
      {loc.at === "players" && <PlayersLanding steamid={steamid} onPlayer={openPlayer} onTeam={openTeam} />}
      {loc.at === "player" && <PlayerView key={loc.steamId} steamId={loc.steamId} onTeam={openTeam} onMatch={openMatch} />}
    </div>
  );
}

// ---- Breadcrumbs ---------------------------------------------------------

/** A trail link's name, from what the pages already read (kept, so no new asks). */
function CrumbName({ loc }: { loc: Loc }) {
  const seasons = useSeasons().data;
  const team = useTeam(loc.at === "team" ? loc.id : null).data;
  const player = usePlayer(loc.at === "player" ? loc.steamId : null).data;
  const match = useMatch(loc.at === "match" ? loc.id : null).data;
  if (loc.at === "team") return <>{team?.name ?? t("Team")}</>;
  if (loc.at === "player") return <>{player?.name ?? t("Player")}</>;
  if (loc.at === "match") return <>{match && match.teams.length >= 2 ? `${match.teams[0].teamName} vs ${match.teams[1].teamName}` : t("Match")}</>;
  if (loc.at === "season") return <>{seasons?.find((s) => s.id === loc.id)?.name ?? t("Season")}</>;
  return null;
}

function Crumbs({ loc, go }: { loc: Loc; go: (l: Loc) => void }) {
  const trail: Array<[ReactNode, Loc]> = [];
  const add = (l: Loc) => {
    if (l.at === "team" || l.at === "match" || l.at === "player") {
      add(l.from);
      trail.push([<CrumbName key={trail.length} loc={l} />, l]);
    } else if (l.at === "season") {
      trail.push([t("Seasons"), { at: "seasons" }]);
      trail.push([<CrumbName key={trail.length} loc={l} />, l]);
    } else if (l.at !== "home") {
      trail.push([{ matches: t("Matches"), seasons: t("Seasons"), teams: t("Teams"), players: t("Players"), bans: t("Bans") }[l.at], l]);
    }
  };
  add(loc);
  if (trail.length === 0) return null;
  return (
    <nav className="e2-crumbs" aria-label={t("Where you are")}>
      <button onClick={() => go({ at: "home" })}>{t("RGL")}</button>
      {trail.map(([label, l], i) => (
        <span key={i} className="e2-crumb">
          <span className="e2-sep" aria-hidden>
            ›
          </span>
          {i === trail.length - 1 ? <span aria-current="page">{label}</span> : <button onClick={() => go(l)}>{label}</button>}
        </span>
      ))}
    </nav>
  );
}

// ---- Search: a SteamID opens the player, a name finds teams ---------------

function RglSearch({ onTeam, onPlayer }: { onTeam: (id: number) => void; onPlayer: (steamId: string) => void }) {
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const term = q.trim();
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", down);
    return () => document.removeEventListener("mousedown", down);
  }, [open]);
  const done = () => {
    setOpen(false);
    setQ("");
  };
  const id = looksLikeSteamId(term);
  const found = useNameSearch(open ? term : "");
  return (
    <div
      className="e2-search"
      ref={box}
      onBlur={(e) => {
        // Tabbing out of the box closes the panel; moving to a hit inside does not.
        if (!box.current?.contains(e.relatedTarget as Node | null)) setOpen(false);
      }}
    >
      <input
        type="search"
        value={q}
        placeholder={t("Player, team or SteamID")}
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={(e) => {
          if (e.key === "Escape") done();
          if (e.key === "Enter" && id) {
            done();
            onPlayer(term);
          } else if (e.key === "Enter") {
            // The first hit: a player before a team, as they are listed.
            const p = found.players[0];
            const tm = found.teams[0];
            if (p) {
              done();
              onPlayer(p.steamId);
            } else if (tm) {
              done();
              onTeam(tm.id);
            }
          }
        }}
        aria-label={t("Search RGL")}
      />
      {open && term.length > 0 && (
        <div className="e2-search-panel">
          {id ? (
            <button
              className="e2-hit"
              onClick={() => {
                done();
                onPlayer(term);
              }}
            >
              <span className="e2-hit-name">
                <RglMark size={12} /> {t("Open this player's RGL profile")}
              </span>
              <span className="e2-hit-meta">↵</span>
            </button>
          ) : term.length < SEARCH_FROM ? (
            <p className="hint">{t("Type at least {0} letters.", { "0": SEARCH_FROM })}</p>
          ) : (
            <SearchHits
              found={found}
              onPlayer={(s) => {
                done();
                onPlayer(s);
              }}
              onTeam={(x) => {
                done();
                onTeam(x);
              }}
            />
          )}
        </div>
      )}
    </div>
  );
}

function Home(props: { steamid: string | null; onTeam: (id: number) => void; onPlayer: (s: string) => void; onMatch: (id: number) => void; go: (l: Loc) => void }) {
  const { steamid, onTeam, onPlayer, onMatch, go } = props;
  const seasons = useSeasons();
  const latest = seasons.data?.[0]?.id ?? null;
  return (
    <div className="e2-home">
      <div className="e2-main">
        <LiveMatches onTeam={onTeam} onMatch={onMatch} go={go} />
        {seasons.error ? (
          <section className="panel e2-panel">
            <ApiError error={seasons.error} />
          </section>
        ) : seasons.data && latest === null ? (
          <section className="panel e2-panel">
            <p className="hint">{t("RGL lists no seasons.")}</p>
          </section>
        ) : latest === null ? (
          <section className="panel e2-panel">
            <Loading>{t("Loading the season…")}</Loading>
          </section>
        ) : (
          <SeasonView id={latest} onTeam={onTeam} more={() => go({ at: "seasons" })} />
        )}
      </div>
      <aside className="e2-rail">
        {steamid && <YourRgl steamid={steamid} onTeam={onTeam} onPlayer={onPlayer} />}
        <section className="panel e2-panel">
          <div className="e2-panel-head">
            <h2>{t("Coming soon")}</h2>
          </div>
          <ul className="rgl-coming">
            {coming().map(([what, text]) => (
              <li key={what}>
                <strong>{what}</strong>
                <span className="muted">{text}</span>
              </li>
            ))}
          </ul>
          <p className="hint">{t("From RGL's public API, api.rgl.gg.")}</p>
        </section>
      </aside>
    </div>
  );
}
