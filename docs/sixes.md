# 6v6, beside Highlander

What the app knows and does about 6v6, as built (November 2026). The plan,
with every step's status and the questions still open, is on the 6v6 plan
page; this file is what is in the code.

6v6 is switched on in dev builds only (`hl_core::format::SIXES_ENABLED`): it
ships once its rating is ready (question 15). Formats are never mixed: a
Soldier's Highlander logs never feed a 6v6 number and the other way round.

---

## 1. The format

Six a side: two Scouts, two Soldiers, a Demoman and a Medic. The other five
classes are off-classes, brought out for a moment (a Heavy on last, a Sniper
on Product) and swapped back.

- **Roles.** The pocket Soldier stays with the Medic; the roamer plays ahead
  and hunts picks. One Scout plays with the combo (Medic, Demoman, pocket),
  the other on the flank.
- **5CP** (process, sunshine, gullywash, metalworks, granary): first to 5
  rounds in 30 minutes, 4 minutes a round; level at the limit is a draw in
  the regular season. Every round starts with a midfight at the middle point.
- **KOTH** (product, bagel): first to 3 rounds, no time limit.
- **ETF2L, Season 53 on**: Premiership to Division 3 and Fresh in round-robin
  groups of eight, Division 4, Low and Open in a Swiss system. Two maps a
  week on a fixed rotation. League points per map as ETF2L gives them (2 a
  map won, 1 drawn on the live tables): the app shows ETF2L's score and never
  works it out.

## 2. What the app does with it

| Part | Where | Code |
|---|---|---|
| Which formats you play, asked in setup | Setup, step 2 | `formats_played` setting |
| A Highlander / 6v6 menu on Matches, Profile, ETF2L | the header | `ui/src/components/FormatTab.tsx` |
| Your 6v6 logs downloaded, kept apart | Matches › 6v6 | `worth_fetching`, `hl_only` (hl-db) |
| Officials, scrims, pugs, sized for five teammates | Matches › 6v6 | `context::Sizes::SIXES` |
| Roles from the Medic's heals | 6v6 scoreboard | `hl_ingest::roles` |
| Off-class stints: round, time, mates, which last | scoreboard tooltip | `roles::stints` |
| Midfights | rounds panel | `ui/src/lib/gameformat.ts` |
| 6v6 numbers and Medic against Medic | "6v6 numbers" panel | `hl_ingest::sixes_stats` |
| A preview rating by role and off-class | numbers panel, 6v6 profile | `hl_ingest::sixes_rating`, `weights.sixes.toml` |
| The live season, matches by week | ETF2L › 6v6 | `hl_ingest::sixes` |
| Stored seasons, teams, playoffs, Nations' Cup | ETF2L › 6v6 | `leagues::fetch_sixes` |
| ETF2L's 6v6 news and recruitment | ETF2L › 6v6 | `news`, `recruitment` with a format |

**The rating preview** is Highlander's method -- each component a percentile
against everyone else's performances in the same role, the weighted average
on the app's scale -- with the plan's starting weights. Its pool is the
players in your own stored 6v6 logs, and the components not measured yet
(round impact above all) are left out and the rest scaled up. The 6v6 rating
replaces it once fitted on the 6v6 league sample.

**Checking it against who won:** `hl sixes-validate` rates every stored 6v6
log and reports how often the side with the higher rating won, the rating
of each main role head to head, and each role's components on their own
(ties count half, so 50% is a coin). It reads the database: run it with
the app closed. Its numbers mean little until the league
sample has run; they are what the reviewers argue the weights with.

