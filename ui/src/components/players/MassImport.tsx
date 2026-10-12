import { useEffect, useState, useSyncExternalStore } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { errorMessage, type MassBatch, type MassCandidate, type MassRunStep, type MassSearchStep, type MassSummary } from "../../api/types";
import { capitalize, formatDate } from "../../lib/format";
import { t, tx } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";

/**
 * Mass import (Q69, Emiel): find a player's demos on one map on demos.tf,
 * pick them (or the newest 10, 25 or 50), and import them in the background
 * so Lives on a map has far more than the matches this machine recorded.
 * When a run ends it asks whether to keep the demo files; the second tab
 * manages every run's files afterwards. Routes stay whatever happens to the
 * files.
 */

const CLASSES = ["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"];
const AMOUNTS = [10, 25, 50];

// ---- The run, followed app-wide: it carries on with the window closed. ----

type RunState = { step: MassRunStep | null; done: MassSummary | null; error: string | null; search: MassSearchStep | null };
let run: RunState = { step: null, done: null, error: null, search: null };
const subs = new Set<() => void>();
let listening = false;
function set(next: Partial<RunState>) {
  run = { ...run, ...next };
  subs.forEach((f) => f());
}
function listen() {
  if (listening) return;
  listening = true;
  void api.onMass({
    onSearch: (search) => set({ search }),
    onProgress: (step) => set({ step, done: null, error: null }),
    onDone: (done) => set({ done, step: null }),
    onError: (e) => set({ error: errorMessage(e.error), step: null }),
  });
}
function useRun(): RunState {
  listen();
  return useSyncExternalStore(
    (f) => {
      subs.add(f);
      return () => subs.delete(f);
    },
    () => run,
  );
}

