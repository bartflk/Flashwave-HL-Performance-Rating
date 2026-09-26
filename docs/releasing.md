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

## Cutting a release

1. **Bump the version in three places** — the workspace `Cargo.toml`,
   `package.json`, and `src-tauri/tauri.conf.json`. The window title and
   Settings both derive from the first, so there is no fourth place any
   more (there used to be, and 0.4.0 shipped calling itself 0.3).

2. **Write `docs/release-<version>.md`.** The updater card shows its first
   few lines, so lead with what changed rather than with a heading.

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
     "target/release/bundle/nsis/latest.json" \
     --title "Flashwave.tf <version> alpha" \
     --notes-file docs/release-<version>.md \
     --prerelease
   ```

   **`latest.json` must be attached to the release.** Without it the
   updater has nothing to read and every client silently stays put.

## How the client finds it

The app asks for:

```
https://github.com/bartflk/Flashwave-HL-Performance-Rating/releases/latest/download/latest.json
```

GitHub keeps `/releases/latest/` pointed at the newest non-draft release,
so publishing is all it takes — there is no separate manifest to host and
nothing to keep in step by hand.

One consequence worth knowing: **a pre-release is not "latest"** as far as
that URL is concerned, *unless every release is a pre-release*. All of
these are marked `--prerelease`, so the newest one wins. If you ever publish
a stable release, the pre-releases after it stop being offered.

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
