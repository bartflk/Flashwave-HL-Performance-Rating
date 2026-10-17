/**
 * Settings' context icons: plain line drawings at 18 px, in the text's own
 * colour, one per section. Drawn here rather than pulled from an icon
 * library for twenty shapes.
 */

export type IconName =
  | "update"
  | "scale"
  | "alert"
  | "globe"
  | "user"
  | "palette"
  | "clock"
  | "fileX"
  | "shield"
  | "list"
  | "map"
  | "film"
  | "drive"
  | "gear"
  | "database"
  | "archive"
  | "scroll"
  | "network"
  | "info"
  | "chevron"
  | "search";

const PATHS: Record<IconName, string> = {
  update: "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7",
  alert: "M12 3 2 20h20L12 3zM12 10v4M12 17.5v.5",
  globe: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c2.5 2.7 2.5 15.3 0 18M12 3c-2.5 2.7-2.5 15.3 0 18",
  user: "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21c1-4 4.5-6 8-6s7 2 8 6",
  scale: "M4 9V4h5M20 15v5h-5M4 4l7 7M20 20l-7-7M15 4h5v5M9 20H4v-5",
  palette: "M12 3a9 9 0 1 0 0 18c1.2 0 1.8-.9 1.4-2-.4-1.2.4-2 1.6-2H18a3 3 0 0 0 3-3c0-6-4-11-9-11zM7.5 11.5h.01M9.5 7.5h.01M14.5 7.5h.01M16.5 11.5h.01",
  clock: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2",
  fileX: "M14 3H6v18h12V7l-4-4zM14 3v4h4M9.5 12.5l5 5M14.5 12.5l-5 5",
  shield: "M12 3l8 3v6c0 4.5-3.4 8-8 9-4.6-1-8-4.5-8-9V6l8-3zM9 12l2 2 4-4",
  list: "M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01",
  map: "M3 6l6-3 6 3 6-3v15l-6 3-6-3-6 3V6zM9 3v15M15 6v15",
  film: "M4 4h16v16H4zM8 4v16M16 4v16M4 8h4M4 12h4M4 16h4M16 8h4M16 12h4M16 16h4",
  drive: "M3 13l3-8h12l3 8v6H3v-6zM3 13h18M17 16h.01",
  gear: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.9 1.2V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-2.9-1.2l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.7 1.7 0 0 0 3 15.5H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.2-2.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A1.7 1.7 0 0 0 10 4.6V4a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 2.9 1.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1A1.7 1.7 0 0 0 20.4 11H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 0z",
  database: "M12 3c4.4 0 8 1.3 8 3s-3.6 3-8 3-8-1.3-8-3 3.6-3 8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3",
  archive: "M3 4h18v4H3zM5 8v12h14V8M10 12h4",
  scroll: "M8 3h11v15a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3v-2h11v2a3 3 0 0 0 3 3M8 3a3 3 0 0 0-3 3v10M11 8h5M11 12h5",
  network: "M12 3v6M12 15v6M5 12a7 7 0 0 1 14 0M3 12h4M17 12h4M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
  info: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 11v6M12 7.5v.5",
  chevron: "M6 9l6 6 6-6",
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4-4",
};

export function SettingsIcon({ name, size = 18 }: { name: IconName; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.75} strokeLinecap="round" strokeLinejoin="round" aria-hidden focusable="false">
      <path d={PATHS[name]} />
    </svg>
  );
}
