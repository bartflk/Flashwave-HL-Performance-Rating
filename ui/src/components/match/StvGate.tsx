import type { MatchDetail } from "../../api/types";
import { beforeDemosTf, noStvReason, useStv } from "../../lib/stv";
import { LookAgain } from "./DemoPanel";
import { t, tx } from "../../lib/i18n";

/**
 * What the STV demo would add, said where it would add it (Flashy).
 *
 * A panel that needs the STV and has none used to vanish, so nobody knew it
 * existed. It now stays, blurred over a sketch of what it shows, with the
 * reason and the download in front of it.
 */

type Kind = "spychecks" | "cart" | "positions";

const COPY: Record<Kind, { title: string; why: string }> = {
  spychecks: {
    title: "Spychecks",
    why: "Whether a Spy was cloaked is only in a demo: every hit on an invisible Spy, who found them, and when.",
  },
  cart: {
    title: "The cart",
    why: "Where the cart was is only in a demo: every second BLU was up players and the cart stood still, and how far it moved after each won fight.",
  },
  positions: {
    title: "Positions",
    why: "Where all eighteen players stood, second by second, is only in the STV demo: who anchors, who rotates, which callouts each player lives in.",
  },
};

export function StvLocked({ d, kind }: { d: MatchDetail; kind: Kind }) {
  const stv = useStv(d);
  const copy = COPY[kind];
  return (
    <section className="panel stv-locked">
      <h2>{t(copy.title)}</h2>
      <div className="stv-locked-body">
        <div className="stv-locked-preview" aria-hidden>
          <Sketch kind={kind} />
        </div>
        <div className="stv-locked-card">
          <div className="stv-locked-text">
            <strong className="stv-locked-head">{t("STV demo required")}</strong>
            <span className="hint">{t(copy.why)}</span>
          </div>
          {/* One download, in Demo linking: every locked panel points there
              rather than each starting its own. */}
          {stv.download?.state === "running" ? (
            <span className="hint stv-locked-busy">{t("Downloading…")}</span>
          ) : stv.canFetch ? (
            <button className="km-chip on" onClick={goToDemoLinking}>{t("Get the STV demo ↑")}</button>
          ) : (
            <span className="hint">{noStvReason(d)}</span>
          )}
        </div>
      </div>
    </section>
  );
}

/** Scroll to Demo linking's download, opening its fold if it is shut, and flash it. */
function goToDemoLinking() {
  const box = document.getElementById("demo-linking-download");
  if (!box) return;
  const fold = box.closest(".fold");
  if (fold?.classList.contains("closed")) (fold.querySelector("h2") as HTMLElement | null)?.click();
  // After the fold has had a moment to open.
  setTimeout(() => {
    box.scrollIntoView({ behavior: document.hidden ? "auto" : "smooth", block: "center" });
    box.classList.remove("flash");
    // Restarted each time, so a second click flashes again.
    void box.offsetWidth;
    box.classList.add("flash");
  }, 30);
}

/** One line near the top of a match without an STV. */
export function StvBanner({ d }: { d: MatchDetail }) {
  const stv = useStv(d);
  if (stv.has) return null;
  return (
    <div className="stv-banner">
      <span className="stv-banner-icon" aria-hidden>▶</span>
      <p>
        <strong>{t("For better statistics, download the STV demo.")}</strong>{" "}
        <span className="hint">{t("It adds spychecks, positions, the cart on payload, and everyone's aim and movement, not just yours.")}</span>
      </p>
      <Fetch d={d} compact />
    </div>
  );
}

function Fetch({ d, compact }: { d: MatchDetail; compact?: boolean }) {
  const stv = useStv(d);
  const dl = stv.download;
  if (dl && dl.state === "running") {
    const pct = dl.total ? Math.round((dl.bytes / dl.total) * 100) : null;
    return (
      <div className="stv-fetching">
        <div className="progress-track">
          <div className={pct === null ? "progress-fill indeterminate" : "progress-fill"} style={pct === null ? undefined : { width: `${pct}%` }} />
        </div>
        <span className="hint">{pct === null ? t("Downloading…") : tx("Downloading… {0}%", { "0": pct })}</span>
      </div>
    );
  }
  if (!stv.canFetch) {
    // No demo found: say so, and ask demos.tf again right there, unless the
    // match is older than anything demos.tf still keeps.
    if (compact && !beforeDemosTf(d.playedAt)) {
      return (
        <span className="stv-banner-none">
          <span className="hint">{noStvReason(d)}</span>
          <LookAgain d={d} />
        </span>
      );
    }
    return <span className="hint">{noStvReason(d)}</span>;
  }
  return (
    <button className={compact ? "primary stv-fetch-btn" : "primary"} onClick={() => void stv.fetch()}>
      {t("Download the STV demo")}
    </button>
  );
}

/** A stand-in for what the panel shows, drawn only to be blurred. */
function Sketch({ kind }: { kind: Kind }) {
  const rows = 2;
  return (
    <div className="stv-sketch">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="stv-sketch-row">
          <span className="stv-sketch-name" style={{ width: `${40 + ((i * 17) % 35)}%` }} />
          <div className="stv-sketch-bar">
            {(kind === "positions" ? [38, 24, 18, 12] : [70 - i * 12]).map((w, j) => (
              <span key={j} className={`stv-sketch-seg s${(i + j) % 3}`} style={{ flexGrow: w }} />
            ))}
            {kind !== "positions" && <span className="stv-sketch-seg rest" style={{ flexGrow: 30 + i * 12 }} />}
          </div>
        </div>
      ))}
    </div>
  );
}
