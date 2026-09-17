# HL Rating

A local performance rating system for Team Fortress 2 Highlander: pulls match
history from logs.tf, indexes the demos in your TF2 folder, and rates your play
per class against class-relative baselines.

See [PLAN.md](PLAN.md) for the full design and milestone plan.

**Status: M0** — the app boots, the database migrates, and configuration
persists. No match data yet.

## Prerequisites

| | |
|---|---|
| Node.js 20+ | installed |
| **Rust (stable)** | **not installed** — https://rustup.rs |
| **MSVC C++ build tools** | **not installed** — see below |
| WebView2 runtime | installed |

Rust needs the MSVC linker. Visual Studio Build Tools 2019 is present on this
machine but without the C++ workload, so install it from the Visual Studio
Installer: *Modify* → **Desktop development with C++** → Install. Then:

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
```

## Layout

```
crates/hl-core     domain types: SteamID, classes, config, tf path detection
crates/hl-db       SQLite access and migrations
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
