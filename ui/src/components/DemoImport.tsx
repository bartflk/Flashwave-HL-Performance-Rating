import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { api, inTauri } from "../api/client";
import { errorMessage, type DemoImported } from "../api/types";
import { t, tx } from "../lib/i18n";
import { Spinner } from "./Spinner";

/**
 * Q18 (beowulf): a match from a demo alone, for a server that had no
 * logs.tf config and so wrote no log. The demo becomes the log: every kill,
 * hit, uber and capture is in it, and the match is rated like any other.
 */
export function DemoImport() {
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [got, setGot] = useState<DemoImported | null>(null);

  async function pick() {
    setError(null);
    setGot(null);
    let path: string | null = null;
    if (inTauri) {
      const chosen = await open({ multiple: false, directory: false, filters: [{ name: t("TF2 demo"), extensions: ["dem"] }] });
      path = typeof chosen === "string" ? chosen : null;
    } else {
      // The browser build has no file dialog; a made-up path shows the flow.
      path = "D:\\demos\\match-20260925-2211-koth_proot_b5b.dem";
    }
    if (!path) return;
    setBusy(true);
    try {
      const imported = await api.importDemo(path);
      setGot(imported);
      await Promise.all([
        qc.invalidateQueries({ queryKey: ["matches"] }),
        qc.invalidateQueries({ queryKey: ["index_stats"] }),
        qc.invalidateQueries({ queryKey: ["demo_stats"] }),
      ]);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <h3 style={{ marginTop: 22 }}>{t("A match with no log")}</h3>
      <p className="hint" style={{ marginTop: 6 }}>
        {t("For a server that wrote no logs.tf log. Pick the match's demo, an STV ideally, and it becomes the match: kills, damage, ubers, captures and rounds are all read from it, and it is rated like any other.")}
      </p>
      <div className="row" style={{ marginTop: 10, gap: 8 }}>
        <button disabled={busy} onClick={() => void pick()}>
          {busy ? (
            <>
              <Spinner /> {t("Reading the demo…")}
            </>
          ) : (
            t("Pick a demo…")
          )}
        </button>
      </div>
      {got && (
        <p className="hint" style={{ marginTop: 10 }}>
          {tx("Added {0}: {1} rounds, {2} kills, {3} players.{4}", {
            "0": <strong>{got.map ?? got.title}</strong>,
            "1": got.rounds,
            "2": got.kills,
            "3": got.players,
            "4": got.yours ? t(" It is in your matches now.") : t(" You are not in this one, so it joins the pool everyone is rated against rather than your match list."),
          })}
        </p>
      )}
      {error && <p className="error" style={{ marginTop: 10 }}>{error}</p>}
    </>
  );
}
