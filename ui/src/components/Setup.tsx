import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, inTauri } from "../api/client";
import { errorMessage, type AppStatus, type TfPathInfo } from "../api/types";

/**
 * First-run setup. Two things must be true before anything else in the app
 * works: we know whose performance this is, and we know where the demos live.
 */
export function Setup({ status, onDone }: { status: AppStatus; onDone: () => void }) {
  const steamidDone = status.config.steamid !== null;
  const tfDone = status.config.tfPath !== null;

  return (
    <div className="centered">
      <div className="card">
        <div className="card-head">
          <h1>Set up</h1>
          <p className="sub">
            Two answers and the app is ready. Both can be changed later.
          </p>
        </div>

        <SteamIdStep done={steamidDone} current={status.config.steamid} onSaved={onDone} />
        <TfPathStep done={tfDone} current={status.config.tfPath} onSaved={onDone} />
      </div>
    </div>
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
  current,
  onSaved,
}: {
  done: boolean;
  current: string | null;
  onSaved: () => void;
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
    <section className={done ? "step done" : "step"}>
      <div className="step-num">2</div>
      <div className="step-body">
        <h2>Your TF2 folder</h2>
        {done && !info && (
          <div className="status-line">
            <span className="dot ok" />
            <code>{current}</code>
          </div>
        )}
        <p className="hint">
          Point at the <code>tf</code> directory inside your Team Fortress 2 install — the one
          containing <code>cfg</code> and your <code>.dem</code> files.
        </p>
        <div className="row">
          <button onClick={() => void browse()} disabled={busy}>
            Browse…
          </button>
          <button onClick={() => void autoDetect()} disabled={busy}>
            Auto-detect
          </button>
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
