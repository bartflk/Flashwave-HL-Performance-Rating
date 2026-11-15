import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";
import type { RglLink } from "../api/types";
import { openRgl } from "../lib/goto";
import { t } from "../lib/i18n";
import { RglMark } from "./RglMark";

/** Your matches that were RGL matches (hl_ingest::rgl_own), by log. */
export function useRglLinks(): Map<number, RglLink> {
  const q = useQuery({ queryKey: ["rgl_own_links"], queryFn: api.getRglOwnLinks, staleTime: 30 * 60_000 });
  const m = new Map<number, RglLink>();
  for (const l of q.data ?? []) m.set(l.logId, l);
  return m;
}

function about(l: RglLink): string {
  return [l.seasonName, l.division, l.week, l.opponent ? t("vs {0}", { "0": l.opponent }) : null].filter(Boolean).join(" · ");
}

/**
 * An RGL match among your matches: its badge in the match list (Flashy chose
 * "B now": RGL context, not counted as an official). A click opens the RGL
 * match page.
 */
export function RglLinkTag({ logId }: { logId: number }) {
  const l = useRglLinks().get(logId);
  if (!l) return null;
  return (
    <button
      className="badge badge-rgl"
      title={about(l)}
      onClick={(e) => {
        e.stopPropagation();
        openRgl({ kind: "match", id: l.matchId });
      }}
    >
      <RglMark size={11} /> {t("RGL")}
    </button>
  );
}

/** The same on a match page's header: what RGL match it was, and the way to it. */
export function RglLinkLine({ logId }: { logId: number }) {
  const l = useRglLinks().get(logId);
  if (!l) return null;
  return (
    <div className="rgl-link-line">
      <RglMark size={14} />
      <span>{about(l)}</span>
      {l.won !== null && <span className={l.won ? "team-won" : "team-lost"}>{l.won ? t("Won") : t("Lost")}</span>}
      <button className="km-chip" onClick={() => openRgl({ kind: "match", id: l.matchId })}>
        {t("Open on the RGL tab")} ›
      </button>
    </div>
  );
}
