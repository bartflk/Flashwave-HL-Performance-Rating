import { t } from "../../lib/i18n";
import { formatDate } from "../../lib/format";
import { useMapBanner } from "../../lib/mapbanner";
import { Loading } from "../Spinner";
import { useLive, useSeason } from "./data";
import type { Loc } from "./data";

/**
 * The live RGL season across the top of the RGL home, as ETF2L's
 * Highlander home has its season (Flashy: "the rgl page doesnt have a real
 * home either, i would love a banner with a picture in there just like the
 * etf2l highlander"). RGL publishes no season artwork, so the picture is one
 * of the season's own maps; what the season is, in numbers, beside it.
 */
export function RglStrip({ go }: { go: (l: Loc) => void }) {
  const live = useLive();
  const id = live.season?.id ?? null;
  const s = useSeason(id).data;
  const banner = useMapBanner(s?.maps ?? [], id ?? 0, s !== undefined);
  if (!live.season) return live.none ? null : <Loading className="e2-strip-wait">{t("Loading the season…")}</Loading>;
  const matches = live.matches ?? [];
  const at = (iso: string | null) => (iso ? Math.floor(new Date(iso).getTime() / 1000) : null);
  const times = matches.map((m) => at(m.matchDate)).filter((x): x is number => x !== null && !Number.isNaN(x));
  const now = Date.now() / 1000;
  const played = matches.filter((m) => m.winner !== null || m.isForfeit || m.maps.some((x) => x.homeScore !== null)).length;
  const scheduled = matches.filter((m) => {
    const when = at(m.matchDate);
    return when !== null && when > now;
  }).length;
  const from = times.length ? Math.min(...times) : null;
  const to = times.length ? Math.max(...times) : null;
  const isLive = scheduled > 0 || (to !== null && to > now - 3 * 86_400);
  const teams = s ? s.divisions.reduce((n, d) => n + d.teams.length, 0) : null;
  return (
    <section className={`e2-strip ${banner ? "" : "e2-strip-plain"}`} style={banner ? { backgroundImage: `url("${banner}")` } : undefined}>
      <div className="e2-strip-text">
        <span className="e2-strip-eyebrow">
          {t("RGL Highlander")}
          {isLive && <span className="e2-live-chip">{t("Live")}</span>}
        </span>
        <h1>{live.season.name}</h1>
        {from !== null && to !== null && (
          <span className="e2-strip-dates">
            {formatDate(from, true)} – {formatDate(to, true)}
          </span>
        )}
        <div className="e2-strip-stats">
          {teams !== null && (
            <span>
              <strong>{teams}</strong>
              <em>{t("teams")}</em>
            </span>
          )}
          <span>
            <strong>{played}</strong>
            <em>{t("matches played")}</em>
          </span>
          {scheduled > 0 && (
            <span>
              <strong>{scheduled}</strong>
              <em>{t("scheduled")}</em>
            </span>
          )}
          {s && s.maps.length > 0 && (
            <span>
              <strong>{s.maps.length}</strong>
              <em>{t("maps in the pool")}</em>
            </span>
          )}
        </div>
        {s && (
          <span className="e2-strip-divs rgl-strip-divs">
            {s.divisions.map((d) => (
              <span key={d.name} className="rgl-strip-div">
                {d.name}
              </span>
            ))}
          </span>
        )}
      </div>
      <div className="e2-strip-actions">
        <button className="km-chip" onClick={() => go({ at: "season", id: live.season!.id })}>
          {t("Standings & fixtures")} ›
        </button>
        <button className="km-chip" onClick={() => go({ at: "seasons" })}>
          {t("All seasons")} ›
        </button>
      </div>
    </section>
  );
}
