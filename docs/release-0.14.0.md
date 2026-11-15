# Flashwave.tf 0.14.0 beta — Beta 4

**On 0.5.0 or later (Windows)?** The app offers this update itself: take it
from the card, or Settings › Updates › Check for updates.

**New install on Windows:** download the `-setup.exe` and run it. **Windows
will say "Windows protected your PC"** — the installer is not code-signed.
Right-click the download → **Properties** → tick **Unblock** → **OK**. Check
it is my build first:

```powershell
Get-FileHash .\Flashwave.tf_0.14.0_x64-setup.exe -Algorithm SHA256
```

**Linux:** the `.AppImage` runs on most distributions
(`chmod +x Flashwave.tf_*.AppImage && ./Flashwave.tf_*.AppImage`); the `.deb`
is for Debian, Ubuntu and Mint (`sudo apt install ./Flashwave.tf_*.deb`).
They are built on GitHub a few minutes after this release appears.

The hashes are at the bottom. Updating keeps everything.

---

**6v6 is here.** Your 6v6 matches, rated by role, beside Highlander and never
mixed with it. ETF2L's 6v6 seasons with a Home of their own, player and team
pages for 6v6, ETF2L bans on every name, a much bigger RGL tab, and an
activity log that copies a problem report in one click.

After updating, a card asks which formats you play. Highlander players see no
change unless they tick 6v6 (Settings › Formats you play). Highlander ratings
are re-rated on model v14 at the first start.

## 6v6

- **Your 6v6 matches**, kept apart from Highlander's: hover Matches, Profile
  or ETF2L and pick 6v6.
- **A rating by role**: pocket, roamer, flank Scout, pocket Scout, Demoman and
  Medic each have their own model, and the five off-classes theirs. Roles are
  read from the log (the Medic's heals tell the pocket from the roamer), and
  you can correct your own.
- **Against the whole 6v6 field from the first sync**: each game is compared
  with every 6v6 official in the logs.tf database (63,959 games of ETF2L, RGL
  and other leagues), and the weights were fitted to who won them. The
  higher-rated side won 89.9% of those officials.
- **How ratings work** has a 6v6 page: each role's parts, their share, and
  which are not measured yet (round impact joins once it is). It is in the
  new **?** menu in the top bar.
- **The match page in 6v6**: role matchups, 6v6 numbers (damage taken,
  airshots, Medic picks, space created, sticky, reflect and sentry kills),
  Medic against Medic, off-class stints, opponent strength from 6v6 ratings,
  and each player's 6v6 division.
- **Your 6v6 profile** to the Highlander profile's standard: your 6v6 team,
  role and medals, best games, seasons and form. Anyone else's from
  ETF2L › 6v6 › Players, with a search.
- **ETF2L 6v6**: a Home with the season's banner, today's officials and each
  division's leader; the season by week, its teams, news and recruitment;
  medals and MVPs for each season.

## ETF2L

- **Bans**: a Banned tag beside every name with an active ETF2L ban, the
  reason on hover.

## RGL

- Match pages with lineups, head-to-head, logs and their statistics.
- Team pages with tabs, trophies, form, opponents and map pool, and Compare.
- Player pages laid out as the app's own, with medals and a rating.
- Matches live now and tonight, by week, with calendar export.
- Seasons as events, with podiums, champions and the map pool; a Rankings
  page; a season banner on the Home.
- Your own RGL matches are marked in your match list.

## When something goes wrong

- **Settings › Activity log**: every request and line the app writes, live,
  with each site's status. A waterfall of it runs on the sync card.
- **Copy report** on a failed sync, on any error and in the **?** menu: the
  error, the app's details and the log's last lines, ready to paste. Keys and
  SteamIDs in URLs are left out.
- A sync no longer fails with "database is locked" while the 6v6 ratings are
  rebuilt in the background.
- A site that asks the app to slow down is asked more slowly, without slowing
  the others.

## Also

- Map images for Dustbowl, Egypt, Gorge, Junction, Mountain Lab, Barnblitz
  and King, from maps.tf2.lol.
- Class matchups held to a readable width; Medic against Medic centred.
- Tests across the app, and the fixes they found.

## Thanks

boSe. for what each 6v6 role should be judged on, and everyone on the
Discord who tested the 6v6 preview. Icewind for drops.tf and demos.tf. The
6v6 rating's league pool and its fitted weights come from every 6v6 official
in logs.tf's database, as do the Highlander history and scales; thanks to
logs.tf. Map images from maps.tf2.lol, more.tf and demos.tf. The RGL tab reads
RGL's public API. Map screenshots, medal art and the kill map's icons are
Valve's, from the Official TF2 Wiki.

**SHA-256**

```
(filled in at release)
```

The Linux files are built on GitHub; their hashes are on the release page
beside each file.
