import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { ComponentGuide, SixesGuide, SixesGuideRole } from "../../api/types";
import { ApiError } from "../ApiError";
import { ClassIcon } from "../ClassIcon";
import { Loading } from "../Spinner";
import { classLabel } from "../analysis/common";
import { roleClass, unitLabel } from "../../lib/gameformat";
import { sixesComponent } from "../../lib/sixesguide";
import { t, tx } from "../../lib/i18n";

/**
 * "How ratings work" for 6v6 (Flashy: "is 'how ratings work' page update for
 * 6's as well?"): each role's model from the 6v6 weights, laid out like the
 * Highlander page, with the parts not measured yet marked and what each one
 * counts for today.
 */

type Group = ComponentGuide["group"];
const GROUPS: Group[] = ["fragging", "survival", "teamplay", "objective", "medic", "speciality"];
const GROUP_LABEL: Record<Group, () => string> = {
  fragging: () => t("Kills and damage"),
  survival: () => t("Staying alive"),
  teamplay: () => t("Playing for the team"),
  objective: () => t("The objective"),
  medic: () => t("Medic"),
  speciality: () => t("Class speciality"),
};
const pct = (x: number) => `${Math.round(x * 100)}%`;

export function SixesGuideSections() {
  const q = useQuery({ queryKey: ["sixes_rating_guide"], queryFn: api.getSixesRatingGuide, staleTime: 5 * 60_000 });
  const [picked, setPicked] = useState("pocket");
  if (q.isPending) return <Loading>{t("Loading…")}</Loading>;
  if (q.isError) return <ApiError error={q.error} />;
  const g = q.data;
  return (
    <>
      <p className="hint rg-model">{tx("Drawn from the live 6v6 model ({0}): every share on this page is the one rating 6v6 games right now.", { "0": g.modelVersion })}</p>
      <Pipeline g={g} />
      {!g.roundWin && (
        <section className="panel rg-pending">
          <h2>{t("Not measured yet")}</h2>
          <p className="hint">
            {t("Round impact is half of most roles' model, and a few parts need every log's server log or an agreed definition. Until they are measured, a game is rated on the parts that are, each scaled up to fill the gap: the \"counts now\" figure beside each part.")}
          </p>
        </section>
      )}
      <RoleSection g={g} picked={picked} onPick={setPicked} />
      {g.victimValues.length > 0 && <KillValues g={g} />}
    </>
  );
}

function Pipeline({ g }: { g: SixesGuide }) {
  const steps: [string, string][] = [
    [t("Measure"), t("Each player is rated on their 6v6 role, the pocket and the roamer, the two Scouts, the Demoman and the Medic, told apart by how they played. A main role needs {0} minutes on the class; an off-class stint needs {1}.", { "0": g.minMinutes, "1": g.offMinMinutes })],
    [t("Compare"), t("Each number becomes a percentile against the same role in 6v6 league games: the pool built from every 6v6 official in the logs.tf database. Where fewer is better, it is flipped, so a high percentile is always good.")],
    [t("Weigh"), t("The role's model adds the percentiles up by their share. A part the game does not have is left out, and the others take up its share.")],
    [t("Scale"), t("The result is placed against the pool so that 1.00 is a typical game for the role and one standard deviation is 0.25, the same scale as Highlander's.")],
  ];
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
    </section>
  );
}

