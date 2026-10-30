import { useState } from "react";
import { friendlyText } from "../../lib/apierror";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { capitalize, splitMap } from "../../lib/format";
import { t as tr } from "../../lib/i18n";

/**
 * "This was on ___" (Q30): the last resort for rounds nothing could place,
 * and the way to correct one that was placed wrong. The choice is stored
 * as the player's word, beats everything but the log's own map field, and
 * "Not sure" takes it back.
 *
 * `rounds` are the rounds the kill map is showing: the chosen map's, or the
 * whole match.
 */
export function MapPicker({ logId, rounds, current }: { logId: number; rounds: number[]; current: string | null }) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(current === null);
  const [pick, setPick] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const maps = useQuery({ queryKey: ["knownMaps"], queryFn: api.knownMaps, enabled: open, staleTime: 5 * 60_000 });

  if (!open) {
    return (
      <button className="linkish" onClick={() => setOpen(true)}>
        {tr("Wrong map?")}
      </button>
    );
  }

  const save = async (map: string | null) => {
    setBusy(true);
    setError(null);
    try {
      await api.setRoundMap(logId, rounds, map);
      // The rounds' maps feed the match page, the list and the kill map.
      for (const key of [["analysis", logId], ["match", logId], ["matches"]]) void qc.invalidateQueries({ queryKey: key });
      setPick("");
      if (map !== null) setOpen(false);
    } catch (e) {
      setError(friendlyText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <span className="map-picker">
      <select value={pick} onChange={(e) => setPick(e.target.value)} disabled={busy} aria-label={tr("Which map these rounds were on")}>
        <option value="">{current === null ? tr("This was on…") : tr("It was on…")}</option>
        {(maps.data ?? []).map((m) => (
          <option key={m} value={m}>
            {capitalize(splitMap(m).name ?? m)}
          </option>
        ))}
      </select>
      <button onClick={() => void save(pick)} disabled={busy || pick === ""}>
        {tr("Set")}
      </button>
      {current !== null && (
        <button onClick={() => void save(null)} disabled={busy} title={tr("Take back a map you set; the app decides again")}>
          {tr("Not sure")}
        </button>
      )}
      {error && <span className="error">{error}</span>}
    </span>
  );
}