function mb(bytes: number): string {
  return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`;
}

/** The button that opens the window, for Lives on a map. */
export function MassImportButton(props: { accountId: number; name: string; map: string | null; cls: string | null }) {
  const [open, setOpen] = useState(false);
  const r = useRun();
  const busy = r.step !== null;
  return (
    <>
      <button className={busy ? "km-chip on" : "km-chip"} onClick={() => setOpen(true)} title={t("Download their demos on this map from demos.tf and read them")}>
        {busy ? tx("Importing {0} of {1}…", { "0": Math.min(r.step!.done + 1, r.step!.total), "1": r.step!.total }) : t("More from demos.tf…")}
      </button>
      {open && <MassImportWindow {...props} onClose={() => setOpen(false)} />}
    </>
  );
}

function MassImportWindow({ accountId, name, map, cls, onClose }: { accountId: number; name: string; map: string | null; cls: string | null; onClose: () => void }) {
  const [tab, setTab] = useState<"find" | "imported">("find");
  const batches = useQuery({ queryKey: ["mass_batches"], queryFn: api.massBatches });
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);

  return (
    <div className="modal-scrim" role="dialog" aria-modal="true" aria-label={t("Demos from demos.tf")} onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal mi">
        <header className="mi-head">
          <h2>{tx("Demos from demos.tf: {0}", { "0": <strong>{name}</strong> })}</h2>
          <button className="linkish" onClick={onClose} aria-label={t("Close")}>
            ×
          </button>
        </header>
        <nav className="pp-tabs" role="tablist">
          <button role="tab" aria-selected={tab === "find"} className={tab === "find" ? "pp-tab on" : "pp-tab"} onClick={() => setTab("find")}>
            {t("Find and import")}
          </button>
          <button role="tab" aria-selected={tab === "imported"} className={tab === "imported" ? "pp-tab on" : "pp-tab"} onClick={() => setTab("imported")}>
            {tx("Imported ({0})", { "0": batches.data?.length ?? 0 })}
          </button>
        </nav>
        {tab === "find" ? <Find accountId={accountId} map={map} cls={cls} onManage={() => setTab("imported")} /> : <Imported batches={batches.data ?? []} />}
      </div>
    </div>
  );
}

/** Highlander's usual maps, offered when the player has no routes yet. */
const POOL = ["product", "upward", "vigil", "steel", "proot", "swiftwater", "ashville", "warmtic", "badwater", "lakeside", "cascade", "eruption"];

function Find({ accountId, map: initialMap, cls, onManage }: { accountId: number; map: string | null; cls: string | null; onManage: () => void }) {
  const qc = useQueryClient();
  const r = useRun();
  const [map, setMap] = useState<string>(initialMap ?? POOL[0]);
  const maps = POOL.includes(map) ? POOL : [map, ...POOL];
  const [klass, setKlass] = useState<string | null>(cls);
  const [want, setWant] = useState(50);
  const [found, setFound] = useState<MassCandidate[] | null>(null);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [answered, setAnswered] = useState<string | null>(null);

  async function search(n: number, pickAll: boolean) {
    if (!map) return;
    setSearching(true);
    setError(null);
    setFound(null);
    set({ search: null });
    try {
      const got = await api.massSearch(accountId, map, klass, n);
      setFound(got);
      // Already here: nothing to fetch, so not picked by default.
      setPicked(new Set(pickAll ? got.filter((c) => !c.here).map((c) => c.demosTfId) : []));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSearching(false);
    }
  }

  async function start() {
    if (!found || !map) return;
    setError(null);
    setAnswered(null);
    const picks = found.filter((c) => picked.has(c.demosTfId)).map((c) => ({ demosTfId: c.demosTfId, time: c.time, map: c.map }));
    try {
      await api.massStart(accountId, map, klass, picks);
      set({ done: null, error: null, step: { batchId: 0, done: 0, total: picks.length, demosTfId: null, stage: "log", bytes: 0, ofBytes: null } });
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  async function keep(k: boolean) {
    if (!r.done) return;
    const freed = await api.massKeep(r.done.batchId, k);
    // Answered: the question does not come back on reopening the window.
    set({ done: null });
    setAnswered(k ? t("Kept. Manage them under Imported.") : t("Deleted, {0} freed. The routes stay.", { "0": mb(freed) }));
    void qc.invalidateQueries({ queryKey: ["mass_batches"] });
    void qc.invalidateQueries({ queryKey: ["lives_on_map"] });
    void qc.invalidateQueries({ queryKey: ["lives_maps"] });
  }

  // A run going (or just ended) owns the window.
  if (r.step) {
    const s = r.step;
    const pct = s.ofBytes ? Math.round((s.bytes / s.ofBytes) * 100) : null;
    return (
      <div className="mi-body">
        <p>{tx("Importing demo {0} of {1}", { "0": Math.min(s.done + 1, s.total), "1": s.total })}</p>
        <div className="progress-track">
          <div className="progress-fill" style={{ width: `${Math.round(((s.done + (pct ?? 0) / 100) / Math.max(1, s.total)) * 100)}%` }} />
        </div>
        <p className="hint">
          {s.stage === "log" ? t("Finding its match…") : pct !== null ? tx("Downloading and reading… {0}%", { "0": pct }) : t("Reading…")}
          {" · "}
          {t("One at a time, about a minute each. You can close this window; it carries on.")}
        </p>
        <div className="modal-actions">
          <button onClick={() => void api.massCancel()}>{t("Stop after this demo")}</button>
        </div>
      </div>
    );
  }
  if (r.done && !answered) {
    const d = r.done;
    return (
      <div className="mi-body">
        <p>
          {tx("Done: {0} imported", { "0": d.done })}
          {d.failed > 0 && tx(", {0} could not be matched", { "0": d.failed })}
          {d.skipped > 0 && tx(", {0} skipped", { "0": d.skipped })}.
        </p>
        <p className="hint">{tx("The demo files take {0}. Their routes are read and stay either way; a deleted demo can be downloaded again.", { "0": mb(d.bytesOnDisk) })}</p>
        <div className="modal-actions">
          <button className="primary" onClick={() => void keep(false)}>{tx("Delete the files ({0})", { "0": mb(d.bytesOnDisk) })}</button>
          <button onClick={() => void keep(true)}>{t("Keep them")}</button>
          <button className="linkish" onClick={onManage}>{t("See which failed")}</button>
        </div>
      </div>
    );
  }

  const chosen = found?.filter((c) => picked.has(c.demosTfId)) ?? [];
  return (
    <div className="mi-body">
      {answered && <p className="demo-drop-ok">{answered}</p>}
      {r.error && <p className="error">{r.error}</p>}
      <div className="mi-form">
        <label className="an-field">
          <span className="an-label">{t("Map")}</span>
          <select
            value={map}
            onChange={(e) => {
              setMap(e.target.value);
              setFound(null);
            }}
          >
            {maps.map((m) => (
              <option key={m} value={m}>
                {capitalize(m)}
              </option>
            ))}
          </select>
        </label>
        <label className="an-field">
          <span className="an-label">{t("Class")}</span>
          <select value={klass ?? ""} onChange={(e) => setKlass(e.target.value || null)}>
            <option value="">{t("Any class")}</option>
            {CLASSES.map((c) => (
              <option key={c} value={c}>
                {capitalize(c)}
              </option>
            ))}
          </select>
        </label>
        <div className="mi-quick" role="group" aria-label={t("How many")}>
          {AMOUNTS.map((n) => (
            <button key={n} className="km-chip" disabled={searching || !map} onClick={() => void search(n, true)} title={t("Find their newest {0} and pick them all", { "0": n })}>
              {tx("Newest {0}", { "0": n })}
            </button>
          ))}
          <button className="km-chip" disabled={searching || !map} onClick={() => void search(want, false)}>
            {t("Search and pick…")}
          </button>
          <select value={want} onChange={(e) => setWant(Number(e.target.value))} aria-label={t("Up to")}>
            {[25, 50, 100].map((n) => (
              <option key={n} value={n}>
                {tx("up to {0}", { "0": n })}
              </option>
            ))}
          </select>
        </div>
      </div>
      {klass && <p className="hint">{t("With a class, demos.tf is asked about each demo in turn (one a second), so a search takes up to a couple of minutes.")}</p>}
      {searching && (
        <p className="hint">
          {r.search?.step === "listing"
            ? tx("Reading demos.tf's list: page {0}, {1} on this map so far…", { "0": r.search.page, "1": r.search.found })
            : r.search?.step === "classes"
              ? tx("Checking which class they played: {0} demos checked…", { "0": r.search.checked })
              : t("Asking demos.tf…")}
        </p>
      )}
      {error && <p className="error">{error}</p>}
      {found && found.length === 0 && <p className="hint">{t("demos.tf has none for them on this map with these settings.")}</p>}
      {found && found.length > 0 && (
        <>
          <div className="mi-pick">
            <span className="hint">{tx("{0} found, {1} picked", { "0": found.length, "1": chosen.length })}</span>
            {AMOUNTS.map((n) => (
              <button key={n} className="linkish" onClick={() => setPicked(new Set(found.filter((c) => !c.here).slice(0, n).map((c) => c.demosTfId)))}>
                {tx("newest {0}", { "0": n })}
              </button>
            ))}
            <button className="linkish" onClick={() => setPicked(new Set())}>{t("none")}</button>
          </div>
          <div className="mi-list">
            <table className="match-table">
              <tbody>
                {found.map((c) => (
                  <tr key={c.demosTfId} className={c.here ? "muted" : ""}>
                    <td>
                      <input
                        type="checkbox"
                        checked={picked.has(c.demosTfId)}
                        disabled={c.here}
                        aria-label={t("Pick demo {0}", { "0": c.demosTfId })}
                        onChange={(e) =>
                          setPicked((was) => {
                            const next = new Set(was);
                            if (e.target.checked) next.add(c.demosTfId);
                            else next.delete(c.demosTfId);
                            return next;
                          })
                        }
                      />
                    </td>
                    <td className="nowrap">{formatDate(c.time)}</td>
                    <td className="nowrap">
                      {c.class && <ClassIcon cls={c.class} size={16} />}{" "}
                      {c.team && <span className={c.team === "Red" ? "team-red" : "team-blue"}>{c.team === "Red" ? "RED" : "BLU"}</span>}
                    </td>
                    <td className="muted">{c.map}</td>
                    <td className="hint">{c.here ? t("already here") : c.localLog ? t("match stored") : t("log from logs.tf")}</td>
                    <td>
                      <a href={`https://demos.tf/${c.demosTfId}`} target="_blank" rel="noreferrer">
                        #{c.demosTfId}
                      </a>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="modal-actions">
            <button className="primary" disabled={chosen.length === 0} onClick={() => void start()}>
              {tx("Import {0} demo{1}", { "0": chosen.length, "1": chosen.length === 1 ? "" : "s" })}
            </button>
            <span className="hint">{tx("About {0} to download, one demo at a time. Each is checked against its match first.", { "0": mb(chosen.length * 60e6) })}</span>
          </div>
        </>
      )}
    </div>
  );
}

