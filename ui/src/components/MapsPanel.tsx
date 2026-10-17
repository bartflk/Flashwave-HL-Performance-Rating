import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { errorMessage, type MapRow } from "../api/types";
import { capitalize, splitMap } from "../lib/format";
import { t, tx } from "../lib/i18n";
import { MapAligner } from "./MapAligner";
import { CalloutPresetButtons } from "./CalloutPresets";
import { Loading } from "./Spinner";

/**
 * Settings, Maps (Q31, Flashy): every map the app knows, where its top-down
 * image and callouts come from, and the way to put in your own image and
 * line it up. Built-in images are more.tf's renders; yours always win, and
 * Remove goes back to the built-in one.
 */
export function MapsPanel() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["mapsOverview"], queryFn: api.mapsOverview });
  const [aligning, setAligning] = useState<MapRow | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [all, setAll] = useState(false);

  // An image or placement feeds every kill map on that map.
  const changed = () => {
    void qc.invalidateQueries({ queryKey: ["mapsOverview"] });
    void qc.invalidateQueries({ queryKey: ["overview"] });
    void qc.invalidateQueries({ queryKey: ["overviewImage"] });
  };

  const run = (base: string, what: () => Promise<string | null>) => async () => {
    setBusy(base);
    setNote(null);
    try {
      setNote(await what());
      changed();
    } catch (e) {
      setNote(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  if (q.isPending) return <div className="panel"><h2>{t("Maps")}</h2><Loading>{t("Loading…")}</Loading></div>;
  if (q.isError) return <div className="panel"><h2>{t("Maps")}</h2><p className="error">{errorMessage(q.error)}</p></div>;

  // Maps you played, and any map with something of yours; the rest on ask.
  const shown = all ? q.data.maps : q.data.maps.filter((m) => m.matches > 0 || m.image === "yours" || m.callouts === "yours");
  const hidden = q.data.maps.length - shown.length;
  const name = (m: MapRow) => capitalize(splitMap(m.name).name ?? m.base);

  return (
    <div className="panel">
      <h2>{t("Maps")}</h2>
      <p className="hint" style={{ marginTop: 6 }}>
        {t("The top-down image under each kill map, and its callouts. The app's default images are more.tf's; import your own for any map, and line it up if it does not sit right. Callouts export as a file to share, and a .callouts.json dropped on the window imports.")}
      </p>
      {q.data.unknownMatches > 0 && (
        <p className="hint" style={{ marginTop: 6 }}>
          {tx("{n} of your matches have rounds on a map nobody could tell. The kill map of each one lets you say which.", { n: <strong>{q.data.unknownMatches}</strong> })}
        </p>
      )}
      <div className="table-wrap" style={{ marginTop: 12 }}>
        <table className="match-table maps-table">
          <thead>
            <tr>
              <th>{t("Map")}</th>
              <th className="num">{t("Matches")}</th>
              <th>{t("Image")}</th>
              <th>{t("Callouts")}</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((m) => (
              <tr key={m.base}>
                <td className="nowrap" title={m.name}>{name(m)}</td>
                <td className="num">{m.matches || <span className="muted">–</span>}</td>
                <td>
                  <div className="map-cell">
                    <OriginTag o={m.image} />
                    {m.image !== "none" && m.placement === "none" && <span className="warn-text">{t("not lined up")}</span>}
                    {m.image !== "none" && m.placement === "yours" && <span className="muted">{t("lined up by you")}</span>}
                    <span className="maps-actions">
                      <button className="mini" disabled={busy !== null} onClick={() => void run(m.base, async () => ((await api.importOverview(m.base)) ? t("Image saved for {map}.", { map: name(m) }) : null))()}>
                        {t("Import…")}
                      </button>
                      {m.image !== "none" && (
                        <button className="mini" disabled={busy !== null} onClick={() => setAligning(m)}>
                          {t("Line up")}
                        </button>
                      )}
                      {(m.image === "yours" || m.placement === "yours") && (
                        <button
                          className="mini"
                          disabled={busy !== null}
                          title={t("Delete your image and line-up for this map, and use the app's")}
                          onClick={() => void run(m.base, async () => (await api.removeOverview(m.base), t("{map} is back to the app's default image.", { map: name(m) })))()}
                        >
                          {t("Use default")}
                        </button>
                      )}
                    </span>
                  </div>
                </td>
                <td>
                  <div className="map-cell">
                    <OriginTag o={m.callouts} />
                    {(m.calloutsAuthor || m.draft || m.zones > 0 || m.unplaced > 0) && (
                      <span className="muted">
                        {[m.calloutsAuthor ? t("by {0}", { "0": m.calloutsAuthor }) : null, m.draft ? t("draft") : null, m.zones > 0 ? t("{n} zones", { n: m.zones }) : null, m.unplaced > 0 ? t("{n} to place", { n: m.unplaced }) : null]
                          .filter(Boolean)
                          .join(" · ")}
                      </span>
                    )}
                    <CalloutPresetButtons map={m.base} canExport={m.callouts !== "none"} canUndo={m.calloutsUndo} />
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {hidden > 0 && (
        <button className="mini" style={{ marginTop: 10 }} onClick={() => setAll(true)}>
          {t("Show {n} more maps you have not played", { n: hidden })}
        </button>
      )}
      {note && <p className="hint" style={{ marginTop: 8 }}>{note}</p>}
      {aligning && (
        <MapAligner
          row={aligning}
          onClose={() => setAligning(null)}
          onSaved={() => {
            setAligning(null);
            setNote(t("{map} is lined up.", { map: name(aligning) }));
            changed();
          }}
        />
      )}
    </div>
  );
}

/** Where an image or a set of callouts comes from, as a small tag. */
function OriginTag({ o }: { o: MapRow["image"] }) {
  const [cls, label] =
    o === "yours" ? ["custom", t("Custom")] : o === "built in" ? ["default", t("App default")] : ["none", t("None")];
  return <span className={`origin-tag ${cls}`}>{label}</span>;
}
