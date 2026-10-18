# Releasing, and how the in-app updater works

The app checks GitHub on startup and offers the new version in a card. It
downloads the installer itself and runs it, so nobody has to go through the
"Windows protected your PC → Unblock" dance for an update — only for their
first install.

## The one thing you cannot lose

```
%USERPROFILE%\.flashwave-keys\flashwave.key
```

Every update is signed with this, and the matching public key is compiled
into the app. An installed copy will refuse anything not signed by it.

**If you lose this key, you cannot update anyone.** Not "it gets harder" —
every existing install becomes a dead end that has to be replaced by hand.
Back it up somewhere that is not this machine. It has no password, so treat
the file itself as the secret: anyone holding it can push an update to
every install.

That signature is *not* code signing and does nothing for SmartScreen. It
answers a different question — not "does Microsoft trust this publisher"
but "was this built by whoever holds the key". Which is the question that
matters when the download comes from a GitHub release page anyone can open
a PR against.

## Picking the number

Versions follow [Semantic Versioning 2.0.0](https://semver.org/): `MAJOR.MINOR.PATCH`,
no leading zeroes, and a published version is never changed -- a fix is a new
version. What counts as the "public API" here is what other people and older
installs depend on: the database (migrations must keep upgrading every older
one), `config.json` and `weights.toml`, the `.lang` format, backups, and the
updater's `latest.json`.

- **PATCH** (0.5.0 → 0.5.1): only fixes and speed-ups, nothing new to see.
- **MINOR** (0.5.1 → 0.6.0, patch back to 0): anything new -- a panel, a stat,
  a setting. While the app is 0.y.z (SemVer's initial development), a release
  that removes or breaks something is a minor bump too, and its notes say so.
- **MAJOR**: from 1.0.0 on, anything that breaks the list above.
- Test builds are pre-releases: `0.6.0-beta.1`, which sorts before `0.6.0`.

## Cutting a release

1. **Bump the version in three places** — the workspace `Cargo.toml`,
   `package.json`, and `src-tauri/tauri.conf.json`. The window title and
   Settings both derive from the first, so there is no fourth place any
   more (there used to be, and 0.4.0 shipped calling itself 0.3).

2. **Write `docs/release-<version>.md`.** The updater card shows its first
   few lines, so lead with what changed rather than with a heading. If the
   rating model changed, regenerate the browser mock of "How ratings work"
   (`hl guide --json > ui/src/api/fixtures/rating_guide.json`); the app's
   own page reads the live model and needs nothing.

2b. **Make the league snapshot.** The app ships every official's results,
   rosters and league ratings (`league/snapshot.sqlite3.gz`, PLAN Q43), so the
   player lookup works on a new install. It is made from your database, rated
   for this release's model -- on a copy, never the live file:

   ```powershell
   cargo run --release -p hl-cli -- copy "$env:TEMP\hl-snapshot-src.sqlite3"
   cargo run --release -p hl-cli -- --db "$env:TEMP\hl-snapshot-src.sqlite3" rate
   cargo run --release -p hl-cli -- --db "$env:TEMP\hl-snapshot-src.sqlite3" snapshot
   ```

   (`hl copy` needs the app closed. With it open, copy the newest file in
   `backups` instead; it is a closed, consistent copy.) Commit the new
   `league/snapshot.sqlite3.gz`. `npm run release` refuses to build when the
   snapshot is for another rating model than the build.

3. **Build it signed:**

   ```bash
   npm run release
   ```

   This refuses to start without the key, builds, and writes `latest.json`
   next to the installer using the signature of the file it just made.

4. **Check the version is really in the binary** before publishing:

   ```powershell
   (Get-Item target\release\hl-app.exe).VersionInfo.FileVersion
   ```

5. **Publish, with both files:**

   ```bash
   gh release create v<version> \
     "target/release/bundle/nsis/Flashwave.tf_<version>_x64-setup.exe" \
     "target/release/bundle/msi/Flashwave.tf_<version>_x64_en-US.msi" \
     "target/release/bundle/nsis/latest.json" \
     --title "Flashwave.tf <version> beta" \
     --notes-file docs/release-<version>.md \
     --latest
   ```

   **`latest.json` must be attached to the release.** Without it the
   updater has nothing to read and every client silently stays put.

   **And the release must be marked Latest -- not a pre-release.** "beta" (alpha until 0.10.0)
   lives in the title and the app's badge only. (0.7.0 shipped saying
   "beta" by mistake; its release page and update card were corrected.)

6. **Let the Linux build land.** Publishing the release pushes the tag, which
   starts the *Linux release* workflow on GitHub (`.github/workflows/release.yml`).
   About 15 minutes later the release has the AppImage and the `.deb`, and,
   if the key is in the repository's secrets (below), `latest.json` has a
   `linux-x86_64` entry beside the Windows one. Check it on the Actions tab;
   if it failed, run it again by hand from there.

7. **Check what the updater will actually see** -- give GitHub's cache a
   minute:

   ```bash
   curl -sL https://github.com/bartflk/Flashwave-HL-Performance-Rating/releases/latest/download/latest.json
   ```

   It must say the new version. If it says the old one, the release is not
   marked Latest.

## Linux

Linux cannot be built on the Windows PC, so GitHub builds it (Ubuntu 22.04,
for the widest reach) from the tag. For AppImage installs to update
themselves the AppImage must be signed with the same key as the Windows
installer, which means giving GitHub a copy of it:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY < "$USERPROFILE/.flashwave-keys/flashwave.key"
```

That is a real decision, not a formality: the key has no password, and a
GitHub secret is readable by any workflow that runs in this repository. Put
it there only if you are content that whoever can change the workflows here
could sign an update. Without it the Linux files are still built and
attached, unsigned; `latest.json` gets no Linux entry, the app on Linux
finds no update and says nothing, and Linux users update by downloading.

A `.deb` never installs updates itself, signed or not: the package manager
owns its files. The app shows the update and its button opens the release
page instead.

## Keeping `target` small

`npm run dev` and `npm run build` (so `npm run release` too) first run
`scripts/sweep.mjs`: when `target` is over 30 GB (`HL_TARGET_CAP_GB`), it
deletes the incremental caches, then if still over, everything but
`release/bundle`. Cargo never cleans up on its own -- hl-queue's `target`
had reached 68 GB. `npm run sweep -- --report` says how big it is.

## How the client finds it

The app asks for:

```
https://github.com/bartflk/Flashwave-HL-Performance-Rating/releases/latest/download/latest.json
```

GitHub keeps `/releases/latest/` pointed at the newest non-draft release,
so publishing is all it takes — there is no separate manifest to host and
nothing to keep in step by hand.

One consequence worth knowing: **a pre-release is never "latest"** while
any full release exists. 0.4.1 went out as a full release, so when 0.5.0
was first published as a pre-release this URL kept serving 0.4.1's manifest
and nobody would have been offered the update. Publish every release as
Latest (`--latest`), and check the URL afterwards (step 6).

## Why the restart is a button

The app holds an exclusive lock on the database while its window is open.
An installer replacing files under a running process is the shape of the
thing that corrupted the database twice in September 2026, so the update
downloads, installs, and then *asks*. Closing cleanly first is the safe
order, and it is worth the extra click.

## If an update fails

It lands in **Settings › Problems** with the reason, like everything else,
and **Copy report** puts it somewhere it can be sent. The usual causes:

- **`latest.json` was not uploaded** — the check finds nothing, silently.
- **The URL in the manifest does not match the tag.** `npm run release`
  builds it from the version, so a tag that is not `v<version>` produces a
  404 at download time.
- **Built without the key.** Then there is no `.sig`, the script stops
  before writing a manifest, and you find out at build time rather than
  from a tester.
- **The manifest starts with a byte-order mark.** A JSON reader rejects it
  and the check finds nothing. `release.ps1` writes it without one; if you
  ever write `latest.json` by hand, check its first byte is `{`.
- **The release is a pre-release.** See above: `/releases/latest/` skips it.
