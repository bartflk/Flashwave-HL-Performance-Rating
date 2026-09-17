# HL Performance Rating System — Plan v0.2

**Stack:** Tauri 2 + Rust core + React/TypeScript + SQLite
**v1 demo scope:** index only (header scan, log matching, in-game jump-back). No tick parsing until v2.

---

## 1. Architecture

```
┌────────────────────────────────────────────────┐
│ React 18 + TypeScript + Vite                   │
│ TanStack Query/Router · Recharts · Tailwind    │
└───────────────┬────────────────────────────────┘
                │ Tauri IPC (typed commands + events)
┌───────────────┴────────────────────────────────┐
│ Rust workspace                                 │
│  hl-core      domain types, SteamID, errors    │
│  hl-db        SQLite, migrations, queries      │
│  hl-logstf    logs.tf client + normalizer      │
│  hl-demos     header scan, watcher, matcher    │
│  hl-rating    baselines, percentiles, scoring  │
│  src-tauri    commands, state, background jobs │
└───────────────┬────────────────────────────────┘
                │
        SQLite (WAL) + raw JSON blobs
```

Rule: **every crate below `src-tauri` is a plain library with no Tauri dependency.** You can then drive the whole pipeline from a CLI or a test harness without the GUI — which is how you will actually iterate on the rating model.

### Key crates

| Need | Crate |
|---|---|
| HTTP | `reqwest` (rustls) + `governor` for rate limiting |
| Async | `tokio` |
| DB | `sqlx` (sqlite, compile-time checked queries) |
| Serde | `serde`, `serde_json` |
| Demo headers | `tf-demo-parser` (demostf/parser) — header-only for v1 |
| File watching | `notify` + `notify-debouncer-full` |
| Time | `time` or `chrono` |
| Errors | `thiserror` (libs) + `anyhow` (app) |
| Logging | `tracing` + `tracing-subscriber` |
| Parallelism | `rayon` (demo scanning) |

---

## 2. Repository layout

```
/
├── PLAN.md
├── Cargo.toml                  # workspace
├── crates/
│   ├── hl-core/
│   ├── hl-db/
│   │   └── migrations/         # 0001_init.sql, ...
│   ├── hl-logstf/
│   ├── hl-demos/
│   ├── hl-rating/
│   └── hl-cli/                 # dev harness: sync, reprocess, dump
├── src-tauri/
│   ├── src/commands/
│   └── tauri.conf.json
└── ui/
    ├── src/routes/
    ├── src/components/
    └── src/api/                # generated TS types from Rust
```

Use `ts-rs` or `specta` to generate TypeScript types from the Rust structs. Hand-maintaining two copies of 40 stat fields is a guaranteed source of silent bugs.

---

## 3. Data model

Raw-first: **store the source, derive everything else.** Normalization rules will be wrong at least twice; re-deriving from blobs costs seconds, re-fetching 500 logs costs an afternoon.

```sql
-- SOURCE OF TRUTH
log_raw(log_id INTEGER PK, fetched_at, etag, json TEXT)

-- IDENTITY
player(steamid64 PK, steamid3, display_name, is_me BOOL, rgl_div, updated_at)

-- NORMALIZED MATCH DATA
match(log_id PK, map, gamemode, played_at, duration_s,
      blue_score, red_score, title, uploader, format)

match_round(log_id, round_num, start_s, length_s, winner,
            firstcap, blue_dmg, red_dmg, blue_ubers, red_ubers,
            PK(log_id, round_num))

match_player(log_id, steamid64, team, kills, deaths, assists, suicides,
             dmg, dmg_real, dt, dt_real, hr, heal, ubers, drops,
             headshots, headshots_hit, backstabs, medkits, medkits_hp,
             sentries, cpc, ic, longest_killstreak, airshots,
             PK(log_id, steamid64))

match_player_class(log_id, steamid64, class, time_s, kills, assists,
                   deaths, dmg, PK(log_id, steamid64, class))

match_player_weapon(log_id, steamid64, class, weapon,
                    kills, dmg, avg_dmg, shots, hits)

match_medic(log_id, steamid64, advantages_lost, biggest_advantage_lost_s,
            deaths_with_95_uber, deaths_within_20s_after_uber,
            avg_time_to_build_s, avg_time_before_using_s, avg_uber_length_s)

match_ubertype(log_id, steamid64, medigun, count)
heal_spread(log_id, healer, target, heal)

-- DEMOS (v1: index only)
demo(id PK, path, file_hash, filename, map, server, recorder_nick,
     ticks, duration_s, recorded_at, file_mtime, size_bytes,
     kind TEXT CHECK(kind IN ('pov','stv')), source, indexed_at)

demo_link(demo_id, log_id, confidence REAL, method TEXT, tick_offset INTEGER,
          PK(demo_id, log_id))

-- DERIVED (disposable, rebuildable)
baseline(class, gamemode, stat, n, mean, sd, p10, p25, p50, p75, p90, computed_at)

rating(log_id, steamid64, class, engine_version, score,
       components JSON, computed_at, PK(log_id, steamid64, class, engine_version))

-- PLUMBING
sync_state(source PK, cursor, last_run_at, last_error)
app_config(key PK, value)
```

