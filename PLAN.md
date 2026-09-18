# HL Performance Rating System — Plan v0.7

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
[victim_value]        # what killing this class is worth
medic       = 3.0
demoman     = 2.2
sniper      = 1.8     # denying their picks
heavy       = 1.4
soldier     = 1.2
engineer    = 1.1
scout       = 1.0
pyro        = 0.9
spy         = 0.9
```

A starting point, not a claim. These get replaced by fitted weights once there is enough data — regress round outcome on per-round features and let the numbers argue.

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
| **M5** | ETF2L context, officials vs scrims split, teammate tracking | |
| **v2** | Deep demo parse: positions, heatmaps, engagement ranges | |

M1 acceptance: every Highlander log on the account stored, classified, deduplicated, and rebuildable from raw blobs with no refetching.

---

## 8. Still open

1. ~~Baselines~~ — settled in M3: the other players in your own matches, with you excluded.
2. **Final impact weights** — the TOML above is a first guess; expect to argue with it.
3. **Linux demos** — a second machine holds more POV demos. Import path to be designed; the `demoid` route may make it unnecessary.
4. **Sixes** — detected and stored, excluded from ratings. A later update.
5. **Verify jump ticks in-game.** The arithmetic is tested end to end (a sidecar killstreak at raw tick 51,212 lands at 50,879 after the 5 s lead), but only TF2 can confirm the demo shows the right moment.
6. **One real STV download.** The fetch is built and its metadata step verified live; the multi-megabyte download and the upload-time alignment of STV demos are untested.
7. **File watcher.** Demos are rescanned at startup, after every sync, and on demand; a live `notify` watcher was planned and is not built.

