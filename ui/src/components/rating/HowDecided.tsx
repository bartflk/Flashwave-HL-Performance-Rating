import { t } from "../../lib/i18n";

/**
 * The rest of "how is it decided" (Q80, Flashy: people keep asking how the
 * MVPs and everything else are decided): the MVPs, medals, ranks, opponent
 * strength and the logs.tf career, each with the rules the code uses, the
 * numbers in them as the code has them (`catalogue.rs`: MVP_FINAL_WEIGHT
 * 0.7, MVP_PLAYOFF_FULL_MAPS 4; `logstf.rs`: STRENGTH_MIN_GAMES 20,
 * RANK_MIN_GAMES 100). Every tip's "How it's decided" lands on one of these.
 */
export function HowDecided() {
  return (
    <>
      <section className="panel rg-decided" id="guide-mvp">
        <h2>{t("How MVPs are chosen")}</h2>
        <p>{t("Every division's Grand Final has an MVP for each class and one for the event. The Grand Final is decided from the logs of the final itself, and the playoff run before it counts for a part.")}</p>
        <ol>
          <li>{t("The candidates are the players of each class on either finalist. A player is on the side ETF2L's roster puts them on; a merc, or someone ETF2L never listed, is on the side the final's logs put them on (with whichever finalist's players they shared a colour).")}</li>
          <li>{t("They must have played more than half of the final's logs: one good half of two is not the final's MVP.")}</li>
          <li>{t("Their score is 70% their average rating in the final and 30% their average over the playoff matches before it. With fewer than four playoff maps that 30% shrinks in proportion (two maps count 15%), so a short run cannot outweigh the final.")}</li>
          <li>{t("The best score on each class is the class MVP, whichever finalist they played for. A tie goes to the team that won.")}</li>
          <li>{t("The event MVP is the best of the winning team's players, as HLTV's goes to the champions, even when the losing side's player on their class rated higher.")}</li>
        </ol>
        <p className="hint">{t("A final whose logs are not held has no MVP rather than a guess.")}</p>
      </section>

      <section className="panel rg-decided" id="guide-medals">
        <h2>{t("How medals are given")}</h2>
        <ul>
          <li>{t("Gold and silver go to the winner and runner-up of a division's Grand Final, a forfeit included.")}</li>
          <li>{t("Bronze goes to the winner of the third place match; in a double elimination bracket, to the loser of the lower bracket final; where neither was played, the two losing semi-finalists share it.")}</li>
          <li>{t("A division without playoffs gives its medals by the final table (wins, then map difference), once the season is over.")}</li>
          <li>{t("A player gets their team's medal if they played at least one official for it that season and were on its roster at its last match: a merc who only played for them does not, and neither does someone who left before then (ETF2L's transfers say who did).")}</li>
        </ul>
      </section>

      <section className="panel rg-decided" id="guide-ranks">
        <h2>{t("How ranks are worked out")}</h2>
        <p>{t("A rank is a player's place among everyone on the same class in the same season's division, by their average rating over that season's officials. Four or more officials are needed to be ranked, so one great night does not top a table.")}</p>
      </section>

      <section className="panel rg-decided" id="guide-strength">
        <h2>{t("How opponent strength is measured")}</h2>
        <ul>
          <li>{t("Each player's competitive career rating on logs.tf, from the summary built into the app, on the class they played in the match.")}</li>
          <li>{t("Under 20 competitive games on that class, their most played class is used instead, and the number is shown in italics.")}</li>
          <li>{t("A side's strength is the average over its players with 20 or more games. Players the summary does not know are left out, never counted as average; the bar says how many it is over.")}</li>
          <li>{t("A side is favoured when its average is 0.02 or more above the other's. An upset is the weaker side winning with a gap of 0.03 or more.")}</li>
        </ul>
      </section>

      <section className="panel rg-decided" id="guide-logstf">
        <h2>{t("The logs.tf career")}</h2>
        <ul>
          <li>{t("Built from a whole logs.tf database: every Highlander log since 2012, with the same game uploaded twice kept once and combined logs left out.")}</li>
          <li>{t("Each log's kind is read from its title: TF2Center lobbies and pugs (tf2pickup, hlpugs.tf...) are the pickup pool; league officials and other team games the competitive one. Each pool is rated against itself, with its own 1.00.")}</li>
          <li>{t("Rated by the same model as everything else, but on logs.tf's own numbers: the parts only a server log gives (kills in context, opening duels, fight KAST...) are left out and the others carry their weight. So these sit a little apart from the app's ratings of the games it holds.")}</li>
          <li>{t("A rank on logs.tf is among everyone with 100 or more competitive games on the class. Players with fewer than five Highlander logs are not in the summary.")}</li>
        </ul>
      </section>
    </>
  );
}