Everything under `-- DERIVED` must be droppable and rebuildable with one command. Version the rating engine in the row so old ratings don't silently mix with new ones.

---

## 4. Ingest pipelines

### 4.1 logs.tf

```
GET https://logs.tf/api/v1/log?player=<steamid64>&limit=100&offset=N
    -> { logs: [{ id, title, map, date, players, views }] }

GET https://logs.tf/api/v1/log/<id>
    -> full match JSON
```

- Filter to Highlander: `players == 18` (keep a manual override — pugs and ringers make this fuzzy).
- **Self-throttle to ~1 req/s.** No official rate limit is published; behave as if there is one.
- Store the blob, then normalize in a separate pass. `sync` and `reprocess` are different commands.
- Incremental: remember the highest log id seen in `sync_state`; a full backfill is a separate explicit action.
- Manual add: paste a log URL or id. This is how you will test everything.

### 4.2 Demos — v1 index only

1. Configurable `tf/` path. Your TF2 is **not** at the default Steam location — make this a required first-run setting with a folder picker, and validate that `<path>/demos` or `<path>/*.dem` exists.
2. `notify` watcher plus a full rescan on startup.
3. Header-only parse via `tf-demo-parser`: map, server, nick, tick count, duration. Milliseconds per file, no memory blowup.
4. Also read P-REC / in-game killstreak `_events.txt` / `.json` bookmark files if present — free, high-signal "something happened here" markers.
5. Hash by (size + first 64KB) so renames don't force a re-index.

### 4.3 Demo <-> log matching

Score candidate pairs and take the best above a threshold:

- map equal (**required**)
- `|demo_recorded_at − log.date|` within a window (demo file mtime ~ match end; log date ~ match start) — strongest signal
- demo duration ~ log duration
- recorder nick matches a player name in the log

Store the confidence and the method. Surface low-confidence links in the UI as "probably this match?" with a manual confirm — never silently guess.

### 4.4 Jump-back (the v1 demo payoff)

Given a linked demo and a timestamp from the log (a round start, an uber, a death), emit:

```
playdemo <name>; demo_gototick <tick>
```

Copy to clipboard, and optionally write a `.cfg` into `tf/cfg/`. Log time -> demo tick needs the demo's start offset; derive it once per link from round-start alignment and store it on `demo_link.tick_offset`. This is the feature that makes the app part of your review routine instead of a stat page you look at once.

---

## 5. Rating engine

```
raw stat
  -> per-minute / per-round normalization (time-weighted: stopwatch rounds vary wildly)
  -> percentile vs baseline(class, gamemode)
  -> weighted composite per class
  -> opponent adjustment (RGL div, later)
  -> shrinkage toward class mean by sample size
  -> score + confidence interval
```

**v1 = hand-set weights per class.** You know Highlander; encode that knowledge in a TOML file, not in Rust source, so you can tune without recompiling:

