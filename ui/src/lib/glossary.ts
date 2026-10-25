import { k } from "./i18n";

/**
 * What every number in the app means, how it is worked out, and where it
 * comes from (Q80, Flashy: "a lot of people are giving me questions about a
 * lot of the ratings, mvp allocation and how everything is decided").
 *
 * One entry per statistic, shown by `StatTip` on hover; `more` points at the
 * section of the How ratings work page that says it at length. Keep these
 * true to the code: the MVP and medal rules are `catalogue.rs`'s, the rating
 * is `hl_rating::model`'s, the logs.tf career is `logstf.rs`'s.
 */

export type Source = "logs.tf" | "server log" | "STV demo" | "ETF2L" | "trends.tf" | "logs.tf summary" | "this app";

export interface Entry {
  label: string;
  what: string;
  how: string;
  source: Source[];
  /** A section of the How ratings work page. */
  more?: "rating" | "mvp" | "medals" | "ranks" | "strength" | "logstf";
}

export const GLOSSARY = {
  rating: {
    label: k("Rating"),
    what: k("How well someone played a game on their main class, against everyone else's games on that class. 1.00 is a typical game; 1.25 is one standard deviation better."),
    how: k("Each part of the game (kills in context, damage, deaths, medic picks...) becomes a percentile against the pool, the parts are weighed by the class's model, and the result is placed on the 1.00 scale."),
    source: ["logs.tf", "server log"],
    more: "rating",
  },
  form: {
    label: k("Form"),
    what: k("The average rating of the last 20 games in view."),
    how: k("The arrow compares it with the 20 games before those."),
    source: ["this app"],
    more: "rating",
  },
  career: {
    label: k("Career"),
    what: k("The average rating of every rated game in view: the class, kind of game and period picked."),
    how: k("Each game counts once, whatever its length, as long as the class was played for the minimum minutes."),
    source: ["this app"],
    more: "rating",
  },
  winRate: {
    label: k("Win rate"),
    what: k("The share of games won, ties left out."),
    how: k("Won over won plus lost, from each log's final score."),
    source: ["logs.tf"],
  },
  timePlayed: {
    label: k("Time played"),
    what: k("Time spent on this class in the games in view."),
    how: k("logs.tf's time per class, summed."),
    source: ["logs.tf"],
  },
  record: {
    label: k("Record"),
    what: k("Wins, losses and ties in the games in view."),
    how: k("From each log's final score: more rounds or points than the other side is a win."),
    source: ["logs.tf"],
  },
  dpm: {
    label: k("Damage per minute"),
    what: k("Damage dealt on this class, per minute played on it."),
    how: k("logs.tf's damage on the class over its time on the class, summed across the games before dividing."),
    source: ["logs.tf"],
  },
  accuracy: {
    label: k("Accuracy"),
    what: k("Hits over shots with this class's weapons."),
    how: k("From logs.tf's weapon statistics, which only logs with the accuracy plugin have; the count says how many of the games did."),
    source: ["logs.tf"],
  },
  kd: {
    label: k("K / D"),
    what: k("Kills per death on this class."),
    how: k("Kills over deaths, summed across the games."),
    source: ["logs.tf"],
  },
  mvp: {
    label: k("Season awards"),
    what: k("Each division's season ends with three kinds of award: the Season MVP (the champions' best), up to five EVPs (the best seasons on any team) and the best player of each class."),
    how: k("All three use the season rating: every official on the class for the player's own team (merc games are left out), adjusted for the strength of the team faced, playoffs counting 1.5 times and the Grand Final twice, pulled towards 1.00 so a short season cannot run away with it. Enough officials to be eligible: 60% of the most anyone in the division played that season, never fewer than four."),
    source: ["logs.tf", "server log", "ETF2L"],
    more: "mvp",
  },
  eventMvp: {
    label: k("Season MVP"),
    what: k("The best player of the team that won the division."),
    how: k("50% their season rating, 35% their Grand Final, 15% the rest of their playoffs (to the season when there were none), each pulled towards 1.00. Players of the champions with enough officials (60% of the most anyone in the division played, at least four) who played more than half of the final's logs. A tie goes to more officials, then more final maps."),
    source: ["logs.tf", "ETF2L"],
    more: "mvp",
  },
  evp: {
    label: k("EVP"),
    what: k("One of the division's best seasons, on any team: five per division, three in a division of fewer than eight teams."),
    how: k("The best season ratings in the division on any class, each player once, the Season MVP aside. Enough officials on the class to be eligible: 60% of the most anyone in the division played, at least four."),
    source: ["logs.tf", "ETF2L"],
    more: "mvp",
  },
  classAward: {
    label: k("Best of the class"),
    what: k("The top of the class's season table among the division's three medal teams, EVPs aside."),
    how: k("The highest season rating among players on the gold, silver and bronze teams with 60% of the most officials anyone in the division played (at least four) on the class. The same number the season table ranks by. An EVP already has an award, so the class's next best medallist takes it. EVPs are open to every team."),
    source: ["logs.tf", "ETF2L"],
    more: "mvp",
  },
  seasonRating: {
    label: k("Season rating"),
    what: k("A player's season on one class: what the season tables rank by and the awards are decided on."),
    how: k("One rating per official (its maps by minutes played), each adjusted for how strong the team faced was (0.075 for every 0.10 of their strength, from what they won that season), playoffs counting 1.5 times and the Grand Final twice, then pulled towards 1.00 by three officials' worth of average games, so a short season counts for less. The three and the 0.075 are measured on the league's officials, not chosen."),
    source: ["logs.tf", "server log", "ETF2L"],
    more: "ranks",
  },
  impact: {
    label: k("Impact"),
    what: k("How much a player's kills added to their team's chance of winning the round, and their deaths took away. 1.00 is an average game on the class; 1.25 one standard deviation better."),
    how: k("Every kill moves the round-win chance from the numbers alive before it to after (with the uber advantage), measured on over a million league kills; the killer gets the change (three quarters with an assist), the player who died loses it. Per 30 minutes, against the class's average. Needs the server log; never part of the rating."),
    source: ["server log", "this app"],
    more: "rating",
  },
  kast: {
    label: k("KAST"),
    what: k("The share of fights a player was in where they got a kill or an assist, survived, or were traded."),
    how: k("From the server log's fights: each fight the player took part in counts once."),
    source: ["server log"],
  },
  medal: {
    label: k("Medal"),
    what: k("First, second or third in an ETF2L division."),
    how: k("From the Grand Final, the third place match (or the lower bracket final, or the losing semi-finalists), or the final table where a division had no playoffs. The team's roster at its last match takes it, if they played an official for it that season; a merc or someone who left before then does not."),
    source: ["ETF2L"],
    more: "medals",
  },
  rank: {
    label: k("Rank"),
    what: k("Their place among everyone on the class in that season's division, by season rating."),
    how: k("Over their officials that season, playoff maps counting 1.5 times and the Grand Final twice, pulled towards 1.00 by four maps; three or more officials to be ranked."),
    source: ["logs.tf", "ETF2L"],
    more: "ranks",
  },
  strength: {
    label: k("Opponent strength"),
    what: k("How strong each lineup was coming in: the average logs.tf career rating of its players."),
    how: k("Each player's competitive career rating on the class they played (their most played below 20 games on it), averaged over the players with 20 or more games. Unknown players are left out, never counted as average."),
    source: ["logs.tf summary"],
    more: "strength",
  },
  logstf: {
    label: k("logs.tf career"),
    what: k("Every Highlander game on logs.tf since 2012, rated against all of them."),
    how: k("Competitive (officials and other team games) and pickup (TF2Center lobbies and pugs) are rated apart, each with its own 1.00. Duplicate uploads and combined logs are left out. Rated on logs.tf's own numbers, without server-log parts."),
    source: ["logs.tf summary"],
    more: "logstf",
  },
  logstfRank: {
    label: k("Rank on logs.tf"),
    what: k("Their place among everyone with 100 or more competitive games on the class."),
    how: k("By career rating; fewer than 100 games is not ranked."),
    source: ["logs.tf summary"],
    more: "logstf",
  },
  best: {
    label: k("Best"),
    what: k("Their highest single-game rating."),
    how: k("A game with every part at the top of the pool reaches about 1.62, the ceiling of the scale."),
    source: ["this app"],
    more: "rating",
  },
} satisfies Record<string, Entry>;

export type GlossaryKey = keyof typeof GLOSSARY;

/** A column's own explanation, where a glossary entry would be overkill:
 *  its name, what it is, how it is counted, and from what. */
export function tip(label: string, what: string, how: string, ...source: Source[]): Entry {
  return { label, what, how, source };
}
