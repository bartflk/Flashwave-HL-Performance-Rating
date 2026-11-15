# Flashwave.tf 0.14.1 beta — Beta 4, fixes

**On 0.5.0 or later (Windows)?** The app offers this update itself: take it
from the card, or Settings › Updates › Check for updates.

**New install on Windows:** download the `-setup.exe` and run it. **Windows
will say "Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.14.1_x64-setup.exe -Algorithm SHA256
```

**Linux:** the `.AppImage` runs on most distributions
(`chmod +x Flashwave.tf_*.AppImage && ./Flashwave.tf_*.AppImage`); the `.deb`
is for Debian, Ubuntu and Mint (`sudo apt install ./Flashwave.tf_*.deb`).
They are built on GitHub a few minutes after this release appears.

The hashes are at the bottom. Updating keeps everything.

---

**Player stats before Season 24 are back.** 0.14.0 rates on a new model and
set the older seasons' officials aside to be downloaded again, and its
built-in league history did not have them: Seasons 12 to 23's rankings and
player stats were empty, though the medals still showed. This version's
league history has all of them, rated on the new model; the app takes it in
at its first start, with nothing to download.

## Fixes

- **Seasons 12 to 23**: player stats, rankings and season awards back, for
  new installs and updated ones alike. The league download now fetches
  officials it has picked but not downloaded before it looks for new ones.
- **ETF2L bans**: when ETF2L's ban list failed part-way, no Banned tag showed
  at all. The pages that were read are kept.
- **6v6 after an update**: a new 6v6 model left the 6v6 profile empty until
  the next sync. The app now rates 6v6 at startup, as it does Highlander.
- **RGL divisions**: teams that dropped out no longer take a seed; the seeds
  are numbered over the teams still playing, and the free-agent slot comes
  last.
- An RGL season card read "42 teams · [object Object]", and a team's
  "Renamed" tooltip showed a stray comma. Both fixed.

## Thanks

Everyone who reported the missing stats, and Icewind for drops.tf, where the
older seasons' logs came from again.

**SHA-256**

```
(filled in at release)
```

The Linux files are built on GitHub; their hashes are on the release page
beside each file.