```toml
[sniper]
picks_per_min      = 0.30
headshot_ratio     = 0.20
dpm                = 0.15
deaths_per_min     = -0.20
time_to_first_pick = 0.15
```

**v2 = fit the weights.** Once ~200+ logs are stored, run a logistic regression of round outcome on per-round player features and replace the hand weights with learned ones. Keep both engines available and versioned so you can compare.

Hard rules:

- Never show a rating from fewer than N matches without a visible confidence band.
- Never compare across classes without saying you're comparing percentiles, not raw output.
- Split by `class_stats`, never by "what they main" — HL players flex constantly and it will wreck your numbers.

### Class metrics worth encoding

| Class | Beyond dmg/kills |
|---|---|
| Scout | Time alive, cap contribution, 1v1 win rate |
| Soldier | Damage during uber, airshots, self-damage economy |
| Pyro | Airblast/uber denial, reflect kills, spy-check rate |
| Demo | Pick rate, share of combo damage, sticky trap value |
| Heavy | Damage in uber, survival post-uber, heal-received share |
| Engineer | Sentry dmg/kills, uptime, tele usage |
| Medic | Build time, drops, uber advantage won, heal distribution, deaths at >=95% |
| Sniper | Headshot ratio, duel win rate, picks/min, time-to-first-pick |
| Spy | Backstabs per life, value of targets picked, escape rate |

---

## 6. IPC surface (first cut)

```rust
// config
get_config() / set_config(key, value)
detect_tf_path() -> Option<PathBuf>
set_tf_path(path) -> Result<TfPathInfo>

// sync
sync_logs(full: bool) -> JobId          // emits progress events
add_log_by_url(url: String) -> LogId
reprocess_all() -> JobId
scan_demos() -> JobId

// read
list_matches(filter) -> Vec<MatchSummary>
get_match(log_id) -> MatchDetail
get_player_profile(steamid64) -> Profile
get_class_profile(steamid64, class) -> ClassProfile
get_rating_trend(steamid64, class, range) -> Vec<RatingPoint>
list_demos(filter) -> Vec<DemoSummary>
get_demo_links(log_id) -> Vec<DemoLink>

// actions
confirm_demo_link(demo_id, log_id)
build_jump_command(log_id, demo_id, at_seconds) -> String
recompute_baselines() -> JobId
```

Long-running work returns a `JobId` and streams progress over a Tauri event channel. Do not block IPC on a 500-log backfill.

---

## 7. Milestones

**M0 — Skeleton**
Tauri app boots, SQLite + migrations, config screen (steamid64 + `tf/` path with folder picker).
*Done when:* the app remembers your settings across restarts.

**M1 — logs.tf pipeline**
Sync, store raw, normalize, match list UI.
*Done when:* your full HL history is in the DB and listed, and `reprocess_all` rebuilds every derived table from blobs with zero refetches.

**M2 — Match detail + class profile**
Full box score per match, per-round timeline, per-class aggregates and raw trends.
*Done when:* you'd genuinely rather open this than logs.tf for your own matches.

**M3 — Rating v1**
Baselines, percentiles, TOML-weighted composite, trend chart with confidence bands.
*Done when:* a bad game and a good game land where your gut says they should.

**M4 — Demo index + jump-back**
Watcher, header index, log matching with manual confirm, clipboard jump command.
*Done when:* you can go from "this round went badly" in the app to that moment in TF2 in under ten seconds.

**M5 — Insights & team view**
Auto-written observations, heal distribution, combo stats, opponent-adjusted rating, RGL context.

**v2 (deferred):** deep demo parsing — positional heatmaps, death maps, distance-to-medic, engagement ranges, STV downloads from demos.tf.

---

## 8. Open questions

1. Your SteamID64 — needed before anything syncs.
2. Where is your TF2 install? Not at the default Steam path.
3. Which league(s)? Determines whether RGL API integration earns its place in M5.
4. Do you already keep demos, and are they POV (your own recordings) or STV downloads? Changes what M4 can match against.
5. Rating philosophy: should it measure **impact on winning**, or **execution quality regardless of outcome**? They diverge sharply for Medic and Engineer, and it's a design choice, not a technical one.
