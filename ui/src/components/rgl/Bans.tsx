import { useInfiniteQuery } from "@tanstack/react-query";
import { api } from "../../api/client";
import type { RglBan } from "../../api/types";
import { t, tx } from "../../lib/i18n";
import { ApiError } from "../ApiError";
import { Loading } from "../Spinner";
import { KEEP, day, forGood } from "./data";

export function BansPage({ onPlayer }: { onPlayer: (s: string) => void }) {
  const q = useInfiniteQuery({
    queryKey: ["rgl_bans"],
    queryFn: ({ pageParam }) => api.getRglBans(pageParam),
    initialPageParam: 0,
    getNextPageParam: (last, all) => (last.length < 50 ? undefined : all.length * 50),
    staleTime: KEEP,
  });
  const bans: RglBan[] = q.data?.pages.flat() ?? [];
  const now = Date.now();
  return (
    <>
      <header className="ts-pagehead">
        <span className="ts-eyebrow">{t("RGL")}</span>
        <h1>{t("Bans")}</h1>
        <p className="hint">{t("The league's bans, newest first, every format's: RGL keeps one list.")}</p>
      </header>
      <section className="panel e2-panel">
        {q.isError && bans.length === 0 ? (
          <ApiError error={q.error} />
        ) : q.isPending ? (
          <Loading>{t("Loading the bans…")}</Loading>
        ) : bans.length === 0 ? (
          <p className="hint">{t("No bans.")}</p>
        ) : (
          <div className="table-wrap">
            <table className="match-table rgl-bans">
              <thead>
                <tr>
                  <th>{t("Player")}</th>
                  <th>{t("Reason")}</th>
                  <th>{t("Banned")}</th>
                  <th>{t("Until")}</th>
                </tr>
              </thead>
              <tbody>
                {bans.map((b, i) => {
                  const over = !!b.expiresAt && !forGood(b.expiresAt) && new Date(b.expiresAt).getTime() < now;
                  return (
                    <tr key={`${b.steamId}-${b.createdAt ?? i}`} className={over ? "rgl-ban-over" : undefined}>
                      <td>
                        <button className="linkish" onClick={() => onPlayer(b.steamId)}>
                          {b.alias ?? b.steamId}
                        </button>
                      </td>
                      <td>{b.reason ?? "–"}</td>
                      <td className="nowrap muted">{day(b.createdAt)}</td>
                      <td className="nowrap">
                        {forGood(b.expiresAt) ? <span className="rgl-ban-perm">{t("Permanent")}</span> : over ? <span className="muted">{tx("ended {0}", { "0": day(b.expiresAt) })}</span> : day(b.expiresAt)}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
        {/* A failed "Show more" says so under the table instead of replacing it. */}
        {q.isError && bans.length > 0 && <ApiError error={q.error} />}
        {q.hasNextPage && (
          <button className="e2-more rgl-bans-more" onClick={() => void q.fetchNextPage()} disabled={q.isFetchingNextPage}>
            {q.isFetchingNextPage ? t("Loading…") : q.isError ? t("Try again") : t("Show more")}
          </button>
        )}
        {!q.hasNextPage && bans.length > 0 && <p className="hint rgl-bans-end">{t("That is the whole list.")}</p>}
      </section>
    </>
  );
}

// ---- A team ----------------------------------------------------------------
