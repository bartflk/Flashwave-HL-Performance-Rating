import { t } from "../lib/i18n";
import { classLabel } from "./analysis/common";

/** A number on its tier's colour, osu!-style; plain where there is no scale. */
export function TierPill({ text, tier, cls }: { text: string; tier: { pct: number; bg: string; fg: string } | null; cls?: string | null }) {
  if (!tier) return <>{text}</>;
  const where =
    tier.pct >= 100
      ? t("Top 1% of {0} games", { "0": classLabel(cls ?? "") })
      : t("Better than {0}% of {1} games", { "0": Math.floor(tier.pct), "1": classLabel(cls ?? "") });
  return (
    <span className="tier-pill" style={{ background: tier.bg, color: tier.fg }} title={where}>
      {text}
    </span>
  );
}
