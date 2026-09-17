# HL Performance Rating System — Plan v0.3

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
- **Duplicate logs come flagged.** 248 logs are per-round uploads of matches that also have a combined log. Counting both would double-count a fifth of the history, and nothing in logs.tf alone reveals it.
- **demos.tf demo ids come attached**, which answers "find the demo if I don't have it locally" with no searching at all.

The cost is a third-party dependency. Mitigations: cache their index permanently like any other raw source, keep a class-coverage heuristic as a fallback classifier, and make every classification overridable by hand. Their docs note format detection is based on player count and playtime, so it shares the failure modes above — a strong default, not gospel.

### What the account actually contains

Measured, not estimated (trends.tf indexes 1,280 of the 1,492 logs.tf logs):

| | |
|---|---|
| Highlander | 1,105 |
| Sixes | 146 |
| Other / Prolander | 29 |
| **ETF2L official (Highlander)** | **155** |
| Logs with a demos.tf demo | **1,047** |
| Duplicate-of-another logs | 248 |
| Local POV demos | 101 |
| Range | 2014-05-17 → 2026-09-17 |

Two consequences worth stating plainly:

- **Officials are a real corpus, not a rounding error.** 155 ETF2L Highlander logs is enough to rate officials separately from scrims. The ETF2L API's own player-results endpoint returns far fewer, because it reflects only current team rosters — trends.tf's league tagging is the better source.
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
| Time to first pick | `rounds[].events` | Opening a round vs reacting to it |
| Assists | `classkillassists` | Damage that set up a teammate's kill |

`rounds[].events` carries timestamped events (caps, charges, medic deaths), which makes time-to-first-pick and "picks immediately before an uber push" computable without touching a demo.

### Deliberately deferred

- **Baselines** — self-relative vs division vs global. Start self-relative; the 17 other players in every log provide a free division-ish pool later.
- **Final weights** — hand-set now, fitted later.
- **Cross-class comparability** — within-class only until baselines are settled.

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

`duplicate_of` must be respected by **every** aggregate query. 248 of 1,280 logs are duplicates; forgetting that filter inflates a fifth of the history.

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
| **M1** | trends.tf index + logs.tf sync + normalize; match list | next |
| **M2** | Match page phase 1: matchups, round timeline, box score | |
| **M3** | Rating v1: Sniper in full, other classes generic; profile page | |
| **M4** | Demos: local index, demos.tf fetch by demoid, linking, jump-back | |
| **M5** | ETF2L context, officials vs scrims split, teammate tracking | |
| **v2** | Deep demo parse: positions, heatmaps, engagement ranges | |

M1 acceptance: every Highlander log on the account stored, classified, deduplicated, and rebuildable from raw blobs with no refetching.

---

## 8. Still open

1. **Baselines** — self, division, or global. Deferred deliberately until there is data to look at.
2. **Final impact weights** — the TOML above is a first guess; expect to argue with it.
3. **Linux demos** — a second machine holds more POV demos. Import path to be designed; the `demoid` route may make it unnecessary.
4. **Sixes** — detected and stored, excluded from ratings. A later update.
