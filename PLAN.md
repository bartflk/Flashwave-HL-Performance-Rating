# HL Performance Rating System — Plan v1.8

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
| **v2** | Deep demo parse: aim and viewangles, engagement ranges (positions largely come from M6 now) | |

M1 acceptance: every Highlander log on the account stored, classified, deduplicated, and rebuildable from raw blobs with no refetching.

---

## 8. Still open

1. ~~Baselines~~ — settled in M3: the other players in your own matches, with you excluded.
2. **Final impact weights** — the TOML above is a first guess; expect to argue with it. Victim values v2 (Pyro 1.5, Spy 1.3, Scout 1.15 above Engineer) are applied; per-map and per-side values are designed in §3.
3. **Linux demos** — a second machine holds more POV demos. Import path to be designed; the `demoid` route may make it unnecessary.
4. **Sixes** — detected and stored, excluded from ratings. A later update.
5. **Verify jump ticks in-game.** The arithmetic is tested end to end (a sidecar killstreak at raw tick 51,212 lands at 50,879 after the 5 s lead), but only TF2 can confirm the demo shows the right moment.
6. **One real STV download.** The fetch is built and its metadata step verified live; the multi-megabyte download and the upload-time alignment of STV demos are untested.
7. **File watcher.** Demos are rescanned at startup, after every sync, and on demand; a live `notify` watcher was planned and is not built.
8. **Opponent strength.** ETF2L division and tier are now stored for every official, and scrim opponents are often named. The rating pool still weighs every performance equally. Weighting by the opponent's division is the natural next step.
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

**Map images (after M8).** For the 12 maps played on this account that more.tf covers (Upward, Vigil, Product, Ashville, Steel, Proot, Proplant, Gullywash, Cascade, Process, Swiftwater, Bagel), the kill map now draws more.tf's overview image, downloaded with the owner's permission to `<app data>/overviews/<map>.png`. The images are **not in the repo**: they are more.tf's renders, kept on this machine only.
- **Placement** uses more.tf's per-map transform: each image is a square covering 1024 × `scale` game units, centred on (`x` + 910·`scale`, `y` − 512·`scale`).
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

**To settle:** there is no level for most pug opponents. Weight only where known, or fall back to the owner's own division?

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

#### Step 4. Map and side baselines

**Why.** A 50th-percentile Vigil defence and a 50th-percentile Upward attack are different games. This covers function's "a Sniper pick on Upward is worth more than on Vigil" without guessing per-map values.

**Measure per side.** Split each Sniper performance into its attack and defence halves on attack/defence maps. Payload and A/D control points are split by the side worn each round (already known per kill). Kills, deaths and damage per round come from the raw log. KOTH has no side and stays whole.

**Baseline.** Percentile pools per (map, side), and only where a pool has 150 performances or more. Otherwise fall back to (map), then (all maps). The fallback used is shown on each rating.

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
