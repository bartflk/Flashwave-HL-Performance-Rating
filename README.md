<p align="center"><img src="ui/public/logo.svg" width="96" alt="HL Rating logo"></p>

# HL Rating

A performance rating for **TF2 Highlander**, built from your logs.tf history. It rates every game you played against the players you actually faced, like HLTV's rating does for CS, and shows each match kill by kill.

- **Ratings per class, 0–100**, where 50 is a typical game by the players you face. The Sniper model is the most developed; see [docs/sniper-rating.md](docs/sniper-rating.md) for every value it uses and why.
- **Match pages:** the nine class matchups, a round timeline, a kill map on the real map, heatmaps, play-by-play, fights (openings, trades, Fight KAST), and who had the uber advantage second by second.
- **Profile:** form, trend, strengths and weaknesses, split by officials, scrims and pugs, and **by season**.
- **Officials, scrims and pugs** are told apart automatically from ETF2L rosters and your regular teammates.
- **Demos (optional):** point it at your TF2 folder and any kill jumps to that moment in your recording.

Everything is stored on your PC. No account, no server.

> **0.1 beta.** This is the first public test build. Numbers and screens will change; feedback is very welcome in [Issues](../../issues).

## Install (Windows)

1. Download `HL Rating_0.1.0_x64-setup.exe` from the [Releases page](../../releases) (the newest "0.1 beta" entry).
2. Run it. Windows may show **"Windows protected your PC"** because the installer is not code-signed (signing costs money every year). Click **More info → Run anyway**.
3. The installer adds HL Rating to the Start menu. It needs Microsoft WebView2, which Windows 10 and 11 already have; if not, the installer fetches it.

## First run

1. **Your SteamID.** Any format works: SteamID64, `[U:1:…]`, `STEAM_0:…` or a link to your Steam profile.
2. **Your TF2 folder (optional).** Only needed for demo jumps. Auto-detect finds most installs; you can skip it and set it later in Settings.
3. **Start the first sync.** It finds every match you played, then fetches logs from logs.tf **one every 2 seconds** to go easy on their servers. The detailed kill data comes **100 matches per sync**, so a long history takes a few syncs: press **Sync** again later for the rest. The app is usable while it runs.

## Where the data comes from

| Source | Used for |
|---|---|
| [logs.tf](https://logs.tf) | Match stats, and the raw server log behind each match: every kill with positions, every hit, heal and uber |
| [trends.tf](https://trends.tf) | Which matches you played, which logs were combined from which |
| [ETF2L](https://etf2l.org) | Officials, divisions, rosters and seasons |
| [demos.tf](https://demos.tf) | STV demos, downloaded only when you ask |

No demo parsing is needed for any rating: the raw logs.tf log covers all 18 players back to 2014.

## Your data

The database lives at `%APPDATA%\gg.highlander.rating\hl.sqlite3`. Deleting it resets the app. Uninstalling does not delete it.

---

## Building from source

| | |
|---|---|
| Node.js 20+ | https://nodejs.org |
| Rust (stable) | https://rustup.rs |
| MSVC C++ build tools | see below |
| WebView2 runtime | ships with Windows 10/11 |

Rust on Windows needs the MSVC linker and the Windows SDK. Install the **Desktop development with C++** workload from the Visual Studio Installer, or:

```
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install Rustlang.Rustup
```

Restart your terminal afterwards so `cargo` is on the PATH. Then:

```
npm install          # once, at the repo root
npm install --prefix ui
npm run dev          # builds Rust, starts Vite, opens the app window
npm run build        # installers in target/release/bundle
```

`scripts\dev.cmd` does the same thing by double-click: it installs anything
missing, then starts the app in development mode. Copy it to the Desktop if
you want it to hand.

### Releasing

Pushing a version tag builds the Windows installers on GitHub and attaches them to a draft release:

```
git tag v0.1.0-beta
git push origin v0.1.0-beta
```

Bump `version` in `src-tauri/tauri.conf.json` and in the root `Cargo.toml` (`[workspace.package]`) first. Review the draft on the Releases page, then publish it.

### Frontend without Rust

Outside the app window, the UI falls back to `ui/src/api/mock.ts` with real match fixtures, so screens can be worked on in a plain browser. The mock is left out of release builds.

```
npm run ui           # http://localhost:5173
```

### CLI harness

`hl` reads and writes the same database as the app:

```
cargo run -p hl-cli -- status
cargo run -p hl-cli -- sync                # index + fetch new logs
cargo run -p hl-cli -- reprocess           # rebuild from stored logs, no network
cargo run -p hl-cli -- match 4109131       # matchups for one match
cargo run -p hl-cli -- profile sniper      # your profile on a class
cargo run -p hl-cli -- fights sniper       # kills in context against the pool
cargo run -p hl-cli -- seasons sniper      # your seasons
cargo run -p hl-cli -- validate sniper     # how often the rating picks the winner
```

### Layout

```
crates/hl-core     domain types: SteamID, classes, config, tf path detection
crates/hl-db       SQLite access and migrations
crates/hl-ingest   sources, raw logs, game state, fights, seasons, validation
crates/hl-rating   rating model, matchups, match detail, profile; weights.toml
crates/hl-demos    demo headers, sidecars, demo-to-log linking, jump ticks
crates/hl-cli      developer harness
src-tauri          Tauri shell: commands, state, window
ui                 React + TypeScript frontend
scripts/logo.py    the app logo and icon source
```

See [PLAN.md](PLAN.md) for the full design and milestone history.

### Tests

```
cargo test --workspace
cd ui && npx tsc --noEmit
```

---

*Not affiliated with Valve. Team Fortress 2 is a trademark of Valve Corporation. Class icons are Valve's, from the [Official TF2 Wiki](https://wiki.teamfortress.com). The app logo is original.*
