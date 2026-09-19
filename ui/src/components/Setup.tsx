import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, inTauri } from "../api/client";
import { errorMessage, type AppStatus, type TfPathInfo } from "../api/types";

/**
 * First-run setup: who you are (required), where TF2 lives (optional, only
 * for demos), then the first sync. Everything can be changed later in
 * Settings, which reopens this screen.
 */
export function Setup(props: { status: AppStatus; onDone: () => void; onFinish: () => void }) {
  const { status, onDone, onFinish } = props;
  const steamidDone = status.config.steamid !== null;
  const tfDone = status.config.tfPath !== null;
  const [tfSkipped, setTfSkipped] = useState(false);

  return (
    <div className="centered">
      <div className="card setup-card">
        <div className="card-head setup-head">
          <img src="/logo.svg" alt="" className="setup-logo" />
          <div>
            <h1>Welcome to HL Rating</h1>
            <p className="sub">
              Ratings for your TF2 Highlander games, built from your logs.tf history. Everything is stored on this PC.
            </p>
          </div>
        </div>

        <SteamIdStep done={steamidDone} current={status.config.steamid} onSaved={onDone} />
        <TfPathStep
          done={tfDone}
          skipped={tfSkipped && !tfDone}
          current={status.config.tfPath}
          onSaved={onDone}
          onSkip={() => setTfSkipped(true)}
        />
        <FirstSyncStep ready={steamidDone && (tfDone || tfSkipped)} onFinish={onFinish} />
      </div>
    </div>
  );
}

/** Start the first sync and hand over to the app, where the sync strip shows progress. */
function FirstSyncStep({ ready, onFinish }: { ready: boolean; onFinish: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function start() {
    setBusy(true);
    setError(null);
    try {
      // Already running (a second click, or a return visit from Settings) is fine.
      if (!(await api.syncBusy())) await api.syncStart(false);
      onFinish();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  }

  return (
    <section className={ready ? "step" : "step waiting"}>
      <div className="step-num">3</div>
      <div className="step-body">
        <h2>Get your matches</h2>
        <p className="hint">
          The first sync finds every match you played on logs.tf and trends.tf, then fetches them one every 2 seconds
          to go easy on logs.tf. Detailed kill data comes 100 matches per sync, so a long history takes a few syncs:
          press <strong>Sync</strong> again later to fetch the rest. You can use the app while it runs.
        </p>
        <div className="row">
          <button className="primary" disabled={!ready || busy} onClick={() => void start()}>
            Start first sync
          </button>
          <button className="linkish" disabled={!ready || busy} onClick={onFinish}>
            Skip, sync later
          </button>
        </div>
        {error && <p className="error">{error}</p>}
      </div>
    </section>
  );
}

function SteamIdStep({
  done,
  current,
  onSaved,
}: {
  done: boolean;
  current: string | null;
  onSaved: () => void;
}) {
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api.setSteamId(value.trim());
      setValue("");
      onSaved();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={done ? "step done" : "step"}>
      <div className="step-num">1</div>
      <div className="step-body">
        <h2>Your SteamID</h2>
        {done && (
          <div className="status-line">
            <span className="dot ok" />
            <code>{current}</code>
          </div>
        )}
        <p className="hint">
          Any format works — SteamID64, <code>[U:1:12345]</code>, <code>STEAM_0:1:6172</code>, or a
          link to your profile.
        </p>
        <div className="row">
          <input
            type="text"
            placeholder={done ? "Change SteamID…" : "76561198000000000"}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && value.trim() && !busy) void save();
            }}
          />
          <button
            className="primary"
            disabled={busy || value.trim().length === 0}
            onClick={() => void save()}
          >
            {done ? "Update" : "Save"}
          </button>
        </div>
        {error && <p className="error">{error}</p>}
      </div>
    </section>
  );
}

function TfPathStep({
  done,
  skipped,
  current,
  onSaved,
  onSkip,
}: {
  done: boolean;
  skipped: boolean;
  current: string | null;
  onSaved: () => void;
  onSkip: () => void;
}) {
  const [info, setInfo] = useState<TfPathInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  /** Inspect without committing, so the user sees what we found first. */
  async function preview(path: string) {
    setBusy(true);
    setError(null);
    try {
      setInfo(await api.inspectTfPath(path));
    } catch (e) {
      setInfo(null);
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function browse() {
    // The native picker only exists inside the app window; in browser dev mode
    // fall back to typing a path.
    const picked = inTauri
      ? await open({ directory: true, title: "Select your TF2 `tf` folder" })
      : window.prompt("Path to your tf folder");
    if (typeof picked === "string" && picked.length > 0) await preview(picked);
  }

  async function autoDetect() {
    setBusy(true);
    setError(null);
    try {
      const found = await api.detectTfPath();
      setInfo(found);
      if (!found) setError("No TF2 install found in the usual Steam locations — browse for it.");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function confirm() {
    if (!info) return;
    setBusy(true);
    setError(null);
    try {
      await api.setTfPath(info.path);
      setInfo(null);
      onSaved();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={done || skipped ? "step done" : "step"}>
      <div className="step-num">2</div>
      <div className="step-body">
        <h2>
          Your TF2 folder <span className="optional">optional</span>
        </h2>
        {skipped && !info && (
          <div className="status-line">
            <span className="dot warn" />
            <span className="muted">Skipped: set it later in Settings to jump into demos.</span>
          </div>
        )}
        {done && !info && (
          <div className="status-line">
            <span className="dot ok" />
            <code>{current}</code>
          </div>
        )}
        <p className="hint">
          Only needed for demos: with it, clicking a kill jumps to that moment in your recording. Point at the{" "}
          <code>tf</code> folder inside your Team Fortress 2 install, the one containing <code>cfg</code>.
        </p>
        <div className="row">
          <button onClick={() => void autoDetect()} disabled={busy}>
            Auto-detect
          </button>
          <button onClick={() => void browse()} disabled={busy}>
            Browse…
          </button>
          {!done && !skipped && (
            <button className="linkish" onClick={onSkip} disabled={busy}>
              Skip for now
            </button>
          )}
        </div>

        {info && (
          <>
            <div className="status-line">
              <span className={info.valid ? "dot ok" : "dot bad"} />
              <code>{info.path}</code>
            </div>
            <ul className="notes">
              {info.notes.map((n) => (
                <li key={n}>{n}</li>
              ))}
            </ul>
            <div className="row">
              <button className="primary" onClick={() => void confirm()} disabled={busy || !info.valid}>
                Use this folder
              </button>
              <button className="linkish" onClick={() => setInfo(null)} disabled={busy}>
                Cancel
              </button>
            </div>
          </>
        )}
        {error && <p className="error">{error}</p>}
      </div>
    </section>
  );
}
