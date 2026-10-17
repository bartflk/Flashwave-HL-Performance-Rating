import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type ClassGuide, type ComponentGuide, type RatingGuide } from "../../api/types";
import { ClassIcon } from "../ClassIcon";
import { classLabel } from "../analysis/common";
import { t, tx } from "../../lib/i18n";
import "./rating.css";
import { Loading } from "../Spinner";

/**
 * How ratings work (Flashy): every class, what its rating is made of, and
 * how a game becomes a number.
 *
 * Nothing on this page is written by hand about the weights: they come from
 * `get_rating_guide`, which lays out the same model the rating uses, so a
 * retuned model redraws the page on its own. What each component means is
 * written next to the component in Rust, where a new one will not compile
 * without it.
 */

type Group = ComponentGuide["group"];

/** Stack order, and the palette's validated order on the dark surface. */
const GROUPS: Group[] = ["fragging", "survival", "teamplay", "objective", "medic", "speciality"];

const GROUP_LABEL: Record<Group, string> = {
  fragging: "Kills and damage",
  survival: "Staying alive",
  teamplay: "Playing for the team",
  objective: "The objective",
  medic: "Medic",
  speciality: "Class speciality",
};

const pct = (x: number) => `${Math.round(x * 100)}%`;

export function RatingGuidePage({ onBack }: { onBack: () => void }) {
  const q = useQuery({ queryKey: ["rating_guide"], queryFn: api.getRatingGuide, staleTime: Infinity });
  const [picked, setPicked] = useState("demoman");

  return (
    <div className="rg-page">
      <button className="linkish back" onClick={onBack}>{t("← Back")}</button>
      <header className="rg-head">
        <h1>{t("How ratings work")}</h1>
        {q.data && (
          <p className="hint">
            {tx("Drawn from the live model ({0}): every weight on this page is the one rating your games right now, and changes when the model does.", { "0": q.data.modelVersion })}
          </p>
        )}
      </header>

      {q.isPending && <Loading>{t("Loading…")}</Loading>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data && (
        <>
          <Pipeline g={q.data} />
          <ClassSection g={q.data} picked={picked} onPick={setPicked} />
          <Overview g={q.data} onPick={setPicked} />
          <KillValues g={q.data} />
          <Unused g={q.data} />
        </>
      )}
    </div>
  );
}

/** The four steps from a game to its number, and the scale it lands on. */
function Pipeline({ g }: { g: RatingGuide }) {
  const steps: [string, string][] = [
    [t("Measure"), t("Each player is rated on the class they played most, if they played it for {0} minutes or more. Every component below is counted for that game, most of them per 10 minutes.", { "0": g.minMinutes })],
    [t("Compare"), t("Each number becomes a percentile against every game on the same class in your matches, on the same map where there are enough of them. Where fewer is better, it is flipped, so a high percentile is always good.")],
    [t("Weigh"), t("The class's model adds the percentiles up by their share. A component a game does not have (the oldest logs have no server log) is left out, and the others take up its share.")],
    [t("Scale"), t("The result is placed so that 1.00 is a typical game and one standard deviation is {0}, the way HLTV's rating reads.", { "0": g.ratingSpread.toFixed(2) })],
  ];
  // Where 0.50 ... 1.50 sit on a normal curve with this spread.
  const marks = [-2, -1, 0, 1, 2].map((sd) => ({ at: 1 + sd * g.ratingSpread, share: ["2%", "16%", "50%", "84%", "98%"][sd + 2] }));
  return (
    <section className="panel rg-pipeline">
      <h2>{t("From a game to a number")}</h2>
      <ol className="rg-steps">
        {steps.map(([title, text], i) => (
          <li key={title} className="rg-step">
            <span className="rg-step-n">{i + 1}</span>
            <strong>{title}</strong>
            <p className="hint">{text}</p>
          </li>
        ))}
      </ol>
      <div className="rg-scale" role="img" aria-label={t("The rating scale: 1.00 is a typical game")}>
        <svg viewBox="0 0 400 60" preserveAspectRatio="none" className="rg-curve" aria-hidden>
          <path
            d={`M0,56 ${Array.from({ length: 81 }, (_, i) => {
              const x = (i / 80) * 400;
              const z = (i / 80) * 6 - 3;
              return `L${x.toFixed(1)},${(56 - 50 * Math.exp((-z * z) / 2)).toFixed(1)}`;
            }).join(" ")} L400,56 Z`}
          />
        </svg>
        <div className="rg-scale-marks">
          {marks.map((m) => (
            <span key={m.at} style={{ left: `${((m.at - (1 - 3 * g.ratingSpread)) / (6 * g.ratingSpread)) * 100}%` }}>
              <b>{m.at.toFixed(2)}</b>
              <small>{tx("better than {0}", { "0": m.share })}</small>
            </span>
          ))}
        </div>
      </div>
    </section>
  );
}