function RoleSection({ g, picked, onPick }: { g: SixesGuide; picked: string; onPick: (r: string) => void }) {
  const r = g.roles.find((x) => x.role === picked) ?? g.roles[0];
  const measured = r.components.filter((c) => c.measured).length;
  const pick = (x: SixesGuideRole) => (
    <button key={x.role} role="tab" aria-selected={x.role === r.role} className={x.role === r.role ? "rg-pick on" : "rg-pick"} onClick={() => onPick(x.role)}>
      <ClassIcon cls={roleClass(x.role)} size={22} />
      <span>{x.off ? classLabel(roleClass(x.role)) : unitLabel(x.role)}</span>
    </button>
  );
  return (
    <section className="panel rg-class" id="rg-class">
      <h2>{t("What each role is rated on")}</h2>
      <div className="rg-picker" role="tablist" aria-label={t("Role")}>
        {g.roles.filter((x) => !x.off).map(pick)}
      </div>
      <div className="rg-picker rg-picker-off" role="tablist" aria-label={t("Off-classes")}>
        <span className="rg-picker-label">{t("Off-classes")}</span>
        {g.roles.filter((x) => x.off).map(pick)}
      </div>
      <div className="rg-class-head">
        <ClassIcon cls={roleClass(r.role)} size={34} />
        <div>
          <h3>{unitLabel(r.role)}</h3>
          <p className="hint">{tx("{0} parts; {1} measured now.", { "0": r.components.length, "1": measured })}</p>
        </div>
      </div>
      <div className="rg-share" role="img" aria-label={r.components.map((x) => `${sixesComponent(x.key).label} ${pct(x.share)}`).join(", ")}>
        {[...r.components]
          .sort((a, b) => GROUPS.indexOf(sixesComponent(a.key).group) - GROUPS.indexOf(sixesComponent(b.key).group) || b.share - a.share)
          .map((x) => (
            <span
              key={x.key}
              className={`rg-seg rg-g-${sixesComponent(x.key).group}${x.measured ? "" : " rg-seg-off"}`}
              style={{ flexGrow: x.share }}
              title={`${sixesComponent(x.key).label}: ${pct(x.share)}${x.measured ? "" : ` · ${t("not measured yet")}`}`}
            >
              {x.share >= 0.09 && <em>{pct(x.share)}</em>}
            </span>
          ))}
      </div>
      <div className="rg-legend">
        {GROUPS.filter((gr) => r.components.some((x) => sixesComponent(x.key).group === gr)).map((gr) => (
          <span key={gr}>
            <i className={`rg-g-${gr}`} /> {GROUP_LABEL[gr]()}
          </span>
        ))}
        <span>
          <i className="rg-seg-off rg-g-teamplay" /> {t("not measured yet")}
        </span>
      </div>
      <ul className="rg-rows">
        {r.components.map((x) => {
          const c = sixesComponent(x.key);
          return (
            <li key={x.key} className={x.measured ? "rg-row" : "rg-row rg-row-off"}>
              <div className="rg-row-top">
                <span className="rg-row-label">{c.label}</span>
                <span className="rg-row-bar" aria-hidden>
                  <i className={`rg-g-${c.group}`} style={{ width: `${(x.share / r.components[0].share) * 100}%` }} />
                </span>
                <span className="rg-row-share">{pct(x.share)}</span>
              </div>
              {c.description && <p className="rg-row-desc">{c.description}</p>}
              <div className="rg-tags">
                {c.unit && <span className="rg-tag">{c.unit}</span>}
                <span className="rg-tag">{GROUP_LABEL[c.group]()}</span>
                {!x.higherIsBetter && <span className="rg-tag">{t("fewer is better")}</span>}
                {x.measured ? (
                  <span className="rg-tag" title={t("Its share of the parts measured now, which is what it counts for in a rating today")}>
                    {tx("counts {0} now", { "0": pct(x.counts) })}
                  </span>
                ) : (
                  <span className="rg-tag rg-tag-src" title={t("Left out of every rating until it is measured; the measured parts share its weight")}>
                    {t("not measured yet")}
                  </span>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function KillValues({ g }: { g: SixesGuide }) {
  const max = Math.max(...g.victimValues.map(([, v]) => v));
  return (
    <section className="panel rg-kills">
      <h2>{t("What a kill is worth")}</h2>
      <p className="hint">{t("Kills are valued by the class that died, measured from how often killing it won the round in 6v6: the longer the bar, the more that kill is worth. The average kill is 1.")}</p>
      <ul className="rg-kill-rows">
        {g.victimValues.map(([cls, v]) => (
          <li key={cls}>
            <span className="rg-kill-class">
              <ClassIcon cls={cls} size={18} /> {classLabel(cls)}
            </span>
            <span className="rg-kill-bar" aria-hidden>
              <i style={{ width: `${(v / max) * 100}%` }} />
            </span>
            <span className="rg-kill-v">{v.toFixed(2)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
