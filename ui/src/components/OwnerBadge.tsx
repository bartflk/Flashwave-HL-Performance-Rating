import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";

/**
 * Your picture and name, top right. Both come from your ETF2L profile, or
 * Steam's public profile without one; until they arrive (the lookup runs in
 * the background after setup and on every sync) the SteamID stands in.
 */
export function OwnerBadge({ steamid }: { steamid: string | null }) {
  const q = useQuery({
    queryKey: ["owner", steamid],
    queryFn: api.getOwner,
    // Keep asking until the picture has arrived; then it only changes on a sync.
    refetchInterval: (query) => (query.state.data?.avatar ? false : 5000),
  });
  const o = q.data;
  const name = o?.name ?? null;
  const initial = (name ?? "?").trim().charAt(0).toUpperCase();

  return (
    <div className="owner" title={steamid ?? undefined}>
      <div className="owner-text">
        <span className="owner-name">{name ?? "You"}</span>
        <code className="owner-id">{steamid}</code>
      </div>
      {o?.avatar ? (
        <img className="owner-avatar" src={o.avatar} alt="" />
      ) : (
        <span className="owner-avatar owner-initial" aria-hidden>
          {initial}
        </span>
      )}
    </div>
  );
}