function ClassPicker({ g, picked, onPick }: { g: RatingGuide; picked: string; onPick: (c: string) => void }) {
  return (
    <div className="rg-picker" role="tablist" aria-label={t("Class")}>
      {g.classes.map((c) => (
        <button key={c.class} role="tab" aria-selected={c.class === picked} className={c.class === picked ? "rg-pick on" : "rg-pick"} onClick={() => onPick(c.class)}>
          <ClassIcon cls={c.class} size={22} />
          <span>{classLabel(c.class)}</span>
        </button>
      ))}
    </div>
  );
}

function ClassSection({ g, picked, onPick }: { g: RatingGuide; picked: string; onPick: (c: string) => void }) {
  const c = g.classes.find((x) => x.class === picked) ?? g.classes[0];
  return (
    <section className="panel rg-class" id="rg-class">
      <h2>{t("What each class is rated on")}</h2>
      <ClassPicker g={g} picked={c.class} onPick={onPick} />
      <div className="rg-class-head">
        <ClassIcon cls={c.class} size={34} />
        <div>
          <h3>{classLabel(c.class)}</h3>
          <p className="hint">
            {c.ownModel
              ? tx("{0} components, fitted against who won the {1} matchup.", { "0": c.components.length, "1": classLabel(c.class) })
              : t("Rated by the shared fallback model: this class has no model of its own yet.")}
          </p>
        </div>
      </div>
      <ShareBar c={c} />
      <GroupLegend groups={GROUPS.filter((gr) => c.components.some((x) => x.group === gr))} />
      <ul className="rg-rows">
        {c.components.map((x) => (
          <li key={x.key} className="rg-row">
            <div className="rg-row-top">
              <span className="rg-row-label">{t(x.label)}</span>
              <span className="rg-row-bar" aria-hidden>
                <i className={`rg-g-${x.group}`} style={{ width: `${(x.share / c.components[0].share) * 100}%` }} />
              </span>
              <span className="rg-row-share">{pct(x.share)}</span>
            </div>
            <p className="rg-row-desc">{t(x.description)}</p>
            <div className="rg-tags">
              <span className="rg-tag">{t(x.unit)}</span>
              <span className="rg-tag">{t(GROUP_LABEL[x.group])}</span>
              {!x.higherIsBetter && <span className="rg-tag">{t("fewer is better")}</span>}
              {x.source === "serverLog" && <span className="rg-tag rg-tag-src" title={t("Read kill by kill from the server log. The oldest logs have none, and the rating leaves this out for them.")}>{t("needs the server log")}</span>}
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** The class's rating as one bar, a segment per component, coloured by group. */
function ShareBar({ c }: { c: ClassGuide }) {
  const ordered = [...c.components].sort((a, b) => GROUPS.indexOf(a.group) - GROUPS.indexOf(b.group) || b.share - a.share);
  return (
    <div className="rg-share" role="img" aria-label={ordered.map((x) => `${x.label} ${pct(x.share)}`).join(", ")}>
      {ordered.map((x) => (
        <span key={x.key} className={`rg-seg rg-g-${x.group}`} style={{ flexGrow: x.share }} title={`${t(x.label)}: ${pct(x.share)}`}>
          {x.share >= 0.09 && <em>{pct(x.share)}</em>}
        </span>
      ))}
    </div>
  );
}

function GroupLegend({ groups }: { groups: Group[] }) {
  return (
    <div className="rg-legend">
      {groups.map((gr) => (
        <span key={gr}>
          <i className={`rg-g-${gr}`} /> {t(GROUP_LABEL[gr])}
        </span>
      ))}
    </div>
  );
}

/** Every class against every component at once: which class is rated on what. */
function Overview({ g, onPick }: { g: RatingGuide; onPick: (c: string) => void }) {
  const used = new Map<string, ComponentGuide>();
  for (const c of g.classes) for (const x of c.components) if (!used.has(x.key)) used.set(x.key, x);
  const total = (key: string) => g.classes.reduce((s, c) => s + (c.components.find((x) => x.key === key)?.share ?? 0), 0);
  const rows = [...used.values()].sort((a, b) => GROUPS.indexOf(a.group) - GROUPS.indexOf(b.group) || total(b.key) - total(a.key));
  const max = Math.max(...g.classes.flatMap((c) => c.components.map((x) => x.share)));
  return (
    <section className="panel rg-overview">
      <h2>{t("All nine at a glance")}</h2>
      <p className="hint">{t("Each cell is that component's share of that class's rating. Click a class to read its model.")}</p>
      <div className="rg-table-wrap">
        <table className="rg-table">
          <thead>
            <tr>
              <th />
              {g.classes.map((c) => (
                <th key={c.class}>
                  <button className="rg-th-class" onClick={() => { onPick(c.class); document.getElementById("rg-class")?.scrollIntoView({ behavior: "smooth", block: "start" }); }} title={classLabel(c.class)}>
                    <ClassIcon cls={c.class} size={20} />
                  </button>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.key}>
                <th scope="row">
                  <i className={`rg-dot rg-g-${r.group}`} aria-hidden /> {t(r.label)}
                </th>
                {g.classes.map((c) => {
                  const x = c.components.find((y) => y.key === r.key);
                  return (
                    <td key={c.class} title={x ? `${classLabel(c.class)} · ${t(r.label)}: ${pct(x.share)}` : undefined}>
                      {x && (
                        <span className="rg-cell" style={{ ["--rg-a" as string]: String(0.18 + 0.72 * (x.share / max)) }}>
                          {pct(x.share)}
                        </span>
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

/** What killing each class is worth, behind impact kills and assists. */
function KillValues({ g }: { g: RatingGuide }) {
  const max = Math.max(...g.victimValues.map(([, v]) => v));
  const rows = [...g.victimValues].sort((a, b) => b[1] - a[1]);
  return (
    <section className="panel rg-kills">
      <h2>{t("What a kill is worth")}</h2>
      <p className="hint">{t("Impact kills and assists value each kill by the class that died, measured from how often killing it won the round: the longer the bar, the more that kill is worth. Maps and, on attack/defend, the victim's side adjust these.")}</p>
      <ul className="rg-kill-rows">
        {rows.map(([cls, v]) => (
          <li key={cls}>
            <span className="rg-kill-class"><ClassIcon cls={cls} size={18} /> {classLabel(cls)}</span>
            <span className="rg-kill-bar" aria-hidden><i style={{ width: `${(v / max) * 100}%` }} /></span>
            <span className="rg-kill-v">{v.toFixed(1)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Components the rating can measure but no model uses right now. */
function Unused({ g }: { g: RatingGuide }) {
  const used = new Set(g.classes.flatMap((c) => c.components.map((x) => x.key)));
  const unused = g.glossary.filter((x) => !used.has(x.key));
  if (unused.length === 0) return null;
  return (
    <section className="panel rg-unused">
      <h2>{t("Measured, but not in any model")}</h2>
      <p className="hint">{t("No class's model uses these right now: each was tried against who won, and the models above did as well or better without it. They are still measured.")}</p>
      <ul className="rg-rows">
        {unused.map((x) => (
          <li key={x.key} className="rg-row">
            <div className="rg-row-top">
              <span className="rg-row-label"><i className={`rg-dot rg-g-${x.group}`} aria-hidden /> {t(x.label)}</span>
            </div>
            <p className="rg-row-desc">{t(x.description)}</p>
          </li>
        ))}
      </ul>
    </section>
  );
}
