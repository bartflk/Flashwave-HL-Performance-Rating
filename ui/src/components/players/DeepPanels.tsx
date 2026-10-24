import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type ClassDeep, type DeepAttribute, type DeepStat } from "../../api/types";
import { rating } from "../../lib/format";
import { k, t, tx } from "../../lib/i18n";
import { tip, type Entry, type GlossaryKey } from "../../lib/glossary";
import { SortTh, useSort } from "../../lib/sort";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";
import { Loading } from "../Spinner";
import { StatTip } from "../StatTip";

/**
 * A player's numbers the way HLTV shows a player's (the MVP research,
 * October 2026): the headline numbers each with a verdict against the
 * class, attribute cards 0-100 that open into the stats behind them,
 * rating by the opposite number's division, attack against defence, the
 * moments that win rounds, and weapons. Everyone on the class is the
 * yardstick: the league's officials and the owner's own matches.
 */

/** Each stat's name and explanation. */
const STATS: Record<string, [string, string, string]> = {
  rating: [k("Rating"), k("Their average rating on the class."), k("Every rated game on the class.")],
  impact: [k("Impact"), k("Round-win chance their kills added and their deaths gave away, against the class: 1.00 is average."), k("Per 30 minutes, pulled towards the class average over few maps.")],
  impact_per30: [k("Impact/30"), k("Rounds' worth of win chance added per 30 minutes."), k("Kills' value minus deaths' value, per 30 minutes.")],
  kills_per30: [k("K/30"), k("Kills per 30 minutes."), k("logs.tf's kills on the class.")],
  deaths_per30: [k("D/30"), k("Deaths per 30 minutes."), k("logs.tf's deaths on the class; fewer is better.")],
  assists_per30: [k("A/30"), k("Assists per 30 minutes."), k("logs.tf's assists on the class.")],
  dpm: [k("DA/M"), k("Damage dealt per minute."), k("logs.tf's damage on the class.")],
  dtm: [k("DT/M"), k("Damage taken per minute; less is better."), k("logs.tf's damage taken.")],
  kd: [k("K/D"), k("Kills per death."), k("Summed before dividing.")],
  kast: [k("KAST"), k("Share of fights with a kill, assist, survival or trade."), k("From the server log's fights.")],
  opening_kills_per30: [k("Opening kills/30"), k("First kills of fights, per 30 minutes."), k("From the server log's fights.")],
  first_picks_per30: [k("First picks/30"), k("First kills of a round, per 30 minutes."), k("From the server log.")],
  opening_success: [k("Opening duels won"), k("Of the fights they opened or were opened on, the share they won."), k("Opening kills over opening kills and deaths.")],
  trade_kills_per30: [k("Trades/30"), k("Kills avenging a teammate within 3 seconds."), k("From the server log.")],
  traded_death_share: [k("Deaths traded"), k("Share of their deaths a teammate avenged within 3 seconds."), k("From the server log.")],
  cleanup_kills_per30: [k("Clean-ups/30"), k("Kills with their side already a player up."), k("From the server log.")],
  down_kills_per30: [k("Man-down kills/30"), k("Kills with their side a player or more down."), k("From the server log.")],
  multi_kills_per30: [k("Multi-kills/30"), k("Bursts of two or more kills, each within 5 seconds."), k("From the server log.")],
  heals_per_minute: [k("Heals/M"), k("Healing per minute."), k("logs.tf.")],
  ubers_per30: [k("Ubers/30"), k("Charges used per 30 minutes."), k("logs.tf.")],
  drops_per30: [k("Drops/30"), k("Deaths with a full charge per 30 minutes; fewer is better."), k("logs.tf.")],
  build_s: [k("Build time"), k("Average seconds to build a charge; less is better."), k("logs.tf's Medic statistics.")],
  headshots_per30: [k("Headshots/30"), k("Headshot kills per 30 minutes."), k("logs.tf.")],
  backstabs_per30: [k("Backstabs/30"), k("Backstabs per 30 minutes."), k("logs.tf.")],
  airshots_per30: [k("Airshots/30"), k("Airshots per 30 minutes."), k("logs.tf.")],
  forces_per30: [k("Forced ubers/30"), k("Enemy charges popped under their damage."), k("From the server log.")],
  caps_per30: [k("Caps/30"), k("Points captured per 30 minutes."), k("logs.tf.")],
};

