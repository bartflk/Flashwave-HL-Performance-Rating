# HL Rating

A local performance rating system for Team Fortress 2 Highlander: pulls match
history from logs.tf, indexes the demos in your TF2 folder, and rates your play
per class against class-relative baselines.

See [PLAN.md](PLAN.md) for the full design and milestone plan.

**Status: M6** — syncs your full match history from trends.tf and logs.tf,
rates every performance against the players you actually face, and shows it
two ways: a match page with the nine class matchups, a round timeline and the
scoreboard; and a profile per class with form, trend, and which parts of your
game are strong or weak. Your demos are linked to their matches, and any moment on
a match's timeline can be jumped to in TF2. Every match is sorted into an
ETF2L official, a scrim or a pug, the profile splits by the three, and a
Teammates page shows your ETF2L teams and who you play with most.

## Prerequisites

| | |
|---|---|
| Node.js 20+ | https://nodejs.org |
| Rust (stable) | https://rustup.rs |
| MSVC C++ build tools | see below |
| WebView2 runtime | ships with Windows 10/11 |

Rust on Windows needs the MSVC linker and the Windows SDK. Install the
**Desktop development with C++** workload from the Visual Studio Installer, or:

```
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Then Rust:

```
winget install Rustlang.Rustup
```

Restart your terminal afterwards so `cargo` is on the PATH.

## Running

```
npm install          # once, at the repo root
npm run dev          # builds Rust, starts Vite, opens the app window
```

`npm run build` produces an installer in `src-tauri/target/release/bundle`.

### Frontend without Rust

The UI detects when it is running outside the app window and falls back to
`ui/src/api/mock.ts`, so you can iterate on screens in a plain browser without
waiting on Rust rebuilds:

```
npm run ui           # http://localhost:5173
```

### CLI harness

`hl` reads and writes the same database as the app, for inspecting state and
reproducing bugs without a window:

```
cargo run -p hl-cli -- status
cargo run -p hl-cli -- set-steamid 76561198000000000
cargo run -p hl-cli -- tf detect
cargo run -p hl-cli -- sync                # index + fetch new logs
cargo run -p hl-cli -- reprocess           # rebuild from stored logs, no network
cargo run -p hl-cli -- matches 30 --officials
cargo run -p hl-cli -- match 4109131       # matchups for one match
cargo run -p hl-cli -- rate                # rebuild baselines and ratings
cargo run -p hl-cli -- profile sniper      # your profile on a class
cargo run -p hl-cli -- demos               # scan and link demos
```

## Layout

```
crates/hl-core     domain types: SteamID, classes, config, tf path detection
crates/hl-db       SQLite access and migrations
crates/hl-ingest   trends.tf + logs.tf clients, normalizer, classifier, dedupe
crates/hl-rating   rating model v1, matchups, match detail, profile; tunable weights.toml
crates/hl-demos    demo headers, sidecars, log clocks, demo-to-log linking, jump ticks
crates/hl-cli      developer harness
src-tauri          Tauri shell: commands, state, window
ui                 React + TypeScript frontend
scripts            placeholder icon generation
```

Everything below `src-tauri` is a plain library with no Tauri dependency, so the
whole pipeline is drivable from the CLI and from tests.

## Tests

```
cargo test --workspace
cd ui && npx tsc --noEmit
```

## Data

The database lives at `%APPDATA%\gg.highlander.rating\hl.sqlite3`. Deleting it
resets the app; migrations re-run on next launch.
