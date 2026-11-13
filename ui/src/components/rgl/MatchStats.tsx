import { useState, type ReactNode } from "react";
import type { UseQueryResult } from "@tanstack/react-query";
import type { RglAward, RglMapStats, RglMatch, RglMatchStats, RglSideTotals, RglStatPlayer } from "../../api/types";
import { rating, signed } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { ahead, classesIn, matchup, sidesInOrder } from "../../lib/rglstats";
import { ApiError } from "../ApiError";
import { Seg } from "../analysis/MapChrome";
import { ClassIcon } from "../ClassIcon";
import { Loading } from "../Spinner";
import { classLabel } from "../analysis/common";

type Team = RglMatch["teams"][number] | undefined;
type Colour = "Red" | "Blue";

const n0 = (n: number) => Math.round(n).toLocaleString();
const dash = (n: number | null, f: (n: number) => string) => (n === null ? "–" : f(n));

/** A colour's dot, so a row says which side it is without a word. */
function Dot({ colour }: { colour: Colour }) {
  return <span className={`rgl-st-dot ${colour === "Red" ? "red" : "blu"}`} aria-hidden />;
}

/** What a side is called: its RGL team where the rosters said, else its colour. */
function sideName(s: RglSideTotals, home: Team, away: Team): string {
  const team = [home, away].find((x) => x && x.teamId === s.teamId);
  return team ? (team.teamTag ?? team.teamName) : s.side === "Red" ? t("RED") : t("BLU");
}

/** The match page's Statistics section: one map at a time, from its log. */
export function MatchStats({ q, home, away, onPlayer }: { q: UseQueryResult<RglMatchStats | null>; home: Team; away: Team; onPlayer: (steamId: string) => void }) {
  const [pick, setPick] = useState(0);
  if (q.error) {
    return (
      <section className="panel e2-panel">
        <div className="e2-panel-head">
          <h2>{t("Statistics")}</h2>
        </div>
        <ApiError error={q.error} />
      </section>
    );
  }
  if (q.isPending) {
    return (
      <section className="panel e2-panel">
        <div className="e2-panel-head">
          <h2>{t("Statistics")}</h2>
        </div>
        <Loading>{t("Reading the logs for their numbers… the first time, each log is fetched from drops.tf; after that it is kept.")}</Loading>
      </section>
    );
  }
  const maps = q.data?.maps ?? [];
  const unread = q.data?.unread ?? [];
  // No log, nothing new: the Logs panel says why.
  if (maps.length === 0 && unread.length === 0) return null;
  const map = maps[Math.min(pick, maps.length - 1)];
  const labels = maps.map((m, i) => (maps.filter((o) => o.map === m.map).length > 1 ? `${m.map} · ${maps.filter((o, j) => o.map === m.map && j <= i).length}` : m.map));
  return (
    <section className="panel e2-panel rgl-stats">
      <div className="e2-panel-head">
        <h2>{t("Statistics")}</h2>
        {maps.length > 1 && <Seg label={t("Map")} value={String(pick)} onChange={(v) => setPick(Number(v))} options={maps.map((m, i) => ({ v: String(i), label: labels[i], title: m.map }))} />}
      </div>
      {map && <MapStats key={map.logId} map={map} home={home} away={away} onPlayer={onPlayer} />}
      {unread.length > 0 && <p className="hint">{tx("{0} of the logs could not be read; they are asked for again the next time this page opens.", { "0": unread.length })}</p>}
      <p className="hint">
        {t("From the logs, fetched from drops.tf (by Icewind) once and kept. A stand-in is in the numbers but not in the lineups.")}
        {map && !map.rated && ` ${t("The app's rating is not shown: it needs the rating baselines, made by a full rating pass.")}`}
      </p>
    </section>
  );
}

function MapStats({ map, home, away, onPlayer }: { map: RglMapStats; home: Team; away: Team; onPlayer: (steamId: string) => void }) {
  const [left, right] = sidesInOrder(map, home?.teamId);
  const nameOf = (s: RglSideTotals) => sideName(s, home, away);
  return (
    <div className="rgl-st-body">
      <Totals left={left} right={right} nameOf={nameOf} minutes={map.minutes} />
      <Awards map={map} onPlayer={onPlayer} />
      <PlayerTable map={map} nameOf={(p) => nameOf(map.sides.find((s) => s.side === p.side)!)} onPlayer={onPlayer} />
      <div className="rgl-st-pair">
        <MedicMatchup map={map} first={left.side} />
        <PlayerMatchup map={map} first={left.side} />
      </div>
    </div>
  );
}

