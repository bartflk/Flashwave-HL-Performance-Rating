import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { Ledger, MatchContext, MatchDetail, MatchSides, SideTeam, TeamSeason } from "../../api/types";
import { capitalize, formatDate, minutes, splitMap, teamLabel } from "../../lib/format";
import { openTeam } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { KindPicker, kindReason } from "../ContextBadge";
import { BookmarkButton } from "../Bookmarks";
import { Country } from "../Country";
import { MedalGlyph } from "../players/MedalGlyph";
import { DivisionBadge, seasonLong, seasonShort } from "../players/PlayerProfile";
import { Backdrop } from "../Backdrop";

/**
 * The top of a match page, in the Teams tab's language: the two teams face
 * each other across the score, with ETF2L's logos, countries and how each
 * stood that season when the match is an official; RED and BLU otherwise.
 * The map's own overview sits faintly behind it.
 */
export function MatchHero({ d, ledger }: { d: MatchDetail; ledger?: Ledger | null }) {
  const sidesQ = useQuery({ queryKey: ["match_sides", d.logId], queryFn: () => api.getMatchSides(d.logId), staleTime: 5 * 60_000 });
  const sides = sidesQ.data ?? null;

  // The resolved maps: a combined log's own map field is whatever its
  // uploader typed. `?? []`: a part of a combined log once came without
  // segments, and a missing list must not take the page with it.
  const maps = [...new Set((d.segments ?? []).map((s) => s.map).filter((m): m is string => m !== null))];
  const single = maps.length === 1 ? maps[0] : maps.length === 0 ? d.map : null;
  const art = useQuery({ queryKey: ["overview_image", single], queryFn: () => api.overviewImage(single!), enabled: !!single, staleTime: Infinity }).data;

  const mine = d.myTeam;
  // A match you did not play: the ETF2L official it was, found by its time
  // and its players (Flashy: the S36 High final read as RED against BLU).
  const leagueQ = useQuery({ queryKey: ["league_sides", d.logId], queryFn: () => api.getLeagueSides(d.logId), enabled: !mine, staleTime: 10 * 60_000 });
  const league = mine ? null : (leagueQ.data ?? null);
  const left: "Red" | "Blue" = mine ?? "Red";
  const right: "Red" | "Blue" = left === "Red" ? "Blue" : "Red";
  const rounds = (side: "Red" | "Blue") => (side === "Red" ? d.redScore : d.blueScore);
  const c = d.context;
  const official = c?.kind === "official" ? c.official : null;
  // Q55 (Clark): an official's headline is ETF2L's result, your side first;
  // the logs' rounds, which in stopwatch are not the same, go beneath.
  const etf2l: [number, number] | null =
    official?.score && mine
      ? official.score
      : league && league.redScore !== null && league.blueScore !== null
        ? [league.redScore, league.blueScore]
        : null;
  const score = (side: "Red" | "Blue") => (etf2l ? (side === left ? etf2l[0] : etf2l[1]) : rounds(side));
  const result = etf2l ? (etf2l[0] > etf2l[1] ? "W" : etf2l[0] < etf2l[1] ? "L" : "T") : d.result;
  const roundsDiffer =!!etf2l && (etf2l[0] !== rounds(left) || etf2l[1] !== rounds(right));

  // Your side carries your team; the other side the opponent, once known.
  const leftTeam: SideTeam | null = mine ? sides?.team ?? null : (league?.red ?? null);
  const rightTeam: SideTeam | null = mine ? sides?.opp ?? null : (league?.blue ?? null);

  return (
    <header className={`panel mx-hero mx-${left.toLowerCase()}-left has-backdrop`}>
      <Backdrop />
      {art?.image && <div className="mx-art" style={{ backgroundImage: `url("${art.image}")` }} aria-hidden />}

      <div className="mx-top">
        <div className="mx-eyebrow">
          {c &&
            (official?.division ? (
              <KindPicker logId={d.logId} c={c}>
                <DivisionBadge d={{ name: official.division, tier: official.tier ?? 4 }} />
              </KindPicker>
            ) : (
              <KindPicker logId={d.logId} c={c} />
            ))}
          <span className="ts-eyebrow">
            {league ? [league.division, league.round, league.seasonName].filter(Boolean).join(" · ") : eyebrow(c, sides)}
          </span>
        </div>
        <div className="mx-badges">
          <BookmarkButton
            b={{
              kind: "match",
              id: d.logId,
              label: [maps.length > 0 ? maps.map((m) => splitMap(m).name ?? m).join(" + ") : splitMap(d.map ?? "").name ?? t("Match"), formatDate(d.playedAt)].join(" · "),
              sub: rightTeam?.name ? t("vs {0}", { "0": rightTeam.name }) : d.title,
            }}
          />
          {!c && d.league && <span className="badge badge-league">{d.league.toUpperCase()}</span>}
          {d.demos.some((x) => x.kind === "pov") && <span className="badge badge-pov">{t("POV demo")}</span>}
          {d.demosTfId && <span className="badge badge-demo">{t("STV demo")}</span>}
          {d.format && d.format !== "highlander" && <span className="badge">{d.format}</span>}
        </div>
      </div>

      <div className="mx-board">
        <Side side={left} team={leftTeam} you={!!mine} played={!!mine} season={sides?.season ?? null} align="left" />

        <div className="mx-center">
          {result && mine && <span className={`mx-result mx-result-${result}`}>{result === "W" ? t("Victory") : result === "L" ? t("Defeat") : t("Draw")}</span>}
          <div
            className="mx-score"
            aria-label={t("{0} to {1}", { "0": score(left), "1": score(right) })}
            title={etf2l ? t("ETF2L's result for the match") : undefined}
          >
            <span className={`mx-n team-${left.toLowerCase()}`}>{score(left)}</span>
            <span className="mx-colon">:</span>
            <span className={`mx-n team-${right.toLowerCase()}`}>{score(right)}</span>
          </div>
          <MapLine d={d} maps={maps} single={single} left={left} ledger={ledger ?? null} />
          {roundsDiffer && (
            <span className="hint" title={t("Rounds won in the logs. In stopwatch this is not the same as ETF2L's result.")}>
              {tx("Rounds in the logs {0}", { "0": <strong>{rounds(left)}–{rounds(right)}</strong> })}
            </span>
          )}
          {official?.defaultWin && <span className="warn-text">{t("default win")}</span>}
        </div>

        <Side side={right} team={rightTeam} you={false} played={!!mine} season={sides?.season ?? null} align="right" />
      </div>

      {leftTeam?.id != null && rightTeam?.id != null && (
        <Meetings team={leftTeam.id} opp={rightTeam.id} oppName={rightTeam.name} current={c?.etf2lMatchId ?? league?.matchId ?? null} />
      )}

      <footer className="mx-foot">
        <span className="ts-meta">
          <span>{formatDate(d.playedAt, true)}</span>
          <span className="ts-dot">·</span>
          <span>{minutes(d.durationS)}</span>
          {d.title && (
            <>
              <span className="ts-dot">·</span>
              <span className="mx-title" title={t("The title the log was uploaded with")}>“{d.title}”</span>
            </>
          )}
          {c && c.kind !== "official" && (
            <>
              <span className="ts-dot">·</span>
              <span>{kindReason(c)}</span>
            </>
          )}
          {c?.linkMethod === "roster" && (
            <>
              <span className="ts-dot">·</span>
              <span>{t("found by roster; trends.tf had not tagged it")}</span>
            </>
          )}
        </span>
        <Links d={d} />
      </footer>
    </header>
  );
}