**The logs.tf dump:** `hl logsdump-sixes <logs.sql> <index-v2.sqlite3> <out.sqlite3>`
reads every 6v6 official, scrim and pug in the 119 GB logs.tf dump (each game
once, as `hl logsdump`'s index decided; lobbies left out), about 1.5 million
games: each player's role and the rating's measurements, from the JSON alone
(the dump has no raw logs, so no round impact or valued kills). `hl
sixes-validate --from <out.sqlite3>` checks the rating against who won on them.
The release waits on it (question 15). The dump is a copy of logs.tf's database.

**The built-in league pool:** `hl logsdump-sixes-pool <sixes.sqlite3> crates/hl-ingest/src/sixes_pool.json`
turns that file's officials (63,959 games, 908,430 performances) into each role's
value at every percentile per component, and its score average and spread:
76 KB, built into the app. The 6v6 rating reads a game's percentiles from it, so
it is against the whole competitive field from the first sync; round impact and
kills valued by victim, which the dump cannot give, stay against the stored pool.
A game whose score has such a part is also scaled against the role's stored
games, not the pool's score spread, which was measured without them (audit F3).
The pool's officials are 6v6 league games of every league whose logs are titled
as officials, ETF2L's, RGL's and others', not ETF2L's alone: only 844 of them
carry an ETF2L title (audit F4). One pool per league needs each log tagged with
its league from the ETF2L and RGL match pages' log links.

**The weights, model s1:** `hl sixes-fit <sixes.sqlite3>` fits each role's measured
components to who won (a logistic model on the two sides' players in a role, fitted
on the older 60% of the officials and scored on the newer 40%). Option A took it
with guard rails: round impact and the not-yet-measured components keep their
weight, and a death measure is capped at 25% of the fitted block. On the dump's
officials the higher-rated side won 89.9% (s0: 88.8%); by role, Medic 83.9%
(80.4%), Pocket Scout 78.5% (73.9%), Demoman 77.6% (76.4%), Pocket 76.1% (72.6%),
Flank Scout 70.4% (68.8%), Roamer 69.8% (68.2%).

**The weights, model s2 (boSe.'s review, 15 Nov):** the pocket rated less on kills
and more on DPM and dying less (kills 10, DPM 12, über forces 6, deaths 4); the
flanks marked down less for dying alone and more for dying together (roamer
forces 8, untraded deaths 5; flank Scout Medic picks 8, untraded deaths 4; both
flanks down 6 each); the pocket Scout's kills in an über he took (8) and his
deaths against his Medic's outside 1 to 8 more per 30 minutes (6); the Medic's
caps contested only while übered or last alive. The new measures are read from
the raw log (`sixes_raw.rs`): an über's target is whoever its Medic healed most
in its first 2 seconds; both flanks down means the other flank died within 10 s,
on 5CP after the round's first capture and on Product only. On the league
sample's 12,635 decided officials, with round impact: team 92.2% (s1 92.2%);
flank Scout 77.1% (76.1%), roamer 77.0% (76.2%), pocket Scout 80.5% (80.5%),
Demoman 79.6% (79.6%), Medic 84.3% (84.4%), pocket 76.9% (77.7%). Both flanks
down picks the winner 75.5% (flank Scout) and 72.4% (roamer) by itself; kills in
an über 60.1%; the deaths band 55.5%; the Medic's caps 50.4%, as he is rarely
on the point übered or last alive.

**Model s3 (boSe.: "All 3 should stay"):** über forces, backcaps and Sniper
pressure are measured as proposed (`sixes_raw.rs`) and count at s2's shares.
On the same officials: team 92.2%; flank Scout 77.7%, roamer 76.8%, pocket
76.6%, the rest unchanged. Backcaps alone pick the winner 76.1%; über forces
45%, under a coin in every role: the side forcing more übers is mostly the
side under pressure. Kept, as agreed, for boSe. to look at again.

## 3. Proposed definitions, for the reviewers

Three of boSe.'s numbers have no agreed definition. The app shows a
proposal for each on the 6v6 numbers panel, marked with an asterisk, and
the rating reads none of them until the reviewers agree (their names are
not in `weights.sixes.toml`; `sixes_stats::add_proposed`).

| Stat | Proposed definition | Open question |
|---|---|---|
| Über forces | When a Medic deploys an über, every enemy who dealt 50 damage or more to the Medic's team in the 4 seconds before gets one | A forced über is one used early, before the team wanted it: is pressure just before it enough, or must the über also be used without an advantage? |
| Backcaps | A capture whose cappers do not include their own team's Medic | Should the middle point count, and should a cap the whole team made while the Medic was dead count? |
| Sniper pressure | Damage as Sniper on the enemy Medic, Demoman and pocket Soldier | Is damage enough, or does time spent holding a sightline the combo must play around matter? |

"Space created" (question 17) has no proposal yet: it needs positions every
moment, from demos.

## 4. Sources

| Source | For 6v6 |
|---|---|
| [drops.tf](https://drops.tf/about) by Icewind | Your 6v6 logs and their raw server logs, asked first, as for Highlander: the heals that tell the roles apart, the class switches that make off-class stints, each kill's weapon |
| [logs.tf](https://logs.tf) | The same, for a log drops.tf has not got yet |
| [trends.tf](https://trends.tf) | Which of your logs are 6v6, and which ETF2L official each belongs to |
| [ETF2L's API](https://api.etf2l.org) | 6v6 competitions, tables, results, match pages and lineups, scheduled matches, recruitment (type `6v6`) |
| ETF2L's site | 6v6 news (WordPress category 22) and the 6v6 rules |

The release notes that switch 6v6 on credit drops.tf by Icewind with them.
