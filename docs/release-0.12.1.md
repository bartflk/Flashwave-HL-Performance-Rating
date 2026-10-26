# Flashwave.tf 0.12.1 beta — sync fixes

**On 0.5.0 or later (Windows)?** The app offers this update itself: take it
from the card, or Settings › Updates › Check for updates.

**New install on Windows:** download the `-setup.exe` and run it. **Windows
will say "Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.12.1_x64-setup.exe -Algorithm SHA256
```

**Linux:** the `.AppImage` runs on most distributions
(`chmod +x Flashwave.tf_*.AppImage && ./Flashwave.tf_*.AppImage`); the `.deb`
is for Debian, Ubuntu and Mint (`sudo apt install ./Flashwave.tf_*.deb`).
They are built on GitHub a few minutes after this release appears.

The hashes are at the bottom. Updating keeps everything.

---

Games that never arrived now do. **Press Sync after updating**: the missing
ones come in on that sync, nothing else to do.

## Missing games

- **A Highlander evening uploaded again as one log** ("STEAMHAPPY MIX" and
  the like) was hidden: its maps were put behind the combined log, which
  trends.tf calls "other" for having no single map, so it was never
  downloaded. It now counts as Highlander when its maps all are (ivg).
- **A Highlander game a sub joined** (20 or so players in the log) was
  labelled "other" by trends.tf and never downloaded; importing one by hand
  left it hidden too. The app's own check, which counts each side's classes,
  now decides those, and they appear.

## A sync that keeps going

- **drops.tf is the main source for logs.** logs.tf is only asked about a
  log from the last hour, which drops.tf has not copied yet, so a sync no
  longer runs into logs.tf's limits. Finding your games still uses trends.tf,
  with logs.tf checked for the last hour.
- **trends.tf being down no longer stops a sync**; the games come from the
  logs.tf check instead.
- **logs.tf turning the app away no longer stops the downloads**; the rest
  come from drops.tf.

## Thanks

ivg for the report, and Icewind for drops.tf.

**SHA-256**

```
E31AE2DF405E8C3D5599DF691A8CE868AC849CEDD84494F0B133890BD2D7EC5C  Flashwave.tf_0.12.1_x64-setup.exe
4AEE8DE0B77E5529B381847F47F2A7BD432BA74B88B445F14B56EB1A784BEF32  Flashwave.tf_0.12.1_x64_en-US.msi
```

The Linux files are built on GitHub; their hashes are on the release page
beside each file.