/** "ETF2L Highlander · Season 36 (Autumn 2026) · Round 1", or the kind of game. */
function eyebrow(c: MatchContext | null, sides: MatchSides | null): string {
  const o = c?.official;
  if (c?.kind === "official" && o) {
    const parts = [o.category ?? t("ETF2L official")];
    if (sides?.season !== null && sides?.season !== undefined) parts.push(seasonLong(sides.season, sides.seasonName ?? ""));
    else if (o.competition) parts.push(o.competition);
    if (o.round) parts.push(o.round);
    return parts.join(" · ");
  }
  if (c?.kind === "scrim") return t("Scrim");
  if (c?.kind === "pug") return t("Pug");
  return t("Match");
}

/** One team: logo, name, country, its season so far; RED or BLU without one. */
function Side(props: { side: "Red" | "Blue"; team: SideTeam | null; you: boolean; played: boolean; season: number | null; align: "left" | "right" }) {
  const { side, team, you, played, season, align } = props;
  const honours = useQuery({
    queryKey: ["team_honours", team?.id],
    queryFn: () => api.getTeamHonours(team!.id!),
    enabled: team?.id != null && season !== null,
    staleTime: 5 * 60_000,
  }).data;
  const that: TeamSeason | undefined = season === null ? undefined : honours?.seasons.find((s) => s.season === season);
  // Unnamed: "Your team" and "Opponents" when you played, else RED and BLU
  // (the pill above says the colour either way).
  const name = team?.name ?? (played ? (you ? t("Your team") : t("Opponents")) : teamLabel(side));
  const open = team?.id != null ? () => openTeam(team.id!) : undefined;
  const logo = team?.avatar ? (
    <img src={team.avatar} alt="" />
  ) : (
    <span className="mx-logo-letter">{team ? team.name.slice(0, 1).toUpperCase() : side === "Red" ? "R" : "B"}</span>
  );
  return (
    <div className={`mx-side mx-side-${align} mx-side-${side.toLowerCase()}`}>
      {open ? (
        <button className="mx-logo" onClick={open} title={t("Open {0}", { "0": name })}>
          {logo}
        </button>
      ) : (
        <span className="mx-logo">{logo}</span>
      )}
      <div className="mx-who">
        <span className="mx-tags">
          <span className={`mx-colour mx-colour-${side.toLowerCase()}`}>{teamLabel(side)}</span>
          {you && <span className="mx-you">{t("You")}</span>}
        </span>
        {open ? (
          <button className="mx-name" onClick={open} title={t("Open {0}", { "0": name })}>
            {name}
          </button>
        ) : (
          <span className="mx-name">{name}</span>
        )}
        {/* One line of pills, one height (Flashy: "the country and the
            standings and medal are not lined up"). */}
        <span className="mx-facts">
          {team?.country && (
            <span className="mx-fact">
              <Country raw={team.country} />
            </span>
          )}
          {that && (
            <span className="mx-fact" title={t("Their regular season, as ETF2L's results stand")}>
              {that.place && <MedalGlyph size={15} place={that.place} />}
              <strong>
                {that.won}–{that.lost}
              </strong>
              <span className="muted">{tx("in {0}", { "0": that.division })}</span>
            </span>
          )}
        </span>
      </div>
    </div>
  );
}

