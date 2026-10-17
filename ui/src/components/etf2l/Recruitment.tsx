import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RecruitPost } from "../../api/types";
import { errorMessage } from "../../api/types";
import { formatDate } from "../../lib/format";
import { openPlayer } from "../../lib/goto";
import { t, tx } from "../../lib/i18n";
import { CLASS_ORDER, classLabel } from "../analysis/common";
import { Seg } from "../analysis/MapChrome";
import { ClassIcon } from "../ClassIcon";

/**
 * ETF2L's recruitment, Highlander only (Q74; Flashy: "recruitments would be
 * nice to incorporate"): players looking for a team and teams looking for
 * players, newest first, filtered by class and skill. A player opens their
 * profile in the app, a team its page; the post itself is on etf2l.org.
 */

export function useRecruitment() {
  return useQuery({ queryKey: ["etf2l_recruitment"], queryFn: api.getRecruitment, staleTime: 10 * 60_000 });
}

/** ETF2L's class names ("Demoman", "Heavy") as the app's keys. */
function classKey(c: string): string {
  const k = c.toLowerCase();
  return k === "heavy" ? "heavyweapons" : k;
}

const iconKey = (c: string) => (classKey(c) === "heavyweapons" ? "heavy" : classKey(c));

/** ETF2L's skill levels, best first, for the filter's order. */
const SKILLS = ["Prem", "High", "Mid", "Low", "Open", "Fresh"];
const skillRank = (s: string | null) => {
  const i = SKILLS.findIndex((x) => s?.startsWith(x));
  return i < 0 ? 99 : i;
};

type Kind = "players" | "teams";

