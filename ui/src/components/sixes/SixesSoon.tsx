import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { t, tx } from "../../lib/i18n";
import "./sixes.css";

/**
 * Matches or Profile in 6v6, before their phase is built (the 6v6 plan:
 * your 6v6 matches are P1, the profile P7). Says what will be here and that
 * nothing from Highlander is shown in its place: formats are never mixed.
 */
export function SixesSoon({ what }: { what: "matches" | "profile" }) {
  const stats = useQuery({ queryKey: ["index_stats"], queryFn: api.indexStats });
  const n = stats.data?.sixes;
  return (
    <div className="content sx-soon">
      <section className="panel sx-soon-panel">
        <span className="ts-eyebrow">{t("6v6")}</span>
        <h1 className="sx-title">{what === "matches" ? t("Your 6v6 matches") : t("Your 6v6 profile")}</h1>
        <p>
          {what === "matches"
            ? t("Your 6v6 officials, scrims and pugs will be listed here, each with its roles, midfights and off-class time. They are kept apart from your Highlander matches, and nothing from one format counts towards the other.")
            : t("Your 6v6 career will be here: ratings by role and off-class, seasons, divisions and best games, next to your Highlander career and never averaged with it.")}
        </p>
        {n !== undefined && (
          <p className="muted">
            {n === 1 ? t("Your index lists 1 6v6 log. It is not downloaded yet.") : tx("Your index lists {0} 6v6 logs. They are not downloaded yet.", { "0": n.toLocaleString() })}
          </p>
        )}
        <p className="hint">{t("Switch back to Highlander from the menu on this tab.")}</p>
      </section>
    </div>
  );
}
