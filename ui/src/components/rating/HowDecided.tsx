import { t } from "../../lib/i18n";

/**
 * The rest of "how is it decided" (Q80, Flashy: people keep asking how the
 * MVPs and everything else are decided): the MVPs, medals, ranks, opponent
 * strength and the logs.tf career, each with the rules the code uses, the
 * numbers in them as the code has them (`catalogue.rs`: SHRINK_MAPS 4,
 * PLAYOFF_WEIGHT 1.5, FINAL_WEIGHT 2, SEASON_MVP_WEIGHTS 0.5/0.35/0.15,
 * AWARD_MIN_OFFICIALS 4, AWARD_SHARE_OF_SEASON 0.6, EVPS 5 / 3 below 8 teams; `logstf.rs`: STRENGTH_MIN_GAMES 20,
 * RANK_MIN_GAMES 100). Every tip's "How it's decided" lands on one of these.
 */
export function HowDecided() {
  return (
    <>
      <section className="panel rg-decided" id="guide-mvp">
        <h2>{t("How the season awards are chosen")}</h2>
        <p>{t("Every division's season ends with three kinds of award, each judged on the whole season rather than one final: the Season MVP, the EVPs and the best player of each class. HLTV gives its MVP to an event's champions and its EVPs to standouts from any team; these follow that.")}</p>
        <h3>{t("The season rating")}</h3>
        <ol>
          <li>{t("Every official a player played on the class that season for their own team counts, playoffs included; games as a merc for another team are shown in the tables but left out of the awards. A playoff official counts 1.5 times and a Grand Final twice, as the harder games.")}</li>
          <li>{t("The average is pulled towards 1.00 by four maps' worth of average games, so a short season cannot run away with it: 1.30 over 4 maps reads about 1.15, while 1.15 over 20 maps reads 1.13.")}</li>
          <li>{t("The season tables rank by this number. An award needs enough of the season: 60% of the most officials anyone in the division played that season, and never fewer than four, so a Premiership season of seven games asks for fewer than a Low season of ten.")}</li>
        </ol>
        <h3>{t("The awards")}</h3>
        <ul>
          <li>{t("Season MVP, one per division: the best player of the team that won it. 50% their season rating, 35% their Grand Final and 15% the rest of their playoffs, each pulled towards 1.00 the same way; with no other playoff games their 15% goes to the season. They need enough officials in the season and to have played more than half of the final's logs. A tie goes to more officials, then more final maps.")}</li>
          <li>{t("EVPs: the five best season ratings in the division on any team and any class, each player once, the Season MVP aside; three in a division of fewer than eight teams.")}</li>
          <li>{t("Best of the class: the top of each class's season table among the division's three medal teams, with enough officials on the class. A great season on a team that went out early is an EVP at most.")}</li>
        </ul>
        <p className="hint">{t("A player the roster does not place, a merc or someone ETF2L never listed, is on the side the final's logs put them on. A division whose final logs are not held has no Season MVP rather than a guess.")}</p>
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
        <p>{t("A rank is a player's place among everyone on the same class in the same season's division, by their season rating: the average over that season's officials, playoff maps counting 1.5 times and the Grand Final twice, pulled towards 1.00 by four maps. Three or more officials are needed to be ranked, so one great night does not top a table.")}</p>
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
