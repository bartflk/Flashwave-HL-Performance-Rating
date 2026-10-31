import { useQuery } from "@tanstack/react-query";
import { ApiError } from "../ApiError";
import { HowCounted } from "../StatTip";
import { api } from "../../api/client";
import { type Analysis, type AimRow, type AimTotals, type DeathRow, type LifeTotals } from "../../api/types";
import { AimCharts } from "./AimCharts";
import { classLabel, playerMap, sliceLabel, sliceScope, type Slice } from "./common";
import { t, t as tr, tx, k } from "../../lib/i18n";
import { Loading } from "../Spinner";
import { SortTh, useSort } from "../../lib/sort";
import { tip } from "../../lib/glossary";

/**
 * Classes whose kills land where the crosshair was. For everyone else the
 * angle to the head is still a real measurement — it is just not a
 * measurement of aim, and the tab says so rather than reporting a Soldier's
 * 28 degrees as though it were a Sniper's.
 */
const HITSCAN = new Set(["sniper", "scout", "heavy", "engineer", "spy"]);

/**
 * One player's aim, read from the match's own demo (PLAN §14, Q16b).
 *
 * The log says who they killed; the demo says where they were looking a
 * second before, how far the view travelled, and how far away the victim
 * was. Only kills the demo carried both players through get an answer, so a
 * Spy killed round a corner is left out.
 *
 * **It answers for anyone the demo can vouch for.** It used to answer only
 * for the owner, and before that it did not take `player` at all — which
 * meant picking someone else in the filter row left these cards showing
 * *your* numbers under *their* name, and a tester read his own 28° crosshair
 * error as a teammate's. Which players it can answer for depends on the
 * demo: an STV carries all eighteen, a POV carries only whoever recorded it.
 * When the match has no STV the tab says that, rather than looking broken.
 */
export function Aim(props: {
  a: Analysis;
  logId: number;
  player: number;
  slice: Slice;
  /** Switch the filter row's player, so the message can fix itself. */
  onPick: (accountId: number) => void;
}) {
  const { a, logId, player, slice, onPick } = props;
  const q = useQuery({ queryKey: ["aim", logId, player], queryFn: () => api.getAim(logId, player) });
  const names = playerMap(a);
  // The filter row above applies here too: a round, or a map of a combined
  // log, narrows the kills and deaths the demo is read for.
  const inSlice = (round: number | null) => slice.rounds === null || (round !== null && slice.rounds.has(round));

  const me = a.players.find((p) => p.isMe) ?? null;
  const mine = me === null || player === me.accountId;
  const who = names.get(player)?.name ?? "that player";
  const cls = a.players.find((p) => p.accountId === player)?.mainClass ?? null;

  if (q.isPending) return <Loading className="an-empty">{t("Reading the demo…")}</Loading>;
  if (q.isError) return <ApiError error={q.error} />;
  const d = q.data;
  if (!d || (d.kills.length === 0 && d.deaths.length === 0)) {
    // Nothing for someone else, on a match with no STV, has one cause and
    // one fix. Saying which beats an empty panel, and offering the way back
    // beats telling them to go and change a dropdown themselves.
    if (!mine && !d?.stv) {
      return (
        <div className="an-empty">
          <p className="hint">{tx("Nothing for {who}: other players' aim comes from the match's STV demo, and this match has only your own. Download the STV from the links at the top and it will fill in.", { who: who })}</p>
          <button onClick={() => onPick(me!.accountId)}>{t("Show my aim")}</button>
        </div>
      );
    }
    return (
      <p className="hint an-empty">
        {mine
          ? t("No demo linked to this match. Set your TF2 folder in Settings.")
          : t("The demo has nothing for {who} — it never carried them long enough to read.", { who: who })}
      </p>
    );
  }

  const kills = d.kills.filter((k) => inSlice(k.roundNum)).sort((x, y) => x.tick - y.tick);
  const deaths = d.deaths.filter((k) => inSlice(k.roundNum));
  const filtered = slice.rounds !== null;
  if (kills.length === 0 && deaths.length === 0) {
    return (
      <p className="hint an-empty">{tx("No kills or deaths {0} {1}.", { "0": mine ? t("of yours") : t("for {who}", { who: who }), "1": sliceLabel(slice) })}</p>
    );
  }

  return (
    <div className="aim">
      <HowCounted>{t("Angles are measured to the middle of the victim's head. A small crosshair error a second before the kill means the angle was already held; a large one followed by a flick means it was a reaction.")}</HowCounted>
      {d.totals && !filtered && <Summary t={d.totals} career={d.career} life={d.life} careerLife={d.careerLife} />}
      {cls !== null && !HITSCAN.has(cls) && (
        <p className="hint">{tx("Crosshair error is the angle to the victim's head, which is a hitscan measure. A{0} {cls} kills with splash and projectiles that never had to be on the head, so read these as where they were looking, not how well they aimed.", { "0": cls === "soldier" || cls === "engineer" ? "n" : "", cls: classLabel(cls) })}</p>
      )}
      {filtered && (
        <p className="hint">
          {tx("{kills} kill{1} and {deaths} death{3} in {4}.", { "1": kills.length === 1 ? "" : "s", "3": deaths.length === 1 ? "" : "s", "4": sliceScope(slice), kills: kills.length, deaths: deaths.length })}</p>
      )}
      <AimCharts kills={kills} deaths={deaths} />
      <details className="aim-details">
        <summary>{t("Every kill, in numbers")}</summary>
        <KillsTable kills={kills} names={names} />
      </details>
      {deaths.length > 0 && (
        <details className="aim-details">
          <summary>{t("Every death, in numbers")}</summary>
          <Deaths rows={deaths} names={names} />
        </details>
      )}
    </div>
  );
}

