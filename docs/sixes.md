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
