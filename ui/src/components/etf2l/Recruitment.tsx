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
import { HtmlBody } from "./News";

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

export type Kind = "players" | "teams";

export function RecruitmentPage({ onOpen }: { onOpen: (p: RecruitPost, kind: Kind) => void }) {
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
            <RecruitCard key={p.id} p={p} kind={kind} onOpen={() => onOpen(p, kind)} />
          ))}
        </div>
      )}
    </>
  );
}

/** A post's card: the whole card opens the post. */
function RecruitCard({ p, kind, onOpen }: { p: RecruitPost; kind: Kind; onOpen: () => void }) {
  return (
    <button className="rc-card" onClick={onOpen} title={t("Read the post")}>
      <span className="rc-who">
        {p.avatar ? <img src={p.avatar} alt="" className="rc-avatar" loading="lazy" /> : <span className="rc-avatar" aria-hidden />}
        <span className="rc-name">{p.name}</span>
      </span>
      <ClassRow p={p} kind={kind} />
      <span className="rc-foot">
        {p.skill ? <span className="rc-skill">{p.skill}</span> : <span className="muted">{t("any skill")}</span>}
        <span className="muted" title={p.lastComment ? t("Last comment {0}", { "0": formatDate(p.lastComment, true) }) : undefined}>
          {p.comments === 1 ? t("1 comment") : tx("{0} comments", { "0": p.comments })}
        </span>
        <span className="e2-more rc-read">{t("Read")} ›</span>
      </span>
    </button>
  );
}

function ClassRow({ p, kind }: { p: RecruitPost; kind: Kind }) {
  return (
    <span className="rc-classes" aria-label={kind === "players" ? t("Plays") : t("Looking for")}>
      <span className="rc-label">{kind === "players" ? t("Plays") : t("Looking for")}</span>
      {p.classes
        .slice()
        .sort((a, b) => CLASS_ORDER.indexOf(iconKey(a)) - CLASS_ORDER.indexOf(iconKey(b)))
        .map((c) => (
          <span key={c} title={c}>
            <ClassIcon cls={classKey(c)} size={20} />
          </span>
        ))}
    </span>
  );
}

/** "15 minutes ago", "3 hours ago", "2 days ago", else the date. */
function ago(time: number): string {
  const s = Math.max(0, Date.now() / 1000 - time);
  const m = Math.max(1, Math.round(s / 60));
  const h = Math.round(s / 3600);
  const d = Math.round(s / 86400);
  if (s < 3600) return m === 1 ? t("1 minute ago") : t("{0} minutes ago", { "0": m });
  if (s < 86400) return h === 1 ? t("1 hour ago") : t("{0} hours ago", { "0": h });
  if (s < 14 * 86400) return d === 1 ? t("1 day ago") : t("{0} days ago", { "0": d });
  return formatDate(time, true);
}

/** The number at the end of a post's link, which its page is found by. */
function postId(link: string): number | null {
  const m = /recruitment\/(\d+)/.exec(link);
  return m ? Number(m[1]) : null;
}

/** One post, read in the app: the message, when it was posted, the poster's
 *  team history, the comments. */
