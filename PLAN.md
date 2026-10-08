# HL Performance Rating System — Plan v2.0

**Stack:** Tauri 2 + Rust core + React/TypeScript + SQLite
**Player:** Flashy — `76561198099396919` / `[U:1:139131191]` / ETF2L 97913
**TF2:** `D:\SteamLibrary\steamapps\common\Team Fortress 2\tf`

---

## 1. What this is

A **post-match review tool for Highlander**, opened after a game.

Three decisions shape everything below:

1. **The unit of analysis is the matchup, not the scoreboard.** Team vs team, and within that, class vs class — your Sniper against their Sniper. "Who won that matchup, and by how much" is the primary question the app answers. It is the thing logs.tf cannot tell you, and the reason the app exists.
2. **Value is class-specific.** A Sniper's worth is high-impact picks (Medic, Demo, the enemy Sniper), not raw damage. Each of the nine classes gets its own model of what makes or breaks a performance. Sniper is built first and deepest.
3. **Logs land first, demos enrich later.** The log is available within seconds of a match ending; the demo is local and slow. The match page renders from log data immediately, then fills in demo-derived detail when it is ready. Never block the first view on a parse.

Scope: Highlander only. Sixes is detected, stored, and excluded from ratings — a later update, not a v1 feature.

---

## 2. Data sources

Four sources, each with one job. This is the biggest change from v0.2, and it removes two problems that would otherwise have been hard.

| Source | Role | Verified |
|---|---|---|
| **trends.tf** `/api/v1/logs` | **Index.** Per log: `format`, `league`, ETF2L `matchid`, demos.tf `demoid`, `duplicate_of`, duration, map, time | yes |
| **logs.tf** `/api/v1/log/<id>` | **Detail.** Full box score, per-round events, per-class and per-weapon stats | yes |
| **ETF2L** `api.etf2l.org` | **Context.** Division and tier, competition, season, opponent identity | yes |
| **demos.tf** | **STV demos.** All 18 players, reachable via the `demoid` trends.tf already provides | via demoid |
| **logs.tf raw logs** | **Per-kill events.** The server log behind every logs.tf page: each kill with time, both classes, weapon and both players' positions. See §9 and "Raw logs (M6)" | yes: 740 logs, every kill matches logs.tf |

### Why trends.tf changes the plan

v0.2 planned to classify format by player count and to identify officials from log titles. Both are now unnecessary:

- **Format comes labelled.** The headcount heuristic was wrong anyway — 183 Highlander logs have 19-21 players because of mid-match subs.
- **League comes labelled**, with the ETF2L `matchid` attached. Titles were useless for this: 1,157 of 1,492 are just `serveme.tf #1563599 RED vs BLU`.
- **Duplicate logs come flagged.** Note the direction, which is easy to get backwards: `duplicate_of` sits on the **combined** log and lists the per-round **parts** it was built from. The combined log is the one to keep. On this account 480 part logs are superseded that way; counting them alongside their combined log would double-count over a third of the history, and nothing in logs.tf alone reveals it. Overlapping combines also exist (49 parts claimed by more than one combined log), so dedupe groups every log connected through a shared part and keeps exactly one — the longest.
- **demos.tf demo ids come attached**, which answers "find the demo if I don't have it locally" with no searching at all.

The cost is a third-party dependency. Mitigations: cache their index permanently like any other raw source, keep a class-coverage heuristic as a fallback classifier, and make every classification overridable by hand. Their docs note format detection is based on player count and playtime, so it shares the failure modes above — a strong default, not gospel.

### What the account actually contains

Measured by the M1 sync, after deduplication (1,494 logs indexed across both sources):

| | |
|---|---|
| **Highlander matches** | **666** (from 1,105 Highlander logs) |
| **ETF2L official matches** | **43** (155 logs before folding in per-round parts) |
| Sixes | 117 |
| Per-round parts superseded | 480 |
| Only on logs.tf, classified locally | 214 (mostly 2014-2019) |
| Logs with a demos.tf demo | 1,047 |
| Logs with no map recorded | 43 |
| Local POV demos | 101 |
| Range | 2014-05-17 → 2026-09-17 |

Two consequences worth stating plainly:

- **Officials are a small but real corpus.** 43 matches is enough to show officials separately from scrims, not yet enough to build statistics on alone — expect officials to be a filter and a badge for a while, with ratings drawn from all Highlander play. The ETF2L API's own player-results endpoint returns fewer still, because it reflects only current team rosters; trends.tf's league tagging is the better source.
- **STV demos exist for 1,047 matches, against 101 local POV demos.** The demo story should lean on demos.tf, not the local folder. POV demos remain the only source for your own aim and viewangles, but for anything team-wide, STV is both richer and ten times more available.

---

## 3. The rating model

### Layers

```
raw stat
  -> per-class, per-gamemode normalization (time-weighted)
  -> impact weighting (who you killed matters more than how many)
  -> class value score        -> the number on the match page
  -> matchup differential     -> you vs your opposite number
  -> percentile vs baseline   -> the profile
```

### Impact weighting — the core idea

`classkills` in the log JSON breaks kills down by **victim class**, per player. Verified on a real log: as Sniper, `{"pyro":3,"sniper":4}`. So "high-impact kills" is directly computable — and so is the sniper duel, since `classkills.sniper` against `classdeaths.sniper` *is* the duel record.

Victim weights are the first thing to tune, and live in TOML so tuning needs no recompile:

```toml
[victim_value]        # what killing this class is worth (v2, applied)
medic       = 3.0
demoman     = 2.2
sniper      = 1.8     # denying their picks
pyro        = 1.5
heavy       = 1.4
spy         = 1.3
soldier     = 1.2
scout       = 1.15
engineer    = 1.1
```

A starting point, not a claim. These get replaced by fitted weights once there is enough data — regress round outcome on per-round features and let the numbers argue.

### Victim values v2: input from a Premiership Sniper

Reviewed with function, a Premiership Sniper. The flat table above undervalues two classes, and the real answer depends on the map and the side.

**General table: applied.** This is now the default in `weights.default.toml`, and it stays the default until the per-map layer exists.

| Class | v1 | v2 | Why |
|---|---|---|---|
| Pyro | 0.9 | **1.5** | With their Pyro dead, your team can spam projectiles freely. That changes the fight. |
| Spy | 0.9 | **1.3** | An important kill, above Soldier, but it rarely decides a teamfight. |
| Scout | 1.0 | **1.15** (proposed) | Should sit above Engineer, except on stopwatch defence (below). The exact number is ours, not agreed. |
| Engineer | 1.1 | 1.1 | Fine as a default. It needs to go up on stopwatch defence. |

Medic, Demoman, Sniper, Heavy and Soldier are unchanged.

**Per map.** A kill's worth depends on the map:
- An Engineer on Vigil last is worth far more than one on Product mid.
- A Sniper pick on Upward is worth more than one on Vigil.

So the model becomes a general table plus per-map overrides.

**Per side.** In stopwatch, killing RED's (defending) Engineer matters much more than killing BLU's. The side changes the value, not just the map.

**As built (M6).** Layered, with each layer optional and falling back to the one above. "Defending" covers every attack/defence map, so a separate per-mode layer turned out to be unnecessary:

```toml
[victim_value]                       # general
pyro = 1.5
spy = 1.3

[victim_value.defending]             # victim defending on any attack/defence map
engineer = 1.6                       # proposed
scout = 1.0                          # proposed

[victim_value.map.pl_vigil]          # one map, both sides (none set yet)
sniper = 1.5

[victim_value.map.pl_vigil.defending]  # one map, defending side
engineer = 1.8

[attack_defend]                      # control-point maps with sides; payload always counts
maps = ["cp_steel", "cp_gravelpit", ...]
```

Lookup order: map and side, then map, then side, then general. Map keys match the start of the map name, so versions do not matter. An unknown class anywhere is an error, not a silent zero.

**What the data allows. This decides the order of work:**
- **Per map: possible now.** Every log has its map, and `classkills` gives victims per player per log.
- **Per side: not possible from the logs.tf summary**, but possible from the raw log. `classkills` covers the whole log, not each round, and the only timed kills in the summary are Medic deaths. The raw server log has every kill with time and victim class; more.tf already parses it (§9). With it, each kill falls in a round, each round has a known attacking and defending side, and per-side values need no demos.
- **Percentiles blunt a map-wide scale.** Ratings are percentiles against the pool. A multiplier that applies to every Sniper on Vigil only moves Vigil games relative to other maps. If the goal is "a good Vigil game is a good game", per-map baselines may be the better tool than a per-map multiplier. Decide when building it.

**Effect on this account (re-rated after applying):** small. Sniper career stays at 48.8. Recent form goes from 45.0 to 45.2. The Impact kills percentile moves from 44.2 to 44.9, with raw impact up from 13.5 to 14.7 per 10 min. Officials read 51.6, down from 51.8; pugs 46.4, up from 46.2. No best or worst game changes place. Because the rating is a percentile, reweighting only moves you when your mix of victims differs from other Snipers'. Yours barely does.

**Order:**
1. ~~Apply the v2 general table~~: done.
2. Per-mode and per-map overrides.
3. Per-side values once per-kill events exist (M6, raw logs).

### Model v2: Sniper weights checked against who won

**Why.** In the S33 Low grand final against Champions of Light (log 3863290), v1 rated angel complex's Sniper 56.5 and yours 50.6. You both got 110 kills. You did 388 DPM to their 310, and they said themselves that you were the more impactful Sniper. You, function and the other Sniper agreed:
- **DPM** at 5% is far too low.
- **The Sniper duel** at 20% is far too high. A Spy or a flank kills the enemy Sniper as well as you can, and on some maps (Vigil second, the hill) a Sniper is forced into bad peeks as a suicide entry.

**What the community writes.** It is split. A teamfortress.tv thread on reading logs calls DPM "mostly meaningless without context". Others answer that you cannot do damage without hitting shots. The wiki's Sniper page is about picks and target priority; it names no stat. Nobody offers numbers, so this account's own matches decided.

**The test.** 693 decided matches have one rated Sniper a side. For each stat, how often did the team whose Sniper was better at it win?

| Better Sniper at | Their team won |
|---|---|
| Deaths (fewer) | 75.6% |
| Impact kills | 70.2% |
| Kills not traded back | 65.1% |
| DPM | 64.3% |
| Medic picks | 63.6% |
| Opening kills | 61.6% |
| Sniper duel | 59.1% |
| First picks of the round | 52.0% |
| Picks into a ready charge | 50.1% |
| Headshot share | 46.8% |

