import { t } from "../lib/i18n";
import { redGreen, tierColour, usePalette, type Tier } from "../lib/tiers";
import { classLabel } from "./analysis/common";
import { HoverCard } from "./StatTip";

/**
 * A number on its tier's colour: osu!'s pill, or red-to-green text over a
 * faint bar of where it stands (Leetify's look), as Settings picks; plain
 * where there is no scale or colours are off. Hovered, a card says where it
 * stands and what that was measured on (Flashy: "a nicer tooltip to include
 * stats like out of how many games, how many players").
 */
export function TierPill({ text, tier, cls, fmt }: { text: string; tier: Tier | null; cls?: string | null; fmt?: (v: number) => string }) {
  const palette = usePalette();
  if (!tier || palette === "off") return <>{text}</>;
  if (palette === "redgreen") {
    const c = redGreen(tier.pct);
    return (
      <HoverCard
        className="tier-rg"
        card={<TierCard tier={tier} cls={cls} fmt={fmt} />}
        label={
          <>
            <span className="tier-rg-bar" style={{ width: `${Math.max(4, Math.min(100, tier.pct))}%`, background: c }} />
            <span className="tier-rg-text" style={{ color: c }}>
              {text}
            </span>
            {tier.pct >= 100 && <span className="tier-rg-star">★</span>}
          </>
        }
      />
    );
  }
  return (
    <HoverCard
      className="tier-pill-wrap"
      card={<TierCard tier={tier} cls={cls} fmt={fmt} />}
      label={
        <span className="tier-pill" style={{ background: tier.bg, color: tier.fg }}>
          {text}
        </span>
      }
    />
  );
}

/** Where a number stands, the scale's marks, and what it was measured on. */
function TierCard({ tier, cls, fmt }: { tier: Tier; cls?: string | null; fmt?: (v: number) => string }) {
  const c = classLabel(cls ?? "");
  const f = fmt ?? ((v: number) => String(Math.round(v)));
  const at = (p: number) => {
    const i = tier.pcts.indexOf(p);
    return i < 0 ? null : tier.cuts[i];
  };
  const marks: [string, number | null][] = [
    [t("Bottom 25%"), at(25)],
    [t("Top 30%"), at(70)],
    [t("Top 10%"), at(90)],
    [t("Top 1%"), at(99)],
  ];
  const where =
    tier.pct >= 100 ? t("Top 1% of games") : tier.pct >= 50 ? t("Top {0}% of games", { "0": Math.max(1, Math.ceil(100 - tier.pct)) }) : t("Better than {0}% of games", { "0": Math.floor(tier.pct) });
  const stat = tier.stat === "dpm" ? t("Damage per minute") : t("Accuracy");
  return (
    <span className="tc">
      <span className="tc-head">
        <strong>{stat}</strong>
        <span className="tc-sub">{tier.map ? t("{0} on {1}", { "0": c, "1": tier.map }) : t("Every {0} game", { "0": c })}</span>
      </span>
      <span className="tc-where" style={{ color: tier.pct >= 100 ? "#f6d05c" : undefined }}>
        {where}
      </span>
      <span className="tc-scale" aria-hidden>
        {tier.pcts.slice(1).map((p, i) => (
          <span key={p} className="tc-seg" style={{ flex: p - tier.pcts[i], background: tierColour(tier.pcts[i], tier.pcts) }} />
        ))}
        <span className="tc-seg" style={{ flex: 1, background: tierColour(100, tier.pcts) }} />
        <span className="tc-marker" style={{ left: `${Math.max(0, Math.min(100, tier.pct))}%` }} />
      </span>
      <span className="tc-marks">
        {marks.map(([label, v]) =>
          v === null ? null : (
            <span key={label} className="tc-mark">
              <span className="tc-mark-label">{label}</span>
              <span className="tc-mark-value">{label === t("Bottom 25%") ? `< ${f(v)}` : `${f(v)}+`}</span>
            </span>
          ),
        )}
      </span>
      <span className="tc-facts">
        <span>
          <b>{tier.games.toLocaleString()}</b> {t("games")}
        </span>
        {tier.officials !== undefined && (
          <span>
            <b>{tier.officials.toLocaleString()}</b> {t("officials")}
          </span>
        )}
        {tier.players !== undefined && (
          <span>
            <b>{tier.players.toLocaleString()}</b> {t("players")}
          </span>
        )}
      </span>
      {tier.missedMap && <span className="tc-note">{t("{0} has too few {1} games for a scale of its own, so this is among every {1} game.", { "0": tier.missedMap, "1": c })}</span>}
      <span className="tc-note">
        {t("Main-class games of 5 minutes or more that this app has read: the league's officials and your own games. An official counts fully, a scrim half and a pug a quarter.")}
      </span>
    </span>
  );
}
