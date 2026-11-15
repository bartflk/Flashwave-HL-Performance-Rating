import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { EtfBan } from "../api/types";
import { formatDate } from "../lib/format";
import { t } from "../lib/i18n";

/**
 * Every active ETF2L ban, by account (hl_ingest::bans), read once and kept
 * a quarter of an hour: the backend reads ETF2L's list once a day.
 */
export function useBans(): Map<number, EtfBan> {
  const q = useQuery({ queryKey: ["etf2l_bans"], queryFn: api.getEtf2lBans, staleTime: 15 * 60_000 });
  const m = new Map<number, EtfBan>();
  for (const b of q.data ?? []) m.set(b.accountId, b);
  return m;
}

/**
 * "Banned until …" beside a name with an active ETF2L ban, the reason on
 * hover (Flashy: "have them applied if they have them active" -- a badge
 * everywhere, nothing taken out of ratings or awards). Nothing otherwise.
 */
export function BanTag({ accountId, short = false }: { accountId: number | null | undefined; short?: boolean }) {
  const bans = useBans();
  const b = accountId == null ? undefined : bans.get(accountId);
  if (!b) return null;
  const until = b.end ? t("Banned until {0}", { "0": formatDate(b.end, true) }) : t("Banned");
  const reason = b.reason ? t("ETF2L ban: {0}", { "0": b.reason }) : t("ETF2L ban");
  return (
    <span className="ban-tag" title={`${reason}${short && b.end ? ` · ${until}` : ""}`}>
      {short ? t("Banned") : until}
    </span>
  );
}