Fitted together (a logistic model on the two Snipers' percentile differences, bootstrapped):
- **Kills and deaths** carry nearly all of it.
- **Kills not traded back** add real signal on top.
- **The duel and headshot share point the wrong way** once kills and deaths are known, both clearly below zero across the bootstrap.
- **DPM and opening duels** add about nothing beyond kills, but cost nothing either.

**How often each weighting picks the winning Sniper's team:**

| Weighting | Picks the winner |
|---|---|
| v1 | 67.5% |
| function's "duel 10, DPM 15" | 68.3% |
| **v2** (below) | **71.4%** |
| fitted on this data | 75.7% |

The fitted weighting is in-sample and leans almost entirely on deaths, and deaths are partly the team losing. So it is a check, not the model.

**v2, applied:**

| Sniper | v1 | v2 |
|---|---|---|
| Impact kills | 30% | 25% |
| Damage / min | 5% | **20%** |
| Deaths | 15% | 15% |
| Opening duels (new): opening kills less opening deaths, per 10 min | – | **15%** |
| Medic picks | 15% | 10% |
| Impact assists | 5% | 5% |
| Sniper duel | 20% | **5%** |
| Kills not traded (new): share of kills not traded back within 3 s | – | **5%** |
| Headshot share | 10% | **0%** |

**Why opening duels are in despite predicting little.** They are what separated the two Snipers in the grand final: 22–5 for you against 15–14. The theory (§11) says the first kill of a fight is the Sniper's job. Adding them cost no accuracy.

**Mechanics.** The two new components come from the fights pass, so logs without a raw log skip them and the other weights take up the slack. The model version is now `v2`: ratings are stored per version and never mix, and the app re-rates at startup when the current version has none.

**Effect:**
- **The grand final:** you 60.8, angel complex 55.8.
- **Your Sniper career:** 48.8 → 51.7. Form 49.0. Officials 55.2, scrims 51.9, pugs 48.0.
- **Your components:** your weakest is still the duel (36th percentile on form). It now moves the rating a quarter as much.

**Open:**
- Other classes still use v1's weights; the same test can be run for each.
- The duel may belong in the profile as information only, not in the rating.

### Matchup scoring

For each of the nine classes, compare the two players who played it:

```
matchup_score(class) = value(mine) - value(theirs)
```

Rendered as nine rows on the match page: who won each matchup, by how much, and which two or three were decisive. This is the headline view.

Subtlety to handle early: with subs, a class can have more than one player per team (those 19-21 player logs). Weight by `class_stats.total_time` and treat the matchup as a time-weighted aggregate rather than a single pairing.

### Sniper — built first and deepest

All available from the log, no demo required:

| Signal | Source | Why it matters |
|---|---|---|
| Impact picks / min | `classkills` × victim weights | The job |
| Sniper duel differential | `classkills.sniper` − `classdeaths.sniper` | Winning the duel unlocks everyone else |
| Headshot ratio | `headshots`, `headshots_hit` | Execution quality, independent of outcome |
| Damage / min | `dapm` | Chip damage counts, weighted low |
| Deaths / min | `deaths`, time | A dead Sniper holds nothing |
| Medic picks, timed | `rounds[].events` `medic_death` | When in the round you took their Medic, and whether it was before their uber |
| Assists | `classkillassists` | Damage that set up a teammate's kill |

**Correction from v0.3:** `rounds[].events` only contains `pointcap`, `charge`, `drop`, `medic_death` and `round_win`. Medic deaths are the **only** kills with a timestamp and a killer. So "picks immediately before an uber push" is computable from the log for Medic picks specifically, but a general time-to-first-pick needs the demo. It moves to M4.

Logs also carry `has*` capability flags per log: 2014 logs predate airshot tracking, some lack accuracy. A stat whose flag is off is stored as **missing, not zero** — otherwise old matches would read as "never landed an airshot" and drag every average down.

### Two traps in logs.tf round data (found in M2)

Both affected almost the whole history, and both hid behind single-round logs, where they are invisible:

1. **Event times are seconds since the log started, not since the round started.** Round 1 starts at zero, so it looks right; every later round is offset by its start. Affected 740 of 759 matches. Each round is now calibrated off its own `round_win` event, which lands exactly at `start + length`.
2. **Stopwatch swaps team colours between halves, and the log writes each round in that round's colours.** A raw `winner: "Blue"` can mean either team. The log records each player's colour per round in `rounds[].players[id].team`; normalization maps every round back to the **stable teams** (a player's overall team) by majority vote, and records `colours_swapped`. Affected 508 of 2,540 rounds across 342 matches — before the fix, the round view mislabelled three of six rounds in the S36 official against TWS.

3. **Round `start_time`s are the game server's local clock, written as if UTC** (found in M4). A CEST server's rounds read two hours early. Invisible until something else — a demo file — has a real timestamp to compare against. Measured offsets: 318 logs at +0h, 169 at +1h, 208 at +2h. The fix anchors each log to a true-UTC moment just after the match ended — its own upload, or for a combined log (often uploaded days later) the upload of its last per-round part — and takes `anchor − raw end` floored to a whole hour. Residual after the offset: 16 s median, 19 s at the 90th percentile.

Also worth knowing: in stopwatch the match score is not rounds won. That official is 4–2 by ETF2L's scoring while the round split is 3–3.

### Model v1 (M3): percentiles against the players you face

v0 (M2) was a generic absolute formula, kept only until real class models existed. v1 replaces it.

**The baseline is the other players in your own matches.** Every stored Highlander log holds 17 other players, so each class already has a pool of ~1,500 performances with no crawling. **You are excluded from it**: 663 of the 1,510 Sniper performances are yours, and rating you against a pool that is 44% you would drag your median to 50 by construction. So a Sniper rating of 72 means "better than 72% of the Sniper performances you have faced". This settles the "self vs division vs global" baseline question for now: it is division-ish, free, and grows with every sync.

**How a rating is built:**

1. A *performance* is one player's time on their **main class** in one match, if it is at least 5 minutes. Only the main class is rated, because `classkills` is recorded per player, not per class played; a flexer's kills cannot be split honestly.
2. Each **component** (impact kills, sniper duel, deaths, ...) is looked up as a mid-rank percentile in that class's pool. Lower-is-better components (deaths, drops) are flipped, so higher is always better.
3. The rating is the **weighted average of the percentiles**, 0–100. A component the log did not record (headshots on an old log) is dropped and the weights renormalised, never scored as zero.

Class models: **Sniper** (impact kills 30%, duel 20%, medic picks 15%, deaths 15%, headshot share 10%, damage 5%, assists 5%), **Medic** (healing, ubers, drops, deaths, assists), **Spy** (impact kills, backstabs, deaths, damage, assists), and a **generic** model for the other six classes (impact kills, damage, deaths, assists, caps). All in `weights.default.toml`, overridable, validated on load so a typo is an error rather than a silently missing component.

Rating runs as its own pass at the end of every sync and every rebuild: all 758 matches, 13,641 performances, in about 3 seconds.

**What it says about you, on first run (663 Sniper games):**

- Career 49, recent form 45 (−3.6 on the 20 games before): a median Sniper against the Snipers you face, currently in a dip.
- **The Sniper duel is the weakest component**: 3,069 kills to 3,568 deaths against enemy Snipers over your career (−499), lowest percentile of anything in your rating. Deaths are the strongest: you die less than the typical Sniper you face.

**What it still does not do:** v1 measures individual execution, which is what you asked for, not who won. In the S36 official against TWS, a 4–2 stopwatch win, v1 still gives the opponents five of nine matchups (three even, one to you). That is a true reading, not a bug: stopwatch is decided on push time, and a team can win it while being out-played class for class. The match page says so explicitly.

The **head-to-head** column remains different in kind: kills between the two players on a class, read straight from `classkills`, no model involved.

### Demos (M4)

**Index.** `tf/`, `tf/demos` and `tf/demos/stv` are scanned for `.dem` files at startup and after every sync; only the 1072-byte header is read (0.6 s for 101 demos). Demo Support `.json` sidecars supply 883 tick-stamped killstreak markers.

**When a demo started.** File modified time minus the demo's own duration, which needs no timezone. Checked against the Demo Support filename timestamp on 95 real demos: median disagreement 0.9 s. (A downloaded STV demo's modified time is the download time, so its start comes from its demos.tf upload time instead, and its jumps are flagged approximate.)

**Linking.** Same map (or the demo's base map name inside a multi-map label like `upward + steel`, or a log with no map on time alone at a stricter threshold), and the demo and the log overlap for at least half of the shorter of the two. One demo can hold several matches and one match can span several demos. Result on this machine: 25 of 101 demos linked, to 23 matches. The unlinked rest are pubs (badwater, pier, borneo), MvM, reviews of other people's games, reconnect fragments, and sessions with no logs.tf log containing the owner.

An earlier approach — fitting the hour offset per demo — was tried and rejected: with a ±12 h search, a short log "fits" inside a long demo at a nonsense offset. The anchor method has no search at all.

**Jumping.** TF2 cannot open a demo and seek in one console line (`demo_gototick` runs before the demo has loaded), so the flow is two steps: copy `playdemo <demo>` once, then click any timeline marker to copy its `demo_gototick <tick>`. Jumps land 5 s early to show the lead-up. Killstreak markers that fall outside every round (pre-match warmup) are left out rather than forced into round 1.

**STV.** demos.tf holds SourceTV demos for 1,047 of these matches. The match page offers a download into `tf/demos/stv` and links the result by demos.tf id. Caveat: a stopwatch match's combined log usually spans several STV demos (one per half), and trends.tf links one of them, so an STV demo often covers part of the match. **Not yet exercised against a real download** — see open items.

### Officials, scrims and pugs (M5)

**ETF2L.** Every sync looks the owner up by SteamID (`/player/{steamid64}`), pages through their results, and fetches each Highlander match: competition, division and tier, week, both clans, ETF2L's score, and **both rosters** (every registered player's SteamID and team; mercs appear with no team). ETF2L publishes a limit of 60 requests a minute; the client runs at one per 1.5 s and honours `Retry-After`. Matches are refetched only while they are under two weeks old when last fetched.

**Officials.** trends.tf tags most. The rest are found by roster: a log within −4 h/+10 h of the scheduled time where the owner's side holds five or more of one clan's roster and the other side five or more of the other's. On this account that rule matched trends.tf on **all 35** officials trends.tf tagged, without a single disagreement, and found **7 more** (16 logs: the 2018 seasons with Undercover Prodigies, bik is ti and Amicitia, and one 2023 Open match). The pre-official warm-up on the same server is correctly left out: its other side is not the opponent's roster. Rosters also say which side the owner played for, so a merc appearance is attributed to the team actually played for.

**Scrims vs pugs.** A *regular* is a teammate who was on the owner's side at least six times within 45 days either way. A game with five or more regulars (of eight) is a team game. The split is sharp: 560 games have five or more, 150 have two or fewer, 14 sit between. A second rule covers what regulars cannot: the first scrims with a newly joined team, before anyone has six games. If five or more of the owner's side are on the owner's own ETF2L roster (from an official within 150 days), it is a scrim. That caught four DD14 scrims in the week after joining.

**Naming.** A scrim's team is the owner's clan from the nearest official whose roster holds four or more of the side. The opponent is named the same way from any official's roster, both sides. 505 of 538 scrims get a team name; 124 get an opponent.

Result on this account: **58 officials, 544 scrims, 156 pugs**. The context pass has no network, runs at startup, after every sync and on rebuild, and takes well under a second.

**What it says:** Sniper rating 52 in officials (51 games, 70% won), 49 in scrims (532), 46 in pugs (80). The profile can be filtered to any of the three; its career records (duel, medic picks) are hidden while filtered because they count every game.

**Teammates.** Per ETF2L team: games, officials, record, the owner's average rating, and the nine most frequent teammates. Per teammate with five or more shared games: games, officials, record, first and last game together, their usual class, and the owner's average rating with them against the owner's other games. That comparison needs ten rated games on each side and is labelled as a correlation: it says who you played well alongside, not who made you play well.

### Raw logs (M6)

**Fetch.** `logs.tf/logs/log_<id>.log.zip` for every kept Highlander log, stored verbatim as a source (80 MB compressed for 740 logs). It served files back to 2014. One request per second, newest first; the first full fetch took 18 minutes. Two logs have no raw file. After about 740 requests logs.tf stopped answering entirely, so the last 16 (the oldest, 2014) are left for the next sync to retry.

**What counts.** Checked against logs.tf's own summary of the same files:
- A kill counts only **inside a round**: after `Round_Start`, before `Round_Win`, `Round_Stalemate` or `Game_Over`. Pre-game and humiliation kills are in the log but not in logs.tf's totals. Missing the stalemate rule put five logs one kill out.
- **Dead Ringer feign deaths** have a kill line but are not kills. logs.tf does credit the **assist** on one, so assists count inside a round, feign or not.
- The victim's class is not on the kill line. It is tracked from each player's latest `spawned as` or `changed role to`. 2014 logs write `Heavy`, not `heavyweapons`.
- An assist line sometimes lands a second after its kill.

Result: on all 740 logs, **every player's kills, assists and kills by victim class match logs.tf exactly** (`hl rawlogs --check`). That is 226,784 kill lines.

**Trap 4: the raw clock.** Raw timestamps are the game server's local time. logs.tf's round times are a whole number of hours away from them: 2 hours on 387 logs, 1 on 344, 0 on 9. The shift is found by matching each `Round_Start` line to logs.tf's rounds. Under the right hour, **all 2,474 rounds match to the second**. Kills are stored shifted into logs.tf's frame, so they fall into rounds and onto demo ticks like every other event.

**A bug it exposed.** logs.tf's `classkillassists` is kills *plus* assists per victim class, not assists. Since M3, "impact assists" counted every kill a second time. Fixed at normalization, so logs without a raw log are right too. For a Sniper, raw impact assists fall from about 8.3 to 1.0 per 10 min, which is what the assist counts say.

**Kills valued one by one.** With a raw log, impact kills and impact assists sum each kill's own value. That value depends on the victim's class, the map, and whether the victim was **defending**. Defending means RED on an attack/defence map: every payload map plus the control-point maps listed in `[attack_defend]` (Steel, Gravel Pit and others). Colours are read at the moment of the kill, so stopwatch halves are right. Lookup order: map and side, then map, then side, then the general value. Defaults: a defending Engineer is worth 1.6 and a defending Scout 1.0. Both are proposed, not agreed (see "Victim values v2"). No per-map values are set yet. Without a raw log, impact falls back to `classkills` at the general values; on a symmetric map the two agree exactly.

**Effect on this account:** small. Sniper career stays at 48.8, and recent form goes from 45.2 to 45.5. Impact kills barely move. The corrected impact assists move from the 44th to the 50th percentile.

**On the match page**, your own kills (green, from the bottom edge) and deaths (red, from the top) now sit on every round's track. Each says who, what class, headshot or not, and the weapon. With a demo linked, every one of them copies its `demo_gototick`: 67 jumpable moments on the TWS official. Deaths from suicides and fall damage have no killer line, so the timeline shows 28 of the 31 deaths logs.tf counts there.

**Stored but not used yet:** positions of both players on every kill, and chat. They feed M7's kill map and play-by-play.

**Not done from the M6 list:** time to first pick, and picks before an uber push. Both are now computable from stored kills and ubers.

### Still deliberately deferred

- **Fitted weights.** Hand-set now; regress round outcome on components once there is reason to trust a fit.
- **Opponent strength.** Every performance counts equally in the pool today; a lobby Sniper and an ETF2L High Sniper weigh the same. ETF2L division context (M5) is the fix.
- **Cross-class comparability.** A Sniper 60 and an Engineer 60 are each "60th percentile of their class's pool" — comparable in meaning, but the pools differ in who is in them.

---

## 4. Data model

Changes from v0.2 in **bold**.

```sql
-- SOURCE OF TRUTH (never deleted; everything else derives from these)
log_raw(log_id INTEGER PK, fetched_at, json TEXT)
trends_raw(log_id INTEGER PK, fetched_at, json TEXT)          -- the index row
etf2l_raw(kind, id, fetched_at, json TEXT, PK(kind, id))

-- IDENTITY
player(account_id PK, steamid64, steamid3, display_name, is_me,
       etf2l_id, etf2l_div, updated_at)

-- MATCH
match(log_id PK, map, gamemode, played_at, duration_s,
      blue_score, red_score, title, uploader,
      format,                    -- highlander | sixes | other
      league,                    -- etf2l | null
      etf2l_match_id,            -- links to ETF2L context
      demos_tf_id,               -- STV demo, when one exists
      duplicate_of,              -- non-null: never count in aggregates
      classified_by)             -- trends | heuristic | manual

match_round(log_id, round_num, start_s, length_s, winner, firstcap, ...)
match_round_event(log_id, round_num, at_s, kind, actor, target, extra)

match_player(log_id, account_id, team, kills, deaths, assists, dmg, dmg_real,
             dt, hr, heal, ubers, drops, headshots, headshots_hit, backstabs,
             medkits, medkits_hp, sentries, cpc, ic, longest_killstreak, ...)
match_player_class(log_id, account_id, class, time_s, kills, assists, deaths, dmg)
match_player_weapon(log_id, account_id, class, weapon, kills, dmg, shots, hits)

match_class_kills(log_id, account_id, victim_class, kills)
match_class_deaths(log_id, account_id, killer_class, deaths)
match_class_assists(log_id, account_id, victim_class, assists)

match_medic(log_id, account_id, advantages_lost, biggest_advantage_lost_s,
            deaths_with_95_uber, avg_time_to_build_s, avg_uber_length_s, ...)
heal_spread(log_id, healer, target, heal)

-- DEMOS
demo(id PK, source, path, file_hash, map, server, recorder_nick, ticks,
     duration_s, recorded_at, kind, demos_tf_id, indexed_at)
demo_event(demo_id, at_tick, kind, value)       -- from the .json sidecars
demo_link(demo_id, log_id, confidence, method, tick_offset, PK(demo_id, log_id))

-- CONTEXT (M5; etf2l_raw above is the source, these are derived)
etf2l_match(match_id PK, competition, comp_type, division, tier, week, round, time,
            clan1_id, clan1_name, clan2_id, clan2_name, r1, r2, default_win, maps)
etf2l_roster(match_id, account_id, team_id, name)     -- team_id NULL for mercs
match_context(log_id PK, kind,                        -- official | scrim | pug
              etf2l_match_id, link_method,            -- trends | roster
              team_id, team_name, opp_team_id, opp_team_name, regulars)

-- DERIVED (droppable, rebuildable with one command)
class_value(log_id, account_id, class, engine_version, score, components JSON)
matchup(log_id, class, blue_account, red_account, blue_value, red_value, diff)
baseline(class, gamemode, stat, n, mean, sd, p10, p25, p50, p75, p90)

-- PLUMBING
sync_state(source PK, cursor, last_run_at, last_error)
app_config(key PK, value)
```

As built in M1, the index lives in `log_index` with a derived `superseded_by` column. Every aggregate query must filter `superseded_by IS NULL`; 480 superseded parts would otherwise inflate over a third of the history.

**Never edit an applied migration** — not even a comment. sqlx checksums them and refuses to open a database whose history no longer matches. `.gitattributes` pins `*.sql` to LF for the same reason: a CRLF checkout on Windows changes the bytes.

---

## 5. Ingest

```
1. trends.tf index   ->  what matches exist, what they are, what they link to
2. logs.tf detail    ->  full JSON for non-duplicate Highlander logs
3. normalize         ->  a separate pass over stored blobs
4. ETF2L context     ->  division/season for logs with an etf2l matchid
5. demos             ->  local folder scan; demos.tf by demoid on demand
```

- Self-throttle every source to ~1 req/s. None publishes a rate limit; behave as if they do.
- Store raw, normalize separately. `sync` and `reprocess` stay different commands.
- Incremental by `updated_since` / highest log id; full backfill is an explicit action.
- Manual add by URL or id — the fastest way to test anything.

---

## 6. The match page

Two phases, because the demo is not ready when you want to look.

**Phase 1 — instant, from the log:**
- Nine class matchups, won or lost, with the decisive ones called out
- Your class value score, and the same for all 17 other players
- Round timeline with caps, ubers and picks
- Impact-kill breakdown: who you killed, and what it was worth

**Phase 2 — once a demo is linked:**
- Per-player detail from the demo where available
- Jump-back: `playdemo <name>; demo_gototick <tick>` to any moment
- Sidecar killstreak markers as timeline pins

POV demos only contain what your client received, so phase 2 is you-only for local demos. With an STV demo from demos.tf — available for 1,047 of your matches — it covers all 18 players. That difference is why STV is worth pulling.

---

## 7. Milestones

| | | |
|---|---|---|
| **M0** | Skeleton, database, first-run setup | **done** |
| **M1** | trends.tf index + logs.tf sync + normalize; match list | **done** |
| **M2** | Match page phase 1: matchups, round timeline, box score | **done** |
| **M3** | Rating v1: Sniper in full, other classes generic; profile page | **done** |
| **M4** | Demos: local index, demos.tf fetch by demoid, linking, jump-back | **done** |
| **M5** | ETF2L context, officials vs scrims split, teammate tracking | **done** |
| **M6** | Raw logs: every kill with time, classes and positions (§9); per-side victim values | **done** |
| **M7** | more.tf-style match views: kill map, heatmaps, damage and kill spread, timeline, play-by-play (§9) | **done** |
| **M8** | Maps per round: resolve combined logs to the map each round was played on (§10) | **done** |
| **M9** | Rating update from Highlander theory: game state, pick context, uber timing, per-map values (§11, for deliberation) | |
| **M10** | Sniper rating v3 and v4 from HLTV's lessons: death context, Fight KAST, situation-valued kills, map and side baselines, fight swing (§12) | |
| **R1** | Public test release: first-run flow (TF2 folder optional, first sync step), TF2-styled theme and original logo, release workflow, README for testers | **done** |
| **M11** | Queue §13: filters bug, database backups, per-class models, opponent strength, themes | |
| **v2** | Deep demo parse (§14): aim and viewangles, crosshair placement, reaction time, scoped share | |

M1 acceptance: every Highlander log on the account stored, classified, deduplicated, and rebuildable from raw blobs with no refetching.

---

## 8. Still open

1. ~~Baselines~~ — settled in M3: the other players in your own matches, with you excluded.
2. **Final impact weights** — the TOML above is a first guess; expect to argue with it. Victim values v2 (Pyro 1.5, Spy 1.3, Scout 1.15 above Engineer) are applied; per-map and per-side values are designed in §3.
3. **Linux demos** — a second machine holds more POV demos. Import path to be designed; the `demoid` route may make it unnecessary.
4. **Sixes** — detected and stored, excluded from ratings. A later update.
5. **Verify jump ticks in-game.** The arithmetic is tested end to end (a sidecar killstreak at raw tick 51,212 lands at 50,879 after the 5 s lead), but only TF2 can confirm the demo shows the right moment.
6. ~~**One real STV download.**~~ Done (20 Sep 2026): 109 MB fetched for log 4122234 in 29 s, linked, and read into 325 routes over all 18 players. A match with both demos now uses the POV for aim and the STV for movement, so neither is counted twice.
7. **File watcher.** Demos are rescanned at startup, after every sync, and on demand; a live `notify` watcher was planned and is not built.
8. ~~**Opponent strength.**~~ **Answered in Q9, and not the way this assumed.** ETF2L division labels 5% of logs and barely varies; the opposite number's own record covers 64% and varies properly. Weighting the pool by it was measured and rejected — it does not make the rating a better measure of the player. It is shown on the profile instead.
9. **Teams with no officials.** Scrims are named from official rosters, so a team that never played an official (2 Blacked Up, March–August 2026) stays unnamed. The player's ETF2L transfer history (`/player/{id}/transfers`) could fill the gap; it is incomplete for older teams.
10. ~~**logs.tf-only combined logs.**~~ Fixed in M8 (§10): parts found by their rounds.
    Was: **logs.tf-only combined logs.** Dedupe relies on trends.tf's `duplicate_of`. A combined log that only logs.tf knows (the 2018 S16 semi-final: `gullywash + badwater` plus both single-map logs) is counted alongside its parts.
11. **Raw logs still to fetch.** 16 of the oldest logs timed out when logs.tf stopped answering; the next sync retries them.
12. **Defending values.** Engineer 1.6 and Scout 1.0 on defence are proposed, not agreed. Worth checking with function, along with the first per-map values.
13. **Time to first pick and picks before an uber push.** Planned for M6, not built; the data is stored.
14. **Midfights and the uber split** as rating inputs (from M7's list): computable, not built.
15. **Hit cap date.** logs.tf's 450 cap started somewhere between December 2014 and June 2016; this account has no logs in that window to pin it down.
16. ~~**Combined logs span several maps.**~~ Resolved round by round in M8 (§10).
17. ~~**Parts of combined logs not yet fetched.**~~ All 289 fetched over a VPN; the exact matches confirmed every earlier answer.
19. **logs.tf rate limit.** It stopped answering twice, after about 750 requests and then about 300 at one per second. Its API is now paced at one request every 2 seconds, and bulk jobs (raw logs, parts) are capped at 100 per sync. 15 of the oldest raw logs are still to fetch.
21. **Uber and drop counts.** The game state matches logs.tf's uber count for 88% of Medics and drops for 93%. The rest are one off under a rule logs.tf does not publish. The state's own counts come straight from the log's lines.
22. **Fights as rating inputs.** Opening duels, traded kills and picks into a charge are measured and shown but do not feed the rating yet (§11 B and D).
23. **Season spans for seasons you did not play.** They need ETF2L match times per competition, 20 per request. A one-off fetch, cached, would give exact spans for every season.
20. **The next rating update.** Items 2, 8, 12, 13 and 14 are expanded in §11, with Highlander theory from the wikis and guides, what the raw logs can measure, and questions for function.
18. **Demo linking per map.** A combined log's demo is still linked by time or label; linking each map segment on its real map is not built.

---

## 9. Learning from more.tf

more.tf (1.47 million matches parsed) reads the **raw server log** behind each logs.tf page, not just the logs.tf summary we use. That is why it can show where every kill happened. Checked on our own `pro vs noob scrim` (log 4121291, Swiftwater, 15 Sep 2026) through its page and its `/api/log/<id>` response.

### What its data has that ours does not

The logs.tf summary gives totals per player, plus timed events only for caps, ubers, drops and Medic deaths. more.tf's parse of the raw log adds:

- **Every kill:** unix timestamp, killer and victim SteamID and class, weapon, and **the killer's and victim's x/y/z position**. That is 418 kills in this match.
- **Per round, per player:** kills, deaths, assists, damage, heals, charges, time alive, and who each Medic healed.
- **Per player:** kills, deaths and damage split by the other player and by the other class (a full who-killed-whom matrix). Also kills, deaths, damage and heals in 10-second intervals.
- **Ubers:** start and end, length, deaths during and after, and high damage taken during.
- **Per round:** first blood, first cap and who capped, and team kills, damage, ubers and drops.
- **Deaths relative to uber:** each player's deaths before, during and after their Medic's uber.
- **Killstreaks with their victims**, and **chat**.

Timestamps are absolute unix times, so they need the same server-clock correction as round times (§3, trap 3).

**Why this matters for the rating:**
- **Per-side victim values** (§3) become possible without demos. Each kill has a time, so it falls in a round, and each round has a side.
- **Time to first pick** and **picks before an uber push**, which M3 deferred to demos.
- **Kill position** gives kill distance, and "was the Sniper holding a sightline or out of position when they died".
- It does all this for all 18 players in every match, back to 2014, against 101 local demos.

**Source decision.** Parse logs.tf's raw log ourselves, store it verbatim like every other source, and treat more.tf's JSON as a reference to check our parser against, not a dependency. Its API is undocumented, and our app must keep working if it changes. **Verify first** that logs.tf still serves raw logs for old matches, and at what size and rate.

### The screenshots, feature by feature

**1. Charts tab.** A player list on the left, grouped by team with class icons, drives every panel. One player is selected at a time.
- **Damage spread:** diverging bars per enemy class. Damage taken extends left in red, damage dealt extends right in blue, and the class icon sits in the middle. This shows, for example, that a Sniper's damage went mostly to Scout and Heavy while the Demoman did most of the damage to them.
- **Kill spread:** the same layout for deaths to and kills on each class. This is our `classkills`/`classdeaths` data, which we already store, drawn per class.
- **Kill map:** the map overview with a dot per event. A green dot is a kill, placed at the victim. A red dot is a death. A yellow ring is where the shooter stood, joined to its dot by a line. Counts show in the header. Click to expand.
- **Kills heatmap and deaths heatmap:** a density glow over the overview where the player got kills, and where they died.

**2. Kill map, expanded.** The same kill map, full size, for one player (here, 39 kills and 28 deaths).
- An enemy filter ("All Enemies" or a single player) shows one duel, such as flashy against the enemy Sniper.
- A strip at the bottom shows a bar for every kill (green) and death (red) across the match's 39:44, so bursts and droughts stand out at a glance.

**3. Timeline tab.** Cumulative kills per player over match time, one stepped line each.
- Switches for Kills, Deaths, Damage and Heals.
- A legend per team with each player's final total.
- A hover crosshair lists every player's running total at that moment, ranked.
- Time before the match (warmup) shows as negative minutes.

**4. Play-by-play tab.** The match as a feed, with filters for Kills, Ubers, Caps, Chat and Streaks.
- Kills read "killer → victim" with class icons and the weapon.
- Ubers show who popped and the length in seconds.
- Caps show the team and the players who capped.
- Chat shows the lines themselves.
- Killstreaks show the length and every victim.
- Rows are timestamped, with the pre-match period marked before "Game starts".

**Also on the log page** (from its text, not screenshotted):
- A box score with damage per minute, damage taken per minute, KA/D and K/D.
- Team totals, including **midfights won**.
- A round table: length, kills per team, ubers per team, damage per team and **midfight winner**.
- A Medic panel: charges by medigun type, average time to build, average time before using, deaths near full charge, average uber length, **major advantages lost** and the biggest one, **crossbow healing**, and a heal-target table.
- A class-vs-class matrix of kills, kills plus assists, and deaths.
- A round selector that filters the page to one round.

**Site-wide:** profiles with win rate per map, class stats, teammates, and recent activity. Also seasonal player cards, weekly season summaries that compare you with peers, and leaderboards.

### What we build, and where it goes

We already have: class matchups, the round timeline with caps, ubers, drops and Medic picks, the box score, demo jump-back, teammates, and officials vs scrims. more.tf has no rating, no matchup view, no demo jumping and no ETF2L context, so the aim is to add its views inside our matchup-first match page, not to copy it.

**M6: raw logs.**
- Download and store each raw log.
- Parse kills, damage, heals, ubers, caps and chat, with positions.
- Correct times with the log clock.
- Test the parser against more.tf's numbers on the same logs.
- Then per-side victim values, time to first pick, and picks before an uber push.

**M7: the views.** In order of use to a Sniper main:
1. **Kill map with the duel filter.** Your kills and deaths on the map overview, filterable to one enemy (their Sniper). Clicking a dot jumps to that moment in the demo, which more.tf cannot do.
2. **Play-by-play**, merged into our round timeline: every kill as a marker, each one jumpable.
3. **Damage spread and kill spread** per player, in the diverging layout.
4. **Timeline chart** (kills, deaths, damage, heals).
5. **Heatmaps.**
6. **Midfight winner** per round, and the **deaths before, during and after uber** split, as new rating inputs for every class.

**Needed for the maps:** ~~an overview image per map~~. Not needed after all: see "As built (M7)".

**Map images (after M8).** Initially, for the 12 maps played on this account that more.tf covers (Upward, Vigil, Product, Ashville, Steel, Proot, Proplant, Gullywash, Cascade, Process, Swiftwater, Bagel), the kill map drew more.tf's overview image, downloaded with the owner's permission to `<app data>/overviews/<map>.png`. At that point the images were **not in the repo**: they were more.tf's renders, kept on this machine only. Additional overview images from demos.tf are Aim Arena Reloaded, Airfield, Airfusion, Badlands, Badwater, Ballin Sky, Biohazard, Borneo, Cardinal, Caverns, Clearcut, Coalplant, Ethic, Granary, Gravelpit, Kalinka, Killbox KBH 2P, Lakeside, Lockdown, Lostarena, Lostvillage, Lostvillage Two, Metalworks, Millstone, Ramjam, Reckoner, Resident CU, Snakewater, Sultry, Sunshine, Tigcrik, Ultiduo Baloo, Ultiduo Grove, Ultiduo R, Vanguard, Viaduct, Villa and Warmtic.
- **Placement:** the original square images use more.tf's per-map transform: each covers 1024 × `scale` game units, centred on (`x` + 910·`scale`, `y` − 512·`scale`). Rectangular images use the map world bounds supplied by demos.tf. Alloy does not work from this dataset.
- **Checked:** 94–99% of stored kill positions land on the drawn map. On Upward, Sniper firing positions sit on balconies and cliff edges. Every kill mark in the app lies within 0.001 px of more.tf's own formula.
- **Look:** the image sits under a 45% dark veil so blue and orange stay readable on sand, and crosses get a dark outline.
- **Fallback:** maps with no image (Lakeside, Warmtic, Govan and others) keep the outline drawn from kill positions. Adding an image for another map needs its `scale`, `x` and `y` in `overview.rs`.

### As built (M7)

A "Kill by kill" section on every match page, read from the stored raw log on demand in about 40 ms, so nothing new is stored. One filter row (player, round) scopes four views.

**Maps drawn from data, not images.** Every stored kill records where both players stood. On Upward that is 38,506 positions from 94 matches, and binned top-down into a 180-cell grid they trace the map: buildings show as gaps, and the cart route and chokes as the densest cells. Versions share one outline (`pl_upward_f10` and `_f12` are both "upward"). Cells seen only once are dropped as strays. A map with fewer than 400 positions has no outline; its kills are drawn on their own frame. This needs no third-party assets and works for any map that has been played enough.

**Kill map.** Kills are blue dots where the victim fell; deaths are orange crosses where the player fell. A white ring marks where the shooter stood, joined by a line. Your Sniper spots show up as clusters of rings. You can filter to one enemy (the duel view) or one round. Hovering shows who, classes, weapon, headshot, and distance in game units. Clicking copies the `demo_gototick`. A strip below places every kill and death on the match's time axis.

**Heatmaps.** "Where you got kills" (your position when you got them) or "where you died", for this match or **across every stored match on the map**. For Upward that is 94 matches and 1,040 deaths. A single hue with no floor: a floor lit every cell anyone had died in and drowned the hot spots. A scale legend shows fewer to more.

**Play-by-play.** Kills (from the raw log), ubers, drops and caps (from logs.tf), chat, and killstreaks (three or more kills without dying), grouped by round. You can filter by type or to rows involving the chosen player. Rows with a demo copy their tick.

**Damage and kills by class.** Back-to-back bars per enemy class: taken or deaths on the left in orange, dealt or kills on the right in blue, on one scale for both sides.

**Timeline.** Running kills, deaths or damage per player on game time, with the gaps between rounds removed. It is an emphasis chart: the chosen player is the accent line, their team light grey, the other team darker grey. Hovering ranks all 18 players at that moment; clicking a line picks that player for every view.

**Colour.** Blue `#5791c8` means you hurt them and orange `#d6763a` means they hurt you, in every view. The pair passes the dataviz validator on the dark surface (colour-blind ΔE 19.7). The obvious green and red pair failed it (ΔE 3.0 for deuteranopes). Shape or side always backs the colour up, and every view has a table.

**Damage, checked against logs.tf:**
- Damage counts only inside a round.
- **logs.tf caps each hit at 450.** A backstab logs six times the victim's health, so without the cap a Spy's damage doubles. The cap arrived between December 2014 and June 2016: all 25 older logs match only uncapped, and all 712 newer ones only capped. The switch is placed at the midpoint.
- With both rules, damage dealt matches logs.tf exactly on every player of all 740 logs.
- Damage *taken* does not always match logs.tf on combined logs; the view says so.

**Not done from the M7 list:**
- Midfight winner per round.
- Deaths before, during and after uber as rating inputs.

Both are computable from what is now stored and belong with the next rating update, not the views.

---

## 10. Combined logs across several maps (M8)

### The problem

After a scrim or official, people combine the per-map logs into one and name it anything: `proot + proplant`, `vigilx2`, `upw/casc`, `how did we win`, `русские не победили :(`. That name is all logs.tf keeps as the map. The example: **SBQRRA vs Champions of Light**, Highlander Season 33 Low grand final (ETF2L 6–3). Log 3863290 is one log with 17 rounds across Ashville, Vigil and Proot, and its map field reads `русские не победили :(`.

Measured on this account:
- 758 kept Highlander logs, **128 with no real map name** (95 free text, 33 empty).
- **125** of those 128 list their parts on trends.tf (`duplicate_of`), and the parts have real map names.
- **78 span more than one map**. Every one of the 78 has a free-text name; not a single multi-map log carries a usable map.
- 22 of the 128 are officials.

**What breaks today:**
- **The kill map** for a multi-map log mixes positions from different maps on one frame.
- **Map outlines and career heatmaps** leave these logs out entirely: kills are matched to maps by the log's map name. Officials are the matches most often combined, so the best data is the data missing.
- **Victim values** get no attack/defence side and no per-map value, because the log has no map. Vigil rounds inside a combined official get no defending Engineer bonus.
- **The match list and header** show the free text instead of the maps.
- **Demo linking** matches such logs on time alone (the `nomap` method, stricter threshold) or a guess from the label.
- Future per-map statistics (win rate per map, rating per map) need the map of every round.

### The answer: a map for every round, from the evidence that exists

A combined log is still one match: it is what ETF2L's result refers to. So the log stays the unit, and each **round** gets a map. From ordered per-round maps come **segments** (consecutive rounds on one map), and each segment gets its own score.

Evidence, strongest first. Each round takes the first source that gives an answer, and the rest are used as checks.

1. **The raw log's own map lines.** Newer uploads write `World triggered "meta_data" (map "pl_vigil_rc10")` at each map load; every round after one belongs to that map. Direct, but older logs lack it (none of the four combined logs sampled have it).

2. **The parts, matched exactly.** trends.tf lists every part the combined log was built from, each with its real map. The combiner copies rounds verbatim, so a part's round start times are the combined log's round start times, to the second, in the same clock.
   - Fetch each part's logs.tf JSON once and keep it as a source. That is a few hundred requests for the whole history, about 8 minutes at one per second, then only new combined logs.
   - Every round then belongs to exactly one part. No thresholds, no guessing.
   - Parts can themselves be combined logs with no map (in the example, part `3863187`, named `ashville`, is itself a combine of the two Ashville parts). Resolve them recursively through their own parts.

3. **The parts, matched by time, with nothing fetched.** trends.tf gives each part its upload time and length, so each part covers a window of real time. A combined log's rounds are placed on real time by the M4 log clock. Tried on the example, this assigns **13 of 17 rounds** correctly and leaves 4 unplaced where windows leave gaps. That makes it a useful fallback, but not good enough to be the main method.

4. **ETF2L's map list** for officials, in the order played: `ashville, vigil, proot` for the example. It checks that the resolved sequence of maps matches, and fills gaps. An unplaced round between two Ashville rounds is Ashville.

5. **The name.** `proot + proplant`, `upward+proot`, `vigilx2` and `upw/casc` give the maps in order. Tokens are matched against map names already seen on this account: `upw` is the only map beginning with those letters, and `x2` means the same map twice. This gives the order but not the round boundaries, so it pairs with 7.

6. **The kills themselves.** Every round's kills carry positions, and M7 already has an outline for every map played enough. Score each candidate map by the share of the round's kill positions that land on its outline. Ashville and Vigil don't overlap, so the score separates them sharply. This works with no metadata at all, fills any round the other sources leave open, and checks the rest. It needs an outline, so a map played only once or twice may not have one.

7. **Neighbours and gaps.** Maps come in unbroken blocks, and changing map takes minutes, while rounds on one map follow each other in seconds. A long gap between rounds marks a likely map change. An unplaced round between two rounds of the same map takes that map.

For the 3 logs with no parts and no ETF2L match (for example `gullywash + badwater` from 2018), 5, 6 and 7 together still give an answer.

### What gets stored

Both tables are **derived**, rebuilt on reprocess from sources already stored plus the fetched part JSON:
- `round_map(log_id, round_num, map, source)`, where `source` is `meta`, `part`, `window`, `etf2l`, `name` or `geometry`, so every answer says where it came from.
- `log_segment(log_id, seq, map, first_round, last_round, red_score, blue_score)`: the per-map sub-results.

A log with one real map name gets one segment and needs no work.

### What changes when it lands

- **Match list and header:** `Ashville · Vigil · Proot` with a score per map, instead of the free text.
- **Kill by kill:** a map switch above the kill map, one tab per segment, each on its own outline.
- **Map outlines and career heatmaps** gain every combined log's kills, officials included.
- **Victim values:** the side and per-map values apply per round, so Vigil in a combined official counts defending kills like any other Vigil.
- **Demo linking** can match each segment on its real map.
- **Per-map statistics** become possible: win rate, rating and duel record per map.

### A fix that falls out: unflagged parts

Open item 10 (logs.tf-only combined logs counted alongside their parts) has the same cure. When every round of one kept log appears, start time for start time, inside another kept log, the first is a part of the second, even when trends.tf never said so. Supersede it. On this account that catches the 2018 `gullywash + badwater` semi-final and its two single-map logs.

### How it will be checked

- The 78 multi-map logs with parts give ground truth once method 2 has run: every round's map known exactly.
- Methods 3, 5 and 6 are then scored against it, round by round, before any of them is trusted on the 3 logs without parts.
- ETF2L's map order is checked against the resolved segments on all 22 combined officials.
- The example must come out as Ashville 6 rounds, Vigil 4, Proot 7, and match ETF2L's 6–3.

### One decision for you

Should a combined log be **rated once** (as now, over all its maps), or **once per map segment**? Per segment is closer to how a match is played: a bad Vigil and a great Proot are two different stories, and per-map baselines want it. But it multiplies the rating rows for those logs, and a short segment can fall under the 5-minute minimum. **Recommendation:** keep rating per log for now, show the per-map breakdown, and switch when per-map baselines arrive.

**Decided:** rate per log for now, as recommended.

### As built (M8)

Every round of every kept Highlander log has a map in `round_map`, and each log's maps in play order are in `log_segment` with rounds won per map. The pass runs at startup, after every sync (after the demo index, which puts each log on the real clock), and on rebuild. It takes about 3 seconds for 2,535 rounds.

**Result on this account: 0 rounds unresolved, 81 logs across more than one map.**

| Source | Rounds (first run) | Rounds (after the parts arrived) |
|---|---|---|
| `log` (the log names one real map) | 1,769 | 1,747 |
| `part` (exact round times) | 0 | **717** |
| `window` (a part's upload window) | 696 | 0 |
| `geometry` (kill positions) | 49 | 26 |
| `meta` (the raw log's own map lines) | 21 | 21 |
| `neighbour` | 0 | 2 |

The second column is after five hidden parts were folded away (2,513 rounds) and the 289 parts were fetched over a different connection. **Scored against the exact part matches, every earlier answer was right: `window` 696 of 696, `geometry` 21 of 21. No round's map changed.**

**logs.tf was unreachable while this was built.** Every request timed out, including its homepage, while trends.tf and more.tf answered normally. It started after the M6 raw-log download of about 750 files, so the likeliest cause is that logs.tf is refusing this address. The part fetch (289 logs) is built and gives up after 3 failures in a row instead of waiting on 289 timeouts. It runs on the first sync that reaches logs.tf, and the pass then upgrades `window` and `geometry` answers to exact ones.

**Checks, with no part data:**
- **The example:** the SBQRRA vs Champions of Light grand final comes out as Ashville rounds 1–6 (4–2 to SBQRRA), Vigil 7–10 (1–3) and Proot 11–17 (4–3), as planned.
- **ETF2L's map order** matches the resolved maps on all 12 multi-map officials.
- **Geometry against windows:** before windows took priority, geometry agreed with them on 680 of 692 rounds. The 12 misses were all on maps with no outline yet (Bagel, Valor), which geometry cannot choose. So windows now come first, and geometry reports no confidence when a candidate map has no outline.
- **Geometry held-out test:** 301 of 310 single-map rounds correct from 18 maps (6 of the 9 misses were Product against Product RCX, the same map), and 309 of 310 from three.
- **All 49 geometry rounds read right against their titles:** `gullywash + badwater` is Gullywash 8 then Badwater 2, and `upward + vigil` is Upward then Vigil.
- **A map-name bug caught on the way:** `tow_tetsudo_b10a` is a real map. Map names now accept any short game-mode prefix, not only the usual ones.

**What changed downstream:**
- **Victim values** use each kill's round map, so combined officials get attack/defence values per map. Sniper form moved 45.5 to 45.6.
- **Map outlines and career heatmaps** include the kills inside combined logs: Vigil now draws on 144 matches.
- **Kill by kill** gets a map switch on combined logs and opens on the first map. "All maps" still works for the feed and timeline; the kill map asks for one map, since positions from different maps cannot share a drawing.
- **The match list** shows `ashville · vigil · proot`, and the **header** shows each map with the rounds won on it, your side first.
- **Hidden parts:** five logs that were whole copies of rounds inside a longer log are now superseded by it. That includes the 2018 semi-final's two single-map logs and a 2014 lobby uploaded twice. Highlander matches go from 758 to 753, and officials from 58 to 56.

---

## 11. Highlander theory and the next rating update (for deliberation)

> The rules and mechanics themselves — class limits, match formats, the map
> pool, Uber and cart numbers — live in [`docs/highlander.md`](docs/highlander.md).
> This section is the layer above: what the community says *wins* games, and
> how each claim could be tested against our own data.

Nothing in this section is built or agreed. It collects what the community says wins Highlander games and turns each claim into something this app could measure. It then expands the upcoming improvements from §3 and §8 into concrete proposals, each with a question to settle first. The theory is written as **hypotheses to test against our own data**, not as fact. Wikis and guides describe how people think the game works, and a measured effect can disagree with them.

### What the theory says

Sources (September 2026): the official TF2 wiki pages for [Highlander](https://wiki.teamfortress.com/wiki/Highlander_(Competitive)), [competitive Sniper](https://wiki.teamfortress.com/wiki/Sniper_(competitive)), [competitive dynamics](https://wiki.teamfortress.com/wiki/Competitive_dynamics), [community competitive play](https://wiki.teamfortress.com/wiki/Community_competitive_play), [competitive Engineer](https://wiki.teamfortress.com/wiki/Engineer_(competitive)) and [competitive Upward](https://wiki.teamfortress.com/wiki/Upward_(competitive)). Also RGL's [glossary](https://docs.rgl.gg/guides/basics/glossary/), the Steam guides [An introduction to European Highlander](https://steamcommunity.com/sharedfiles/filedetails/?id=163882605) and [Comprehensive Highlander Medic Guide](https://steamcommunity.com/sharedfiles/filedetails/?id=495096750), and teamfortress.tv's [TF2's hidden stats](https://www.teamfortress.tv/41723/tf2s-hidden-stats-part-1). comp.tf's Highlander class pages, the usual deeper source, returned 404 at the time. Ask function for anything the wikis get wrong.

**1. The teams are four groups, not nine players.**
- **The combo** is Medic, Demoman, Heavy, and usually Pyro. It takes and holds space, and every push is built around it.
- **The flank** is Scout and Soldier (sometimes Pyro). It pressures from off-angles, cleans up, and threatens the enemy's backline.
- **The pick classes** are Sniper and Spy. They create openings by killing key players before or during a fight.
- **Engineer** anchors a defence and buys time. On attack he gives teleporters and a mini-sentry, and matters much less.

The classic line: "when the Medic dies, the team panics".

**2. Fights are decided by advantages.**
- **Uber advantage:** one team will have its charge and the other will not. A team with it can push "without having to worry about the other team getting the same charge". Losing it is logged: see `lost_uber_advantage` below.
- **Player advantage** ("numbers"): more players alive. A pick is valuable because it creates numbers before a fight.
- **A pick** is "a kill on an enemy that opens up the possibility of a push" (RGL). Its value depends on **what happens next**. A traded pick, where the picker's team loses someone straight back, opens nothing.
- **Uber force:** making the Medic spend their charge to save themselves or a key player, without the charge winning anything.
- **Dry push:** a push with no charge. It is usually only right with numbers.

**3. The Sniper's job is picks on the combo, which first means winning the duel.**
- **Target priority** (wiki, roughly): immediate threats (a Scout on you, the enemy Sniper), then Medic, Demoman, Soldier and Heavy, then Engineer and buildings, then Scout.
- **The duel comes first.** "The Sniper that wins gains a significant advantage": they can then pick freely. The duel is also the weakest part of your rating (§3, M3).
- **Positioning:** show as little of yourself as possible, **change position after a kill or two**, and stay close enough to the team that their Scout cannot reach you.
- **Threats from behind:** Spy and Scout, the classes a Sniper is not looking at.
- **On attack/defence maps:** a pick on a charged combo class can stop a push "in its tracks". Even failed shots "force the enemy team to … play more passively", which no stat captures.

**4. On defence, the Engineer is worth the time he buys.**
- A sentry is "a temporary measure to stall an enemy team". The attackers' Soldiers and Demoman break it, so its value is the seconds it holds them, not the kills it gets.
- The defending Engineer builds in setup time and falls back point to point.
- That is function's "the defending Engineer is worth more", stated from the other side.

**5. Stopwatch is decided by time, not by rounds.**
- Payload is played as stopwatch: the attacker who takes every point fastest wins. A team can win it while losing most fights (§3, the TWS official).
- A defence's output is **seconds held per point**, and an attack's is **seconds taken per point**.
- **Forward holds** (defending far ahead of the objective) trade risk for time.

**6. On KOTH, the midfight sets up the round.**
- The midfight winner "consists of which team gets the most frags during the fight, as well as who caps the point first".
- Losing the Demoman or Medic early usually forces a retreat.
- It is not decisive: "there are multiple instances where a team that wins the mid fight does not win the match".

**7. On stats, net frags beat K/D, and damage lies.**
- *TF2's hidden stats* argues that **(kills − deaths) per minute** tracks match outcome better than K/D. A passive player can pad K/D.
- It also argues that damage per minute rewards spam into chokes, which "builds enemy Übercharge".
- Our rating already weights damage low (5% for Sniper) and deaths separately, which fits.

### What the raw logs let us measure

Counted over the 60 newest stored raw logs. The oldest stored log (2014) has the same events, except `lost_uber_advantage`:

| Event | Per log | Gives us |
|---|---|---|
| `spawned as` (not a trigger) | ~440 | With kills, **who is alive at every second**, and so numbers advantage |
| `chargeready`, `chargedeployed`, `chargeended` | ~30 each | Each Medic's uber state over time, and so **uber advantage** |
| `medic_death_ex (uberpct)` | 21 | The Medic's charge at death: a drop, a near-drop, or a pick before they built |
| `lost_uber_advantage (time)` | 5 | logs.tf's own "advantage lost" with its length, already in the summary |
| `player_builtobject`, `killedobject`, `object_detonated`, `carry`/`dropobject` | ~180, ~130 | **Each sentry's lifetime**, what killed it and where, and sapper kills |
| `pointcaptured (cappers, positions)`, `captureblocked` | ~15, ~17 | Cap times per point, and so **hold and push durations**. Also defensive blocks |
| `Round_Setup_End`, `Round_Overtime` | when present | When stopwatch attack starts; overtime saves |
| `shot_fired`, `shot_hit` | ~3,500, ~1,400 | Accuracy per weapon. For a Sniper, this separates misses from shots never taken |
| `damage` (with victim) | ~5,800 | Damage to a Medic just before a charge pops, and so **uber forces** |
| `healed`, `first_heal_after_spawn` | ~3,000 | Who kept whom alive, and Medic timing |

Kill lines already carry positions (M6), and rounds have maps (M8). Almost every item in the theory can therefore be computed with **no demos**, for all 18 players, across the whole history.

### Upcoming improvements, expanded

Each item below is ordered by how much it moves a Sniper rating and how sure the data is. Each ends with the question to settle before building.

#### A. Game state at every second: the base for everything below

A pure pass over one raw log that rebuilds, second by second:
- who is alive, per team and class;
- each Medic's charge state: building, ready, deployed or dead. The percentage between those points is estimated from build rates, about 40 to 50 s for stock and 32 to 40 s for Kritzkrieg per the Medic guide;
- which team holds which point.

**Validation:**
- The state's deaths must match the kill feed.
- Its uber deploys must match logs.tf's `ubers`.
- Its "advantage lost" moments must line up with the log's own `lost_uber_advantage` lines.

**To settle:** whether respawn waves can be derived from `spawned as` alone. A player who dies and has not respawned counts as dead; that seems safe.

**As built.** `state.rs` rebuilds a match from its raw log. It records every life (spawn to death), every Medic's charge in spans (building, ready, in use), rounds, caps and sentries. It answers: who is alive at any second, each side's charge, who has the uber advantage, and the uber lead in seconds. Nothing is stored. All 740 logs rebuild in 3.8 s, and one match page takes a few milliseconds. `hl state --check` compares it with logs.tf, and `hl state <log> --at <t>` shows who was alive at a moment.

- **Charge between known points is interpolated, not modelled.** The log marks every ready, pop and end, and the charge a Medic held when they died. So the percentage while building is read in hindsight between two real values, with no build-rate model.
- **Deaths:** of 195,103 counted kills, all but 476 (0.24%) close a life the state had open.
- **Players alive:** over 987,874 round seconds, a colour has more than 9 players alive for 0.26% of them, and never more than 11. Those are real overlaps: a player who timed out stays alive until their reconnect line, and a sub spawns before the player leaving.
- **Ubers:** 88% of 1,519 Medic performances match logs.tf's uber count exactly, and most of the rest are one off. logs.tf's counting rule is not public. Counting all pops, only pops after setup, or pops up to the next round start each matched worse than "inside a round".
- **Uber advantage against the log's own lines:** in 2,208 of the 2,535 `lost_uber_advantage` lines (87%), the state shows that team holding the advantage just as the enemy's charge became ready. MedicStats' plugin source shows its "time" is the *size* of the lead in seconds to full charge, not how long it was held. With that definition the state's lead is a median 2 s off (57% within 2 s, 70% within 5 s). Using each Medic's real build rate made this worse (7 s), so the lead is counted at the stock rate, as MedicStats does.

**Raw-log quirks found on the way, all handled:**
- The parts of a combined log are appended in any order, so the clock jumps back mid-file.
- A part can be played in the time gap between two others.
- Some logs hold the same match twice.
- Some servers write every round start and spawn twice.
- Players leave at a map change, or time out and reconnect, with no disconnect line.

Everything open is closed at `Game_Over`, at a round start, where time jumps back, and across a 5-minute silence. A part whose rounds all repeat an earlier part's is skipped.

**On the match page:** the Timeline tab has a strip under the chart on the same time axis:
- players up or down (blue when the chosen side has more alive, orange when fewer);
- each side's charge (brighter when ready, hatched when in use);
- who holds the uber advantage.

A sentence under it gives the shares, for example "up a player 25% of the time, down 39%". Hovering adds "7 v 9 alive · uber in use v 80%" to the tooltip.

#### B. Pick value in context: the trade window

A kill's worth today is its victim value (Medic 3.0 and so on) whatever happens next. Theory says a pick matters when it creates an advantage. Proposal: classify every kill.

- **Opening pick:** the first kill of a fight, where a fight is a gap of more than about 10 s since the last kill. It leaves your team up a player.
- **Traded:** someone on your team dies within about 3 s. A traded pick keeps a reduced value, and **dying yourself right after your own kill** (you did not reposition) is its own stat.
- **Clean-up:** a kill while your team already has numbers, such as the fourth kill of a won fight. It is worth less than an opening pick.
- **Pick into uber:** a kill on the enemy Medic while they hold a charge (a drop), or on a combo class in the seconds before their push.

For a Sniper this is the main addition, because it separates "got 20 kills" from "opened 12 fights".

**To settle:**
- The windows (3 s trade, 10 s fight gap). Measure them from the data: plot the time between consecutive kills and look for the natural gap.
- Whether the context multiplies the victim value or is a separate component.

**As built (B and D together).** `fights.rs` labels every counted kill from the raw log and the game state (A). A pass stores each player's counts per match in `fight_stat` (migration 0009). It runs at startup, after every sync and on rebuild, and reads only logs it has not read yet; all 740 take about 4 s.

**The windows were measured, not guessed,** on this account's 195,095 kills:
- **Fight gap, 10 s.** 89% of gaps between consecutive kills in a round are 10 s or less. The long tail beyond is the lull between fights.
- **Trade window, 3 s.** After a kill, the killing team's next loss peaks 1 s later and halves by about 3 s. There is no sharp knee, so 3 s is a judgement; it is question 1 for function.

**Each kill can be:**
- **opening:** the first of a fight;
- **first pick:** the first of the round;
- **traded:** the killer's team lost someone within 3 s;
- **died after:** the killer themselves died within 3 s;
- **a trade:** it avenged a teammate killed within 3 s before;
- **clean-up:** the killer's team was already up a player;
- **into charge:** a combo player (Medic, Demoman, Heavy, Pyro) killed while their team held a ready uber;
- **a drop:** the Medic died holding a ready uber.

**Per player it also counts:**
- **forced ubers:** the enemy Medic popped after taking 90+ damage in the 3 s before, with credit to each player who dealt 40+ of it. 70% of pops come with no damage on the Medic at all, so a pop under fire is the usual sign of a force. It misses pops forced to save someone else.
- **deaths around their own team's uber:** in the 10 s before it, during it, and in the 10 s after.

**Your Sniper against the Snipers you face**, per 10 minutes unless marked:

| | You | Pool |
|---|---|---|
| Opening duels won | 61% | 59% |
| Opening kills | 1.90 | 1.67 |
| First pick of the round | 14% of rounds | 12% |
| Kills traded back | 30% | 31% |
| Died within 3 s of own kill | 6.0% | 6.3% |
| Picks into a ready charge | 0.70 | 0.63 |
| Deaths during own uber | 0.50 | 0.56 |

You open more fights than the typical Sniper you face, and win slightly more of the ones you open.

**Where it shows:**
- **Match page:** a Fights tab has one row per player (opening duels, first picks, traded share, deaths after own kill, trades, clean-ups, picks into charge, drops, forces, deaths around own uber) and the first pick of every round with its time.
- **Play-by-play:** tags on the rare kinds only: first pick, opening, drop, into charge, died after. Traded and clean-up each fit about a third of all kills, which made the feed noise.
- **Profile:** a Fights card compares you with the players you face, under the same filters.

`hl fights [class]` prints the same comparison in a terminal.

**Not yet a rating component.** As planned: look at it first.

#### C. Duel in detail

The duel today is `classkills.sniper − classdeaths.sniper` over the whole match. Proposed:
- **First duel of each round** won or lost. The theory says whoever wins the duel then has free picks.
- **What happens after a duel win:** the picks you get in the next 20 s, before their Sniper respawns. This measures whether the duel win was used.
- **Duel on each map and side:** the kill map already shows where you lose it.

**To settle:** whether "first duel won" should be a rating component or only a profile stat. It correlates heavily with the existing duel differential.

#### D. Picks and uber timing (open item 13)

Built on A:
- **Time to first pick** of each round, and whether it came before the round's first uber.
- **Picks before an uber push:** kills on the enemy combo in the 10 s before the enemy pops. Those can stop the push, or force it.
- **Uber forces:** the enemy Medic deploys within about 2 s of taking a big hit, without a push following. Credit goes to the damage dealers.
- **Deaths before, during and after your own team's uber**, the split more.tf shows. Dying during your own push is costly for a combo class and matters less for a Sniper.

**To settle:** how to tell a force from a deliberate pop. The Medic's health just before the pop, taken from `damage` lines against them, is the likely signal.

#### E. Midfights (open item 14): KOTH only

On KOTH, the first fight of each round:
- **The midfight winner** is the team that caps first, with kills in the fight as the tiebreak (the wiki's definition).
- **Each player's part** in it: kills and deaths inside the midfight, opening pick included.
- Payload has no midfight. Its analogue is the **first fight after setup**, which B already covers.

**To settle:** whether the midfight becomes a rating component or stays a team stat on the match page.

#### F. Stopwatch: time as the currency

On attack/defence maps:
- **Seconds held per point** on defence and **seconds taken** on attack, from `pointcaptured` times.
- **Each player's defensive time:** seconds of a hold during which you were alive, plus attackers you killed during it. **Held-back kills**, where your kill delays the next cap, get extra credit.
- **Engineer:** each sentry's lifetime and the attackers it killed or held, from `builtobject` and `killedobject`. This backs up the defending Engineer's 1.6 value with measured time rather than a guess.
- **Overtime saves:** a `captureblocked` in overtime that ends the round.

**To settle:**
- Whether hold time belongs in a per-player rating at all. It is team output, and would reward the whole defending team equally.
- Alternative: use it only to fit **victim values per map and side** (G).

#### G. Victim values per map and side, fitted rather than guessed (open item 12)

The M6 layers (`[victim_value.map.*]`, `defending`) exist but are unset. Two routes:
1. **Ask function** for the first values per map, as the chat began: an Engineer on Vigil last, a Sniper pick on Upward. Candidate starting points from the theory:
   - Upward's long sightlines and its last-point instruction to "watch for the enemy Sniper at all times" argue for a **higher Sniper value on Upward**.
   - Sentry-held finals argue for a **higher defending Engineer on Vigil last**.
2. **Measure:** for each class, map and side, how much a kill of that class shifts the chance the killing team wins the fight, or takes or holds the point. This is the "fitted weights" item from §3, made concrete by A.

**Recommendation:** do route 2 on this account's ~2,500 rounds, then show function the fitted table next to the hand-set one. Where they disagree is the conversation worth having.

**To settle:** the sample. Per class, map and side cells get thin fast (Vigil defending Engineer kills might be a few hundred). Pool across maps first, and override per map only where the data is clear.

#### H. Kill value as change in win chance: one model instead of many knobs

A through G each add a hand-tuned number. The principled version is one model:
- From the game state (A) at each second, estimate the chance that each team **wins the fight, takes the point or wins the round**. This means a logistic fit on numbers per class, uber state, point held and map.
- **A kill's value is the jump in that chance.** Picks that open fights, drops, and kills that stop a push all fall out of it without separate rules.
- This is what "win probability added" does in other sports. It turns victim values from opinion into measurement, per map and side automatically.

**Costs:**
- It is a real modelling project.
- It needs care on stopwatch, where "winning the round" is not the outcome that matters. Time is (F).
- A player's rating becomes harder to explain than "3 Medic picks at 3.0".

**To settle:** whether to go straight to H, or ship B, D and G by hand first and use H to check them. **Recommendation:** by hand first. B, D and G are explainable and useful now, and H can replace their numbers later without changing the views.

#### I. Opponent strength (open item 8)

A Premiership Sniper and a lobby Sniper weigh the same in the pool. Proposals:
- **Tag each performance with its level:** the division from the nearest official of either roster (M5). Otherwise unknown.
- **Weight the pool or the result by level:** a 60 against Premiership outranks a 60 in a pug.
- A simpler first step is the **context split** (M5): officials, scrims and pugs are already separate on the profile.

**Settled (Q9).** Neither. There is no need for a division at all: the
opposite number on your class is named in every Highlander log, and their
average over their other games is a better measure of the night's difficulty
than a division label that only 5% of games carry. Weighting was measured
and left out — it changes nothing about how well the rating describes a
player, and it would hide the difficulty rather than report it. The profile
now reports it.

#### Seasons (a friend's suggestion)

"Sort logs by season, then see how you performed during that season, like average DPM each season. It could be as simple as only looking at logs between two dates." trends.tf has the data but no view of it.

**As built:**
- **Where seasons come from.** ETF2L's API lists a competition's matches but gives no season dates, and it pages 20 at a time. So seasons come from your stored officials, with no network. Every competition of one season (divisions, group stages, playoffs) groups under the season's number. Anything else groups under its name before the colon: "Winter 2024: Low Playoffs" is "Winter 2024", and "AFA 2025" and "Experimental Cup #10" are their own.
- **What a season covers.** From six days before its first official (the week of scrims leading up) to the day after its last. The newest season stays open to today while its last official is under 30 days old. Every game in the window counts: officials, scrims and pugs.
- **One period filter, shared by the match list and the profile:** all time, one season, or custom dates. Seasons without an official of yours are not listed; custom dates cover them. The profile's career records (duel, Medic picks) count every game, so they hide while a period is set.
- **By season on the profile:** per season for the chosen class, it shows games (officials / scrims / pugs), record, average rating, DPM, K/D, kills and deaths per 10, opening duels won and share of kills traded. Clicking a row filters everything to that season. `hl seasons [class]` prints it.

**What it shows for your Sniper:** Winter 2024 and Season 33 were your best stretch, at ratings of 52–54 and about 370 DPM. Since Season 34, DPM sits near 300 and ratings in the low-to-mid 40s. Season 36 so far: 14 games, 47.

#### J. Smaller items worth keeping on the list

- **Net frags per minute** as a profile stat next to K/D, per the hidden-stats argument.
- **Sniper accuracy** from `shot_fired`/`shot_hit`: shots taken per minute, which shows passive play, and the hit rate. Check whether logs.tf's `hasacc` era limits it.
- **Deaths to flankers:** a Sniper's deaths to Scout and Spy, per 10 minutes. The theory names them as the threat from behind; a high share suggests positioning too far forward or no cover.
- **Repositioning:** distance between consecutive kill positions in one life. Staying put after two kills is the classic mistake, and positions are already stored.
- **Target priority:** your share of kills on the combo against the Sniper average. Already implicit in impact kills; worth showing on its own.
- **Demo linking per map segment** (open item 18). Unchanged.

### Order, if agreed

1. **A**: the game state. Everything else stands on it.
2. **B and D**: pick context and uber timing. These give the biggest change in what a Sniper rating means.
3. **G, route 1**: first per-map values from function, next to the measured numbers from A, B and D.
4. **C, E and F** as profile stats first, and rating components only once they have been looked at.
5. **H**, once B, D and G have run long enough to check it against.
6. **I** alongside, since it is independent of the rest.

### Questions for function

1. The trade window. How quickly does a teammate's death have to follow your pick before it no longer counts as creating an advantage?
2. Is "first Sniper duel of the round" the moment that matters, or is the duel a running score?
3. Per map: which maps make the Sniper most and least important? Where is the defending Engineer the whole defence?
4. An uber force: is it credit to the player who dealt the damage, to the whole team, or to nobody?
5. On defence, is a Sniper kill that delays a cap worth more than the same kill during a hold that breaks anyway?

---

## 12. Lessons from the HLTV rating: the Sniper rating v3 plan

Other classes keep basic weights; the Sniper is the focus. This section is the write-up of what HLTV's rating does, what carries over to a Highlander Sniper, and the build plan, step by step.

Sources: [Introducing Rating 2.0](https://www.hltv.org/news/20695/introducing-rating-20), [Introducing Rating 2.1](https://www.hltv.org/news/40051/introducing-rating-21), [Introducing Rating 3.0](https://www.hltv.org/news/42485/introducing-rating-30), [Rating 3.0 adjustments go live](https://www.hltv.org/news/43047/rating-30-adjustments-go-live) (all HLTV), [Reverse engineering the HLTV 2.0 rating](https://flashed.gg/posts/reverse-engineering-hltv-rating/) (flashed.gg), and [How Rating 3.0 works](https://escorenews.com/en/csgo/article/71475-how-hltv-rating-3-0-formula-actually-works-round-swing-and-eco-adjustment-explained) (Escorenews). Read September 2026.

### How HLTV's rating works

**Rating 2.0 (2017)** has five sub-ratings, each computed separately for CT and T. Each is scaled by how many standard deviations the player sits from the expected value for that side, so 1.00 is average.

| Sub-rating | What it measures |
|---|---|
| Kills | Kills per round. An assisted kill where the killer did under 60 damage is worth less. |
| Survival | Not dying. **A death a teammate trades counts less.** |
| KAST | Share of rounds with a Kill, an Assist, Survival, or a Trade of your death. It measures consistency. |
| Impact | Opening kills, multi-kills and clutches won (1 v several). |
| Damage | Average damage per round. |

HLTV keeps the formula private. A reverse-engineered fit gives deaths per round the largest weight of any term (−0.53), ahead of kills per round (+0.36) and impact (+0.24).

**Rating 3.0 (CS2, 2025)** keeps kills, damage, survival and KAST, and adds two ideas:
- **Economy adjustment.** A kill is worth the chance of winning that duel with that equipment. A rifle killing a pistol is worth about 0.54 of a kill, and an even rifle duel about 1.1. HLTV found only 45% of duels are between equal equipment, so "eco frags" had been inflating players. AWPers lost a little, because they win most duels anyway.
- **Round Swing.** Each kill's change to the team's chance of winning the round, given the map, side, economy, players alive and bomb state. Credit goes to the final blow, damage share, flash assists, and trades within 5 s.

**HLTV corrected 3.0 after launch:**
- Round Swing was about 40% of the rating and too dominant. It was cut to 33%, and kills went from 12% to 25%.
- Swing left over at the end of a round was split more widely: clutch winners, players who swung the round with kills, defusers and survivors.
- The stated aim was **a 60–40 balance of output (volume) against impact**.
- HLTV lists its own limits. Round Swing undervalues multi-kills, because the fifth kill of a won round changes little, and it slightly favours passive players, so it is paired with a multi-kill rating.

### What carries over to a Highlander Sniper

| HLTV idea | Highlander version | Status here |
|---|---|---|
| Traded deaths count less | A Sniper death your team trades within 3 s opened something. A forced entry peek on Vigil second that your team converts is not a wasted death. | **Step 1** |
| KAST | Rounds are minutes long, so per round is meaningless. Per **fight** instead: the share of fights you were alive for where you got a kill or assist, survived, or were traded. | **Step 2** |
| Economy adjustment | TF2 has no economy. The equivalent is **player numbers and ubers**: a kill in a 9 v 5 clean-up is TF2's eco frag (39% of your kills are clean-ups), and a kill at even numbers or on a charged combo is worth more. | **Step 3** |
| Side-specific expectations | Compare attack Snipers with attack Snipers, per map. Defending Vigil is not the same job as pushing it. | **Step 4** |
| Round Swing | §11 H: win chance per **fight**, not per round. | **Step 5**, capped at about a third |
| 60–40 output against impact | v2 is already about 65–35: kills, DPM, deaths and assists against opening duels, Medic picks, the duel and untraded kills. | Keep checking at every step |
| Transparent sub-ratings | Every component shows raw value, percentile and weight. | Done since M3 |
| Validated against outcomes | HLTV uses case studies. Model v2 used 693 paired matches and who won them. | **Step 0** makes it a tool |

**What does not carry over:**
- **Clutches.** A Highlander round rarely ends 1 v several, because players respawn.
- **Multi-kills in the CS sense.** Killstreaks exist, but a Sniper's streak across two lives does not mean the same thing.

The equivalents are already covered: opening duels, picks into a ready charge, and kill streaks shown in the play-by-play.

### The plan

Each step ends in the same place: re-rate, run the validation from step 0, and look at the grand final (log 3863290) and your career numbers. A step only changes the model if it holds or improves accuracy, or if the players agree it should even when accuracy is flat, as DPM did in v2.

#### Step 0. A validation command

The win test from v2 was a script in a scratch folder. Make it permanent so every later step is measured the same way.

1. `hl validate sniper` in `hl-cli`. It pairs the two Snipers of every decided match, and for each component reports how often the team with the better Sniper won.
2. For the current weights, a candidate weights file (`--weights path.toml`) and a fitted logistic model, it reports how often the rating picks the winner.
3. **Out-of-sample check.** Fit on matches before a date and test on the ones after (by default, everything before Season 34 against Season 34 on). A weighting that only wins in-sample is flagged.
4. Output as a table and as `--json`, so results can go into this plan as they are.

**Done when:** it reproduces v2's numbers (v1 67.5%, v2 71.4%, 679 to 693 matches).

**As built.** `hl validate sniper` takes 1.2 s:
- **The pool.** It extracts every Sniper component, used by the live model or not, for every performance, and measures each against a pool of all Sniper performances. The live rating leaves you out of its pool; here that would favour one player's games.
- **Candidates.** `--weights file.toml` scores a candidate; the file can hold only a `[model.sniper]` table, and the flag can be repeated. A typo in a component name is an error.
- **Other options.** `--split YYYY-MM-DD` moves the before/after date, and `--json` gives the report as data.

It reproduces v2's check exactly: 693 matches, v1 67.5%, v2 71.4%.

**First out-of-sample reading** (split at the start of Season 34: 491 matches before, 202 after):

| Picks the winner | All | Before | After |
|---|---|---|---|
| Live (v2) | 71.4% | 72.3% | 69.3% |
| v1 | 67.5% | 67.8% | 66.8% |
| function's "duel 10, DPM 15" | 68.5% | 68.8% | 67.8% |
| Fitted before the split | – | 76.8% | **73.8%** |

**What that says:**
- v2's lead over v1 holds on matches it was not tuned on: 69.3% against 66.8%.
- A model fitted only on older matches still scores 73.8% on newer ones. So a weighting closer to the fit has real room left, mostly by leaning harder on deaths, impact kills and untraded kills.
- Steps 1–3 are where that room goes: death context, Fight KAST and situation-valued kills.

#### Step 1. Death context (HLTV's traded-death rule)

**Measure.** In `fights.rs`, label every death:
- **traded:** your team killed your killer, or anyone on their team, within 3 s;
- **killer group:** the enemy Sniper, a flanker (Scout, Spy, Soldier) or the combo (Medic, Demoman, Heavy, Pyro);
- **opening:** you were the fight's first death (already counted);
- **stationary:** you died within 300 units of where you got a kill in the same life, after two or more kills from there. That's the "change position after a kill or two" rule from the Sniper theory (§11);
- **during own uber** (already counted).

**Store.** Migration 0010 adds `traded_deaths`, `deaths_to_sniper`, `deaths_to_flank`, `deaths_to_combo` and `stationary_deaths` to `fight_stat`. The fights pass goes to version 2 so every log is re-read once.

**Rate.** Try three versions of the Deaths component in `hl validate`:
- all deaths (v2);
- untraded deaths only;
- untraded deaths plus half of traded ones.

Keep the one that validates best. If they tie, keep the players' view that a traded entry death is not a failure.

**Show:**
- **Play-by-play:** each of your deaths gets one tag: *traded*, *to their Sniper*, *to a flanker*, or *stationary*.
- **Fights tab:** new columns for traded deaths and deaths by killer group.
- **Profile Fights card:** new lines for the share of deaths traded, deaths to flankers per 10 min (lower is better), and stationary deaths per 10 min (lower is better).

**Tests:**
- a traded death at 2 s and an untraded one at 4 s;
- a Spy backstab counting as a flank death;
- a stationary death after two kills from one spot, and none after moving 400 units.

**As built (model v3).** The fights pass (version 2) labels every death:
- **traded:** your team killed anyone on the killer's team within 3 s;
- **killer group:** the enemy Sniper, a flanker (Scout, Spy, Soldier), or the combo (Medic, Demoman, Heavy, Pyro), with Engineers and sentries in none;
- **stationary:** within 300 units of a spot you got two or more kills from in the same life.

Migration 0010 adds the counts to `fight_stat`, and every log was re-read once. The rating gains three candidate components: untraded deaths, deaths to flankers, and stationary deaths.

**What the validation said** (`hl validate sniper`, 693 matches):

| Better at it, and their team won | |
|---|---|
| Deaths (all) | 75.6% |
| Untraded deaths | 74.7% |
| Deaths to flankers | 70.4% |
| Stationary deaths | **45.9%**: predicts nothing |

Fitted together:
- **Untraded deaths and deaths to flankers** both help, on top of everything else.
- **Stationary deaths** is unclear. Holding a spot you are winning from may simply be right, so it stays information only.
- **The best fit trained on older matches** rose from 73.8% to 74.8% on newer ones with the new stats: death context carries real signal.

| Weighting | All | Before S34 | From S34 |
|---|---|---|---|
| v2 | 71.4% | 72.3% | 69.3% |
| Untraded deaths in place of deaths | 71.4% | 72.1% | 69.8% |
| Half deaths, half untraded | 71.6% | 72.5% | 69.3% |
| **v3** (applied) | **72.4%** | 72.9% | **71.3%** |

**v3 Sniper weights:**
- Impact kills 25%, DPM 20%;
- **untraded deaths 15%**, in place of all deaths;
- opening duels 10%, down from 15;
- Medic picks 10%;
- **deaths to flankers 5%** (new);
- kills not traded 5%, duel 5%, impact assists 5%.

The model version is now `v3`. It predicts winners better than v2 on newer matches it was not tuned on (71.3% against 69.3%). Answer to question 1 for function, as built: **a traded death does not count against you.**

**Effect:**
- **The grand final (3863290):** you 61.0, angel complex 57.4.
- **Your Sniper career:** 51.7 → 52.1. Officials 55.9, scrims 52.4, pugs 47.9.
- **Your deaths against the Snipers you face:**
  - 37% traded (theirs 36%);
  - 13% fewer to flankers;
  - 14% more to their Sniper, which matches the weak duel;
  - "stayed put and died" level.

**Where it shows:**
- **Play-by-play:** your own deaths are tagged *traded* or *untraded*, and *stayed put*.
- **Fights tab:** "deaths traded" and "died to Sniper / flank / combo" columns.
- **Profile Fights card:** deaths traded, deaths to their Sniper, deaths to flankers, and stayed-put deaths (shown for reference).

**Version plan, renumbered:** v3 is step 1 alone. Steps 2–3 become v4, and steps 4–5 become v5.

#### Step 2. Fight KAST

**Measure.** For each fight (the 10 s gap rule), note who was alive at its start or spawned into it. For each of those players, the fight counts if they:
- got a kill or an assist in it;
- survived it;
- or died and were traded.

**Store.** `fights_present` and `fights_kast` in `fight_stat` (same migration and fights version as step 1).

**Rate.** Add a Sniper component, "Fight KAST" (% of fights). Validate at 5% and 10%, taking weight from impact kills, which it overlaps least with.

**Show:** a Fight KAST column in the Fights tab and in the profile's By season table.

**Tests:** a player who survived a fight without a kill counts; one who died untraded without a kill does not; one who respawned into a fight counts from their spawn.

**Open question for function:** does a Sniper who never peeks and survives every fight deserve KAST credit? HLTV says yes (survival counts). If that rewards passivity here, count survival only in fights where the Sniper fired a shot (`shot_fired`).

**As built (model v4).**
- **Parsing.** The parser now keeps every `shot_fired` line.
- **Counting.** The fights pass (version 3) splits each round into fights with the 10 s rule, so a fight runs from its first kill to its last. A player is present if alive at any point in it, respawns included.
- **What counts.** A fight counts toward KAST when the player got a kill or assist in it, survived it, or had every death in it traded. Suicides count as untraded deaths.
- **Question 3 (does surviving without shooting count?).** An engaged variant counts survival only after a shot, from 10 s before the first kill.
- **Storage.** Migration 0011 stores fights present, KAST fights and engaged KAST fights. Every log was re-read once.

**Validation** (`hl validate sniper`, 693 matches):

| Better at it, and their team won | |
|---|---|
| Fight KAST | 72.5% |
| Fight KAST, engaged | 71.6% |

Fitted with everything else, Fight KAST helps a little (+0.18, bootstrap range just above zero). It overlaps with untraded deaths. The engaged variant is unclear. **So the answer to question 3, from the data: surviving a fight counts, fired or not.**

| Weighting | All | Before S34 | From S34 |
|---|---|---|---|
| v3 | 72.4% | 72.9% | 71.3% |
| + Fight KAST 5% (from assists) | 72.9% | 73.3% | 71.8% |
| **+ Fight KAST 10%** (from impact kills and assists): **v4** | **73.2%** | 73.5% | **72.3%** |
| + Fight KAST 10%, impact kills 15%, assists kept | 73.6% | 74.1% | 72.3% |
| + engaged Fight KAST 5% | 72.6% | 72.9% | 71.8% |

v4 keeps impact kills at 20% rather than 15%. Both scored the same on newer matches, kills are the core of the job, and HLTV raised kills for the same reason.

**v4 Sniper weights:**
- **Output, 60%:** impact kills 20, DPM 20, untraded deaths 15, deaths to flankers 5. That is HLTV's 60–40 balance.
- **Impact, 40%:** Fight KAST 10, opening duels 10, Medic picks 10, kills not traded 5, duel 5.

**Effect:**
- **The grand final:** you 62.3, angel complex 59.2.
- **Your Sniper career:** 52.1 → 52.4. Officials 56.5.
- **Your Fight KAST:** 85% of fights, the 54th percentile over your career against the Snipers you face.

**Where it shows:** a Fight KAST column in the match page's Fights tab and in the profile's By season table, and a first line on the profile's Fights card.

**Versions, renumbered again:** v4 is step 2. Step 3 will be v5, and steps 4–5 v6.

#### Step 3. Kills valued by the situation (TF2's economy adjustment)

**Measure first.** From the game state (§11 A), for every kill, take the players alive on each side just before it, and whether the victim's team held a ready charge. Then, per state (numbers difference −4 to +4, charge ready or not), measure how often the killing team won the fight, before and after the kill. The rise is what a kill is worth in that state.

**Model.** A situation factor multiplies each kill's victim value:
- factor = the rise in fight-win chance for a kill in that state ÷ the rise at even numbers;
- so even numbers is 1.0, a clean-up at +4 comes out below 1 and a kill while down comes out above;
- a charge-ready victim gets its own measured factor on top.

The table lives in `weights.default.toml` under `[situation]`, generated by a command (`hl situation --write`) so it can be refreshed as data grows.

**Code:**
- `hl-rating::impact` takes the factor per kill.
- `kills.rs` passes the numbers and charge state, from a `GameState` built during the rating pass. The pass then parses raw logs: about 5 ms each, 4 s for all.
- Or the fights pass stores a per-kill factor so rating stays fast. **Recommended:** store it, in `kill_event` via a new column `situation REAL`, written by the fights pass.

**Validate.** Impact kills with and without the factor. If the factor only shrinks clean-ups and accuracy holds, keep it. It answers "39% of my kills were clean-ups" the way HLTV answers eco frags.

**Guard.** The factor is clamped to 0.5–1.5, so no single kill is worth three kills.

**As built (model v5).**

`hl situation` reads every stored raw log and, for each of 193,626 counted kills, takes the state just before it: the killer's team alive minus the victim's (clamped to ±4) and the uber advantage (the killer's team, neither, the victim's). Each state gets two chances, both from the killer's side:

- `base`, how often a team in that state went on to win, whichever side got the next kill;
- `after`, how often it won when it got the kill.

The kill's worth is `after − base`, and its factor is that worth over the worth of a kill at even numbers with no advantage, clamped to 0.5–1.5. States under 200 kills drop the advantage, then step toward even numbers. Winning was measured two ways.

**Winning the fight** (more kills in it; a tie goes to the next cap) is clean but partly tautological — a kill counts toward the outcome it is being valued against:

| numbers | −4 | −2 | 0 | +2 | +3 | +4 |
|---|---|---|---|---|---|---|
| factor | 0.36 | 0.71 | 1.00 | 0.63 | 0.43 | 0.24 |

**Winning the round** is independent, and matches the theory: kills while outnumbered are worth more, clean-ups less.

| numbers | −4 | −2 | 0 | +2 | +3 | +4 |
|---|---|---|---|---|---|---|
| factor | 1.26 | 1.21 | 1.00 | 1.08 | 0.78 | 0.86 |

A new component, **Kills in context** (`situation_kills`), is impact kills with each kill multiplied by its factor. The fights pass (version 4) stores every kill's state in `kill_situation`, so rating stays fast. The factors live in `weights.default.toml` under `[situation]` and are refreshed with `hl situation --toml`.

**What the validation said** (693 matches, split at Season 34):

| Sniper model | all | before | after |
|---|---|---|---|
| v4 (impact kills) | 73.2% | 73.5% | 72.3% |
| v5, full measured table (fight) | 73.2% | 73.3% | 72.8% |
| v5, full measured table (round) | 73.2% | 73.5% | 72.3% |
| **v5, clean-up discount only** | **73.6%** | **73.9%** | **72.8%** |
| v5 with both kill components | 73.6% | 74.1% | 72.3% |

On its own the component is slightly *worse* than plain impact kills at picking the winner (69.4% against 70.2%), because scaling kills down adds nothing where both Snipers clean up equally. In the model it is slightly better. Every gain here is 1–3 matches out of 693: real but small, and it is kept because it is the theory-backed half of the measurement rather than because the numbers demand it.

**Shipped:** only the clean-up discount, every row the same — a kill at three up counts 0.75, at four up 0.5, everything else 1.0. The full measured tables are recorded above and can be dropped in at any time.

**Effect:** the grand final (3863290) reads flashy 62.6, angel complex 59.3 (v4: 62.3 and 59.2).

**Open for step 5:** fight swing values a kill by the same idea but continuously. If it lands, this factor comes out so the same thing is not counted twice.

#### Step 4. Map and side baselines

**Why.** A 50th-percentile Vigil defence and a 50th-percentile Upward attack are different games. This covers function's "a Sniper pick on Upward is worth more than on Vigil" without guessing per-map values.

**Measure per side.** Split each Sniper performance into its attack and defence halves on attack/defence maps. Payload and A/D control points are split by the side worn each round (already known per kill). Kills, deaths and damage per round come from the raw log. KOTH has no side and stays whole.

**Baseline.** Percentile pools per (map, side), and only where a pool has 150 performances or more. Otherwise fall back to (map), then (all maps). The fallback used is shown on each rating.

> **What the data said, September 2026.** The threshold and the split both had to move.
>
> At 150, only 27 of 891 (class, map) pools qualify, covering 42% of performances. At **100**, 54 qualify and cover 64% — and those 54 are exactly the current ETF2L pool for every class (product 217, vigil 215, upward 194, ashville 116, steel 115, proot 105). Everything older falls back, which is the right way round. So the threshold is 100.
>
> Splitting by side then halves every pool to ~107, under any threshold worth having. Worse, **80% of performances on attack/defence maps play both sides** — stopwatch is ABBA, so almost everyone attacks and defends in the same log. A performance cannot be filed under one side at all; its components have to be split, and that needs per-round damage and time, which the raw-log parser does not read. Side pools themselves are fine (~323-386 per class per side, pooled across maps) — it is the splitting that is blocked.
>
> **Shipped:** per-map pools, threshold 100, falling back to every map together. The Sniper spread across maps went from **0.192 to 0.097**; Vigil from 0.903 to 1.017 and Product from 1.095 to 1.012. Out-of-sample accuracy is unchanged at 72.4%, in-sample down 0.6 points — which is what a fairness fix looks like.
>
> **Still to do:** the pool's name on the match page ("vs Vigil Snipers, 215 games"); `Baseline::pool_for` is there for it. And the side half, as Q5b.

**Rate.** A performance's percentile on each component is the time-weighted average of its halves' percentiles against their own pools.

**Validate.** Accuracy should hold. The check that matters here is fairness: your average rating by map should flatten, with Vigil defence no longer your lowest just because it is the hardest job.

**Show.** The rating table on the match page names the pool ("vs Vigil defence Snipers, 212 games").

#### Step 5. Fight swing (HLTV's Round Swing, per fight)

**Model.**
- Fit, once and stored, a logistic model of **who wins the fight** from the state: players alive per side, both charge states, and which side holds the point.
- A fight's winner is the side with more kills in it; a tie goes to whoever takes the next cap.
- A kill's swing is the change in the killing side's win chance.

**Credit, following HLTV's corrected split:**
- 1 share to the killer;
- 1 share to damage dealers on the victim in the 5 s before, by damage;
- 1 share to a trade: the kill that avenges a death passes some swing back to the traded player.

**Component.** Sniper "Fight swing", per 10 min. Weight about 15%, never over a third of the rating: HLTV's 40% was too much. Take it from impact kills and opening duels, which it partly replaces.

**Validate.** Out-of-sample only; this is the easiest step to overfit. It ships only if it beats step 4 out of sample.

> **What the data said, September 2026.** Built, measured, and not shipped into the weights.
>
> The model needed no fitting: `hl situation` already measures, over 194,025 kills, how much a kill in each state raises the chance of winning the fight. At even numbers with no uber it is 50.0% -> 69.2%, so the kill is worth **0.192**; a clean-up at four up is worth **0.047**. `hl situation --swing` writes that out as a `[swing]` table, and the component is the sum of it per 10 minutes — unclamped and unnormalised, which is the whole point of it against step 3's factor.
>
> **Alone it is the best single component in the model**: 69.7% of 680 games, z 10.3, just ahead of Kills in context at 69.3%. **Fitted beside Kills in context it is worth nothing**: 0.04 [-0.43, 0.63], "unclear". They measure the same thing from different ends — Kills in context knows *who* was killed (a Medic is worth more than an Engineer), swing knows *when* (the numbers and the uber). Neither subsumes the other, and neither adds much to the other.
>
> Out of sample, against the live 72.4%: swapping swing in for Kills in context gives **70.9%**, 0.15/0.05 gives 72.4%, 0.05/0.15 gives 71.4%, and only an even **0.10/0.10 reaches 72.9%** — one peak with worse on both sides and a coefficient the bootstrap calls unclear. That is noise, so the live model is left alone.
>
> **Kept anyway:** the component, the measured table, and the generator. Q7's teamfights need a fight model, and this is one.

**Order note.** Step 3's situation factor is a simple version of this. If step 5 lands, step 3's factor can be dropped from impact kills so the same thing is not counted twice.

### After each step: the balance check

Keep output (impact kills, DPM, deaths, assists) at about 60% and impact (opening duels, Medic picks, the duel, untraded kills, Fight KAST, swing) at about 40%. That is HLTV's stated balance, and v2 already sits near it (65–35).

### Versions

| Model | Contents | Gate |
|---|---|---|
| v2 (live) | DPM 20, duel 5, opening duels, untraded kills | 71.4% against v1 67.5% |
| **v3** | Steps 1–3: death context, Fight KAST, situation-valued kills | Holds or beats v2 out of sample; the grand final still reads right |
| **v4** | Steps 4–5: map and side baselines, fight swing | Beats v3 out of sample; ratings by map flatten |

Each version bumps `MODEL_VERSION`, re-rates at startup and keeps the old ratings apart. PLAN gets an "As built" note per step with the validation table.

### Questions for function

1. Should a traded entry death count as a death at all, count as half, or count as nothing?
2. Is 300 units and two kills the right "should have moved" rule?
3. Should a Sniper who survives a fight without firing get KAST credit for it?
4. Clean-up kills at 9 v 5: worth half a kill, or nearly a full one, since they stop the enemy regrouping?

---

## 13. The queue

Everything agreed but not yet built, in the order it will be taken. Each row
says why it is where it is; the sections after it hold the detail. Feedback
from testers (function, boSe, Taiga) is marked with who asked.

| # | Job | Size | Why here |
|---|---|---|---|
| ~~Q1~~ | ~~**Filters ignored by two tabs** (function)~~ | small | **Already fixed in the code; what was left was the label lying.** Damage-and-kills and Fights both filter on the slice, verified live: picking R2 moves the damage totals from 9,319/11,997 to 1,367/1,272 and re-counts the fights table. The remaining defect was wording — a slice is one round, one *map* of a combined log, or the whole match, and the panels only knew two of those, so a seven-round map was announced as "in this round". `sliceLabel`/`sliceScope` in `common.ts` name the slice once and Spread, Fights and Aim all read it. Spread's doc comment also still claimed damage had no round split, which the code stopped being true of some time ago. |
| ~~Q2~~ | ~~**Database backup before every sync**, and a warning about the uninstaller~~ | small | **Backups and the warning already shipped; the gap was that the warning gave no way to act.** The Backups panel said "keep one elsewhere if it matters to you" and offered no means to do it — the only copies were the automatic ones beside the database, which is exactly what the uninstaller's "delete application data" removes. **Save a copy elsewhere…** now opens a file dialog and writes one with `VACUUM INTO` (`backup::save_as`), refusing to overwrite an existing file, since this is the button people press when they are worried about losing data. |
| ~~Q3~~ | ~~**Deep demo parse** (§14)~~ | large | **Done (27 Sept): every demo read is kept whole as a timeline** (§14b): every tick's positions and angles, every state change, cart, buildings and all game events, 1-7 MB a demo, faithful to 0.05°. Later passes derive from it without the file. Found and fixed two aim bugs on the way (conditions, POV lookback). Left out, not open: reaction time needs the map's geometry (what a player could see), which no file the app reads holds; it comes back if the BSPs are ever parsed. |
| ~~Q4~~ | ~~**Scout picks on KOTH worth more** (boSe)~~ | small | **Measured twice, not applied — and the second measurement points the other way.** Redone after Q8 across all nine models: 1.4/1.7/2.0/2.5 on koth_ maps move the nine accuracies by +0.7 (Soldier) to −0.8 (Engineer), total under half a point, no trend. Then measured *directly* with `hl situation --victims`: for every kill, how often the killer's team won the round less what the situation predicted, per victim class and mode, each mode against its own baseline (71,808 KOTH kills, 38,931 stopwatch). On KOTH the Scout is +6.0% against a +6.2% mode average — the average class to kill, well inside one standard error. On stopwatch he is +1.1% against a −1.9% average, the **best** class to kill of the nine, about four standard errors clear. The effect boSe described exists and is on the other mode; a guess at why is that on payload the Scout is who gets back to the cart. Not applied either way: the stopwatch column's label is weak (a stopwatch round is won on the clock, and its average kill scores −1.9%). The real finding is that `hl validate` cannot feel this table at all — the Medic's KOTH premium is the biggest effect in it (+10.7 vs +6.2) and applying it costs 0.4 points. Full write-up in `weights.default.toml`. |
| ~~Q5~~ | ~~**§12 step 4: map baselines**~~ | medium | **Done, in half.** Per-map pools ship: the Sniper spread across maps halves from 0.192 to 0.097, Vigil 0.90 -> 1.02 and Product 1.10 -> 1.01, with out-of-sample accuracy unmoved at 72.4%. |
| Q5b | **§12 step 4: the side half** | large | 80% of performances on attack/defence maps play both sides, so a performance cannot be filed under one: its components have to be split per round. **Not blocked after all** — `RawLog.damage` holds every hit with its second, attacker and victim; it is simply never stored. Time on class per round is the remaining unknown. |
| ~~Q6~~ | ~~**§12 step 5: fight swing**~~ | large | **Built and measured; the live model is unchanged.** Alone it is the strongest single component there is (69.7% of 680, z 10.3), but fitted beside Kills in context it is "unclear" [-0.43, 0.63] — they measure the same thing from different ends. Swapping it in is *worse* out of sample (70.9% against 72.4%); only an even 0.10/0.10 split edges ahead, by 0.5 points, inside the noise. The component, the measured `[swing]` table and the `--swing` generator stay: Q7 needs the fight model anyway. |
| ~~Q6b~~ | ~~**Fight swing's credit split**~~ | medium | **Done: Demoman only.** `fight_swing_shared` gives each kill's swing to whoever damaged the victim in the 5 s before (killer floor 0.5), and replaces impact kills in the Demoman model (full 76.8 -> 77.3, held out 72.1 -> 73.0). Swapped in for every other class and worse each time: Engineer, Heavy and Soldier at the time; Scout -0.4 / -0.5, Pyro -0.6 / -0.5, Sniper -0.6 / -2.0, Spy -0.3 / -1.0 on 27 Sept (full / held out). Medic has no kills term to swap. |
| ~~Q7~~ | ~~**Teamfights** (Taiga)~~ | medium | **Measured, not yet shown.** `teamfights.rs` says who was in each fight and when they arrived, counting damage as well as kills — a Soldier who lands two rockets and lives was in the fight, and the kill list would never say so. Collapse is the spread between a side's first and last arrival. The UI for it, and the uber-exchange half of Taiga's ask, are Q7b. |
| ~~Q7b~~ | ~~**Teamfights on the match page**~~ | medium | **Done, and the measure had to change first.** Q7's definition of arriving together -- the gap between a side's first and last arrival -- was shown for the first time and called **0 of 51** teamfights together on a real match, and 1% across the whole history. With eight or nine a side there is nearly always one who arrives late for a good reason, and the gap measures them. It is now the **share of a side in within 3 s of its own first arrival**, and that one means something: across **48,074 side-fights**, a side 80-100% in together won 53.2% and lost 32.8%; 0-20% in together won 35.3% and lost 52.2%; every bucket between runs the same way. That is Taiga's claim, measured. The Fights tab gains a Teamfights section: your side's average share in together, fights won, the selected player's habit ("in 48 of 58, usually 4 s after the first kill"), and a row per fight with each side's "6 of 8", the losses and the selected player's arrival. It follows the round and map filter. `Teamfight::together` is redefined at the source with a test for the one-straggler case that broke the old one, and the mock's analysis fixtures carry real teamfights so a browser shows the real shape. **Still blocked:** Taiga's third part, uber exchanges as space, which needs Q11's cart timeline to say what the space was worth. |
| ~~Q8~~ | ~~**Every class gets its own model** (boSe)~~ | large | **Done, and it moved every class.** `hl validate` works for all nine now — Highlander guarantees one of each class a side, so every class has ~690 pairs, as many as the Sniper. `validate::propose` turns a fit into rounded weights and five-fold cross-validation over blocks of time scores the procedure, so no model is quoted on the data it was tuned on. Cross-validated, picking the winner: scout 70.7→74.5, soldier 72.2→73.2, pyro 73.5→78.0, demoman 72.0→75.6, heavy 72.0→76.8, engineer 71.8→73.4, medic 71.1→75.4, spy 71.0→72.9, sniper 72.9→75.9. Model v7; the reasoning for each of the nine is in `weights.default.toml`. Three findings: **DPM earns its place nowhere** (added at 0.10 to every model it moves accuracy −0.6 to +0.3 — noise both ways; damage already reaches the rating as the kills it sets up and the assists it becomes, and this is the second time it has measured redundant); **the old Spy model lost to `generic`** (71.0 against 72.3), which only pairing a hand-written model against the fallback could show; and **staying alive predicts winning almost too well** — left alone the fit gave the Sniper 0.55 and the Medic 0.65 to death-shaped components, so no model may now give dying more than half its weight, a rule a test holds. The one place the fit was overruled is the Medic: it drops ubers and drops as collinear with dying, and a Medic rating that cannot see a drop is not a Medic rating. They are in at 0.15 and 0.10 and the 1.6 points are documented as the price. |
| Q8b | **Validate more than the winner** | medium | Everything above is fitted against "did their team win", which is confounded: a player on a winning team dies less because his team is winning. It is the only label the data has, and it is why the survival cap exists at all. A second label — round win share, or a held-out human ranking of performances — would let the next model answer "how well did he play" rather than "was he on the better side". |
| ~~Q9~~ | ~~**Opponent strength** (open item 8)~~ | medium | **Built as a view, not an adjustment — and ETF2L division turned out to be the wrong instrument.** Division is stored for every official, but that is 35 logs of 749 with a division at all, spanning Low/Mid/Open/Div 3 with exactly one High match and no Premiership. It cannot weigh a pool it labels 5% of. What can: Highlander puts one of each class a side, so every match names your opposite number, and their average over their *other* games is a strength rating available for **64% of performances**. The effect is real — facing an opponent 0.20 better costs 0.076 rating points, and the bottom against the top fifth of opposition is 1.059 against 0.945. **Not applied to the rating.** Correcting for it changes split-half reliability over 147 players with 10+ games by **0.000** (0.761 either way), because opponent strength varies *within* one player's games (sd 0.070) more than it varies *between* players (sd 0.046) — there is no standing level of difficulty to subtract, only night-to-night variation. Adjusting anyway would smuggle a prior about the player into a number that is meant to describe one game, which is the opposite of what the rating is for. So it is shown: "Who you played" on the profile, three fixed bands either side of 1.00, and `hl profile <class>` prints the same. Flashy's Sniper: 1.13 against opponents averaging 0.82, 1.01 against 1.16. |
| ~~Q10~~ | ~~**Colour themes in settings** (function)~~ | small | **Bigger than "small", because the palette was only half in variables.** 58 `rgba()` literals and 47 hex literals across five stylesheets had to become roles first — the accent at three depths for a button gradient, the rules and tracks a table is drawn with, the surfaces under them — or a theme would have repainted the chrome and left orange gradients behind. Four dark themes ship: Gravel (the default), Dustbowl, Coldfront, Swiftwater. Two things are deliberately never themed: **RED and BLU**, because a scoreboard that recolours them is lying, and the **kill/death pair**, which was validated together for colour-blindness (CVD dE 19.7) and carries a meaning across every analysis view. The canvas in the kill map cannot parse `var()`, so it resolves the colours at draw time through `themeColour`. **No light theme**: the app is translucent light tints over dark surfaces, and inverting it is its own job rather than a fifth entry in a list. |
| ~~Q11~~ | ~~**Cart time in a numbers advantage**~~ (§15.1, Flashy) | medium | **Built as a match-page panel, from the kept STV timelines.** Seconds BLU was up 3+ with the cart still, split into nobody on the cart and a defender blocking; every stall of 3 s+ with its `demo_gototick`; and seconds of cart movement in the 30 s after the attackers' 1st, 2nd, 3rd won fight of a round. `hl cart <demo>`. On the three payload STVs: 64 s, 25 s and 10 s wasted a match, most of it with nobody on the cart. |
| ~~Q12~~ | ~~**Momentum** (§15.2)~~ | large | **Done (1 Oct), the first cut.** The cart panel draws each payload round as a line -- how far BLU pushed, with rollback taking ground away -- with every fight a dot (blue BLU won it, red the defence did) and the holds shaded: the cart still 10 s+ with a fight in it, how many pushes it turned back, what each side lost, where (by callout), and whether it broke or held to the end, each with its `demo_gototick`. On Swiftwater, C held 126 s and turned back six pushes. Found on the way: a map can have more than one cart entity (Swiftwater's STV has three) and Q11's panel mixed them; each round now uses the one that moved like a cart. `hl momentum <log>` prints it. |
| Q13 | **Translations** (tenshi, with boSe on French and obi on Portuguese and Spanish) | medium | **Built, with draft translations in four languages.** `ui/src/lib/i18n.ts` uses the English text as the key, so a translator sees the sentence rather than an invented key and a missing entry shows the English instead of a blank. **Russian** was added on request beside French, Spanish and Portuguese (Brazilian). **147 strings** are wrapped: the nav, every panel heading, table headers, labels and buttons across twenty components, the analysis tabs, the profile filters and every notification title -- a mechanical pass limited to static single-line text inside tags with plain attributes, so nothing inside an `onClick` could be mistaken for a label. Where a file already used `t` as a local name it imports the function as `tr`, and `scripts/i18n.mjs` finds both. All four tables are filled, **147 of 147**, and render correctly including Cyrillic. **They are drafts written without a native speaker**, and the app says so in Settings rather than claiming volunteers did them; `docs/translating.md` asks the volunteers to review rather than start from nothing. **Left:** paragraph and hint text, which is most of the remaining English and the part most likely to still change. **1 Oct:** every string filled in all four, 1,369 of 1,369 (the map-image and callout strings included). Still drafts until a native speaker reads them: that part is theirs. |
| ~~Q14~~ | ~~**Look other players up** (Flashy)~~ | large | **Built, against your pool.** It needed no new model and no new fetching: every Highlander log holds seventeen other players, all of them already rated, because the pool a rating is measured against is *built* from exactly these performances. So a player's page is `load_profile` with their account instead of yours — same model, same scale, same breakdown. A **Players** tab searches by name (across every name a player has used, since people rename constantly) or any form of Steam ID, and a player's page shows: games in your matches and the span of them, previous names, on-your-team against against-you with the head-to-head record, a row per class with games and average rating, and their best and worst games, each clickable through to the match. `hl who <name or steamid>` prints the same. One asymmetry is stated in the UI rather than hidden: you are held out of your own baseline so you are never compared with yourself, and everybody else is in it. What this deliberately is not is trends.tf — it cannot see a game you were not in, so every figure says "in your matches". Fetching a stranger's whole history is Q14b and is a different size of job: seven hundred logs per lookup against somebody else's server. |

| ~~Q14b~~ | ~~**A looked-up player's whole history**~~ | large | **Done another way (1 Oct).** Not by fetching ~700 logs per lookup: the league sample (Q33) holds every ETF2L official of six years with its logs, rated in the shared pool (Q34, which settled the pool question), so every player has ratings, stat bars and ranks (Q36); and trends.tf's page gives the rest of their career -- W-L, per-class DPM, accuracy and hours -- in one request (Q37). Pugs and scrims outside the owner's matches stay unrated, which is the point: the sample is officials. |

| ~~Q15~~ | ~~**An error console, and telling people when something fails**~~ | medium | **Done.** Three parts. An `ErrorBoundary` round every page and every `Fold`, so a thrown error is one dead panel with a message and a "try again" rather than a black window — proved by throwing on purpose: the header, the nav and every other panel stayed up. A `problems` store keeping the last 200 failures, collapsing repeats into a count. And **Settings › Problems**, which lists them with a **Copy report** button that produces markdown with the version and the counts already in it, because those are the first two questions anyone asks. Sync failures now record *why*: `explain()` turns "error sending request … (os error 10060)" into "could not reach the server", and a 404 into "the server does not have this log" — the difference between worth retrying and never will be. |
| ~~Q21~~ | ~~**A demo appearing starts a sync** (Flashy)~~ | medium | **Done.** `watch.rs` polls the TF2 folders every 10 s and waits for a file's size to stop moving for 20 s — a demo is written continuously while the match runs, so a filesystem event fires hundreds of times and never says "finished", which is the only thing worth knowing. A finished demo raises `demos://new`, the window starts a sync and shows a card naming the file. logs.tf is not instant, so it tries up to three times a minute and a half apart before saying so plainly rather than spinning. Only demos that appear *while the app is open* count: nobody wants last season's four hundred announced at startup. |
| ~~Q16~~ | ~~**The Aim tab ignores the selected player** (ivg)~~ | medium | **Half done: it has stopped lying.** `AnalysisPanel` never passed `player` to `<Aim>`, so changing the player left the aim cards showing *your* numbers under their name — ivg read their own 28° crosshair error as a teammate's. It takes `player` now, and when the selection is not you it says aim is read from your own demo and names who it cannot answer for. **The other half was Q16b**, now done: `demo_aim` has a shooter column and the pass reads every player an STV carried. |
| ~~Q16b~~ | ~~**Aim for everyone, not just you**~~ | large | **Done, and measured on a real match.** The pass took a SteamID and answered for that one player; it reads every player the demo carried now, and `demo_aim`, `demo_death` and `demo_life` each gained the player they are about (migration 0024, pass version 11). On log 4122234 that is **311 kills across 17 players, up from 29** — the same file, read properly. Three things had to be decided rather than coded. **Which demo speaks for whom:** a POV demo records its own client's view angles as the player made them, while an STV takes everyone's off the wire, quantized; so a match with both is read twice, the POV for the owner and the STV for the other seventeen, and neither player gets the second-best number available. **What a demo may not vouch for:** a POV demo carries other players only while its recorder could see them, so every shot and death now records whether the demo held that player across the whole window, the averages ignore the ones it did not, and a POV demo contributes nobody but its recorder. **That crosshair error is a hitscan measure:** the per-player means make this unmissable — the two Snipers read 1.7° and 3.2° at ~1,250 units, the Scouts 9-10° at ~270, a Soldier 27.8° with the Black Box and a sentry kill 28.1°, because a rocket and a sentry never had to be on the head. The numbers are right; calling them aim for those classes would not be, so the tab says which is which. The Aim tab now answers for whoever is selected, and when a match has only your own POV it says that is why, with the STV download a click away, instead of looking empty. **Over the whole history:** 25 matches re-read in 396 s, **1,405 kills stored against 54 shooters** where there were 508 against one — the four matches with an STV give 16 to 18 players each, the twenty-one with only a POV give the owner, which is the rule working. **Found by running it:** the route code has subtracted `tick - last_seen` since pass version 7, and a recording that spans two matches restarts its tick counter. A release build wrapped the subtraction to a huge number, which closed the route and so looked correct; a debug build panics. The seam is handled for what it is now — every open route closed and the frame buffer cleared — because a route joined across a map restart draws a line between two maps. |
| ~~Q22~~ | ~~**Ubers and numbers in the rounds panel** (Flashy)~~ | medium | **Done.** Each round now carries two strips under its lanes: who was up players and who held the uber, across that round only, on the same axis as its caps and picks — a round lost while a player down for most of it reads very differently from one lost even. The hover says the share ("up a player 42% of the round, down 23%"). It reuses the kill-by-kill panel's analysis query, so when both are open it costs one fetch, not two. |
| ~~Q17~~ | ~~**Captures weighed by what they cost** (ivg)~~ | medium | **Done, model v8.** Measured twice. First with `hl situation --caps`, against what the numbers already predicted: on stopwatch a cap into 6+ alive is worth +32.8% over its position and one into 3-5 alive +13.8%; on KOTH there is no gradient at all; and the *class* half of the suggestion does not survive splitting by mode -- every class's captures sit at 23-30% on stopwatch. Then as a rating component: `caps_contested` counts, for every capture a player is credited with, the enemies alive to stop it, read from the raw log the moment before it went in. Beside the flat count the fit calls it "unclear" in all nine classes -- it does not *add* to caps. But *replacing* caps with it, at the same weight, in the four models that use caps, is **never worse on the full sample or the held-out one**: scout 74.0 -> 74.1 (held-out +0.5), pyro 76.6 -> 76.7 (+2.0), engineer 75.2 -> 76.0 (+1.0), medic 76.0 -> 76.3 (+0.0). Small, consistently signed, and it fixes the named unfairness -- walking onto a wiped point now adds nothing -- at no cost, so it is applied. **The price:** the component needs the raw server log, so the ~200 oldest logs that exist only on logs.tf lose it rather than keep a flat count, as `untraded_deaths` already does. `docs/rating-formula.md` is updated and still matches the TOML on all 62 weights. |
| ~~Q18~~ | ~~**Manual demo upload**~~ (beowulf) | large | **Done: the demo becomes the log.** `hl_demos::synth` writes a server log (spawns, kills with positions, damage, assists, ubers, Medic deaths, rounds, captures) and a logs.tf summary from the timeline, so every pass runs unchanged. Checked against the real log of a match its STV covers whole (swiftwater): kills 309/309, deaths 313/311, damage 98.6%, ubers, drops, headshots, backstabs and caps exact, heals 91%. Settings > Import > A match with no log; `hl import-demo`. See §23. |
| ~~Q19~~ | ~~**The match page has too much furniture** (Flashy)~~ | small | **Done.** The "Combined from N logs" panel is gone: the part logs are links in the header beside logs.tf, demos.tf and ETF2L, which is what they always were. The "Reading" select is gone as a panel too and now sits in the scoreboard's header, next to the numbers it scopes. Two panels removed from the top of every combined match. Titles trimmed where they were sentences. |
| ~~Q20~~ | ~~**Fewer explanations, everywhere** (Flashy)~~ | small | **Done, second pass.** The long `title=` tooltips were the same prose hidden behind a hover — the matchup one was 300 characters and is now one line. Seasons, teammates, the scoreboard and the profile's empty state all lost their paragraphs. The rule stands: cut what explains the thing the reader is looking at, keep what they cannot infer. |

| ~~Q23~~ | ~~**Delete downloaded demos when they have been read** (Flashy)~~ | medium | **Done.** A missing demo no longer aborts the aim pass and no longer wipes what it produced (verified: 24 of 25 matches read, the missing demo's 43 rows preserved, totals identical to a clean run). `prune_demos` used to delete the row of any demo no longer on disk, cascading its link -- which threw away the `demos_tf_id` that is the only way to fetch it back; a downloaded demo is now marked `deleted_at` and kept, while a POV demo, which has no way back, is still forgotten (its derived rows stay, orphaned and untouched, because the pass only walks linked logs). **Settings > Downloaded demos** lists what the app fetched, deletes one or all, and refuses anything the pass has not finished with; **Delete after reading** is a toggle, off by default, swept right after the aim pass. **Getting one back** reuses the existing download: a deleted STV no longer counts as one the match has, so the match page offers the demos.tf download again with its progress bar, and re-indexing the file clears `deleted_at`. `Copy playdemo` is replaced by a note for a deleted demo rather than handing over a command for a file that is not there. See §16. |

| ~~Q24~~ | ~~**Say what it is doing, everywhere it does something slow** (Flashy)~~ | medium | **Done.** The aim pass's progress is spent rather than discarded -- `|_, _| {}` was eating the count of the slowest job in the product, 396 s for 25 demos, all of it after the card said "Sync finished" -- and names the match it is on. `fights::derive_all` takes a callback and reports. Five stages that ran silently now name themselves: the profile refresh, demos.tf matching, the demo folder scan, round-map resolution and the rebuild's ETF2L pass. **The folder scan keeps a name rather than a fraction on purpose:** measured at 0.6 s for 109 demos in a debug build, which is exactly the case §17's rule is for -- short work gets named, long work gets a bar. The inline-bar idea for work started from a page is dropped for now: nothing started from a page currently takes long enough to need one, and the STV download, which does, already has its own. See §17. |

| ~~Q25~~ | ~~**Penalize delaying your own team's spawn**~~ (ivg, boSe) | medium | **Applied, measured the §18b way.** The head count (`caps_mates_dead`) still reads as rewarding; the *delay* reads the other way. A KOTH cap that cost a dead teammate 8 s+ over their usual wait won 16 points less often than the numbers predicted (715 caps), one costing under 3 s won 11 points more. `caps_spawn_delay` (seconds of those long delays, per capper) is in the fights pass (v7): the player with less of it was on the winning side in all nine classes (54.7-60.3%). Weighted where never worse, full and held out: Scout 0.10, Soldier, Pyro, Medic 0.05, Sniper 0.10. See §18b. |
| ~~Q26~~ | ~~**Demo and Sniper valued by mode and side**~~ (ivg) | medium | **Closed: measured, half right, and not applied.** `hl situation --victims` now also splits stopwatch by the killer's side, each side against **its own** baseline -- pooled, every attacking kill read +30% and every defending one -26% whoever died, because the attackers win a stopwatch round by capping and the defenders by the clock. **On KOTH, ivg is right:** Demo +7.40% and Sniper +6.09% over the situation, against a +6.48% average, within 2 SE of each other. **On payload, neither claim shows up:** attacking, Demo +3.71% / Sniper +3.25%; defending, Demo +1.97% / Sniper +2.63% -- both gaps under half a standard error. So the data says Demo = Sniper everywhere, where the live table has them at 2.2 and 1.8. **But equalising them at 2.0 makes the rating slightly worse at its one job:** lower on the full sample in 7 of 9 models and held-out in 6 of 9, about -1.7 points in total. The same thing Q4 found: a kill's worth measured against the round and a victim value that helps pick the better player are not the same number. The 2.2/1.8 split stays. |
| ~~Q27~~ | ~~**Spychecking**~~ (ivg) | large | **Built as a match-page panel, not a rating component.** Read off the kept STV timelines, so it works after the demo file is deleted. 10-36 per match across the 5 STVs on disk; see §20. Still a rating component only if STV coverage becomes normal. |
| ~~Q28~~ | ~~**Callouts, positions and tendencies** (Flashy)~~ | large | **Built; the callouts are drafts.** Zones in game units, one JSON file a map: seeds in `callouts/`, the owner's copy in `<data>/callouts/` always wins. Product is drawn (27 zones from the TF2 wiki's descriptions; 69% of 65,266 kill positions land in one, RED/BLU mirror to within 3%); Upward, Steel and Swiftwater ship their wiki names unplaced; Vigil, Ashville and Proot have no written source. The kill map draws the jigsaw, counts kills per zone, and has an editor (click corners, name, save). A Positions panel shows each player's time per zone from the STV. **Tendencies done (1 Oct):** the profile's "Where you play" panel, per map with drawn callouts, for the class shown: where the player's kills and deaths happened over every match with a server log, and -- where STVs exist -- where they stood and the moves they make most, zones read by their own side ("Own Left"). On a copy, the owner's Sniper on Product: 22% of fights on their Left, then China and Rock; standing Left, Point and Valley. `hl tendencies <class>` prints it. See §21. |
| ~~Q29~~ | ~~**Teams and seasons** (Flashy)~~ | large | **Part 1 done: a Teams tab.** A year of ETF2L Highlander (three seasons, 19 competitions, 430 results on the first fetch): division tables, the season's pool, and a page per team with record, win % per map, results and who played, with each player's rating where your pool has one. A sync reads 60 match pages, so the per-map scores and rosters fill in over a few. **Part 2** (best players rated across all their games) done by Q34-Q36: the league in the pool, a rating and rank for every player. See §22. |
| ~~Q30~~ | ~~**Maps recognised from the STV when logs.tf cannot say** (Flashy)~~ | medium | **Done (29-30 Sept).** A linked demo's header now names the map of every round inside its recording, ahead of the raw log's map line (it agrees with the log's own field on 77 of 77 rounds, and beat the map line both times they differed); linking or downloading a demo re-resolves at once; and the app ships map shapes for 24 maps (`maps/geometry.json`), so a fresh install recognises Vigil from kill positions alone. With every map name logs.tf gives wiped from a copy of the database, 92 of 94 rounds with a demo and 95% of the rest still resolve correctly, against none before. The kill map says when the map is unknown and how to fix it. The missing images were first addressed by shipping more.tf's renders; additional map images and boundary data are from demos.tf. **Done (30 Sept):** a "this was on ___" picker under the kill map, and demos.tf now finds the STV of a log whose map is unknown, by time alone. See §24. |
| ~~Q31~~ | ~~**A Maps section in Settings, with top-down image import** (Flashy)~~ | medium | **Done (30 Sept).** Settings, Maps: every map you played (the rest on ask) with where its image, placement and callouts come from, how many matches were on it, and how many matches have a round on no known map. Import a PNG, JPEG or WebP for any map; it wins over the built-in one, and Remove mine goes back. Line up drags this install's kill positions over the image and scrolls to scale; the placement is saved beside the image. Images need not be square. See §24. |
| ~~Q32~~ | ~~**Callout presets: import and export per map** (Flashy)~~ | small | **Done (30 Sept).** `<map>.callouts.json`: the stored shape plus `format` and an optional author. Export from the Maps row and the kill-map editor; Import checks the file (format, every zone named with three real corners, under 1 MB), says what it replaces, warns when the file is for another map, and keeps the replaced copy for one Undo. A `.callouts.json` dropped anywhere on the window imports to the map it names. See §24. |
| ~~Q33~~ | ~~**A league sample: ETF2L officials from every division** (Flashy)~~ | large | **Done (30 Sept).** 812 officials (1,832 logs, 203 MB) from all five tiers, March 2022 to September 2026, every map in each division's pool, downloaded in the background at one request every ~6 s with more.tf filling in while logs.tf rests, and every official's roster for the player catalogue. Every tier is short of 300 because that is every official trends.tf links. Kept in its own tables. See §25. |
| ~~Q34~~ | ~~**The league in the rating pool, the owner included** (Flashy)~~ | medium | **Done (30 Sept).** The pool is the owner's 13,622 performances plus 44,437 from 2,469 league-sample logs, nobody left out; the scale is the league's. On a backup: the owner's Sniper 1.050 -> 1.018 over 664 games, and every Sniper in the owner's matches 1.024 -> 0.995 -- a level shift (against the whole league, Premiership included, the owner's matches read slightly below average), not a reshuffle: the owner's lead over them is +0.026 before, +0.023 after; games move 0.02-0.04 on average. (A first before/after ran a stale binary and showed no shift; corrected.) Picking the winner of the owner's matchups, old -> league pool: -0.2 points on all matches, -0.6 held out (lower in 7 of 9 classes, all inside one standard error of ~2.7), accepted because Q36's ranks need the league scale. See §25. |
| ~~Q35~~ | ~~**Player profiles: who they are, their teams and medals** (Flashy)~~ | large | **Done (30 Sept).** The Players tab searches the whole catalogue (every ETF2L official of six years, and the owner's matches) with a division badge, main class and medal counts per row; a profile has the header (avatar, country, aliases, main class played and declared, current team, highest division, "1x Low winner"), a trophy strip, and Overview / Teams / Achievements / In your matches tabs. Medals: a playoff Grand Final's winner and loser, the 3rd Place match's winner, and the final table where a division had no playoffs, only once a season is over; three tests hold the rules. On the owner: gold in S33 Low with SBQRRA (Grand Final 6-3), matching ETF2L. Also fixed on the way: the app's image policy blocked every remote image, so the Teams tab's team avatars never showed in the built app. See §26. |
| ~~Q36~~ | ~~**Ratings for everyone: ranks and stat bars** (Flashy)~~ | medium | **Done (30 Sept).** The league sample's 44,437 ratings are kept (`league_rating`, with each game's component groups); every profile shows the rating per class (last three months played, else the career), best, and HLTV-style stat bars -- Kills and damage, Staying alive, Playing for the team, Objective, Medic, Speciality, each the weighted percentile against the league; its ranks ("#3 Sniper in Low, S33", eight games or more in that season and division); and the Players tab a Top players table per class, division and season. On the owner: Sniper 1.00 recent, 1.02 career over 664; #3 of 7 in Low S33, #6 of 7 in Mid S34. See §26. |
| ~~Q37~~ | ~~**Career numbers from trends.tf, on demand** (Flashy)~~ | small | **Done (1 Oct).** The profile's overview reads the player's trends.tf page (Highlander only) when it opens, kept a day in `trends_career`: W-L-T, winrate and hours of Highlander, and per class W-L-T, winrate, DPM, accuracy and hours; aliases and teams are parsed too. Each table found by its heading, one missing left empty; trends.tf down shows the last copy and says so. A test holds the parser to a saved copy of Flashy's page (458-412-25, Sniper 429-391-22 at 345 DPM). Credited with a link. See §26. |
| ~~Q38~~ | ~~**The division of the people you play** (Flashy)~~ | medium | **Done (30 Sept).** Every player on a match page carries their ETF2L division at the time -- the season the match was in, else their nearest season within a year, shown faded and dashed -- beside their name in the scoreboard and the class matchups; the matchups header gives each side's average ("BLU ~Mid (2.1, 9/9)"). On a S36 High official 17 of 18 players read High S36; a pug ran from Premiership to Low. The profile's "Who you played" by division followed (1 Oct): the opposite number's division at the time, Premiership to Fresh Meat; on the owner's Sniper, 1.05 against Premiership (10 games), 0.91 High, 0.99 Mid, 1.09 Low, 1.18 Open. See §26. |
| ~~Q39~~ | ~~**Refit the models on the league**~~ | medium | **Done (1 Oct): model v9.** `hl validate <class> --league` now scores the league's officials as matchups too, with each one's division: ~4,650 a class, 3,900 of them ETF2L. Seven classes refitted and kept only where better on all, before and after S34: Heavy 74.9 -> 79.0, Spy 71.6 -> 74.4, Engineer 74.9 -> 77.2, Scout 75.6 -> 76.8, Sniper 75.5 -> 76.3, Pyro 76.8 -> 77.3, Demoman 80.0 -> 80.3. Soldier and Medic keep v7 (refits 0.5 and 0.3 worse; the Medic keeps ubers and drops by judgement). DPM hurts or is unclear for all nine (SchmitShot answered). One model serves every division: no division's own fit beat the pooled one beyond noise. Dying still held to half of any model. See §25 and the notes in weights.default.toml. |
| ~~Q40~~ | ~~**A player card from the scoreboard** (Flashy)~~ | medium | **Done (1 Oct).** Click a name on the scoreboard or in the matchups (or Enter on it): a card beside it with their avatar, division then and at best, medal tiles with counts and best title, main class rating (last 3 months, else career) with their best rank, the class played here when it is not their main, four stat bars, you and them, and Open profile / ETF2L / trends.tf. Escape or a click elsewhere closes it. Built from the profile's own queries (same keys), so no new command. See §26. |
| ~~Q41~~ | ~~**MVPs per class for every event** (Flashy)~~ | medium | **Done (1 Oct).** HLTV-style: for each Grand Final (played, not forfeited), the best player of each class on the two finalists -- 70% their final rating, 30% their playoff run before it -- and the event MVP, the best player of the team that won. A candidate played more than half of the final's logs (a sub with one good half is not the MVP); a final with no logs held has none, rather than a guess. Violet star tiles and titles on the profile ("1x Open Sniper MVP"), listed in Achievements with the numbers. 64 events on a copy; `hl mvp [season]` lists them, `hl medals [season]` the medals. |
| Q42 | **A Linux build** (Flashy) | medium | **Built, waiting on a first Linux run.** The code was nearly portable; three things were not. The app lock was a bare file on Linux, so a crash would have locked the app out until someone deleted `app.lock`: it is now an `flock` the kernel drops with the process. TF2 detection only looked in Program Files and drive letters: it now also finds the native, Flatpak and Snap Steam folders. The updater assumed an installer it could run: an AppImage updates itself, a `.deb` opens the download page. GitHub builds the AppImage and the `.deb` on Ubuntu 22.04 from the release tag (`release.yml`, which replaces a Windows job that had failed on every tag since 0.4.1 for want of the key) and adds Linux to `latest.json` when the key is a repository secret, the maintainer's decision (docs/releasing.md). CI now runs on Linux too, and installs the root packages it had been missing since 0.6.0. **Left:** someone on Linux to try the first build; nobody has run it yet. |
| Q43 | **The league in every install: a snapshot** (Flashy) | medium | **Built, waiting on the release.** The league sample is ~1 GB, almost all of it logs.tf's JSON (180 MB) and raw server logs (471 MB, already zipped one by one, so they barely compress further). What the player lookup, medals and MVPs read is small: ETF2L's seasons, rosters and teams, who played each official, and the league ratings -- with the pool's baselines and scale, so a new install rates on the league's "1.00". `hl snapshot` writes those from a rated copy of the maintainer's database (14 MB gzipped, most of it the v9 baselines, kept exact so everyone's numbers match), the app imports it once per snapshot at startup, rows already present winning. Its officials are marked `snapshot`, so the league download skips them, and `rate_all` keeps its scale while the install's own pool is smaller. The owner's own officials, rated as the owner's games, are rated as league games for the snapshot so their lookup is complete. A snapshot for another rating model is not imported; `npm run release` refuses to build with one. A website serving fresh data will replace it later. |
| Q44 | **Ping from demos** (Flashy) | small | **Built, waiting on a release.** Every demo carries every player's ping: TF2's scoreboard object (the player resource) reaches every client and holds it, so STV and POV demos alike have it, about once a second, smoothed as the scoreboard shows it. Checked on two POV demos (Product, Sunshine): every player's ping changing through the match, 5 to 101 ms, lag visible (YuvalOs 59-101 ms on Product). The timeline (v2) records it per player on change; a match page's **Ping** panel shows each player's time-weighted average, lowest and highest, a line over the match, and spikes -- 40 ms or more above their own median. Not packet loss (TF2's scoreboard has none), and not per-shot latency. Timelines from before v2 are re-recorded the next time their demo is read, if the file is still there. |
| Q45 | **Pyro reflects** (ivg, ImABush) | medium | **Built, waiting on a release; the rating does not use it yet.** The server logs only reflect *kills* (`deflect_rocket`), and TF2's `object_deflected` event never reaches a demo. What does: every rocket and pipe carries how often it has been reflected (`m_iDeflected`), its owner, team and path. The deep pass follows projectiles; when the counter rises it records the reflect (who, what, where, which way it was flying), and where the projectile ended. Each reflect is a **hit** (the Pyro damaged an enemy as it landed, or a reflect kill), a **miss**, **sent back** again, or **not seen** (out of a POV demo's view). ImABush's "would it have hit you or a teammate" is an estimate: the path carried on, rockets straight and pipes with gravity, passing within 110 units of the Pyro or a living teammate inside 1.2 s -- walls are not known. On the owner's Product POV demo: Tezzix 10 reflects, 1 hit (84 on sillybilly94), 3 misses, 6 not seen (the POV limit), 4 headed at the team. Match page: a **Reflects** panel per Pyro, and every reflect with its `demo_gototick`. STV demos give the full count; using it in the Pyro rating would need it on every game, which logs cannot give. |
| ~~Q46~~ | ~~**Teams: seasons as tiles, podiums, a team page like a profile** (Flashy)~~ | medium | **Done (3 Oct).** The dropdown is gone: every season is a tile (ETF2L's news banner where a post has one, else a styled card; dates, divisions, teams, champion, your team and finish; the live season marked). A season opens on its podiums -- each division's gold, silver, bronze and MVP -- then its division tables. A team page is built like a player's: header with avatar, flag, latest division, record, medal titles and tiles, ETF2L link; tabs Overview (recent officials, lineup with class icons linking to profiles, the current pool's maps), Roster, Seasons (division, W-L, finish), Results. `hl season-tiles [--banners]`. |
| ~~Q47~~ | ~~**Spy, model v10** (Taiga, Suomipe)~~ | small | **Done (3 Oct).** From the Spy mains' answers: a kill's worth is who and when, not how, so backstabs leave the Spy's weights; Medic picks come in at 0.10 (Suomipe's swap); impact kills 0.25, assists 0.10, deaths 0.15, untraded 0.10, untraded deaths 0.15, fight KAST 0.10. Backtest agreement 75.5 -> 75.8 overall, unchanged after Season 34. A full re-rate runs on the next start. |
| ~~Q48~~ | ~~**More from ETF2L's API** (Flashy)~~ | medium | **Done (3-4 Oct).** Every join and leave from `/team/{id}/transfers` and `/player/{id}/transfers`, kept in `etf2l_transfer` (migration 0037) and shipped in the league snapshot. Medals: a team's medal goes to the players still on its roster at its last match of the season -- 17 withheld on the first 10 teams read, each checked by date (Anglo S35: four left in March, the team played on to 31 May); the profile marks those seasons *left early*. Team pages get a Roster history tab (on the roster now with time on the team, who came and went, every join and leave by year with the leader who made it); profiles a Team history with dates, 6v6 and fun teams included. A dev build reads every team's list in the background, medal winners first, then the active teams every six hours; a release reads a team's or player's list when its page opens. Team info from `/team/{id}` (kept in `etf2l_raw`, kind `team`; a day while the team plays, a month after): its tag and links in the header, former names (*Formerly*), ETF2L's role for each current member (Leader, Deputy, Buddy, Inactive: a chip by their name, inactive ones dimmed) in the lineups and roster history, and a Cups list on the Seasons tab -- every Highlander cup it entered, one entry per cup over its stages, the placing from ETF2L's awards. Fixtures: ETF2L's whole scheduled list (`/matches?scheduled=1`, a handful at a time) read at most every half hour, each match with its pairing's head-to-head from the database; shown as *Coming up* on the seasons grid, *Upcoming* on a season, *Next match* on a team (that team on the left); a playoff's division taken from its competition's name. Head-to-head on a team's Results tab: every team met more than once with a W-L bar, one click for just those matches (no request: the team's own results). Also (Flashy, 3 Oct): when the app opens it asks logs.tf (trends.tf if refused) for your 30 latest logs and syncs by itself only when Highlander ones (16-22 players) are not stored yet, with a *New logs* card saying how many since it was last opened. ETF2L demos (4 Oct): a match demos.tf has no demo for offers *SourceTV demo on ETF2L* when the stored match page lists one; the download button then fetches every STV upload of the match (zips of one or more `.dem`; RAR and first-person ones left alone), and each demo is checked against the match's logs the way a dropped demo is (map, shared players, kills at one shift), so each map's demo links to its own log on its exact clock; a demo of no stored log is deleted. A page read over a day ago is read again first. Tested on ETF2L's zip for match 93065 (one 56 MB pl_vigil_rc10 SourceTV demo); none of the owner's stored officials has an ETF2L STV upload, so the whole chain waits on a real one. Also: the transfers reader waits 8 s between teams (2 s hit ETF2L's limit about once a minute), and the CLI runs on a 64 MB stack (`hl fixtures` overflowed the main thread after the 3 Oct merges). |
| ~~Q49~~ | ~~**Movement map: breaks at teleports, and the buildings** (Emiel)~~ | medium | **Done (7 Oct).** A route no longer draws a straight line across the map when a player teleports: a step faster than 3,500 units a second (and over 400 units) breaks the line, still one route for hover and pick, with a faint dotted link on the picked one. Sentries, dispensers and teleporter entrances and exits are drawn in their team's colour from the demo timeline's object rows (`hl_ingest::buildings`), only those standing during a picked life. Demos read from now on record a teleporter's end; older ones have it worked out from the routes' jumps. The builder is the game's entity handle (low 11 bits). |
| ~~Q50~~ | ~~**drops.tf first for logs, and ETF2L's new address** (Flashy; drops.tf by Icewind)~~ | medium | **Done (7 Oct).** Log JSON and raw server logs from drops.tf first (logs.tf's own, checked: raw logs byte for byte on six logs from 600,321 to 4,132,080; JSON the same but for the last digit of a few averages), logs.tf and more.tf behind; the league downloader keeps going while logs.tf rests. ETF2L's v2 API from `api.etf2l.org` from 1 Nov 2026 14:00 CET, `api-v2.etf2l.org` as the fallback either way. Icewind credited in Settings › Data sources, the League sample counts and the README; the release notes still to do. |
| ~~Q51~~ | ~~**Demo linking on combined logs** (ivg)~~ | small | **Done (7 Oct).** A dropped demo may be on any of the log's maps: the map field split at `+` and `,`, each round's segment map and each part's. Linked to the combined log; its kills line it up on that part's stretch of the clock. |
| ~~Q52~~ | ~~**A demos.tf STV the app does not find** (Emiel)~~ | small | **Done (7 Oct).** Two causes. demos.tf lists 50 demos a page, not 100, so the walk's 20 pages reached 1,000 demos back, and this match was 39 pages (1,950 demos) down Emiel's list; the walk now resumes where it stopped (`demostf_walk`: newest seen, oldest reached), so deep history is reached over a few syncs at 20 pages each. And a combined log can hold one id, where its STV is one demo per part: the parts are matched too (each uploaded seconds after its own log: 990239, 990254, 990266 to 3426174, 3426198, 3426219, checked in a test), and downloading the combined log's STV fetches every part's. |
| ~~Q53~~ | ~~**Add a demo by its demos.tf link** (Emiel)~~ | small | **Done (7 Oct).** Demo linking has an *Add by link* box: `https://demos.tf/990239`, `demos.tf/990239` or `990239`. Downloaded into `tf/demos/stv`, checked like a dropped demo (map, players, kills), linked with method `demos.tf` and its id kept so it can be fetched back; a demo that is not this match's is not kept. |
| ~~Q54~~ | ~~**A real STV refused: "none of its kills line up"** (Emiel)~~ | small | **Done (7 Oct), not run on Emiel's file.** The likely cause: an old match's server log is fetched a few hundred a sync, newest first, and until then the match has no kills to line a demo up with, which read as "none of its kills line up". Linking a demo (dropped, by link, or from ETF2L) now fetches the server log first (`kills::ensure_one`), and a match with none says so. drops.tf has this one's (780 kills, 19:08 to 20:02). To check on the real demo, 990266 needs downloading. |
| ~~Q55~~ | ~~**The score should be ETF2L's for officials** (Clark)~~ | small | **Done (7 Oct).** An official's headline score on the match page is ETF2L's result, your side first, with Victory or Defeat from it; the logs' rounds sit beneath when they differ ("Rounds in the logs 3–9"). The Matches list shows ETF2L's result and W/L too, the logs' rounds in the tooltip. |
| ~~Q56~~ | ~~**Filter: matches with their STV downloaded** (Emiel)~~ | small | **Done (7 Oct).** A *With STV* chip at the end of the class row: only matches with a SourceTV demo linked on this machine and not deleted (`MatchFilter::stv_only`). The class and map counts follow it. |
| ~~Q57~~ | ~~**A player's lives on one map, by class and side** (Emiel)~~ | large | **Done (7 Oct).** A *Lives on a map* tab on player profiles: pick a map (the ones a demo followed them on), a class and a side, and every route is drawn on the map's picture, RED and BLU in their colours, a dot where a life ended in a death; hover one for its match, round, length and demo. The class and side are read from each demo's stored timeline at the life's middle (`hl_ingest::lives`), so no demo is read again; the map is the life's round's. It says how many matches that is and how many had an STV. Checked on a copy of Flashy's data: Product 143 lives in 10 matches (Engineer, Scout and Sniper, both sides), Upward 57 split 29 BLU / 28 RED. `hl lives <player> [map]` prints the same. |
| ~~Q58~~ | ~~**Import a log from the Matches page** (Flashy)~~ | small | **Done (7 Oct).** An *Import* button beside Refresh opens a small box for a log id or logs.tf link, fetched there and then; a log you are in opens straight away, one you are not in says it joined the pool. The Settings box stays. |
| ~~Q59~~ | ~~**Class counts follow the filters** (Emiel)~~ | small | **Done (7 Oct).** The class chips count what the format, Officials / Scrims / Pugs, the period and the map leave; the map list counts what those and the class leave. A picked class or map stays on offer at 0 so it can be unpicked. |
| ~~Q60~~ | ~~**Bookmarks** (Clark)~~ | medium | **Done (7 Oct).** A ☆ beside the name on match, player, team and season pages keeps the page; a ★ in the top bar (with a count) lists them by kind, newest first, to open or remove. Kept in the database (`app_config` key `bookmarks`, JSON), with the name the page had when kept. Seasons open from it through a new `openSeason`. |
| ~~Q61~~ | ~~**Sort matches by classes killed or died to** (Clark)~~ | medium | **Done (7 Oct).** A *Sort by a class* picker beside it: *Kills on Soldier*, *Deaths to Sniper* and so on, counted from the stored raw logs as logs.tf counts kills (in a round, no feigns), with the count as a sortable column. A match with no raw log yet shows a dash and sorts last. The class is checked against the nine before it goes into the SQL. |
| Q62 | **demos.tf only goes back so far: say so, and stop looking before it** (a tester's DM) | small | demos.tf's oldest downloadable STV is thought to be [312627](https://demos.tf/312627). **Ask Icewind to confirm first.** Once he does: no demos.tf lookup for a match older than that demo's date (a hard limit, so old matches stop costing a request each), and an info line where the STV lookup sits ("demos.tf's STV demos go back to <date>; this match is older") instead of "demos.tf has no STV demo for this match". Mention the limit in Settings › Data sources and in the README's demos.tf row too. |
| ~~Q63~~ | ~~**Set a match's kind by hand** (Flashy)~~ | small | **Done (7 Oct).** The Official / Scrim / Pug badge, on the match list and the match header, is a menu: Official, Scrim, Pug, or Automatic. Kept in `match_kind_override` (migration 0038) with what the context pass had decided; the pass applies it over its own every run, so a sync never undoes it, and Automatic puts the pass's kind back at once. Shown with a ✎ and `link_method = 'manual'`; every filter, count and split by kind follows it. The menu is fixed-positioned so the list's clipped cell cannot cut it off. |

### Reported by testers, and fixed

| # | What | Reported | Cause |
|---|---|---|---|
| ~~B7~~ | ~~**A hook's dependency array changes length between renders**~~ | found while verifying Q23 | **Not a bug: a hot-reload artefact, closed with the evidence so nobody chases it again.** React's two arrays are two versions of `KillMap`'s canvas effect. `c2ed192` had eleven deps -- `img, view, heat, heatColor, frame, display, W, H, scale, paths, focus`; Q10 (`a3c34aa`) added `heatResolved, killColour, deathColour` right after `heatColor`, making fourteen. The logged values line up one for one: `var(--accent-2)` is `heatColor`, the three colours that appear are the three that were added, in the positions they were added, and `636, 640, 3.55` are `W, H, scale`. The browser pane served a cached pre-Q10 module and Vite swapped in the current one; React compares a hook's deps across a hot swap and complains once. The literal is a fixed fourteen, so it cannot change length in the built app or on a clean load. |
| ~~B6~~ | ~~**Reading one half of a combined log broke the page**~~ | Flashy | `Cannot read properties of undefined (reading 'map')`. Two commands built two different shapes for the same page. `get_match` returned a private `MatchResponse` that flattened `MatchDetail` together with `context` and `segments`; `get_parts` and `fetch_part` returned a bare `MatchDetail`, with neither. The TypeScript declared one type for both and asserted it at the `invoke` boundary, where nothing is checked -- so picking a part handed the header an object whose `segments` was `undefined` and `d.segments.map(...)` threw. The mock hid it: it spread the combined log's own segments onto every part, so a browser never saw the shape the built app received. Fixed at the source -- one `MatchView` in `hl-ingest`, used by both commands, so the two paths cannot drift again -- with `?? []` left in the header as a belt and the mock corrected to give a part no segments, which is the truth. |
| ~~B1~~ | ~~**Queued demo downloads never start**~~ | Gilaric | There was no queue. `fetch_stv` took a busy flag and *refused* a second download, while the window had already drawn a card for it — so it sat at "0 MB so far" until you cancelled and started it again, by which time the first had finished. Now there is a real queue: one at a time, in the order asked for, `stv://queued` says where each one is, and dismissing a card that has not started cancels it. |
| ~~B2~~ | ~~**Rating over time squashes after "Show as table" twice**~~ | Anonymous | The ResizeObserver was attached in a `useEffect(..., [])` to an element the table toggle unmounts. A detached element reports 0×0, so the observer fired once with zero and the width clamped to its 320 minimum; coming back built a *new* element the observer was no longer watching, so it never recovered until a reload. Fixed by `useMeasuredWidth`, a ref callback that follows the element, ignores zero outright, and gives the chart a `viewBox` so a stale measurement scales instead of stubbing. The other three charts use it too. |
| ~~B5~~ | ~~**Picking a season knocked the filter row out of line**~~ | Flashy | The season's date range was a plain span beside the period select, inside a flex box in a fixed grid column — so choosing a season made that cell taller and everything beside it shifted. The same defect as B4 one element along. It is now positioned in the bottom-right corner of the filter row with its line always reserved, so picking a season changes nothing about the layout: measured before and after, row height 124px both times and neither select moved a pixel. |
| ~~B4~~ | ~~**Custom dates broke the page, and F5 would not clear it**~~ | ivg | Two faults. The period is saved across restarts and was read back with `JSON.parse(raw) as Period` — a promise, not a check — so a value that breaks a render breaks every render, including after a reload. It is validated now, and anything malformed is thrown away rather than kept. Dates the wrong way round are read as a range instead of selecting nothing, and `toInput` returns "" for a non-finite number rather than letting `new Date(NaN).toISOString()` throw. The layout half: the picker sits in a 230px grid column and "Custom dates" adds two date inputs to it, which wrapped inside the cell and collided with the row below. **Found while fixing it:** the validation helper was a `const` arrow used by `load()`, which runs while the module is still evaluating — the resulting ReferenceError landed in load's own `catch` and silently reset everyone's period to "all time". A function declaration now. |
| ~~B3~~ | ~~**"2 failed" with no way to see which**~~ | KamikaZe | A sync counted its failures and said nothing more. Settings now lists every log that would not download with its reason and attempt count, retries one or all of them, and takes a log id or logs.tf link to fetch on the spot — for a match no index ever listed, or one logs.tf holds under a second id. Also `hl failed` and `hl import <id\|url>`. |

Smaller open items stay in §8 and are folded into whichever job touches them:
demo linking per map segment (18) and the in-game jump-tick check (5) belong
with Q3; the untested STV download (6) and the file watcher (7) ride along
with it.

---

## 14. Deep demo parse (Q3)

**Why.** logs.tf answers what happened. A demo answers how: where the Sniper
was looking, how far the shot was, how long they held the angle, how quickly
they reacted, how much of the game they spent scoped. None of that is in any
log, and the demos are already on the machine (101 here, 25 linked to matches).

**What is read today.** More than this section once described. The 1072-byte
header gives the map, duration and tick count, and Demo Support `.json`
sidecars give killstreak ticks — but the body is parsed too, through
`tf-demo-parser`. `hl-demos::aim` walks every tick and, for **every player
the demo carried** (Q16b, not only the recorder), reports the crosshair error
at each kill and a second before, the flick, the range and height, where the
killer stood relative to the victim's view, who was near enough to help, the
share of time scoped, and a route per life. What is still unread from the
list below is **reaction time** — the moment a victim first became visible —
which needs line of sight against the map's geometry rather than against the
demo alone.

**What a parse gives.** A TF2 demo holds the server's snapshots: every
player's position, view angles, health, class and weapon, tick by tick (66 or
so a second). From that, per kill and per life:

- **Crosshair placement**: the angle between where the Sniper was looking and
  the head of the player they killed, in the seconds before the shot.
- **Reaction time**: from the victim first being visible-ish (in the Sniper's
  view cone and in the open) to the shot.
- **Flick size**: how far the view moved in the half second before the kill.
- **Engagement range**: the distance of every kill, already possible from the
  log's positions, but exact here, with height.
- **Scoped share**: how much of the life was spent zoomed, and how long each
  hold lasted before a shot.
- **Where the rest of the team was**: the distance from the Sniper to their
  nearest teammate when they died — the "nobody was watching my flank" number.

**How.** `tf-demo-parser` (the crate behind demos.tf) reads TF2 demos in Rust.
The work is a spike first: parse one linked demo, print the owner's ticks, and
time it. If a demo takes seconds rather than minutes, a background pass over
linked demos is worth building; if not, only the seconds around each kill get
parsed.

**Where it goes.**

- `hl demo parse <file>` in the CLI first, so the numbers can be checked
  against the demo by eye before any screen is built.
- A `demo_tick` or per-kill `demo_detail` table, derived, rebuildable.
- The match page's play-by-play gains the aim numbers per kill; the profile
  gains scoped share and crosshair placement as trends.
- Rating comes last, and only for what validates: crosshair placement and
  reaction time are the candidates.

**Risks.** POV demos hold only what the recording player's client received, so
teammate positions are partial. Old demos may use protocol versions the parser
does not know. Both are checked in the spike before anything is built on top.


### 14b. The demo, kept (built 27 Sept 2026)

**The shift.** Every pass above asks the demo one question and keeps only
the answer; the next question means reading the file again, and a demo
deleted to save space (Q23) can never be asked anything new. So the aim
pass now records the whole demo in the same walk, once, into
`demo_timeline`, and later passes are derived from that. `hl-demos/src/
timeline.rs` has the format; in short:

- **Samples**, every tick, for every player alive and carried: position and
  view angles, as per-player deltas, deflated.
- **Changes**, at the exact tick: health, class, team, alive, carried, the
  full condition bits, Medic charge, medigun and heal target, Spy cloak and
  disguise.
- **Objects** on change (the cart, every building) and **every game event**
  exactly as the parser decoded it (hurts, deaths, captures, spawns, charges).
- One time counter across a recording's restarts, with seams back to the
  demo's own ticks.

Reading it back: `Timeline::decode`, then `now(slot, t)`, `stretches(slot)`
(a player's match as spans of unchanging state), `ticks_where(slot, test)`,
`sample_near(slot, t)`, and the events. `hl kept [DEMO_ID]` lists what is
kept and derives from one with no file: uber time, time burning, scoped
share, Medic heal uptime and pops, captures.

**Chosen by measurement** (`hl timeline <file> --stride N`, three demos):

| every | stored (72 MB STV) | "a second before", timeline vs demo |
|---|---|---|
| 1 tick | 2.8 MB | 0.05° mean, 0.8° worst |
| 2 ticks | 2.1 MB | 0.9° mean, 124° worst |
| 4 ticks | 1.4 MB | 1.5° mean, 124° worst |
| 8 ticks | 1.0 MB | 2.1° mean, 29° worst |

Every tick: the questions still to come (reaction time, flicks) live in
exactly the ticks a coarser record drops. Scoped share from the timeline
matches the demo's to 0.0000 on STVs and 0.0016 on a POV demo. Across all 25
matches with a demo: 28 kept, 92 MB, 1-7 MB each; the whole pass takes 91 s.

**Two bugs found on the way, both fixed (aim pass version 12):**

- `tf-demo-parser`'s `has_condition` tests `byte >> bit == 1`, true only when
  that condition is the highest one set in its byte: scoped + teleported read
  as not scoped, cloaked + ubered as visible. `hl-demos/src/deep.rs` reads
  the condition props off the wire alongside the parser and tests bits
  properly.
- "A second before" and the flick counted *frames* back. A POV demo is
  written at the client's update rate and skips ticks (15% of frames on one
  of ours), so there it read further back than a second. Both now look back
  by ticks, and each shot records the tick it used.

**Kept, not dropped.** A demo with a timeline whose file disappears is marked
deleted rather than removed, whoever put it there -- your own recording
included -- so its links to matches survive. Auto-delete (Q23) now also
waits for the timeline before a demo counts as finished with.

**Not yet:** reaction time still needs line of sight against map geometry,
which no demo holds. Projectiles are not kept (thousands per match, nothing
asked of them yet); add them with a version bump when something needs them.

---

## 15. Three from the Discord (Q11-Q13)

Taken from the suggestion threads on 24 September 2026. Each is written down
as what it measures, what it needs, and what could go wrong — none is agreed.

### 15.1 Cart time in a numbers advantage (Q11)

**Built (27 Sept 2026).** The blocker is gone: the kept timeline (Q3) has the
cart as an object, every tick. `hl_demos::cart::cart` is a pure function over
it, and the match page shows it beside spychecks.

- A round runs from `teamplay_setup_finished` to its win. BLU pushes in
  every round: stopwatch swaps the teams, not the colours.
- Moving: the cart covered more than 12 units in the second.
- Up: BLU alive minus RED alive, +3 or better.
- Still while up splits in two, which the game makes exact: a cart with an
  attacker on it and no defender moves, so a still one either has **nobody
  on it** (no live attacker within 250 units across the ground -- pushers
  were measured up to ~180 from the cart's origin) or has **a defender
  blocking it**.
- Fights: the round's deaths split at 10 s gaps, as in the fights pass; won
  when RED lost more. The push is the seconds the cart moved in the 30 after.

On the STVs on disk (upward, vigil, swiftwater): 64, 25 and 10 seconds up
3+ with the cart still; 54 of upward's 64 with nobody on it. After the first
won fight of a round the cart moved 20-28 of the next 30 seconds; after the
third, as little as 1-15.


**The ask** (Flashy): measure and detect when the cart is standing still in a
9v5 or better, to see how much time is wasted not pushing. Also the average
cart time after winning a teamfight, 1/2/3 fights in.

**Why it is a real measurement and not a proxy.** A single enemy near the cart
stops it dead — it does not slow down, it stops (see `docs/highlander.md` §5).
So a still cart while five enemies are dead is not a resourcing problem or an
unlucky angle. It is either nobody walking to it, or a defender alive on it
that nobody has killed. Both are mistakes, and both are the team's, which is
the first thing this app would measure that is not about one player.

**What it needs.** Cart position over time, which comes from an STV demo and
nothing else — logs.tf does not record it. The demo parser already reads
player positions per tick (`hl-demos::parse`); the cart is a separate entity,
and finding it in `tf-demo-parser`'s entity stream is the unknown. Failing
that, the cart's *progress* is in the round events, which is coarser but free.

**The measurement, first cut.**

- For every second of a live payload round: cart moving or not, and the
  numbers difference (already derived — `kill_situation`).
- **Wasted seconds** = time at +3 or better with the cart still.
- **Conversion** = seconds of cart movement in the 30 seconds after a won
  fight, which is the "1/2/3x after teamfights" part of the ask.

**What could go wrong.** A still cart at +4 can be correct: the last defender
is holding a forward angle and the team is repositioning rather than feeding.
A number that calls that a mistake will be wrong often enough to be ignored.
So the first version reports it per round with the clip to watch, rather than
folding it into a rating.

### 15.2 Momentum (Q12)

**The ask** (Flashy): a statistic for when the cart gets stuck and you start
killing or dying a lot at one spot.

**What it is.** Not a new measurement so much as a shape over the ones we will
have: a round is a sequence of states — pushing, stalled, collapsing — and
momentum is which one you are in and how long you stay there. A hold that
breaks after four failed pushes reads very differently from one that breaks
first try, and the scoreboard shows neither.

**What it needs.** Q11's cart timeline, and Q6's fight model for the win
chance per fight. With both, a round becomes a line: territory on one axis,
time on the other, with fights marked. Without them it is a guess.

**Why it is last.** It has no measurement of its own. Built before Q6 and Q11
it would be an opinion drawn as a chart.

**First cut, when it comes.** Per round: a stall is N seconds with no
territory gained and at least one fight lost; report the number of stalls,
the longest, and where on the map they happened. The map overview already
draws positions, so "where" is close to free.

### 15.3 Translations (Q13)

**The ask** (tenshi): the app in the languages people actually speak, so that
not knowing English well is not a barrier. Flashy's shortlist: Russian,
Ukrainian, German, Polish, French, Italian, Spanish, English. **boSe offered
to take French; obi offered Portuguese and Spanish.**

**What it needs.** The strings are currently written into the components. The
work is:

1. Pull every user-facing string into one catalogue keyed by id. This is the
   bulk of it, and the part that has to happen before anyone can translate.
2. A small runtime: a language setting, a `t("key")` lookup, fall back to
   English when a key is missing so a half-finished language still runs.
3. One file per language, plain JSON, so a volunteer edits a file and opens a
   PR without touching the app.
4. A language picker in Settings, next to the colour themes of Q10.

**Why not yet.** Every string this app has has moved at least once in the last
week, and several have been rewritten twice. A string moved after it is
translated is a string translated again, by a volunteer, for nothing. The
screens should settle first.

**What to be careful about.** The tone is the product here: plain sentences
that explain rather than label. That does not survive machine translation, and
it is unfair to hand a volunteer 400 strings with no context. So: ship the
catalogue with a note per string saying where it appears, and start with one
language end to end (French, since boSe offered) before opening the rest.

**The numbers are not strings.** Dates, thousands separators and decimals
already go through `toLocaleString`; the rating's two decimals are deliberate
and should stay a full stop in every language, because it is HLTV's number.


---

## 16. Deleting downloaded demos (Q23)

**Why.** An STV demo is read once and never needed again. On this machine:

| | count | on disk |
|---|---|---|
| POV, your own recordings | 101 | 3.53 GB |
| STV | 8 | 632 MB |
| ...of those, downloaded by the app | 4 | — |

About **80 MB each**. Downloading a season of officials is ~3 GB of files
whose only job was to be parsed once.

### What may be deleted, and what may not

Only `kind = 'stv' AND demos_tf_id IS NOT NULL` — files the app put on disk
itself, and which it can fetch again.

**Never a POV demo.** TF2 wrote those, not us; they are the player's own
recordings, sitting in their own folder, and some are the only copy of a
match that predates the app. Q16b also made them load-bearing: a POV demo
is the only source of the owner's own view angles as they made them, where
an STV has everyone's quantized for the wire. A cleanup feature that eats
3.5 GB of irreplaceable recordings to save 600 MB of replaceable ones has
the ratio exactly backwards.

**Never an STV the app did not download.** No `demos_tf_id` means no way to
get it back, which makes it somebody's own file too.

### The blocker, which is a bug on its own

`aim::pass` opens the file with `std::fs::read(path)?`. That error travels
up through `for_log` to `derive_all`, which propagates it — so **one missing
demo aborts the whole aim pass**, and every match queued behind it goes
unread. Deleting a demo today would quietly break syncing.

This is worth fixing whether or not the rest gets built: a demo the player
moved, renamed, or cleaned up by hand does the same thing right now. A file
that is gone is "nothing to read here", not a failure.

### What makes it safe: deletion is reversible

`demos_tf_id` lives on the `demo` row, not in the file. Delete the file,
keep the row, and the existing download code can fetch it again from the
same id. So this is not "destroy data", it is "evict a cache" — which is
the difference between a feature and a footgun.

- `ALTER TABLE demo ADD COLUMN deleted_at INTEGER` (unix seconds, nullable).
- The scan must not resurrect the row when the file is absent, nor drop it.
- The match page keeps working throughout: everything it draws — routes,
  aim, deaths — is already derived and in the database.

### When a demo is finished with

When every log it is linked to has been read at the current pass version:
`aim_log.version = aim::VERSION` for all of them. That table already exists
and already records the version. Auto-delete fires only then.

### What is actually lost, and must be said out loud

1. **A future pass version cannot re-read it.** Pass versions bump often —
   11 of them so far, the last one today — and each bump re-reads every
   demo to improve what is stored. A deleted demo's rows stay frozen at the
   version that read them. `aim_log` knows which, so the app can say
   "read at v11, re-download to improve", and the re-download is a click.
2. **`Copy playdemo` stops working** for that match (`DemoPanel`). The
   button should offer to fetch the demo back rather than hand over a
   command for a file that is not there.

### The order to build it

1. A missing demo file is not an error. *(Bug fix, stands alone.)*
2. `deleted_at`, and a scan that respects it.
3. `delete_downloaded_demos` command; a **Downloaded demos** panel in
   Settings: count, total size, a row per match with its size and date,
   delete one or delete all.
4. A **Delete after reading** toggle, off by default. Off is the right
   default: the first thing a new pass version wants is the demos.
5. Re-download from the match page and from the panel.

Steps 1-3 are the feature the ask describes. 4 and 5 are what stop it being
a one-way door.


---

## 17. Progress, everywhere it is missing (Q24)

**The finding, before any design.** The aim pass already counts its work and
hands it to a callback. The app passes `|_, _| {}`:

```rust
match hl_ingest::aim::derive_all(&db, me, false, |_, _| {}).await {
```

That is the longest job in the product. Twenty-five demos took **396 s** on
this machine, about 16 s each, and every second of it is silent — the sync
card has already said "Sync finished" while the window carries on parsing.
The CLI prints `reading demos 7/25`. The app had the same number available
and dropped it on the floor.

### What reports today, and what does not

`Progress` has eleven variants; `fractionOf` gives a determinate bar to six
of them. Per stage of a sync:

| Stage | Reports? | Cost |
|---|---|---|
| Indexing trends.tf / logs.tf | yes, row counts | seconds |
| Fetching logs | yes, with an ETA | the bulk of a first sync |
| Raw server logs, per-map parts | yes | minutes |
| ETF2L officials | yes | seconds |
| `owner::refresh` | **no** | one request |
| `demostf::index` | **no** | a network round trip |
| `index_demos` (scan the TF2 folder) | **no** | 109 demo headers |
| `maps::resolve_all` | **no** | seconds |
| `fights::derive_all` | **no** — takes no callback | tens of seconds |
| `aim::derive_all` | **counts, and is discarded** | **~16 s per demo** |
| `rate_all` | yes | seconds |

A rebuild has the same holes: `etf2l::derive_context`, `maps::resolve_all`
and `fights::derive_all` all run silently between two stages that do report,
so the bar stalls at a number and nothing says why.

Downloading an STV does report (`EV_STV_PROGRESS`, bytes and total, with a
queue position) — that one is already right, and is the model for the rest.

### What to build

1. **Spend the number that already exists.** `derive_all`'s callback becomes
   a `Progress::ReadingDemos { done, total }`, emitted like every other
   phase. One line of Rust, and the worst offender is fixed. *This lands on
   its own, before anything else here.*

2. **Give `fights::derive_all` a callback**, the same shape as
   `kills::rederive_all` which already has one.

3. **Name the short silent stages.** `owner::refresh`, `demostf::index`,
   `index_demos`, `maps::resolve_all` do not need percentages — they need
   `Progress::Stage { what }` so the card can say "Matching demos.tf" and
   the bar can run indeterminate rather than appear stuck. `index_demos`
   knows its file count, so it can report `done/total` properly.

4. **A per-demo line, not just a count.** "Reading gullywash, 3 of 7" beats
   "3 of 7": a demo takes 16 s and naming it is the difference between
   waiting and wondering.

5. **Two places, not one.** The corner card already carries sync, downloads,
   the demo watcher and updates. Long work started *from a page* — opening
   a match whose demo has not been read, re-rating after a weights change —
   should show its own inline bar where it was started, and the corner card
   stays the place for background work.

### The rule to hold

An indeterminate spinner is a last resort, not a default. Every stage above
either knows its total (report a fraction) or is short enough not to need
one (name it and move on). The one honest use of a sliding bar is a
download with no `content-length`, which is already how `DownloadCard`
behaves.

### What not to do

Not a progress bar per row, per panel, per query. The match page loads in
well under a second and a spinner there is noise. This is about the four
places that take **tens of seconds to minutes** — reading demos, fetching
logs, rebuilding, and re-rating — and about the stages between them that
currently look like the app has hung.

\n

---

## 18. Penalizing spawn delays (Q25, ivg)

> "delaying/denying a spawn should penalize the rating. easily detected by
> checking cappers when teammates are dead. order it similarly to the frag
> impacts; delaying an engie is less penalizing than delaying demo/sniper"

**Confirm the mechanic before building anything.** The component's sign
depends entirely on which way the respawn interaction runs, and that is a
TF2 rules question, not a data question -- ask ivg to state it plainly. The
design below assumes their reading: taking a point while your own people are
dead extends their wait, so it is a cost the capper should carry, and the
cost scales with who is stuck in spawn.

**The data is already in hand, and it is the same data as Q17.** `gs.caps`
gives the moment, the team and the cappers; `gs.alive_at(at)` gives who was
alive and on which class. So "who was dead on the capping team when the
point went in, and what were they" is one query away. **Build it with Q17**:
both ask what the state was when a point was taken, both read the same three
structures, and splitting them means walking every raw log twice.

**Weighting.** ivg asks for the frag-impact ordering, which already exists
as `[victim_value]` -- Medic 3.0, Demo 2.2, Sniper 1.8, down to Engineer 1.1.
Reusing that table rather than inventing a second ordering is the whole
point of having it.

**Measure it the way Q17 was measured.** Excess over what the numbers
already predicted, because the confound is severe and obvious: capping while
your teammates are dead is capping while you are *down players*, and a team
down players loses more rounds whatever it does. The numbers baseline
already knows that. The question is whether, having controlled for it, the
cap still reads worse than a cap made at full strength. If it does not, this
is a real-feeling effect that the data does not support, and it joins Q4 and
Q6 in the measured-but-not-applied pile.

**Expect interference with Q17.** "Enemies alive when you capped" and "your
own players dead when you capped" are two cuts of the same moment. Measure
both, then check whether either survives once the other is known.

### 18b. The discussion continued (27 Sept 2026, boSe and ivg)

> boSe: "sometimes teams agree on delaying, also a 2 second delay when a guy
> dies right before cap and a 10 second delay when someone was about to
> respawn does not have the same impact, the first scenario should not
> penalize the capper but the guy that dies off-timing"
>
> ivg: "A stat to look at when deciding if it was intentional or not is
> enemy time / cappers on point. If 4 people cap it's less penalizing
> compared to one or two. And when the enemy is in ot or <30s time I'd say
> capping is more important"

**This explains the first measurement better than the guess did.** The
existing column counts *dead teammates*, which lumps together a mate who
died two seconds ago (a small delay, and his fault) with one who was about
to walk out of spawn (a long delay, the capper's choice). Averaged together,
it is no surprise the column reads as noise-to-rewarding. Four things change:

1. **Measure the delay itself, not the head count.** The raw log carries
   every `spawned as` line, and the parser already reads them. So for each
   teammate dead at the cap, the delay is known exactly: when they actually
   came back, against when a player killed at that moment on that map and
   side usually comes back. No guessing at respawn rules -- the log shows
   what the cap did to the timer, and if a cap turns out *not* to lengthen
   waits on some mode, that mode drops out on its own.
2. **Blame the right player.** boSe's split: a mate who died just before the
   cap (seconds, not a full wave) is the one who died off-timing; a mate who
   was about to respawn and got pushed back a wave is the capper's cost. The
   delay measure separates these for free -- the first case has a short
   delay, the second a long one -- so only the long ones count, and only
   against the capper.
3. **Context that makes the cap worth it anyway.** ivg: many cappers (the
   point goes faster, so it was a push, not a mistake), and the clock --
   enemy in overtime or under 30 s left, where taking the point now is
   worth more than a respawn. Both are already known at the cap: cappers
   from the cap line, the clock from the round's time. Measure each as a
   modifier rather than assuming it.
4. **Agreed delays.** "Sometimes teams agree on delaying" -- a team that
   decides to wait for a full wave and then caps is doing the opposite of
   the thing being penalized, and that shows as *no* dead teammates at the
   cap. Nothing to build; noted so nobody tries.

**Order of work, measure first as with every rating change:**

- `hl situation --spawn-delay`: for every cap, each dead teammate's actual
  wait against the usual wait, bucketed by delay (0-3 s, 3-8 s, 8 s+), by
  number of cappers, and by time left. Read it before deciding anything.
- If long delays still go with *winning*, stop there: the cap is doing what
  a cap should, and the answer to ivg is "the data says the push was worth
  it". If they go with losing, add `spawn_delay_s` (long delays only, the
  victim-value ordering ivg asked for, softened by cappers and clock) to
  the fights pass beside `caps_mates_dead`, and A/B it the usual way:
  applied only if never worse on the full set and the held-out one.

**Measured and applied (27 Sept 2026).** `hl situation --spawn-delay`, 738
logs. A capture does lengthen the capping team's waits: on KOTH the dead at
a cap waited 4-5 s longer than their team's usual, against +1.2 s for the
dead of the team that lost the point (the general bias of a wait spanning
any capture). On stopwatch the effect is small (+0.5 s, +2.6 s for mates
already 8 s dead).

| longest delay among the capper's dead | KOTH caps | excess | stopwatch caps | excess |
|---|---|---|---|---|
| nobody dead | 1813 | +1.2% | 835 | +26.0% |
| under 3 s | 475 | +11.4% | 549 | +30.7% |
| 3-8 s | 1793 | -4.7% | 100 | +24.3% |
| 8 s or more | 715 | **-16.3%** | 45 | -14.2% |

So boSe was right on both halves: a short delay is harmless (it is the
mate who died off-timing), a long one goes with losing. ivg's softener did
not show: with 3+ cappers the 8 s+ caps read -15.9%, with 1-2 -16.5%. The
clock could not be read (logs carry no KOTH timer), so it is not in.

`caps_spawn_delay`, per capper, is the seconds of extra wait of 8 s or more
their caps cost their own dead. The player with less of it was on the
winning side in all nine classes, 54.7% (Spy) to 60.3% (Scout, z 3.5). The
A/B, weight added and the model renormalised:

| class | live | +0.05 | +0.10 | applied |
|---|---|---|---|---|
| scout | 74.1 / 65.8 | 74.6 / 66.8 | 75.1 / 67.8 | 0.10 |
| soldier | 74.6 / 68.0 | 75.1 / 69.0 | 75.0 / 69.0 | 0.05 |
| pyro | 76.7 / 75.9 | 77.2 / 75.9 | 76.9 / 74.9 | 0.05 |
| demoman | 77.3 / 73.0 | 77.4 / 72.5 | 77.4 / 72.5 | none |
| heavy | 77.8 / 73.5 | 78.0 / 72.5 | 77.4 / 71.0 | none |
| engineer | 76.0 / 72.1 | 74.9 / 71.6 | 75.7 / 71.1 | none |
| medic | 76.3 / 68.7 | 76.7 / 69.7 | 76.7 / 69.2 | 0.05 |
| sniper | 75.8 / 75.0 | 76.0 / 75.0 | 76.1 / 75.5 | 0.10 |
| spy | 74.1 / 71.1 | 73.6 / 70.1 | 73.4 / 70.1 | none |

(all matches / held out after the split, % of winners picked.) ivg's
class ordering of the *delayed* player (a Demo stuck in spawn costs more
than an Engineer) is not in yet: the seconds are unweighted. It is the
obvious next refinement if the component earns a larger weight.

---

## 19. Demo and Sniper by mode and side (Q26, ivg)

> "arguably demo = sniper on product, teams can still hold while being down
> one of those 2 classes. the nuance comes with payload, when defending a
> demo pick is arguably more important than a sniper pick. and when
> attacking a sniper pick is generally better"

**Most of this is already built, and unused.** `weights.default.toml` has a
layered `victim_value`: general, then `[victim_value.defending]`, then
`[victim_value.map.<map>]`, then `[victim_value.map.<map>.defending]`, with
lookup running most specific first. Side is resolved through the
`[attack_defend]` map list. What is missing is narrow:

1. **A per-mode layer.** There is per-map and per-side but no per-gamemode,
   which is what "on KOTH" means. Either add `[victim_value.koth]` or accept
   that KOTH is a map prefix and use the existing per-map layer with a
   prefix match, which it already does.
2. **Numbers to put in it.** `hl situation --victims` (Q4) already prints
   what killing each class was worth per mode. It has not been read for the
   Demo/Sniper question specifically.
3. **A side cut in that measurement.** `victim_worth` splits by mode, not by
   whether the victim was attacking or defending. The raw log has every kill
   with its round, and `[attack_defend]` says which side was which, so this
   is an extra dimension on a table that already exists.

**Say the likely outcome first.** Q4 measured this family of adjustments and
found `hl validate` cannot feel them: the Medic's KOTH premium is the
largest effect in the whole victim table (+10.7% against a +6.2% average)
and applying it cost 0.4 points of accuracy. Because the rating is a
percentile against the pool, a multiplier that applies to every Sniper on
payload only moves payload games relative to other maps -- it does not
change who was the better Sniper in the game you are reading. So the honest
expectation is that ivg is describing something true about TF2 that the
rating is structurally unable to reward. Worth measuring, worth writing
down, and probably not worth applying.

---

## 20. Spychecking (Q27, ivg)

> "number of spy hits when he is fully cloaked and not blinking / on fire...
> This should also have a slight cooldown to prevent repetitive hits (like
> when tracking with pistol/shotgun/minigun) from counting as multiple
> spychecks"

**The blocker, stated before the design: cloak is not in any log.** logs.tf
and the raw server log record a damage event with attacker, victim, weapon
and time, and nothing whatever about whether the victim was cloaked. It is a
player condition, and the only place conditions exist is a demo. The aim
pass already reads one of them -- `PlayerCondition::Zoomed`, for scoped
share -- so the mechanism is proven; the coverage is the problem.

**Coverage is the whole decision.** An STV demo carries every player's
conditions. There are **4 STV demos against 749 rated matches**. A component
that exists for half a percent of the pool cannot be in the rating: the
percentile it would be scored against would be built from almost nobody.

So the shape this takes is not a rating component:

- **A match-page stat** where a demo exists, beside the aim numbers. "Six
  spychecks" is interesting on its own and needs no pool.
- **A rating component only if STV coverage becomes normal**, which would
  mean downloading an STV for every match -- and Q23 is about deleting
  those, so the two need deciding together.

**The spike, before anything else.** Confirm `tf-demo-parser` exposes the
cloak condition at all, and that it distinguishes fully cloaked from
blinking. Count spychecks in one STV by hand against the demo. If the number
is not sane, nothing else matters.

**The rules, once the data is confirmed.** A spycheck is a damage event on a
Spy who is cloaked, not blinking (no recent damage of his own), and not on
fire (`PlayerCondition::OnFire`, already the same enum). Cooldown per
attacker per Spy, about two seconds, so a minigun held on a cloaked Spy is
one spycheck and not thirty -- ivg's own correction, and the difference
between measuring a read and measuring a fire rate.

**Built (2026-09-27).** The timeline (Q3) made the spike a few lines: it
already records every player's condition bits and every hurt event, so
`hl_demos::spy::spychecks` is a pure function over it. The rules, as built:

- the victim is a live Spy with `Stealthed` on the tick *before* the hit
  (the blink a hit causes lands on its own tick);
- he has been cloaked for a second without a break (`tf_spy_invis_time`),
  so a Spy still fading in is not a blind read;
- not `StealthedBlink`, `Burning`, `Urine`, `MadMilk` or `Bleeding`;
- one per attacker per Spy per 2 seconds, measured from that attacker's
  latest hit, so a held minigun is one check for as long as it is held.

`hl spychecks <demo> [--list]` over the five STVs on disk: 17, 20, 10, 20
and 36 checks; the hits left out were mostly Spies still fading in (19-62 a
match), then blinking, cooldown and marked. The list carries the demo tick,
so any check can be confirmed in game with `demo_gototick`. The match page
shows who checked and who was found, and every check copies its jump.


---

## 21. Callouts, positions and tendencies (Q28, Flashy)

> "On every map you have a lot of common holds like sniper sitting on cliff
> (product). This would basically map areas where the classes play the whole
> match (would kind of work like positions in counter strike), for example
> some players are b-anchors or rotators... I need callouts for every map,
> visible on the map overlay as well... When you get to the fullscreen map
> overview, you would be able to see the positions highlighted with a jigsaw
> puzzle like overview."

**What is already there.** The overview images and their placement
(`overview.rs`, 94-99% of kill positions land on the drawn map), a
fullscreen kill map, both players' positions on every kill in all stored
raw logs, and -- for STV demos -- every player's position every tick in the
kept timeline (Q3).

**What is missing is the names.** Callouts are community vocabulary, not
data: no file in the game or on logs.tf says where "cliff" is, and teams
disagree at the edges. So they are stored the way translations are (Q13):

1. **One file per map**, `callouts/<map base>.callouts`, each callout a name
   and a polygon in game units (so it survives a new overview render). The
   app ships a seed set and keeps a user folder beside it; an edited file
   is never overwritten, exactly like `.lang`. A zone may carry a height
   band, for maps where a balcony sits over a corridor.
2. **The seed.** Researched per map from what the community publishes
   (callout images, ETF2L and competitive guides), drawn over the overview,
   then checked against the kill density: a callout nobody ever fights in
   is drawn in the wrong place. The Highlander pool first (~12 maps).
3. **An editor**, in the fullscreen overview: drag a zone's corners, rename
   it, save to the user file. Without it every correction is a text edit,
   and nobody will make one.

**Then the positions, from cheapest to richest.**

- **The jigsaw view.** The fullscreen overview draws every callout as a
  zone with its name, shaded by use: for a class, a player, or one match.
- **From kills, for every match.** Each kill and death assigned to a zone:
  "your Sniper kills: 58% from cliff". Works on all 738 logs, but only sees
  where fights ended.
- **From STV timelines, time in each zone.** Where a player *stood*, second
  by second, which is what anchoring and rotating are. Tendencies per
  player -- "holds last as Engineer", "rotates between cliff and house" --
  as the share of alive time per zone and the common paths between zones.
  STV only, so a match-page stat and a profile note where enough STVs
  exist, not a rating component (the same coverage rule as Q27).

**Built (27 Sept 2026).** `hl_ingest::callouts`, `callouts/*.json`.

- **Sources.** The Official TF2 Wiki describes each callout for Product,
  Upward, Steel and Swiftwater in words. comp.tf (which answers automated
  requests with a 404) has Vigil's only as labelled in-game screenshots, and
  callouts.tf was unreachable; nothing written was found for Ashville or
  Proot. So: Product drawn in full from the wiki's descriptions (RED half
  drawn, BLU mirrored across the viaduct); Upward, Steel and Swiftwater ship
  their names as a list to place; the rest start empty.
- **Checked against data.** Of 65,266 kill and death positions on Product,
  69% fall inside a seeded zone, and each RED zone holds within 3% of its
  BLU mirror (Cliff 1,121 / 1,121, Concrete 4,227 / 4,099): the geometry
  and the mirror are right; the 31% outside are gaps to draw.
- **Positions on one match** (product STV): both Spies live in the other
  side's House, both Heavies on their Concrete, Demos and Pyros on the Point.
- **Tendencies across matches: done (1 Oct).** Fights by zone from every
  server log, standing and moves from every STV, by the player's own side.
- **Next:** a player of each map checking the seeds.

**What could go wrong.** Seed callouts that are wrong look authoritative.
Every seeded file says it is a draft until someone who plays the map has
checked it, and the UI shows that.

---

## 22. Teams and seasons (Q29, Flashy)

> "Add a teams page per season, start with maybe 1 year back-tracked ETF2L.
> Look at their win percentages on each map in the map pool. List their best
> players with best scores. Have profile pages for teams and performance
> metrics."

**What is already there.** The owner's own ETF2L history (`etf2l.rs`:
officials, competitions, rosters), and the Teammates page per ETF2L team the
owner played on. Every other team is unknown.

**It splits in two, and only the first half is cheap.**

1. **From ETF2L alone.** The API lists each Highlander season's divisions,
   teams, rosters and results with maps and scores. A year back is about two
   seasons, a few hundred teams, ~3,000 matches: a few hundred requests,
   cached, and refetched only while a season is live (the same 14-day
   settle rule `etf2l.rs` already uses). That gives:
   - a **team page**: roster per season, division, record, results;
   - **win % per map** in the pool, from ETF2L's own results;
   - a season table per division, and head-to-head between two teams.
2. **Best players and performance metrics.** These need the *logs* of those
   officials, not the results: ~3,000 logs.tf fetches, matching each official
   to its log (the time-and-roster match `mark_by_time` already does for the
   owner), and somewhere to keep games the owner never played. That is
   **Q14b**'s problem exactly, and they are one piece of work. Once built, a
   team page lists its best player per class, rated the usual way -- still
   a percentile against the owner's pool, which the page says.

**Order:** part 1 as its own release; part 2 together with Q14b.

**Part 1 built (27 Sept 2026).** `hl_ingest::leagues`, migration 0030 (its
own tables, never the owner's `etf2l_match`: the context pass links logs
to that one by time, and a table of every team's matches would hand it the
wrong ones).

- **What ETF2L gives.** `/competition/list` (20 a page, newest first, all
  formats mixed); `/competition/{id}` with the map `pool`; `/competition/{id}/results`
  with both teams, score, maps and each match's own division and tier; and
  `/matches/{id}` with who played and each map's rounds.
- **Found on the way.** A season's main competition ("Highlander Season 36
  (Autumn 2026)", 110 matches) holds Mid, Low and Open together, so the
  division is read from each match, not the competition's name. A stopwatch
  map is two rows on the match page, one per half, and is one map won or lost.
- **The cost.** 60 requests a minute is ETF2L's limit. The list walk stops at
  a page with an older Highlander season on it and none of the wanted ones;
  an archived competition already held is not asked for again; 60 match pages
  a sync. First fetch on a copy: 19 competitions, 430 results, 60 pages, 289 s.
- **Ratings on a team page** are the pool's: a player shows one only where
  they appear in the owner's matches, and the page says so.

---

## 23. A match from a demo alone (Q18, beowulf)

> A match where the server had no logs.tf config, so no log exists at all.

**The approach: make the missing log, rather than a second app.** Every page
and pass is built on a log, so the demo is turned into one
(`hl_demos::synth`): the server-log lines `rawlog` reads, and the logs.tf
summary `normalize` reads. The demo's own events are what a server logger
writes from, so an STV holds nearly everything:

| from the demo | becomes |
|---|---|
| alive/class changes | `spawned as`, `changed role to`, time on class |
| `player_death` | `killed ... with` (positions from the timeline), assists |
| `player_hurt` | `triggered "damage"` |
| `player_chargedeployed`, charge meter | `chargedeployed`, `chargeready`, `chargeended` |
| `medic_death` | `medic_death_ex`, drops |
| round active / setup / win | `Round_Start`, `Round_Setup_End`, `Round_Win` |
| `teamplay_point_captured` | `pointcaptured` with cappers (entity -> slot, now kept in the timeline) |
| patient's health rising while a Medic targets them | healing |

**Checked against a real log.** The swiftwater STV covers its log whole
(1,844 s against 1,808). Three rules had to be found to match logs.tf:
Dead Ringer deaths (`death_flags` 0x20) are nobody's; kills after a round's
win are not counted; a backstab's hit counts only the health there was.
`player_healed` fires for a tenth of a Medic's heals, so healing is read
from health instead. Result, real/synth: kills 309/309, deaths 311/313,
assists 168/163, damage 124,508/122,706, heals 80,574/73,383 (dispenser heals
are not read), ubers 38/38, drops, headshots, backstabs and caps exact.

**Storing it.** Log id `-(fnv(file name) mod 1e9) - 1`: negative, so it can
never meet a logs.tf id, and the same file re-imports over itself. The demo
must live where the folder scan looks (a scan drops rows it cannot find), so
one picked from elsewhere is copied into `tf/demos`. Its link is method
`import`, which rescans keep, and the demo's start is set from its name
(`match-YYYYMMDD-HHMM`, Demo Support's stamp) so the aim pass lines the two
up exactly (clock offset 0). A demo the scan links to any logs.tf log is
refused: the match exists, and a second copy would count it twice.

Imported on a copy: koth_proot, 3 rounds, 262 kills, 18 players rated, aim
read for 239 kills, in 10 s.

---

## 24. Maps: recognition, top-down images and callout presets (Q30-Q32, Flashy)

> "I need preset files for the callouts since people will be sharing them
> and adjusting them in the app, I need an import and export per map. Can I
> get a maps section in the settings with a top-down image import, callouts
> import and a fix for people who on their current installs do not see the
> maps ... making sure that the map gets recognized correctly from the STV
> if logs.tf isn't working."

### Q30. The map from the STV

**What happens now.** `maps::resolve_all` gives every round a map from, in
order: the log's map field, the raw log's `meta_data` line, a part of a
combined log, the part's upload window, the geometry model, and its
neighbours (`mapres.rs`). Three of those need logs.tf (the field, parts,
windows), and since `9acb2c9` a refusal from logs.tf stops the parts fetch
outright. The geometry model is trained on *this install's* single-map logs,
so a new player with twenty logs and no clean Vigil log has no Vigil
outline, and a round there comes out unsure or on the wrong map. Nothing
asks the demo, whose header says `pl_vigil_rc10` in plain text and is
already stored (`demo.map`).

**The fix.**

1. **A `demo` source**, second only to `log`: every round inside the
   recording span of a linked STV (`start_utc` + `playback_s`, with the
   log's clock offset) takes the demo's map. A demo that covers part of a
   combined log names only those rounds, so a two-map log with one STV per
   map is resolved exactly. A demo whose map disagrees with the log's own
   field loses and is logged; that is a wrong link, not a map question.
2. **Re-resolve when a demo is linked**, not only after a sync: linking,
   downloading and dropping a demo each rerun `resolve_all` for that log.
   It is local and takes milliseconds.
3. **Ship the geometry outlines** for the Highlander pool, like the callout
   seeds, so a fresh install recognises Vigil before it has seen a Vigil
   log. Built from the owner's database by an `hl` command and committed
   as data; a user's own logs are added on top.
4. **Say which problem it is.** The kill map's empty state tells three
   cases apart: map unknown ("no map for this round — link the STV and it
   will be read from it"), map known but no image ("drawn from kills;
   Settings, Maps, to add an image"), and map known with no placement.
5. **A manual override per segment**, last resort: "this was on ___", kept
   in the database and ranked above everything but `log`.

**Checked by:** a copy of a database with the log map fields blanked and
parts removed: every round with a linked STV must still resolve to the
same map as before, and the kill map must draw.

**Built (29 Sept 2026).**

- **Demo source** (`mapres.rs`, `Source::Demo`). Ranked second, after the
  log's own field and *ahead* of the raw log's map line, on evidence:
  where a demo's recording covers a round, it agrees with the log's field
  on all 77 rounds both answer. It disagreed with the map line on 2 of 5,
  and both times the demo was right: log 4109576's round 3 starts 234 s
  into the Steel STV (rounds 3-4 are Steel's two stopwatch halves; the map
  line said Upward), and log 4121291's round 1 starts one second into the
  Vigil STV. Both logs now split into clean pairs.
- **Unplaced demos** settle a whole log only when nothing else names a
  second map, and never when the clock can place them: a round the clock
  puts before or after every demo is not theirs to answer (found on
  4121291, whose Swiftwater rounds came before its Vigil STV).
- **Re-resolved on link**: downloading an STV from demos.tf and dropping a
  demo on a match page both rerun `resolve_all` (about 3 s), and the page
  refetches its analysis.
- **Shipped shapes**: `hl maps --export-geometry` writes every map with
  500+ kill positions from single-map logs; 24 maps, 450 KB, built in and
  added to each install's own.
- **The check**, on a scratch copy of the 28 Sept backup with every map
  field, title, part link, raw log and ETF2L map list wiped: 92 of 94
  rounds with a demo resolve as before (the other 2 are rounds before the
  demo, left to the shapes), and 2,314 of 2,426 rounds without one
  (95%), with 55 wrong and 57 unknown. The same wipe resolved 0 before.
- **The kill map** says "which map these rounds were on is not known", and
  when no STV is linked, that downloading or dropping one fixes it. It no
  longer mistakes free text in logs.tf's map field for a map.

**Finished (30 Sept).**

- **demos.tf by time alone.** The STV search matched by map *and* time, so
  a log whose map was unknown never found its STV. Such a log now reaches
  the matcher and may take a demo on time alone, after every map-matched
  pair, and only a demo that was *recording when the log began* (started
  before it, ran past its start): another match's demo was not. The map
  demos.tf lists for it is kept (`log_index.demos_tf_map`, migration 0032)
  and read as an unplaced demo, so the kill map draws before the STV is
  downloaded. Kept only for logs that had no map: a log with maps knows them
  better than one listing.
- **Unplaced demos no longer narrow the geometry**, and a confident
  geometry answer beats them: one demo found for a combined log names one
  of its halves, and must not hide the other. The wipe check gives the
  same numbers as before.
- **"This was on ___"** (`round_map_manual`, source `manual`): a picker
  under the kill map when the map is unknown, and a small "Wrong map?" when
  it is known. It covers the rounds the kill map is showing, outranks
  everything but the log's own field, and "Not sure" takes it back.

### Q31. The Maps section

A panel in Settings, one row per map the app knows (the placement table,
the callout seeds, and every map in the database):

| Map | Image | Callouts | Matches |
|---|---|---|---|
| Vigil | none · Import… | built in, draft · Export · Import… | 41 (3 unknown) |

- **Top-down image import.** Pick a PNG or JPG; it is copied into
  `<data>/overviews/<map>.png`, which `overview.rs` already reads. For a map
  in the placement table a square image is assumed to use the more.tf
  transform, while rectangular images use demos.tf's supplied bounds; the
  panel then shows it with this install's kill
  positions on top, so a wrong image is visible at once.
- **Alignment for other images.** For a map with no placement, or an image
  that does not line up: drag and scale it over the outline drawn from
  kills, and the placement is saved beside the image
  (`<map>.placement.json`), overriding the built-in one.
- **Remove / reset** per map, back to the built-in state.
- **Built-in images:** the original more.tf renders were shipped with
  permission; demos.tf supplies additional overview images and boundary data.
  Import is for the maps neither dataset covers and for a player who wants
  a different render; the player's own
  image always wins, and Remove goes back to the built-in one.

**Built (30 Sept 2026).** `overview.rs` gained the player's own image
(`<data>/overviews/<map>.{png,jpg,webp}`) and placement
(`<map>.placement.json`), both winning over the built-in ones; image sizes
are read from the file headers, so no image library was added, and an
image that is not square keeps its shape on the kill map (`aspect`). A file
that is not an image, over 25 MB or under 64 px is refused. The list is
`mapsettings.rs`; the section is `MapsPanel.tsx`, the aligner
`MapAligner.tsx`. Measured nothing; checked in the browser against the
fixtures: rows, import, line up, save.

### Q32. Callout presets

Callouts get passed round and corrected like `.lang` files, so they are
handled like them.

- **The file.** `<map>.callouts.json`, the shape the app already stores
  (`CalloutFile`: map, draft, source, zones in game units, names) plus
  `format: 1` and an optional `author`. Game units, so a preset survives a
  different overview image.
- **Export** from the Maps row and from the kill map's editor: a save
  dialog, defaulting to `vigil.callouts.json`.
- **Import:** checked before anything is replaced: the file names this
  map (or the player confirms a mismatch), every zone has at least three
  finite points, names are non-empty, under 1 MB. It says what it will do
  ("27 zones, replaces your 24"), keeps the replaced copy as
  `<map>.previous.json` for one Undo, and becomes the player's own copy,
  so the built-in seed never overwrites it.
- **Drag and drop** a `.callouts.json` onto the window imports it, like a
  demo.
- Later, if asked for: a **map pack** — image, placement and callouts in
  one zip — so a whole map is one file to share.

**Built (30 Sept 2026).** `callouts.rs`: `export`, `read_preset`,
`inspect`, `import`, `undo`. The Undo copy is `<map>.previous.json` beside
the player's own (holding `null` when there was none, so Undo goes back to
the built-in); saving an edit or going back to the built-in drops it, since
Undo would throw the edit away too. A file for another map is refused by
the backend unless the player confirms, because its zones would sit in the
wrong places. The UI is `CalloutPresets.tsx`: the buttons, the confirm
dialog and the window-wide drop, which the demo drop zone now leaves alone.
Checked in the browser against the fixtures: wrong-map warning, import,
export.

### Order and release

Q30, then Q31, then Q32: the bug first, then the section, then the files
that live in it. New features, so they go out as **0.7.0** with the ETF2L
names option.

---

## 25. The league sample and the rating pool (Q33-Q34, Q39, Flashy)

> "I kind of need like a database of logs from each division, like 300
> officials from each, so that the ratings can get a bit less skewed and
> for the app to have more context ... so later on I can make a sort and
> have it tell you what div player you are playing against."
>
> "Since I have a large dataset, you can include me now as well."

### Q33. The league sample (built 30 Sept 2026)

**What it is.** ETF2L Highlander officials from every division, downloaded
slowly in the background (Settings > League sample), kept in their own
tables (migration 0033: `league_log`, `league_log_json`, `league_rawlog`)
so nothing touches the owner's matches until it is decided how it counts.

**How it finds them.** ETF2L's results give every match's division and
tier (`leagues::fetch_seasons`, twelve seasons back); trends.tf lists every
ETF2L Highlander log with its ETF2L match id
(`/api/v1/logs?league=etf2l&format=highlander`). Joined, every log has a
division without asking logs.tf anything. trends.tf links 92-97% of all
officials played, per tier. Combined logs and logs under five minutes are
left out; seasons before 32 are named without brackets ("Highlander Season
22: Premiership Qualifiers") and the name parser reads both.

**How it chooses.** Per tier (0 Premiership, 1 High, 2 Mid, 3 Low, 4 Open),
up to 300 matches, the last three years first and six if a tier cannot
fill, round-robin across maps so every map in the pool is in it.

**How it downloads.** One request a step, ~6 s apart: logs.tf JSON, then
raw server logs; while logs.tf is resting (a 403 rests it 10 minutes),
more.tf's copy stands in for the JSON; one ETF2L match page a step
alongside -- for **every** played official in the window, not only the
sample's, which is the player catalogue (§26). It waits while the owner's
sync runs, resumes after a restart, and re-reads the lists daily. A live
activity bar says what it is doing each second.

**What it holds (30 Sept).** 812 matches, 1,832 logs, all JSON and server
logs downloaded, 203 MB: Premiership 117 matches, High 107, Mid 187, Low
165, Open 236, 14-22 maps each, March 2022 to September 2026. Every tier is
short of 300 because that is every official trends.tf links in the window,
not because of the limit.

### Q34. The league in the rating pool, the owner included

**Why.** The pool was the owner's matches only, with the owner left out:
on their main class they were half of it. With the league in it, they are
one player among thousands and are compared with the league like everyone
else; "1.00" becomes a typical league game.

**Built, not yet switched on (30 Sept).** `league_rating::performances`
puts each sample log through exactly what an owner's log goes through --
kills valued by victim, map and situation, the fights pass (openings,
trades, Fight KAST, cap costs), the shared swing -- in memory, from the
downloaded JSON and raw log, writing nothing to the owner's tables.
`rate_all` builds the baselines from the owner's matches plus the sample
with nobody excluded, measures the scale over both, and stores only the
owner's logs' ratings. A sample log that is also one of the owner's is
skipped, so no game is counted twice. `hl validate --league` measures the
same matchups against the new pool.

**Before switching.** On a copy with the sample in it (Settings > Backups
> Back up now, then copied), report: the owner's ratings per class before
and after, the scale's shift, and `hl validate --league` for all nine
classes against the old pool. Switch only if picking the winner is not
worse. Every rating moves once when it does.

**Cost.** Rating the sample reads ~1,800 raw logs (~10 ms each): about a
minute added to a full rating pass. If that shows at the end of every sync,
cache each sample log's performances keyed by model and fights version.

### Q39. Refit the models on the league

The nine models were fitted on ~690 matchups a class from the owner's
matches. The sample adds ~1,800 a class, from Open to Premiership. Refit
each class with `hl validate` on both, and settle the questions the small
sample could not: whether damage per minute earns a place (SchmitShot's
proposal: every version scored 1-4 points worse on the owner's matches),
whether the Demo and Heavy deaths weights hold, and whether a model fitted
on Open holds in Premiership. A tier column in the fit tells whether one
model serves every division or the top needs its own.

---

## 26. Player profiles, HLTV-style (Q35-Q38, Flashy)

> "Is it possible to correlate all these players with their respective
> main classes and their divs? It would be cool to display this in the
> players tab. Also show their medals, their tournament wins, kinda like a
> cool player profile and overview ... something similar for player
> profiles like HLTV but for TF2."

The Players tab (Q14) finds people in the owner's matches only. The league
sample and its catalogue hold every ETF2L official of six years and who
played it; together with trends.tf, that is enough for a page per player
that reads like an HLTV profile.

### What an HLTV profile becomes

| HLTV | Here | From |
|---|---|---|
| Photo, flag, name | Steam avatar, country flag, ETF2L name, aliases | ETF2L player page; the logs' names |
| Current team | Latest ETF2L team, its avatar and division | The catalogue's newest roster |
| Top 20 | "#3 Sniper in Premiership, S36" | Q36's ratings |
| Player achievements | "2x Premiership winner", "1x High runner-up" | Q35's medals |
| Trophy strip | A row of medals, each with its season and division | Q35 |
| Rating 3.0 | Rating on the league scale, last three months | Q36 |
| Firepower, Entrying, ... /100 | Kills and damage, Staying alive, Playing for the team, Objective, Medic, Speciality, each 0-100 | The component groups of How ratings work, as percentiles against the league on their class (Q36) |
| Recent matches | Their officials: opponent, maps, score | The catalogue |
| Tabs | Info, Teams, Matches, Achievements, Stats | All of the above |

### Q35. The catalogue profile: who they are, their teams and medals

Everything here comes from data already downloaded (the ETF2L results,
rosters and competitions), so it works before any rating change.

**Data.**
- *Player*: every account in `etf2l_season_player`, plus everyone in the
  owner's matches. Name: the newest ETF2L name, then the log name.
- *Seasons*: per account and season, the team(s) they played an official
  for, the division and tier, officials played, won and lost. A player can
  play for two teams or two divisions in one season (a merc, a mid-season
  move): list both, and count their division as the one they played most.
- *Which division a player is* (Flashy, ETF2L's rule): in a season, a
  division counts for them if they played at least three officials in it,
  or played in the Grand Final of the division one below (the finalists
  move up); where both apply, the higher. Their highest division is the
  best that counted in any season, and every per-season division -- match
  tags, ranks -- follows the same rule. A single merc game makes nobody a
  Premiership player.
- *Medals*: per competition with playoffs, the Grand Final winner gold, its
  loser silver, the 3rd Place match winner bronze. A season with no playoff
  stage for a division: the regular-season table's top three. A player
  earns the team's medal by playing at least one official for it that
  season, playoffs or not. Cups ("Highlander Experimental Cup") the same
  way where ETF2L lists them. Stored as derived rows (`player_medal`:
  account, season, competition, division, place, team), rebuilt from the
  results, never fetched.
- *Profile extras from ETF2L's player page* (`/player/<steamid>`, one
  request, cached a week): country, declared classes, Steam avatar,
  registration date. Fetched when a profile is first opened, and for the
  catalogue in the background after the rosters, at ETF2L's pace.

**Main class.** Two readings, shown side by side when they disagree:
*played* -- class time in every log we hold of theirs (owner's matches and
the sample), their most played if at least 60% of their time; and
*declared* -- ETF2L's `classes`. A player with no held logs shows declared
only, and trends.tf's class hours once Q37 is in.

**The page.**
- Header: avatar, flag, name and aliases, current team and division badge,
  main class icons, medal count ("2x Premiership winner").
- Trophy strip: every medal as an icon with its season and division, newest
  first; hover says the competition and the final's score.
- Tabs:
  - *Info*: the recent officials (opponent, maps, score, win or loss) and,
    once Q36 is in, the stat bars.
  - *Teams*: a timeline, season by season: team, division, record, placing.
  - *Matches*: their officials; their games with and against the owner
    (what Q14 shows now).
  - *Achievements*: every medal and title in full.
- Search: by name (every name used) or Steam ID, across the whole
  catalogue, with the division badge, main class icon and medal count in
  each row. Filters: division, class, "has a medal", "played this season".

**Checks.** On the owner's own profile, the seasons must match their ETF2L
page; medals must match ETF2L's news of each final for three seasons of
Premiership and High, by hand. Renamed and merged teams (ETF2L keeps a team
id across renames) must not split a player's season in two.

### Q36. Ratings for everyone: ranks and stat bars

Needs Q34 (the league pool). The sample's ratings are computed in every
full rating pass already; store them in their own table
(`league_rating`: log, account, class, score, parts), never mixed with the
owner's `rating`. Then:
- **Rating**, last three months and career, per class, on the league scale.
- **Stat bars**: each component group's weighted percentile over the same
  games, 0-100, the groups and colours of How ratings work.
- **Ranks**: per season, class and division, players with at least eight
  rated games ranked by average rating. "#3 Sniper in Premiership, S36";
  a Top list per class and division in the Players tab.
- **Honesty**: every figure says how many games it rests on, and a player
  with fewer than eight shows none. The sample is at most ~300 matches a
  division over six years, so a Premiership regular is in dozens of games
  and an Open one-season player in a handful.

### Q40. A player card from the scoreboard

> "When you click on someone on the scoreboard you will get a kind of
> tooltip with their achievements like medals, main class rating and maybe
> something else." (Flashy)

A match page is where you meet people; the profile is a tab away. Clicking
a name -- on the scoreboard or in the class matchups -- opens a small card
beside it, like a Steam or HLTV hover card, with what tells you who they
are at a glance, and a way through to the full profile.

**What the card shows**, top to bottom, each part only when there is data:

1. **Who.** Avatar, name, country; their division then (the match's tag,
   Q38) and their highest division, when different ("High in S35 · best:
   Premiership").
2. **Medals.** The gold, silver and bronze tiles with counts (the same
   rounded, see-through squares as the profile), and the best title
   ("2x Premiership winner").
3. **Main class.** Class icon, rating on the league scale over the last
   three months (else the career) and the number of games; their best rank
   ("#3 Sniper in Low, S33").
4. **On this class in this match.** When the class they played here is not
   their main one, their rating on it too -- an off-class game reads
   differently.
5. **Four stat bars**, compact: the component groups of their main class
   (Kills and damage, Staying alive, Playing for the team, Objective).
6. **You and them.** Games together and against, and the record against
   them, from the owner's matches (what the profile's "In your matches"
   shows).
7. **Links.** "Open profile" (the Players tab, on them), ETF2L and
   trends.tf.

**How it behaves.**
- Click to open, click elsewhere or Escape to close; one card at a time.
  Not on hover: the scoreboard is dense and a hover card would follow the
  mouse across every row.
- Placed beside the row, flipped to stay inside the window; on a narrow
  window, under the row.
- Opens at once with what the match page already has (name, class, the
  division tag), and fills in the rest from one request.

**Data.** One command, `get_player_card(account, class)`: the profile's
header facts, medal counts and best title, `player_stats` for the main
class and for the class played, the best rank, and the you-and-them
counts. Everything it needs exists (Q35, Q36, Q38); the ranks are the one
slow part (they read every rated game), so the card asks for the stats
the profile already caches with the same query key, and a rank that is
not ready yet appears when it is.

**Checks.** The card on the owner's own row says "you" and shows their own
numbers; a player with no catalogue entry (never played an official) shows
the name, the class played and "no ETF2L officials" rather than an empty
card; keyboard: Enter on a focused name opens it, Escape closes it and
returns focus.

### Q37. Career numbers from trends.tf, on demand

trends.tf's player page (`/player/<steamid64>/?format=highlander`) holds
what we cannot compute without their every log: W-L and winrate, per-class
winrate, damage per minute, accuracy and hours, aliases, and their ETF2L
and RGL teams. It has no JSON API, so the page is read the way a browser
reads it.
- One request when a profile is opened, never during search; cached a day
  per player.
- Parsed defensively: a table that is not found is left out, not an error,
  and the section says "trends.tf could not be read" rather than breaking
  the page. A test holds the parser to a saved copy of a real page.
- Credited on the page, with a link to the player's trends.tf profile.

### Q38. The division of the people you play

The ask the sample started with: every player in every match gets a
division tag from the catalogue -- their division the season the match
was played, or nearest season -- shown on the scoreboard and the matchups
("Prem", "High", ...). Then:
- Each match says the average division of each side: a scrim against a
  High team reads differently from one against a Low team.
- The profile's "Who you played" (Q9, opponent strength by rating) gains a
  division view: your rating against Premiership, High, Mid and Low
  players.

### Order

Q34 first (it is built; the check needs one backup), then Q35 (all from
data already here), Q36 (on Q34), Q38 (on Q35's player seasons), Q37 (a
page parser, independent), and Q39 whenever the fits can be run in one
sitting. Q35-Q38 are new features: a minor release.