export function RecruitmentPage({ onTeam }: { onTeam: (id: number) => void }) {
  const q = useRecruitment();
  const [kind, setKind] = useState<Kind>("players");
  const [cls, setCls] = useState<string | null>(null);
  const [skill, setSkill] = useState("");
  const all = (kind === "players" ? q.data?.players : q.data?.teams) ?? [];
  const skills = [...new Set(all.flatMap((p) => (p.skill ? p.skill.split("/") : [])))].sort((a, b) => skillRank(a) - skillRank(b));
  const shown = all.filter((p) => (cls === null || p.classes.some((c) => iconKey(c) === cls)) && (skill === "" || (p.skill ?? "").split("/").includes(skill)));
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">{t("ETF2L Highlander")}</span>
        <h1>{t("Recruitment")}</h1>
        <p className="hint">{t("The newest posts on ETF2L: players looking for a team, and teams looking for players.")}</p>
      </header>
      <div className="e2-filters">
        <Seg
          label={t("Recruitment")}
          value={kind}
          onChange={(k) => {
            setKind(k);
            setSkill("");
          }}
          options={[
            { v: "players", label: tx("Players looking for a team ({0})", { "0": q.data?.players.length ?? 0 }) },
            { v: "teams", label: tx("Teams looking for players ({0})", { "0": q.data?.teams.length ?? 0 }) },
          ]}
        />
        <div className="e2-classes" role="group" aria-label={t("Class")}>
          <button className={cls === null ? "on" : undefined} onClick={() => setCls(null)}>
            {t("All")}
          </button>
          {CLASS_ORDER.map((c) => (
            <button key={c} className={cls === c ? "on" : undefined} onClick={() => setCls(cls === c ? null : c)} title={classLabel(c)} aria-pressed={cls === c}>
              <ClassIcon cls={c} size={20} />
            </button>
          ))}
        </div>
        {skills.length > 0 && (
          <label className="km-field">
            <span>{t("Skill")}</span>
            <select value={skill} onChange={(e) => setSkill(e.target.value)}>
              <option value="">{t("Any")}</option>
              {skills.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
      {q.isPending && <p className="hint">{t("Loading recruitment…")}</p>}
      {q.isError && <p className="error">{errorMessage(q.error)}</p>}
      {q.data && shown.length === 0 && (
        <section className="panel e2-panel">
          <p className="hint">{all.length === 0 ? t("No recruitment posts could be read from ETF2L just now.") : t("No posts match these filters.")}</p>
        </section>
      )}
      {shown.length > 0 && (
        <div className="rc-grid">
          {shown.map((p) => (
            <RecruitCard key={p.id} p={p} kind={kind} onTeam={onTeam} />
          ))}
        </div>
      )}
    </>
  );
}

function RecruitCard({ p, kind, onTeam }: { p: RecruitPost; kind: Kind; onTeam: (id: number) => void }) {
  const open = () => {
    if (kind === "players" && p.accountId !== null) openPlayer(p.accountId);
    else if (kind === "teams" && p.teamId !== null) onTeam(p.teamId);
    else void api.openExternal(p.link);
  };
  return (
    <article className="rc-card">
      <button className="rc-who" onClick={open} title={kind === "players" ? t("Open profile") : t("Open the team")}>
        {p.avatar ? <img src={p.avatar} alt="" className="rc-avatar" loading="lazy" /> : <span className="rc-avatar" aria-hidden />}
        <span className="rc-name">{p.name}</span>
      </button>
      <div className="rc-classes" aria-label={kind === "players" ? t("Plays") : t("Looking for")}>
        <span className="rc-label">{kind === "players" ? t("Plays") : t("Looking for")}</span>
        {p.classes
          .slice()
          .sort((a, b) => CLASS_ORDER.indexOf(iconKey(a)) - CLASS_ORDER.indexOf(iconKey(b)))
          .map((c) => (
            <span key={c} title={c}>
              <ClassIcon cls={classKey(c)} size={20} />
            </span>
          ))}
      </div>
      <div className="rc-foot">
        {p.skill ? <span className="rc-skill">{p.skill}</span> : <span className="muted">{t("any skill")}</span>}
        <span className="muted" title={p.lastComment ? t("Last comment {0}", { "0": formatDate(p.lastComment, true) }) : undefined}>
          {p.comments === 1 ? t("1 comment") : tx("{0} comments", { "0": p.comments })}
        </span>
        <button className="e2-more" onClick={() => void api.openExternal(p.link)}>
          {t("Post")} ↗
        </button>
      </div>
    </article>
  );
}

/** The newest few of each, for the Home page's rail. */
export function RecruitPreview({ onAll, onTeam }: { onAll: () => void; onTeam: (id: number) => void }) {
  const q = useRecruitment();
  const players = (q.data?.players ?? []).slice(0, 5);
  const teams = (q.data?.teams ?? []).slice(0, 3);
  if (players.length + teams.length === 0) return null;
  const line = (p: RecruitPost, kind: Kind) => (
    <li key={`${kind}${p.id}`}>
      <button
        onClick={() => (kind === "players" && p.accountId !== null ? openPlayer(p.accountId) : kind === "teams" && p.teamId !== null ? onTeam(p.teamId) : void api.openExternal(p.link))}
      >
        <span className="rc-mini-classes">
          {p.classes.slice(0, 3).map((c) => (
            <ClassIcon key={c} cls={classKey(c)} size={16} />
          ))}
        </span>
        <span className="rc-mini-name">{p.name}</span>
        <span className="muted rc-mini-skill">{p.skill ?? ""}</span>
      </button>
    </li>
  );
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("Recruitment")}</h2>
        <button className="e2-more" onClick={onAll}>
          {t("All posts")} ›
        </button>
      </div>
      {players.length > 0 && (
        <>
          <span className="e2-sub">{t("Looking for a team")}</span>
          <ul className="rc-mini">{players.map((p) => line(p, "players"))}</ul>
        </>
      )}
      {teams.length > 0 && (
        <>
          <span className="e2-sub">{t("Looking for players")}</span>
          <ul className="rc-mini">{teams.map((p) => line(p, "teams"))}</ul>
        </>
      )}
    </section>
  );
}