export function RecruitPostPage({ p, kind, onTeam }: { p: RecruitPost; kind: Kind; onTeam: (id: number) => void }) {
  const id = postId(p.link);
  const q = useQuery({ queryKey: ["recruit_post", id], queryFn: () => api.getRecruitPost(id!), enabled: id !== null, staleTime: 5 * 60_000 });
  const d = q.data;
  return (
    <article className="rc-post">
      <header className="rc-post-head">
        {p.avatar ? <img src={p.avatar} alt="" className="rc-avatar rc-avatar-big" /> : <span className="rc-avatar rc-avatar-big" aria-hidden />}
        <div className="rc-post-title">
          <span className="e2-label">{kind === "players" ? t("Looking for a team") : t("Looking for players")}</span>
          <h1>{p.name}</h1>
          <span className="rc-post-meta">
            {p.skill && <span className="rc-skill">{p.skill}</span>}
            {d?.posted && (
              <span title={formatDate(d.posted, true)}>
                {d.postedBy ? tx("Posted by {0} {1}", { "0": d.postedBy, "1": ago(d.posted) }) : tx("Posted {0}", { "0": ago(d.posted) })}
              </span>
            )}
            {d?.lastOnline && <span className="muted">{tx("Last online {0}", { "0": ago(d.lastOnline) })}</span>}
          </span>
        </div>
        <div className="rc-post-actions">
          {kind === "players" && p.accountId !== null && (
            <button className="km-chip" onClick={() => openPlayer(p.accountId!)}>
              {t("Open profile")} ›
            </button>
          )}
          {kind === "teams" && p.teamId !== null && (
            <button className="km-chip" onClick={() => onTeam(p.teamId!)}>
              {t("Open the team")} ›
            </button>
          )}
          <button className="km-chip" onClick={() => void api.openExternal(p.link)}>
            {t("On ETF2L")} ↗
          </button>
        </div>
      </header>

      <div className="rc-post-grid">
        <div className="e2-main">
          <section className="panel e2-panel">
            <ClassRow p={p} kind={kind} />
            {q.isPending && <p className="hint">{t("Loading the post…")}</p>}
            {q.isError && <p className="error">{errorMessage(q.error)}</p>}
            {q.data === null && <p className="hint">{t("The post could not be read from ETF2L; it may have been removed.")}</p>}
            {d && (d.html.trim() ? <HtmlBody html={d.html} /> : <p className="hint">{t("No message.")}</p>)}
          </section>
          {d && (
            <section className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{d.comments.length === 1 ? t("1 comment") : tx("{0} comments", { "0": d.comments.length })}</h2>
              </div>
              {d.comments.length === 0 ? (
                <p className="hint">{t("No comments yet.")}</p>
              ) : (
                <ol className="rc-comments">
                  {d.comments.map((c, i) => (
                    <li key={i}>
                      <div className="rc-c-head">
                        <strong>{c.author}</strong>
                        {c.teams && <span className="muted">{c.teams}</span>}
                        {c.time && (
                          <span className="muted rc-c-time" title={formatDate(c.time, true)}>
                            {ago(c.time)}
                          </span>
                        )}
                      </div>
                      <HtmlBody html={c.html} className="np-body rc-c-body" />
                    </li>
                  ))}
                </ol>
              )}
            </section>
          )}
        </div>
        {d && d.history.length > 0 && (
          <aside className="e2-rail">
            <section className="panel e2-panel">
              <div className="e2-panel-head">
                <h2>{t("Team history")}</h2>
              </div>
              <ol className="rc-history">
                {d.history.slice(0, 10).map((h, i) => (
                  <li key={i}>
                    <span className={h.action === "Joined" ? "rc-h-act in" : "rc-h-act out"}>{h.action === "Joined" ? t("Joined") : t("Left")}</span>
                    {h.teamId !== null ? (
                      <button className="e2-team" onClick={() => onTeam(h.teamId!)}>
                        <span>{h.team}</span>
                      </button>
                    ) : (
                      <span>{h.team}</span>
                    )}
                    <span className="muted rc-h-time">{h.time ? formatDate(h.time, true) : ""}</span>
                  </li>
                ))}
              </ol>
            </section>
          </aside>
        )}
      </div>
    </article>
  );
}

/** The newest few of each, for the Home page's rail. */
export function RecruitPreview({ onAll, onOpen }: { onAll: () => void; onOpen: (p: RecruitPost, kind: Kind) => void }) {
  const q = useRecruitment();
  const players = (q.data?.players ?? []).slice(0, 5);
  const teams = (q.data?.teams ?? []).slice(0, 3);
  if (players.length + teams.length === 0) return null;
  const line = (p: RecruitPost, kind: Kind) => (
    <li key={`${kind}${p.id}`}>
      <button onClick={() => onOpen(p, kind)}>
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