/** Their other officials against each other, newest first, from your side. */
function Meetings({ team, opp, oppName, current }: { team: number; opp: number; oppName: string; current: number | null }) {
  const v = useQuery({ queryKey: ["team", team], queryFn: () => api.getTeam(team), staleTime: 5 * 60_000 }).data;
  const met = (v?.results ?? []).filter((r) => r.opponentId === opp && r.matchId !== current);
  if (met.length === 0) return null;
  const won = met.filter((r) => r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor > r.scoreAgainst).length;
  const lost = met.filter((r) => r.scoreFor !== null && r.scoreAgainst !== null && r.scoreFor < r.scoreAgainst).length;
  return (
    <div className="mx-meetings">
      <span className="ts-eyebrow">{t("Between these teams")}</span>
      <span className="hint">{tx("{0}–{1} in {2} other official{3} with {4}", { "0": won, "1": lost, "2": met.length, "3": met.length === 1 ? "" : "s", "4": oppName })}</span>
      {met.slice(0, 5).map((r) => {
        const cls = r.scoreFor === null || r.scoreAgainst === null ? "" : r.scoreFor > r.scoreAgainst ? "result-W" : r.scoreFor < r.scoreAgainst ? "result-L" : "";
        return (
          <span key={r.matchId} className="mx-map-chip" title={[r.division, r.round ?? r.stage, r.maps.join(", ")].filter(Boolean).join(" · ")}>
            {seasonShort(r.season)} {r.round ?? r.stage} <strong className={cls}>{r.defaultWin ? t("default") : `${r.scoreFor ?? "–"}–${r.scoreAgainst ?? "–"}`}</strong>
          </span>
        );
      })}
    </div>
  );
}

