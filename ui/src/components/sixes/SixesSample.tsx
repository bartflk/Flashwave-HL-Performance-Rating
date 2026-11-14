import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { t, tx } from "../../lib/i18n";
import { formatDate } from "../../lib/format";

/**
 * The 6v6 league sample's switch and progress (the 6v6 plan, P5): every
 * ETF2L 6v6 official's logs, downloaded slowly in the background for the
 * 6v6 rating's pools. Off until started here: it is hours of requests.
 */
export function SixesSample() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["sixes_sample"], queryFn: api.getSixesSample, refetchInterval: 15_000 });
  const set = useMutation({
    mutationFn: (on: boolean) => api.setSixesSample(on),
    onSuccess: (s) => qc.setQueryData(["sixes_sample"], s),
  });
  const s = q.data;
  if (!s) return null;
  const c = s.counts;
  return (
    <section className="panel e2-panel">
      <div className="e2-panel-head">
        <h2>{t("6v6 league sample")}</h2>
      </div>
      <p className="muted sx-sample-text">
        {tx("Every ETF2L 6v6 official of the last {0} seasons, downloaded slowly in the background while the app is open, so the 6v6 rating is read against the whole league. Kept apart from your matches and from Highlander's sample.", { "0": s.seasonsBack })}
      </p>
      {c.listed > 0 && (
        <p className="sx-sample-text">
          {tx("{0} of {1} logs downloaded", { "0": c.downloaded.toLocaleString(), "1": c.listed.toLocaleString() })}
          {c.missing > 0 && <span className="muted">{tx(", {0} not to be had", { "0": c.missing.toLocaleString() })}</span>}
          {c.oldest && c.newest && <span className="muted">{tx(" · {0} to {1}", { "0": formatDate(c.oldest), "1": formatDate(c.newest) })}</span>}
        </p>
      )}
      <button className={s.enabled ? undefined : "primary"} disabled={set.isPending} onClick={() => set.mutate(!s.enabled)}>
        {s.enabled ? t("Pause the download") : c.listed > 0 ? t("Carry on downloading") : t("Start the download")}
      </button>
    </section>
  );
}
