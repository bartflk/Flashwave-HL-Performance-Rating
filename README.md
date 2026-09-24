<p align="center"><img src="ui/public/logo.svg" width="96" alt="Flashwave.tf logo"></p>

# Flashwave.tf

A performance rating for **TF2 Highlander**, built from your logs.tf history
and your own demos. It rates every game you played against the players you
actually faced, the way HLTV's rating does for CS, and then shows you the
match kill by kill — including what your crosshair was doing and where you
walked.

Everything runs on your PC: no account, no server, nothing uploaded.

## What it does

**Ratings**

- A rating per class, **0 to 100**, where 50 is a typical game by the players
  you face. Your own games are left out of the pool you are measured against.
- The **Sniper model is the deepest**: impact kills valued by victim, class,
  map and side; damage per minute; untraded deaths; opening duels; Medic
  picks; Fight KAST; deaths to flankers; the Sniper duel. Every value and its
  weight is written up in [docs/sniper-rating.md](docs/sniper-rating.md).
- Kills are **worth what the situation is worth**: cleaning up while three or
  four players up counts for less, measured from 193,626 kills rather than
  guessed.
- Every rating is **checked against results**: `hl validate sniper` pairs the
  Snipers of 693 decided matches and reports how often the model picks the
  winner (73.6%, and 72.8% on matches it has never seen).

**Match pages**

- A **scoreboard** laid out like logs.tf, sortable by any column.
- The **nine class matchups**, each opening into a mirrored breakdown: both
  players' percentiles per component, what decided it, and how many rating
  points each row swung.
- A **round timeline**: caps, ubers and Medic deaths per round, per team.
- A **kill map** on the real map, with your kills and deaths, a heatmap, and
  **your movement drawn one line per life**.
- **Kill by kill**: play-by-play, fights (openings, trades, clean-ups, Fight
  KAST), damage and kills by class, and who held the uber advantage second by
  second.
- **Combined logs** list the individual logs they were built from, and are
  counted once rather than twice.

**Aim, from your own demos**

- Where your **crosshair sat** when each kill landed and one second earlier,
  drawn on a target with the path it took to get there.
- **Flick size**, **range** and **height** of every shot.
- Every death: **who killed you, from how far, and from which direction**,
  drawn as a compass — plus how close your nearest teammate was, and whether
  you were scoped.
- How much of your living time you spend **scoped**.
- Reading a demo takes about four seconds and happens after a sync.

**Your history**

- **Profile**: form, trend, strengths and weaknesses, and the aim figures,
  each against your usual, split by officials, scrims and pugs, and **by
  season**.
- **Match list** sortable by kills, damage, DPM or your rating, with a rating
  column, and filterable by season or date range.
- **Officials, scrims and pugs** are told apart automatically from ETF2L
  rosters and your regular teammates.
- **Demos** are found in your TF2 folder and matched to your matches; any kill
  copies a `demo_gototick` so you can watch that exact moment.
- A **copy of the database** is made before every sync, five kept, listed in
  Settings.

## Coming next

In the order it is queued (the detail lives in [PLAN.md](PLAN.md) §13):

- **Scout picks on KOTH worth more** — a weights change suggested by boSe,
  checked against the validator before it ships.
- **Map and side baselines** — judge a Vigil defence against other Vigil
  defences, so the hardest jobs stop reading as bad games.
- **Fight swing** — value a kill by how much it changes the chance of winning
  that fight, HLTV's Round Swing done properly.
- **Teamfights** — who collapsed on whom, who was dropped off cooldown, and
  what an uber exchange bought, from Taiga's feedback.
- **A model for every class** — Pyro, Engineer and Medic gain the most, since
  logs.tf says least about them.
- **Opponent strength** — weigh a Premiership game differently from a low one.
- **Colour themes** in settings.

## Install (Windows)

> **Uninstalling deletes your history if you let it.** The uninstaller offers
> a **"Delete the application data"** checkbox. Leave it unticked unless you
> mean it: that folder holds every log, demo index and rating the app has
> built. The app keeps five copies of its database in a `backups` folder
> beside it, but the uninstaller removes those too.


1. Download the `x64-setup.exe` from the [Releases page](../../releases).
2. Run it. Windows may show **"Windows protected your PC"** because the installer is not code-signed (signing costs money every year). Click **More info → Run anyway**.
3. The installer adds Flashwave.tf to the Start menu. It needs Microsoft WebView2, which Windows 10 and 11 already have; if not, the installer fetches it.

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
| Your own `.dem` files | Aim, deaths and movement: read on your PC, never uploaded |

Every rating comes from logs alone — the raw logs.tf log covers all 18 players
back to 2014 — so the app works fully without a single demo. Demos add the aim
and movement views, and only for the person who recorded them.

## Your data

The database lives at `%APPDATA%\gg.highlander.rating\hl.sqlite3`, with the
newest five copies in a `backups` folder beside it. A copy is made before
every sync and rebuild, and Settings lists them; to restore one, close the app
and rename the copy over `hl.sqlite3`.

Deleting the database resets the app. **Updating never touches it**, but
uninstalling with the **"Delete the application data"** box ticked removes the
folder, backups and all — keep a copy elsewhere if your history matters. Going
back to an older version after a newer one has opened the database does not
work, since the older build does not know the newer tables.

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
git tag v0.2.0
git push origin v0.2.0
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
