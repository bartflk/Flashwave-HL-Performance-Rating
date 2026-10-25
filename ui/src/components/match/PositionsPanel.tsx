import { useQuery } from "@tanstack/react-query";
import { Help } from "../StatTip";
import { api } from "../../api/client";
import { errorMessage, type MatchDetail, type PositionsView } from "../../api/types";
import { t } from "../../lib/i18n";
import { ClassIcon } from "../ClassIcon";
import { useMeasuredWidth } from "../../lib/measure";

const CLASSES = ["", "scout", "sniper", "soldier", "demoman", "medic", "heavyweapons", "pyro", "spy", "engineer"];
/** Zones shown per player; the rest are one "other" segment. */
const TOP = 6;
/** Rough pixel widths of the two label lines, to decide what fits. */
const nameWidth = (s: string) => s.length * 6.4 + 12;
const PCT_WIDTH = 34;

type Player = PositionsView["players"][number];

/**
 * The zones that come in a RED and a BLU copy -- one side's ground on a
 * mirrored map. A lone "RED spawn (last)" on a payload map is a name, not
 * a side, so a zone counts only when its other side exists too.
 */
function pairedSides(v: PositionsView): Set<string> {
  const names = new Set(v.players.flatMap((p) => p.zones.map((z) => z.zone)));
  const out = new Set<string>();
  for (const n of names) {
    const m = /^(RED|BLU) (.+)$/.exec(n);
    if (m && names.has(`${m[1] === "RED" ? "BLU" : "RED"} ${m[2]}`)) out.add(n);
  }
  return out;
}

/**
 * Q28 (Flashy): where each player spent the match, by callout -- the
 * anchors and the rotators. From the STV's every-second positions, so only
 * for a match with one, and only on a map whose callouts are drawn.
 *
 * Each bar is that player's time inside a callout, as 100%: time between
 * zones is left out rather than drawn as empty bar, and the share of their
 * time the zones cover is said beside the name.
 */
export function PositionsPanel({ d }: { d: MatchDetail }) {
  const map = d.map ?? "";
  const q = useQuery({ queryKey: ["positions", d.logId, map], queryFn: () => api.getPositions(d.logId, map), enabled: map !== "" });
  if (q.isError) {
    return (
      <section className="panel">
        <h2>{t("Positions")}</h2>
        <p className="error">{errorMessage(q.error)}</p>
      </section>
    );
  }
  const v = q.data;
  if (!v) return null;
  const paired = pairedSides(v);
  const sided = paired.size > 0;
  return (
    <section className="panel positions-panel">
      <h2>
        {t("Positions")}
        <Help>
          {t("Where each player spent their time alive, by callout, from the STV demo: one position a second.")}
          {v.draft && t(" The callouts are a draft; fix them with Edit callouts on the kill map.")}
        </Help>
      </h2>
      {sided && (
        <div className="pos-legend hint">
          <span><i className="pos-seg-red" /> {t("RED's side")}</span>
          <span><i className="pos-seg-blue" /> {t("BLU's side")}</span>
          <span><i className="pos-seg-mid" /> {t("the middle")}</span>
        </div>
      )}
      <div className="pos-teams">
        {/* Your team first, named as yours: in a combined log the colours
            swap between halves and maps, so "RED" said less than it seemed. */}
        {(d.myTeam === "Blue" ? [3, 2] : [2, 3]).map((team) => (
          <div key={team} className="pos-team">
            <h3 className={team === 2 ? "team-red" : "team-blue"}>
              {d.myTeam ? ((team === 2) === (d.myTeam === "Red") ? t("Your team") : t("Enemy team")) : team === 2 ? "RED" : "BLU"}
            </h3>
            {v.players
              .filter((p) => p.team === team)
              .map((p) => (
                <Row key={p.accountId} p={p} paired={paired} sided={sided} team={team} />
              ))}
          </div>
        ))}
      </div>
    </section>
  );
}

function Row({ p, paired, sided, team }: { p: Player; paired: Set<string>; sided: boolean; team: number }) {
  const [barW, barRef] = useMeasuredWidth(120, 360);
  const zoned = p.zones.reduce((a, z) => a + z.seconds, 0);
  const top = p.zones.slice(0, TOP);
  const other = zoned - top.reduce((a, z) => a + z.seconds, 0);
  const coverage = Math.round((100 * zoned) / Math.max(1, p.aliveS));
  const pct = (s: number) => Math.round((100 * s) / Math.max(1, zoned));
  return (
    <div className="pos-row">
      <div className="pos-who">
        {p.class > 0 && <ClassIcon cls={CLASSES[p.class]} size={20} />}
        <span className="pos-name" title={p.name}>{p.name}</span>
        <span className="pos-cover" title={t("{0}% of their time alive was inside a drawn callout", { "0": coverage })}>{coverage}%</span>
      </div>
      {zoned === 0 ? (
        <span className="hint">{t("never inside a drawn callout")}</span>
      ) : (
        <div className="pos-bar" ref={barRef}>
          {top.map((z, i) => {
            // On a map with sides, a zone is coloured by whose it is; on a
            // one-way map, by the player's own team.
            const mine = paired.has(z.zone) ? (z.zone.startsWith("RED ") ? "red" : "blue") : null;
            const side = sided ? mine ?? "mid" : team === 2 ? "red" : "blue";
            const name = mine ? z.zone.slice(4) : z.zone;
            const px = (z.seconds / zoned) * barW;
            return (
              <div
                key={z.zone}
                className={`pos-seg pos-seg-${side} pos-rank-${Math.min(i, 3)}`}
                style={{ flexGrow: z.seconds }}
                title={`${z.zone}: ${pct(z.seconds)}% (${Math.max(1, Math.round(z.seconds / 60))} min)`}
              >
                {px >= nameWidth(name) ? (
                  <>
                    <span className="pos-seg-name">{name}</span>
                    <span className="pos-seg-pct">{pct(z.seconds)}%</span>
                  </>
                ) : px >= PCT_WIDTH ? (
                  <span className="pos-seg-pct">{pct(z.seconds)}%</span>
                ) : null}
              </div>
            );
          })}
          {other > 0 && (
            <div className="pos-seg pos-seg-other" style={{ flexGrow: other }} title={t("{0}% in other callouts", { "0": pct(other) })}>
              {(other / zoned) * barW >= PCT_WIDTH && <span className="pos-seg-pct">{pct(other)}%</span>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