const ATTRS: Record<string, [string, string]> = {
  firepower: [k("Firepower"), k("Kills and damage.")],
  healing: [k("Healing"), k("Heals, charges and how fast they build.")],
  opening: [k("Opening"), k("Starting fights: first kills and how often they win the opening duel.")],
  trading: [k("Trading"), k("Avenging teammates, and being avenged.")],
  closing: [k("Closing"), k("Finishing fights: clean-ups, man-down kills, multi-kills.")],
  survival: [k("Survival"), k("Staying alive and in the fight.")],
  impact: [k("Impact"), k("Round-win chance added, and charges forced.")],
  keeping_the_charge: [k("Keeping the charge"), k("Not dying with it.")],
  sniping: [k("Sniping"), k("Headshots.")],
  stabbing: [k("Stabbing"), k("Backstabs.")],
  airshots: [k("Airshots"), k("Hitting players in the air.")],
  objective: [k("Objective"), k("Capping points.")],
  support: [k("Support"), k("Assists.")],
};

function statEntry(key: string): Entry {
  const [label, what, how] = STATS[key] ?? [key, "", ""];
  return tip(t(label), t(what), t(how), key.includes("open") || key.includes("trade") || key.includes("kast") || key.includes("impact") || key.includes("cleanup") || key.includes("down") || key.includes("multi") || key.includes("forces") ? "server log" : "logs.tf");
}

function fmtStat(s: DeepStat): string {
  if (s.key === "rating" || s.key === "impact") return rating(s.value);
  if (s.key === "kast" || s.key === "opening_success" || s.key === "traded_death_share") return `${(s.key === "kast" ? s.value : s.value * 100).toFixed(0)}%`;
  if (s.key === "dpm" || s.key === "dtm" || s.key === "heals_per_minute") return s.value.toFixed(0);
  if (s.key === "kd") return s.value.toFixed(2);
  return s.value.toFixed(1);
}

function fmtAvg(s: DeepStat): string {
  return fmtStat({ ...s, value: s.average });
}

export function DeepPanels({ accountId }: { accountId: number }) {
  const q = useQuery({ queryKey: ["player_deep", accountId], queryFn: () => api.getPlayerDeep(accountId), staleTime: 5 * 60_000 });
  const [cls, setCls] = useState<string | null>(null);
  if (q.isPending) return <Loading>{t("Reading their games…")}</Loading>;
  if (q.isError) return <p className="error">{errorMessage(q.error)}</p>;
  const classes = q.data.classes;
  if (classes.length === 0) return <p className="hint">{t("Not enough games on any class yet: six games on a class are needed to place them among everyone else.")}</p>;
  const c = classes.find((x) => x.class === cls) ?? classes[0];
  return (
    <div className="dp">
      {classes.length > 1 && (
        <div className="segmented dp-classes" role="tablist" aria-label={t("Class")}>
          {classes.map((x) => (
            <button key={x.class} role="tab" aria-selected={x.class === c.class} className={x.class === c.class ? "seg active" : "seg"} onClick={() => setCls(x.class)}>
              <ClassIcon cls={x.class} size={16} /> {classLabel(x.class)} <span className="muted">{x.maps}</span>
            </button>
          ))}
        </div>
      )}
      <ClassPanels c={c} />
    </div>
  );
}