/** The two teams' totals, row by row; the side ahead on a row is bright. */
function Totals({ left, right, nameOf, minutes }: { left: RglSideTotals; right: RglSideTotals; nameOf: (s: RglSideTotals) => string; minutes: number }) {
  const rows: { label: string; title: string; a: number; b: number; lower?: boolean }[] = [
    { label: t("Rounds"), title: t("Rounds won on the map"), a: left.score, b: right.score },
    { label: t("Kills"), title: t("Kills"), a: left.kills, b: right.kills },
    { label: t("Deaths"), title: t("Deaths: fewer is better"), a: left.deaths, b: right.deaths, lower: true },
    { label: t("Damage"), title: t("Damage dealt"), a: left.damage, b: right.damage },
    { label: t("Heals"), title: t("Health healed"), a: left.heals, b: right.heals },
    { label: t("Ubers"), title: t("Ubercharges used"), a: left.ubers, b: right.ubers },
    { label: t("Drops"), title: t("Ubercharges dropped: fewer is better"), a: left.drops, b: right.drops, lower: true },
  ];
  return (
    <div className="rgl-st-block">
      <div className="e2-block-head">
        <h3 className="ts-section">{t("Teams")}</h3>
        <span className="muted">{tx("{0} min", { "0": Math.round(minutes) })}</span>
      </div>
      <table className="match-table rgl-st-totals">
        <thead>
          <tr>
            <th>{t("Stat")}</th>
            <th className="num">
              <Dot colour={left.side} /> {nameOf(left)}
            </th>
            <th className="num">
              <Dot colour={right.side} /> {nameOf(right)}
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => {
            const w = ahead(r.a, r.b, r.lower);
            return (
              <tr key={r.label} title={r.title}>
                <td>{r.label}</td>
                <td className={`num${w === 0 ? " team-won" : ""}`}>{n0(r.a)}</td>
                <td className={`num${w === 1 ? " team-won" : ""}`}>{n0(r.b)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

/** What an award says under the name. */
function awardLine(a: RglAward): ReactNode {
  switch (a.basis) {
    case "rating":
      return tx("{0} rating", { "0": rating(a.value) });
    case "kapd":
      return tx("{0} kills and half the assists per death", { "0": a.value.toFixed(2) });
    case "heals":
      return tx("{0} healed, {1} ubers", { "0": n0(a.value), "1": a.extra ?? 0 });
    case "kills":
      return tx("{0} kills, {1} deaths", { "0": a.value, "1": a.extra ?? 0 });
    default:
      return tx("{0} headshots", { "0": a.value });
  }
}

function Awards({ map, onPlayer }: { map: RglMapStats; onPlayer: (steamId: string) => void }) {
  const a = map.awards;
  const cards: { label: string; title: string; award: RglAward | null }[] = [
    { label: t("MVP"), title: a.mvp?.basis === "rating" ? t("The best rating on the map") : t("No ratings here: the most kills and half the assists per death, medics left out"), award: a.mvp },
    { label: t("Medic MVP"), title: t("The most healing, an uber counting as much as 500 health"), award: a.medicMvp },
    { label: t("Top fragger"), title: t("The most kills; fewer deaths settle a tie"), award: a.topFragger },
    { label: t("Most headshots"), title: t("The most headshots"), award: a.mostHeadshots },
  ].filter((c) => c.award);
  if (cards.length === 0) return null;
  return (
    <div className="rgl-st-block">
      <div className="e2-block-head">
        <h3 className="ts-section">{t("Awards")}</h3>
      </div>
      <div className="rgl-st-awards">
        {cards.map((c) => (
          <button key={c.label} className="rgl-st-award" title={c.title} onClick={() => onPlayer(c.award!.steamId)}>
            <span className="ts-eyebrow">{c.label}</span>
            <span className="rgl-st-award-name">
              <Dot colour={c.award!.side} />
              <ClassIcon cls={c.award!.class} size={18} />
              <span className="rgl-st-name">{c.award!.name}</span>
            </span>
            <span className="muted">{awardLine(c.award!)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

function PlayerTable({ map, nameOf, onPlayer }: { map: RglMapStats; nameOf: (p: RglStatPlayer) => string; onPlayer: (steamId: string) => void }) {
  return (
    <div className="rgl-st-block">
      <div className="e2-block-head">
        <h3 className="ts-section">{t("Players")}</h3>
      </div>
      <div className="table-wrap">
        <table className="match-table rgl-st-players">
          <thead>
            <tr>
              <th>{t("Player")}</th>
              <th>{t("Team")}</th>
              <th className="num" title={t("Kills")}>
                K
              </th>
              <th className="num" title={t("Assists")}>
                A
              </th>
              <th className="num" title={t("Deaths")}>
                D
              </th>
              <th className="num" title={t("Damage dealt")}>
                {t("Dmg")}
              </th>
              <th className="num" title={t("Damage per minute played")}>
                DPM
              </th>
              <th className="num" title={t("DPM minus the class's typical DPM")}>
                {t("vs class")}
              </th>
              <th className="num" title={t("Damage taken per minute played")}>
                DT/M
              </th>
              {map.rated && (
                <th className="num" title={t("The app's rating for this game")}>
                  {t("Rating")}
                </th>
              )}
            </tr>
          </thead>
          <tbody>
            {map.players.map((p) => (
              <tr key={`${p.steamId}-${p.class}`} className={`rgl-st-row ${p.side === "Red" ? "red" : "blu"}`}>
                <td className="rgl-st-player">
                  <button className="linkish rgl-st-link" onClick={() => onPlayer(p.steamId)} title={t("Open their RGL page")}>
                    <ClassIcon cls={p.class} size={18} />
                    <span className="rgl-st-name">{p.name}</span>
                  </button>
                </td>
                <td className="muted">
                  <Dot colour={p.side} /> {nameOf(p)}
                </td>
                <td className="num">{p.kills}</td>
                <td className="num">{p.assists}</td>
                <td className="num">{p.deaths}</td>
                <td className="num">{n0(p.damage)}</td>
                <td className="num">{dash(p.dpm, n0)}</td>
                <td className={`num ${vsClass(p.dpmVs)}`} title={p.dpmVsBasis === "opponent" ? t("Against the other team's player of the class: there is no baseline for it") : t("Against the median of the class in the logs.tf archive")}>
                  {dash(p.dpmVs, (n) => signed(n, 0))}
                </td>
                <td className="num">{dash(p.dtpm, n0)}</td>
                {map.rated && <td className="num rgl-st-rating">{rating(p.rating)}</td>}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/** A difference to the class's typical DPM: green above, red below (the pool's heat, as the Players board). */
function vsClass(n: number | null): string {
  if (n === null || Math.abs(n) < 1) return "muted";
  return n > 0 ? "rgl-st-up" : "rgl-st-down";
}

/** Two players or two medics, row by row: left value, what it is, right value. */
function Versus({ left, right, rows }: { left: RglStatPlayer; right: RglStatPlayer; rows: { label: string; a: number | null; b: number | null; show: (n: number) => string; lower?: boolean }[] }) {
  return (
    <div className="rgl-vs">
      <div className="rgl-vs-who l">
        <Dot colour={left.side} />
        <span className="rgl-st-name" title={left.name}>
          {left.name}
        </span>
      </div>
      <div />
      <div className="rgl-vs-who r">
        <span className="rgl-st-name" title={right.name}>
          {right.name}
        </span>
        <Dot colour={right.side} />
      </div>
      {rows.map((r) => {
        const w = ahead(r.a, r.b, r.lower);
        return (
          <div className="rgl-vs-row" key={r.label}>
            <span className={`l${w === 0 ? " team-won" : ""}`}>{dash(r.a, r.show)}</span>
            <span className="rgl-vs-label muted">{r.label}</span>
            <span className={`r${w === 1 ? " team-won" : ""}`}>{dash(r.b, r.show)}</span>
          </div>
        );
      })}
    </div>
  );
}

function MedicMatchup({ map, first }: { map: RglMapStats; first: Colour }) {
  const [a, b] = matchup(map, "medic", first);
  return (
    <div className="rgl-st-block rgl-st-card">
      <div className="e2-block-head">
        <h3 className="ts-section">{t("Medic matchup")}</h3>
      </div>
      {a && b ? (
        <Versus
          left={a}
          right={b}
          rows={[
            { label: t("Heals"), a: a.heals, b: b.heals, show: n0 },
            { label: t("Heals per minute"), a: a.minutes >= 1 ? a.heals / a.minutes : null, b: b.minutes >= 1 ? b.heals / b.minutes : null, show: n0 },
            { label: t("Ubers"), a: a.ubers, b: b.ubers, show: n0 },
            { label: t("Drops"), a: a.drops, b: b.drops, show: n0, lower: true },
            { label: t("Deaths"), a: a.deaths, b: b.deaths, show: n0, lower: true },
          ]}
        />
      ) : (
        <p className="hint">{t("A medic is missing on one side in this log.")}</p>
      )}
    </div>
  );
}

function PlayerMatchup({ map, first }: { map: RglMapStats; first: Colour }) {
  const classes = classesIn(map);
  const [cls, setCls] = useState<string | null>(null);
  const shown = cls && classes.includes(cls) ? cls : (classes.find((c) => c !== "medic") ?? classes[0] ?? null);
  const [a, b] = shown ? matchup(map, shown, first) : [null, null];
  return (
    <div className="rgl-st-block rgl-st-card">
      <div className="e2-block-head">
        <h3 className="ts-section">{t("Player matchup")}</h3>
        {shown && classes.length > 1 && (
          <Seg label={t("Class")} value={shown} onChange={setCls} options={classes.map((c) => ({ v: c, label: <ClassIcon cls={c} size={18} untitled />, title: classLabel(c) }))} />
        )}
      </div>
      {a && b ? (
        <Versus
          left={a}
          right={b}
          rows={[
            ...(map.rated ? [{ label: t("Rating"), a: a.rating, b: b.rating, show: rating }] : []),
            { label: t("Kills"), a: a.kills, b: b.kills, show: n0 },
            { label: t("Assists"), a: a.assists, b: b.assists, show: n0 },
            { label: t("Deaths"), a: a.deaths, b: b.deaths, show: n0, lower: true },
            { label: t("Damage"), a: a.damage, b: b.damage, show: n0 },
            { label: "DPM", a: a.dpm, b: b.dpm, show: n0 },
            { label: "DT/M", a: a.dtpm, b: b.dtpm, show: n0, lower: true },
          ]}
        />
      ) : (
        <p className="hint">{t("Only one side played this class in this log.")}</p>
      )}
    </div>
  );
}