function Summary(props: { t: AimTotals; career: AimTotals | null; life: LifeTotals | null; careerLife: LifeTotals | null }) {
  const { t, career, life, careerLife } = props;
  const cards: Array<[string, string, string, string | null]> = [
    [k("Crosshair error"), `${t.errorDeg.toFixed(1)}°`, k("when the kill landed"), career && tr("{v} usually", { v: `${career.errorDeg.toFixed(1)}°` })],
    [k("A second before"), `${t.beforeDeg.toFixed(1)}°`, k("how far it had to travel"), career && tr("{v} usually", { v: `${career.beforeDeg.toFixed(1)}°` })],
    [k("Flick"), `${t.flickDeg.toFixed(1)}°`, k("turn in the last half second"), career && tr("{v} usually", { v: `${career.flickDeg.toFixed(1)}°` })],
    [k("Range"), t.rangeUnits.toFixed(0), k("map units"), career && tr("{v} usually", { v: `${career.rangeUnits.toFixed(0)}` })],
    [k("Angle already held"), `${(t.heldShare * 100).toFixed(0)}%`, k("within 3° a second before"), career && tr("{v} usually", { v: `${(career.heldShare * 100).toFixed(0)}%` })],
  ];
  if (life) {
    cards.push(
      [k("Scoped"), `${(life.scopedShare * 100).toFixed(0)}%`, k("of their time alive"), careerLife && tr("{v} usually", { v: `${(careerLife.scopedShare * 100).toFixed(0)}%` })],
      [
        k("Nearest teammate"),
        life.nearestMate === null ? "—" : life.nearestMate.toFixed(0),
        k("units away when they died"),
        careerLife?.nearestMate ? tr("{v} usually", { v: `${careerLife.nearestMate.toFixed(0)}` }) : null,
      ],
      [k("Died alone"), `${(life.aloneShare * 100).toFixed(0)}%`, k("nobody within 900 units"), careerLife && tr("{v} usually", { v: `${(careerLife.aloneShare * 100).toFixed(0)}%` })],
    );
  }
  return (
    <div className="aim-cards">
      {cards.map(([label, value, note, vs]) => (
        <div className="aim-card" key={label}>
          <span className="aim-label">{tr(label)}</span>
          <span className="aim-value">{value}</span>
          <span className="aim-note">{tr(note)}</span>
          {vs && <span className="aim-vs">{vs}</span>}
        </div>
      ))}
      <p className="hint aim-count">{tx("From {kills} kills the demo could answer for.", { kills: t.kills })}</p>
    </div>
  );
}