function ClassPanels({ c }: { c: ClassDeep }) {
  const [average, setAverage] = useState(false);
  return (
    <>
      <section className="panel dp-head">
        <header className="dp-row-head">
          <h3>
            {tx("{0}: {1} games, {2} hours", { "0": classLabel(c.class), "1": c.maps, "2": Math.round(c.minutes / 60) })}
          </h3>
          <label className="dp-avg">
            <input type="checkbox" checked={average} onChange={(e) => setAverage(e.target.checked)} /> {t("Show the class average")}
          </label>
        </header>
        <div className="dp-headline">
          {c.headline.map((s) => (
            <Verdict key={s.key} s={s} average={average} />
          ))}
        </div>
      </section>

      <section className="panel">
        <h3>
          <StatTip entry={tip(t("Attributes"), t("What kind of player they are on this class: each card is 0 to 100 against everyone on the class, the average of the stats inside it."), t("Each stat is placed among players with six or more games on the class; open a card for the numbers."), "logs.tf", "server log")} inline>
            {t("Attributes")}
          </StatTip>
        </h3>
        <div className="dp-attrs">
          {c.attributes.map((a) => (
            <AttributeCard key={a.key} a={a} />
          ))}
        </div>
      </section>

      <div className="dp-grid">
        {c.vsDivisions.length > 0 && (
          <section className="panel">
            <h3>
              <StatTip entry={tip(t("Against each division"), t("Their rating by the division of the opposite number on their class."), t("The opposite number's ETF2L division at the time of the game."), "ETF2L", "logs.tf")} inline>
                {t("Against each division")}
              </StatTip>
            </h3>
            <div className="dp-tiles">
              {c.vsDivisions.map((v) => (
                <div key={v.tier} className="dp-tile">
                  <strong>{rating(v.rating)}</strong>
                  <span>{tx("vs {0}", { "0": v.division })}</span>
                  <span className="muted">{tx("{0} games", { "0": v.maps })}</span>
                </div>
              ))}
            </div>
          </section>
        )}
        <section className="panel">
          <h3>{t("Moments")}</h3>
          <dl className="dp-moments">
            <div>
              <dt>
                <StatTip k="kast" inline>
                  KAST
                </StatTip>
              </dt>
              <dd>{c.kast === null ? "–" : `${c.kast.toFixed(0)}%`}</dd>
            </div>
            <div>
              <dt>{t("Man-down kills")}</dt>
              <dd>{c.downKills}</dd>
            </div>
            <div>
              <dt>{t("Double kills")}</dt>
              <dd>{c.multi2}</dd>
            </div>
            <div>
              <dt>{t("Triple kills or more")}</dt>
              <dd>{c.multi3}</dd>
            </div>
            <div>
              <dt>{t("Longest streak")}</dt>
              <dd>{c.totals.longestStreak}</dd>
            </div>
          </dl>
          {(c.attack || c.defence) && (
            <table className="match-table dp-sides">
              <thead>
                <tr>
                  <th>
                    <StatTip entry={tip(t("Side"), t("On payload and attack/defend maps: attacking (BLU) and defending (RED)."), t("King of the Hill and 5CP have no sides and are left out."), "server log")} inline>
                      {t("Side")}
                    </StatTip>
                  </th>
                  <th>{t("K/30")}</th>
                  <th>{t("D/30")}</th>
                  <th>{t("Impact/30")}</th>
                  <th>{t("Hours")}</th>
                </tr>
              </thead>
              <tbody>
                {(
                  [
                    [t("Attack"), c.attack],
                    [t("Defence"), c.defence],
                  ] as const
                ).map(([label, x]) =>
                  x ? (
                    <tr key={label}>
                      <td>{label}</td>
                      <td className="num">{x.killsPer30?.toFixed(1) ?? "–"}</td>
                      <td className="num">{x.deathsPer30?.toFixed(1) ?? "–"}</td>
                      <td className="num">{x.impactPer30 === null ? "–" : (x.impactPer30 >= 0 ? "+" : "") + x.impactPer30.toFixed(2)}</td>
                      <td className="num muted">{(x.minutes / 60).toFixed(0)}</td>
                    </tr>
                  ) : null,
                )}
              </tbody>
            </table>
          )}
        </section>
      </div>

      {c.weapons.length > 0 && <Weapons c={c} />}
    </>
  );
}

