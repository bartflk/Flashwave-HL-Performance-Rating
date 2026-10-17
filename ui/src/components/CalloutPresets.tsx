import { useEffect, useState } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, inTauri } from "../api/client";
import { errorMessage, type PresetCheck } from "../api/types";
import { capitalize } from "../lib/format";
import { t, tx } from "../lib/i18n";
import { toast } from "../lib/toast";
import { Loading } from "./Spinner";

/**
 * Callout presets (Q32, Flashy): one map's callouts as a file to pass round
 * on Discord, and the way back in. Import checks the file first and says
 * what it will replace; the replaced copy is kept for one Undo.
 */

/** After an import or Undo, everything drawn from callouts. */
function calloutsChanged(qc: QueryClient) {
  for (const key of [["callouts"], ["mapsOverview"], ["positions"]]) void qc.invalidateQueries({ queryKey: key });
}

/** Export… and Import… for one map, with the confirm step and Undo. */
export function CalloutPresetButtons({ map, canExport, canUndo }: { map: string; canExport: boolean; canUndo: boolean }) {
  const qc = useQueryClient();
  const [picked, setPicked] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const exportIt = async () => {
    setNote(null);
    try {
      const path = await api.exportCallouts(map);
      if (path) setNote(t("Saved {0}.", { "0": path }));
    } catch (e) {
      setNote(errorMessage(e));
    }
  };
  const importIt = async () => {
    setNote(null);
    try {
      const path = await api.pickCalloutFile();
      if (path) setPicked(path);
    } catch (e) {
      setNote(errorMessage(e));
    }
  };
  const undo = async () => {
    setNote(null);
    try {
      await api.undoCallouts(map);
      calloutsChanged(qc);
      setNote(t("Import taken back."));
    } catch (e) {
      setNote(errorMessage(e));
    }
  };

  return (
    <span className="preset-buttons">
      {canExport && <button className="mini" onClick={() => void exportIt()}>{t("Export…")}</button>}
      <button className="mini" onClick={() => void importIt()}>{t("Import…")}</button>
      {canUndo && <button className="mini" onClick={() => void undo()}>{t("Undo import")}</button>}
      {note && <span className="hint preset-note">{note}</span>}
      {picked && (
        <CalloutImportDialog
          path={picked}
          map={map}
          onClose={() => setPicked(null)}
          onDone={(msg) => {
            setPicked(null);
            setNote(msg);
          }}
        />
      )}
    </span>
  );
}

/**
 * What an import would do, and the button that does it. `map` is where it
 * goes; null means the map the file names (a file dropped on the window).
 */
export function CalloutImportDialog({ path, map, onClose, onDone }: { path: string; map: string | null; onClose: () => void; onDone: (message: string) => void }) {
  const qc = useQueryClient();
  const [check, setCheck] = useState<PresetCheck | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.inspectCallouts(map, path).then(setCheck, (e) => setError(errorMessage(e)));
  }, [map, path]);

  const other = check !== null && check.map !== check.target;
  const go = async () => {
    if (!check) return;
    setBusy(true);
    setError(null);
    try {
      const got = await api.importCallouts(check.target, path, other);
      calloutsChanged(qc);
      onDone(t("{map} now has these {n} zones. Undo import puts back what was there.", { map: capitalize(check.target), n: got.zones.length }));
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  const file = path.split(/[\\/]/).pop() ?? path;
  return (
    <div className="modal-scrim" role="dialog" aria-modal="true" aria-label={t("Import callouts")}>
      <div className="modal">
        <h3>{t("Import callouts")}</h3>
        <p className="hint">{file}</p>
        {!check && !error && <Loading>{t("Reading the file…")}</Loading>}
        {check && (
          <>
            <p>
              {tx("{zones} zones for {map}{by}{draft}.", {
                zones: <strong>{check.zones}</strong>,
                map: capitalize(check.map),
                by: check.author ? t(" by {0}", { "0": check.author }) : "",
                draft: check.draft ? t(", marked as a draft") : "",
              })}
              {check.names > 0 && " " + t("{n} more named but not drawn.", { n: check.names })}
            </p>
            <p className="hint">
              {check.currentOrigin === "yours"
                ? t("It replaces your own {n} zones on {map}.", { n: check.currentZones, map: capitalize(check.target) })
                : check.currentOrigin === "built in"
                  ? t("It replaces the built-in {n} zones on {map}.", { n: check.currentZones, map: capitalize(check.target) })
                  : t("{map} has no callouts yet.", { map: capitalize(check.target) })}{" "}
              {t("Undo import puts them back.")}
            </p>
            {other && (
              <p className="warn-text">
                {t("This file is for {0}, not {1}: its zones will sit in the wrong places unless the two share a layout.", { "0": capitalize(check.map), "1": capitalize(check.target) })}
              </p>
            )}
          </>
        )}
        {error && <p className="error">{error}</p>}
        <div className="modal-actions">
          <button className="primary" onClick={() => void go()} disabled={busy || !check}>
            {other ? t("Import onto {0} anyway", { "0": capitalize(check!.target) }) : t("Import")}
          </button>
          <button onClick={onClose} disabled={busy}>{t("Cancel")}</button>
        </div>
      </div>
    </div>
  );
}

/**
 * A `.callouts.json` dropped anywhere on the window imports to the map the
 * file names, after the same confirm step. Mounted once, in App.
 */
export function CalloutDrop() {
  const [path, setPath] = useState<string | null>(null);

  useEffect(() => {
    if (!inTauri) return;
    let off: (() => void) | undefined;
    let dead = false;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((e) => {
          if (e.payload.type !== "drop") return;
          const hit = e.payload.paths.find((p) => p.toLowerCase().endsWith(".callouts.json"));
          if (hit) setPath(hit);
        })
        .then((u) => {
          if (dead) u();
          else off = u;
        }),
    );
    return () => {
      dead = true;
      off?.();
    };
  }, []);

  return path ? (
    <CalloutImportDialog
      path={path}
      map={null}
      onClose={() => setPath(null)}
      onDone={(m) => {
        setPath(null);
        toast(t("Callouts imported"), m);
      }}
    />
  ) : null;
}
