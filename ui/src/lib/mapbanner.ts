import { useQuery } from "@tanstack/react-query";
import { api } from "../api/client";

/**
 * A season banner from its own map pool (Flashy: the RGL home "with a
 * picture in there just like the etf2l highlander"): RGL publishes no season
 * artwork, and an ETF2L 6v6 season may have none, so the season's maps stand
 * in -- the top-down picture of one of them the app holds (built in, or the
 * player's own), the same map for the same season every time.
 */
export function useMapBanner(maps: string[], seed: number, enabled = true): string | null {
  const key = maps.join(",");
  return (
    useQuery({
      queryKey: ["map_banner", key, seed],
      enabled: enabled && maps.length > 0,
      staleTime: Infinity,
      queryFn: async () => {
        // Start at a map picked by the season, so seasons differ; the first
        // with a picture from there on.
        const start = Math.abs(seed) % maps.length;
        for (let i = 0; i < maps.length; i++) {
          const map = maps[(start + i) % maps.length];
          try {
            const img = await api.overviewImage(map);
            if (img) return img.image;
          } catch {
            // That map has no picture we can read: the next one.
          }
        }
        return null;
      },
    }).data ?? null
  );
}
