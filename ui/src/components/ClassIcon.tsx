import { capitalize } from "../lib/format";

const KNOWN = new Set(["scout", "soldier", "pyro", "demoman", "heavy", "engineer", "medic", "sniper", "spy"]);

/**
 * A class's leaderboard emblem from the TF2 wiki (Valve's art), with the
 * class name as its tooltip and alt text. An unknown class shows its name.
 */
export function ClassIcon({ cls, size = 20, faded = false }: { cls: string | null; size?: number; faded?: boolean }) {
  if (!cls) return <span className="muted">—</span>;
  const key = cls === "heavyweapons" ? "heavy" : cls;
  if (!KNOWN.has(key)) return <span>{capitalize(cls)}</span>;
  return (
    <img
      src={`/classes/${key}.png`}
      alt={capitalize(key)}
      title={capitalize(key)}
      width={size}
      height={size}
      className={faded ? "class-icon faded" : "class-icon"}
    />
  );
}