function Verdict({ s, average }: { s: DeepStat; average: boolean }) {
  // The bar is the percentile: where they stand among everyone on the class.
  return (
    <div className={`dp-verdict dp-${s.verdict}`}>
      <span className="dp-value">{fmtStat(s)}</span>
      <span className="dp-label">
        <StatTip entry={statEntry(s.key)} inline>
          {t(STATS[s.key]?.[0] ?? s.key)}
        </StatTip>
      </span>
      <span className="dp-bar" title={t("Better than {0}% of players on the class", { "0": Math.round(s.percentile) })}>
        <span className="dp-fill" style={{ width: `${Math.max(3, s.percentile)}%` }} />
        {average && <span className="dp-mark" style={{ left: "50%" }} title={t("The class average")} />}
      </span>
      <span className="dp-word">
        {s.verdict === "good" ? t("Good") : s.verdict === "poor" ? t("Poor") : t("Okay")}
        {average && <span className="muted"> · {tx("avg {0}", { "0": fmtAvg(s) })}</span>}
      </span>
    </div>
  );
}

function AttributeCard({ a }: { a: DeepAttribute }) {
  const [open, setOpen] = useState(false);
  const [name, what] = (ATTRS[a.key] ?? [a.key, ""]).map((x) => t(x));
  return (
    <div className={open ? "dp-attr open" : "dp-attr"}>
      <button className="dp-attr-head" onClick={() => setOpen(!open)} aria-expanded={open} title={what}>
        <span className="dp-attr-name">{name}</span>
        <span className="dp-attr-score">
          {Math.round(a.score)}
          <span className="muted">/100</span>
        </span>
        <span className="dp-attr-caret" aria-hidden>
          {open ? "▴" : "▾"}
        </span>
      </button>
      <span className="dp-bar">
        <span className="dp-fill" style={{ width: `${Math.max(3, a.score)}%` }} />
      </span>
      {open && (
        <table className="dp-attr-stats">
          <tbody>
            {a.stats.map((s) => (
              <tr key={s.key}>
                <td>
                  <StatTip entry={statEntry(s.key)} inline>
                    {t(STATS[s.key]?.[0] ?? s.key)}
                  </StatTip>
                </td>
                <td className="num">{fmtStat(s)}</td>
                <td className="num muted">{tx("avg {0}", { "0": fmtAvg(s) })}</td>
                <td className="num">{Math.round(s.percentile)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function Weapons({ c }: { c: ClassDeep }) {
  const total = c.weapons.reduce((n, w) => n + w.kills, 0) || 1;
  const s = useSort(`deep-weapons`, c.weapons, {
    weapon: (w) => w.weapon,
    kills: (w) => w.kills,
    share: (w) => w.kills / total,
    dmg: (w) => w.dmg,
    acc: (w) => w.accuracy,
  });
  const acc: Entry | GlossaryKey = "accuracy";
  return (
    <section className="panel">
      <h3>{t("Weapons")}</h3>
      <table className="match-table dp-weapons">
        <thead>
          <tr>
            <SortTh k="weapon" s={s}>
              {t("Weapon")}
            </SortTh>
            <SortTh k="kills" s={s}>
              {t("Kills")}
            </SortTh>
            <SortTh k="share" s={s}>
              {t("Share")}
            </SortTh>
            <SortTh k="dmg" s={s}>
              {t("Damage")}
            </SortTh>
            <SortTh k="acc" s={s} tip={acc}>
              {t("Accuracy")}
            </SortTh>
          </tr>
        </thead>
        <tbody>
          {s.sorted.slice(0, 12).map((w) => (
            <tr key={w.weapon}>
              <td>
                <code>{w.weapon}</code>
              </td>
              <td className="num">{w.kills}</td>
              <td className="num">{((w.kills / total) * 100).toFixed(0)}%</td>
              <td className="num">{w.dmg.toLocaleString()}</td>
              <td className="num">{w.accuracy === null ? "–" : `${(w.accuracy * 100).toFixed(0)}%`}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
