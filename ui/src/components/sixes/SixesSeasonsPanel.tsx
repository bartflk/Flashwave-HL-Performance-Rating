import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { t, tx } from "../../lib/i18n";
import { DivisionBadge } from "../players/PlayerProfile";
import { tierOf } from "../etf2l/Brackets";
import { Fold } from "../Fold";

/**
 * The owner's ETF2L 6v6 seasons on the profile's Rating tab, where
 * Highlander shows its season panel: division, team and W–D–L from the
 * stored 6v6 lineups (the 6v6 plan, P7).
 */
export function SixesSeasonsPanel() {
  const profile = useQuery({ queryKey: ["sixes_profile"], queryFn: api.getSixesProfile }).data;
  if (!profile) return null;
  return (
    <Fold id="profile-sixes-seasons">
      <section className="panel">
        <h2>{t("ETF2L 6v6 seasons")}</h2>
        {profile.seasons.length === 0 ? (
          <p className="hint">{t("No ETF2L 6v6 season read yet: they come with the league pass once you play 6v6.")}</p>
        ) : (
          <div className="table-wrap" style={{ marginTop: 12 }}>
            <table className="match-table">
              <thead>
                <tr>
                  <th>{t("Season")}</th>
                  <th>{t("Division")}</th>
                  <th>{t("Team")}</th>
                  <th className="num" title={t("Matches won, drawn and lost")}>{t("W–D–L")}</th>
                </tr>
              </thead>
              <tbody>
                {profile.seasons.map((l) => (
                  <tr key={`${l.season}-${l.division}-${l.team}`}>
                    <td>{tx("Season {0}", { "0": l.season })}</td>
                    <td>
                      <DivisionBadge d={{ name: l.division, tier: Math.floor(tierOf(l.division)) }} />
                    </td>
                    <td>{l.team}</td>
                    <td className="num">
                      <span className="team-won">{l.won}</span>–<span className="muted">{l.drawn}</span>–<span className="team-lost">{l.lost}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </Fold>
  );
}

/**
 * A profile section 6v6 cannot fill yet, greyed out rather than left off
 * (Flashy, question 25: "gray out these features until I get to it").
 */
export function SixesSoonPanel({ title, why, items }: { title: string; why: string; items: Array<[string, string]> }) {
  return (
    <section className="panel">
      <h2>{title}</h2>
      <p className="hint" style={{ marginTop: 6 }}>{why}</p>
      <div className="sx6-soon" aria-disabled="true">
        <span className="sx6-soon-label">{t("Coming to 6v6")}</span>
        {items.map(([name, what]) => (
          <span key={name} className="sx6-soon-chip" title={what}>
            {name}
          </span>
        ))}
      </div>
    </section>
  );
}
