import { useActivity } from "../lib/activity";
import { openSettingsSection } from "../lib/goto";
import { t } from "../lib/i18n";

/** Lines the waterfall shows. */
const ROWS = 4;

/**
 * What a sync is reading right now, as it reads it (Flashy: "a waterfall
 * 3/4 lines high that fades out"): the newest lines of the activity feed,
 * oldest at the top and fading. Debug lines and the database's slow-query
 * notes stay in Settings › Activity log.
 */
export function ActivityWaterfall() {
  const { lines } = useActivity();
  const shown = lines.filter((l) => l.level !== "debug" && l.source !== "database").slice(-ROWS);
  if (shown.length === 0) return null;
  return (
    <button
      className="act-fall"
      onClick={() => openSettingsSection("activity")}
      title={t("Open the activity log")}
      aria-label={t("Open the activity log")}
    >
      {shown.map((l, i) => (
        <span key={l.seq} className={`act-fall-line lvl-${l.level}`} style={{ opacity: 0.35 + (0.65 * (i + 1)) / shown.length }}>
          <span className="act-fall-src">{l.source}</span> {l.text}
        </span>
      ))}
    </button>
  );
}