/** The deaths: who got them, from how far, and who was close enough to help. */
function Deaths({ rows: all, names }: { rows: DeathRow[]; names: ReturnType<typeof playerMap> }) {
  const sort = useSort("aim-deaths", all, {
    killer: (r) => (r.killer !== null ? names.get(r.killer)?.name ?? null : null),
    range: (r) => r.killerRange,
    mate: (r) => r.nearestMate,
    cover: (r) => r.matesNear,
    state: (r) => (r.scoped ? 1 : 0) + (r.matesNear === 0 ? 2 : 0),
  });
  const rows = sort.sorted;
  return (
    <div className="table-wrap">
      <table className="match-table aim-table">
        <thead>
          <tr>
            <SortTh k="killer" s={sort}>{t("Killed by")}</SortTh>
            <SortTh k="range" s={sort} className="num" tip={tip(t("Their range"), t("How far away they were; blank when the demo never carried them"), t(AIM_HOW), "STV demo")}>{t("Their range")}</SortTh>
            <SortTh k="mate" s={sort} className="num" tip={tip(t("Nearest teammate"), t("Distance to the nearest living teammate the demo carried"), t(AIM_HOW), "STV demo")}>{t("Nearest teammate")}</SortTh>
            <SortTh k="cover" s={sort} className="num" tip={tip(t("Cover"), t("Teammates within 900 units"), t(AIM_HOW), "STV demo")}>{t("Cover")}</SortTh>
            <SortTh k="state" s={sort}>{t("State")}</SortTh>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.tick}>
              <td className="nowrap player-name">{(r.killer !== null && names.get(r.killer)?.name) || "—"}</td>
              <td className="num">{r.killerRange === null ? <span className="muted">–</span> : r.killerRange.toFixed(0)}</td>
              <td className="num">
                {r.nearestMate === null ? <span className="muted">–</span> : <span className={r.nearestMate > 900 ? "deg-far" : ""}>{r.nearestMate.toFixed(0)}</span>}
              </td>
              <td className="num">{r.matesNear}</td>
              <td className="nowrap">
                {r.scoped && <span className="badge badge-hs">{t("scoped")}</span>}
                {r.matesNear === 0 && <span className="muted">{t("alone")}</span>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Degrees, tinted: close is good, far is not. */
function Deg({ v, good, bad }: { v: number; good: number; bad: number }) {
  const cls = v <= good ? "deg-good" : v >= bad ? "deg-far" : "";
  return <span className={cls}>{v.toFixed(1)}°</span>;
}

export type { AimRow };

/** How the demo's aim numbers are read. */
const AIM_HOW = k("Read from the STV demo: where each player looked, tick by tick, against where the victim's head was.");

/** Every kill, in numbers, sortable (Q79). */
function KillsTable({ kills, names }: { kills: AimRow[]; names: ReturnType<typeof playerMap> }) {
  const sort = useSort("aim-kills", kills, {
    victim: (x) => (x.victim !== null ? names.get(x.victim)?.name ?? null : null),
    error: (x) => -x.errorDeg,
    before: (x) => -x.beforeDeg,
    flick: (x) => x.flickDeg,
    range: (x) => x.rangeUnits,
    height: (x) => x.height,
    shot: (x) => (x.headshot ? 1 : 0),
  });
  return (
    <div className="table-wrap">
    <table className="match-table aim-table">
      <thead>
        <tr>
          <SortTh k="victim" s={sort}>{t("Victim")}</SortTh>
          <SortTh k="error" s={sort} className="num" tip={tip(t("Crosshair"), t("Degrees between their view and the victim's head when the kill landed"), t(AIM_HOW), "STV demo")}>{t("Crosshair")}</SortTh>
          <SortTh k="before" s={sort} className="num" tip={tip(t("A second before"), t("The same, one second before the kill: how far the crosshair had to travel"), t(AIM_HOW), "STV demo")}>{t("A second before")}</SortTh>
          <SortTh k="flick" s={sort} className="num" tip={tip(t("Flick"), t("How far the view turned in the half second before the shot"), t(AIM_HOW), "STV demo")}>{t("Flick")}</SortTh>
          <SortTh k="range" s={sort} className="num" tip={tip(t("Range"), t("Distance in map units; a Sniper sightline is around 1,500"), t(AIM_HOW), "STV demo")}>{t("Range")}</SortTh>
          <SortTh k="height" s={sort} className="num" tip={tip(t("Height"), t("How far above the shooter the victim stood"), t(AIM_HOW), "STV demo")}>{t("Height")}</SortTh>
          <SortTh k="shot" s={sort}>{t("Shot")}</SortTh>
        </tr>
      </thead>
      <tbody>
        {sort.sorted.map((k) => (
          <tr key={k.tick} className={k.victimSeen ? undefined : "muted-row"}>
            <td className="nowrap player-name">{(k.victim !== null && names.get(k.victim)?.name) || "—"}</td>
            <td className="num">
              <Deg v={k.errorDeg} good={3} bad={15} />
            </td>
            <td className="num">
              <Deg v={k.beforeDeg} good={3} bad={30} />
            </td>
            <td className="num">{k.flickDeg.toFixed(1)}°</td>
            <td className="num">{k.rangeUnits.toFixed(0)}</td>
            <td className="num">{k.height.toFixed(0)}</td>
            <td className="nowrap">
              {k.headshot && <span className="badge badge-hs">{t("headshot")}</span>}
              {!k.victimSeen && (
                <span className="muted" title={t("The demo did not carry them the whole time, so these numbers are stale")}>{t("not on screen")}</span>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
    </div>
  );
}
