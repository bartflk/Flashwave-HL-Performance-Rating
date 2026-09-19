# HL Performance Rating System — Plan v1.1

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
10. **logs.tf-only combined logs.** Dedupe relies on trends.tf's `duplicate_of`. A combined log that only logs.tf knows (the 2018 S16 semi-final: `gullywash + badwater` plus both single-map logs) is counted alongside its parts.
11. **Raw logs still to fetch.** 16 of the oldest logs timed out when logs.tf stopped answering; the next sync retries them.
12. **Defending values.** Engineer 1.6 and Scout 1.0 on defence are proposed, not agreed. Worth checking with function, along with the first per-map values.
13. **Time to first pick and picks before an uber push.** Planned for M6, not built; the data is stored.
14. **Midfights and the uber split** as rating inputs (from M7's list): computable, not built.
15. **Hit cap date.** logs.tf's 450 cap started somewhere between December 2014 and June 2016; this account has no logs in that window to pin it down.

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
