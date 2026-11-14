import { useQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import { t, tx } from "../../lib/i18n";
import "./sixes.css";

/**
 * The Profile tab in 6v6, before its phase is built (the 6v6 plan, P7).
 * Says what will be here and that nothing from Highlander is shown in its
 * place: formats are never mixed.
 */
export function SixesSoon() {
  const stats = useQuery({ queryKey: ["index_stats"], queryFn: api.indexStats });
  const s = stats.data;
  return (
    <div className="content sx-soon">
      <section className="panel sx-soon-panel">
        <span className="ts-eyebrow">{t("6v6")}</span>
        <h1 className="sx-title">{t("Your 6v6 profile")}</h1>
        <p>{t("Your 6v6 career will be here: ratings by role and off-class, seasons, divisions and best games, next to your Highlander career and never averaged with it.")}</p>
        {s && (
          <p className="muted">
            {tx("{0} of your {1} 6v6 logs are downloaded. Matches › 6v6 lists them.", { "0": s.sixesFetched.toLocaleString(), "1": s.sixes.toLocaleString() })}
          </p>
        )}
        <p className="hint">{t("Switch back to Highlander from the menu on this tab.")}</p>
      </section>
    </div>
  );
}