/** The map, or each map of a combined log with the rounds won on it. */
function MapLine({ d, maps, single, left, ledger }: { d: MatchDetail; maps: string[]; single: string | null; left: "Red" | "Blue"; ledger: Ledger | null }) {
  if (maps.length > 1) {
    return (
      <span className="mx-maps">
        {(d.segments ?? []).map((s, i) => {
          const [a, b] = left === "Blue" ? [s.blueWins, s.redWins] : [s.redWins, s.blueWins];
          const cls = d.myTeam ? (a > b ? "result-W" : a < b ? "result-L" : "") : "";
          // ETF2L's result for the map beside the rounds (Q84): a stopwatch
          // map is scored in games, which rounds won do not show.
          const lm = ledger?.maps.find((m) => m.firstRound === s.firstRound) ?? null;
          const etf2l = lm && lm.etf2lRed !== null && lm.etf2lBlue !== null ? (left === "Blue" ? [lm.etf2lBlue, lm.etf2lRed] : [lm.etf2lRed, lm.etf2lBlue]) : null;
          const tip = [
            t("Rounds {firstRound}–{lastRound}", { firstRound: s.firstRound, lastRound: s.lastRound }),
            etf2l ? t("ETF2L: {0}–{1}", { "0": etf2l[0], "1": etf2l[1] }) : null,
            lm?.agrees === false ? t("The rounds and ETF2L's result disagree") : null,
          ]
            .filter(Boolean)
            .join(" · ");
          return (
            <span key={i} className={lm?.agrees === false ? "mx-map-chip mx-map-disagrees" : "mx-map-chip"} title={tip}>
              {capitalize(splitMap(s.map).name ?? "unknown")} <strong className={cls}>{a}–{b}</strong>
              {etf2l && (etf2l[0] !== a || etf2l[1] !== b) && <span className="mx-map-etf2l">{t("ETF2L {0}–{1}", { "0": etf2l[0], "1": etf2l[1] })}</span>}
            </span>
          );
        })}
      </span>
    );
  }
  const { mode, name } = splitMap(single);
  return (
    <span className="mx-map">
      {mode && <span className={`mode mode-${mode}`}>{mode}</span>}
      <strong>{name ?? t("Unknown map")}</strong>
    </span>
  );
}

function Links({ d }: { d: MatchDetail }) {
  const etf2lId = d.context?.etf2lMatchId ?? d.etf2lMatchId;
  // An imported demo's log (Q18) has a negative id and is on no website.
  const links: Array<[string, string]> = d.logId > 0 ? [["logs.tf", `https://logs.tf/${d.logId}`]] : [];
  if (d.demosTfId) links.push(["demos.tf", `https://demos.tf/${d.demosTfId}`]);
  if (etf2lId) links.push(["ETF2L", `https://etf2l.org/matches/${etf2lId}/`]);
  // The sites as pills; a combined log's own logs as a quieter group beside
  // them, so seven look-alike links read as two things (Flashy: "every link,
  // button and text has to have its place").
  return (
    <span className="mh-links">
      {links.map(([label, url]) => (
        <button key={label} className="mx-link" onClick={() => void api.openExternal(url)}>
          {t(label)} <span aria-hidden>↗</span>
        </button>
      ))}
      {d.parts.length > 0 && (
        <span className="mx-part-links">
          <span className="mx-part-label">{t("Its logs")}</span>
          {d.parts.map((p) => (
            <button key={p.logId} className="mx-part-link" onClick={() => void api.openExternal(`https://logs.tf/${p.logId}`)} title={t("Open log {0} on logs.tf", { "0": p.logId })}>
              {p.logId}
            </button>
          ))}
        </span>
      )}
    </span>
  );
}