/** Every run, with its files: keep, delete, or forget the record. */
function Imported({ batches }: { batches: MassBatch[] }) {
  const qc = useQueryClient();
  const [note, setNote] = useState<string | null>(null);
  const refresh = (batchId: number) => {
    void qc.invalidateQueries({ queryKey: ["mass_batches"] });
    void qc.invalidateQueries({ queryKey: ["downloaded_demos"] });
    // Kept or deleted here answers the question the run ended with.
    if (run.done?.batchId === batchId) set({ done: null });
  };
  if (batches.length === 0) return <p className="hint mi-body">{t("Nothing imported yet.")}</p>;
  return (
    <div className="mi-body mi-list">
      {note && <p className="demo-drop-ok">{note}</p>}
      {batches.map((b) => {
        const onDisk = b.demos.reduce((n, d) => n + (d.sizeBytes ?? 0), 0);
        const done = b.demos.filter((d) => d.status === "done").length;
        const failed = b.demos.filter((d) => d.status === "failed");
        return (
          <section key={b.batchId} className="mi-batch">
            <div className="mi-batch-head">
              <strong>
                {capitalize(b.map)}
                {b.class && <> · {capitalize(b.class)}</>}
              </strong>
              <span className="hint">
                {formatDate(b.createdAt)} · {tx("{0} of {1} imported", { "0": done, "1": b.demos.length })}
                {b.finishedAt === null && <> · {t("running")}</>}
                {" · "}
                {onDisk > 0 ? tx("{0} on disk", { "0": mb(onDisk) }) : t("files deleted")}
                {b.kept === true && <> · {t("kept")}</>}
              </span>
            </div>
            <div className="mi-batch-actions">
              {onDisk > 0 && (
                <button
                  className="linkish"
                  onClick={async () => {
                    const freed = await api.massKeep(b.batchId, false);
                    setNote(t("Deleted, {0} freed. The routes stay.", { "0": mb(freed) }));
                    refresh(b.batchId);
                  }}
                >
                  {tx("Delete the files ({0})", { "0": mb(onDisk) })}
                </button>
              )}
              {onDisk > 0 && b.kept !== true && (
                <button
                  className="linkish"
                  onClick={async () => {
                    await api.massKeep(b.batchId, true);
                    refresh(b.batchId);
                  }}
                >
                  {t("Keep them")}
                </button>
              )}
              <button
                className="linkish"
                title={t("Remove this run from the list; its routes and any files stay")}
                onClick={async () => {
                  await api.massForget(b.batchId);
                  refresh(b.batchId);
                }}
              >
                {t("Forget")}
              </button>
            </div>
            {failed.length > 0 && (
              <ul className="mi-failed">
                {failed.map((d) => (
                  <li key={d.demosTfId} className="hint">
                    <a href={`https://demos.tf/${d.demosTfId}`} target="_blank" rel="noreferrer">
                      #{d.demosTfId}
                    </a>
                    : {d.error}
                  </li>
                ))}
              </ul>
            )}
          </section>
        );
      })}
    </div>
  );
}
