import { t } from "../lib/i18n";
import { redGreen, usePalette } from "../lib/tiers";
import { classLabel } from "./analysis/common";

/**
 * A number on its tier's colour: osu!'s pill, or red-to-green text over a
 * faint bar of where it stands (Leetify's look), as Settings picks; plain
 * where there is no scale or colours are off.
 */
export function TierPill({ text, tier, cls }: { text: string; tier: { pct: number; bg: string; fg: string; map?: string | null } | null; cls?: string | null }) {
  const palette = usePalette();
  if (!tier || palette === "off") return <>{text}</>;
  const c = classLabel(cls ?? "");
  const where = tier.map
    ? tier.pct >= 100
      ? t("Top 1% of {0} games on {1}", { "0": c, "1": tier.map })
      : t("Better than {0}% of {1} games on {2}", { "0": Math.floor(tier.pct), "1": c, "2": tier.map })
    : tier.pct >= 100
      ? t("Top 1% of {0} games", { "0": c })
      : t("Better than {0}% of {1} games", { "0": Math.floor(tier.pct), "1": c });
  if (palette === "redgreen") {
    const c = redGreen(tier.pct);
    return (
      <span className="tier-rg" title={where} style={{ color: c }}>
        <span className="tier-rg-bar" style={{ width: `${Math.max(4, Math.min(100, tier.pct))}%`, background: c }} />
        <span className="tier-rg-text">{text}</span>
        {tier.pct >= 100 && <span className="tier-rg-star">★</span>}
      </span>
    );
  }
  return (
    <span className="tier-pill" style={{ background: tier.bg, color: tier.fg }} title={where}>
      {text}
    </span>
  );
}
