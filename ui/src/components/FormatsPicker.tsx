import { useState } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api } from "../api/client";
import { toStored, useFormatsPlayed, type GameFormat } from "../lib/gameformat";
import { friendlyText } from "../lib/apierror";
import { t } from "../lib/i18n";

// Each string is its own t() call, so the translation script finds it.
const FORMAT_INFO: Array<[GameFormat, () => string, () => string]> = [
  ["hl", () => t("Highlander"), () => t("Nine a side, one of each class.")],
  ["6s", () => t("6v6"), () => t("Six a side: two Scouts, two Soldiers, a Demoman and a Medic.")],
];

/** Store the formats played and show the answer everywhere at once. */
export async function saveFormats(qc: QueryClient, formats: GameFormat[]) {
  qc.setQueryData(["formats_played"], await api.setFormatsPlayed(formats.map(toStored)));
}

/**
 * Which formats you play, one tick each (the 6v6 plan, P0): in setup, and in
 * Settings › Formats you play. At least one stays chosen.
 */
export function FormatsPicker() {
  const played = useFormatsPlayed();
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function toggle(f: GameFormat) {
    const has = played.formats.includes(f);
    if (has && played.formats.length === 1) return;
    setBusy(true);
    setError(null);
    try {
      await saveFormats(qc, has ? played.formats.filter((x) => x !== f) : [...played.formats, f]);
    } catch (e) {
      setError(friendlyText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className="fmt-pick" role="group" aria-label={t("The formats you play")}>
        {FORMAT_INFO.map(([f, name, about]) => {
          const on = played.formats.includes(f);
          return (
            <button key={f} className={on ? "fmt-pick-item on" : "fmt-pick-item"} aria-pressed={on} disabled={busy || (on && played.formats.length === 1)} onClick={() => void toggle(f)}>
              <span className="fmt-pick-box" aria-hidden="true">{on ? "✓" : ""}</span>
              <span className="fmt-pick-text">
                <strong>{name()}</strong>
                <span className="hint">{about()}</span>
              </span>
            </button>
          );
        })}
      </div>
      {error && <p className="error">{error}</p>}
    </>
  );
}
